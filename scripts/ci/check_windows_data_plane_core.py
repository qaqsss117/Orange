from __future__ import annotations

import argparse
import hashlib
import hmac
import http.server
import json
import os
import platform
import re
import shutil
import socket
import struct
import subprocess
import sys
import tempfile
import threading
import time
from pathlib import Path, PurePosixPath
from typing import NamedTuple


ROOT = Path(__file__).resolve().parents[2]
MODULE_DIR = ROOT / "native" / "dataplane"
POLICY_PATH = MODULE_DIR / "build-policy.json"
DEFAULT_MANIFEST = ROOT / "artifacts" / "security" / "windows-data-plane-artifacts.json"
DEFAULT_REPORT = ROOT / "artifacts" / "security" / "windows-data-plane-core.json"
EXPECTED_FORBIDDEN_TAGS = {
    "with_acme",
    "with_ccm",
    "with_clash_api",
    "with_dhcp",
    "with_gvisor",
    "with_naive_outbound",
    "with_ocm",
    "with_tailscale",
    "with_utls",
    "with_v2ray_api",
    "with_wireguard",
}
FORBIDDEN_BINARY_MODULES = (
    "github.com/anthropics/anthropic-sdk-go",
    "github.com/openai/openai-go",
    "github.com/sagernet/cronet-go",
    "github.com/sagernet/tailscale",
    "github.com/sagernet/wireguard-go",
    "golang.zx2c4.com/wireguard/windows",
)
MANAGED_HOST_SOURCES = (
    "native/dataplane/runtime.go",
    "native/dataplane/protocol.go",
    "native/dataplane/cmd/orange-data-plane/main.go",
)

sys.path.insert(0, str(ROOT / "scripts" / "security"))
from check_build_artifacts import validate_artifact_manifest  # noqa: E402


class SignatureInfo(NamedTuple):
    status: str
    thumbprint: str
    subject: str


def read_json(path: Path) -> dict[str, object]:
    value = json.loads(path.read_text(encoding="utf-8"))
    if not isinstance(value, dict):
        raise RuntimeError(f"expected a JSON object: {path}")
    return value


def run_checked(
    arguments: list[str],
    *,
    cwd: Path = ROOT,
    environment: dict[str, str] | None = None,
    timeout: float = 180.0,
) -> str:
    executable = shutil.which(arguments[0])
    if executable is None:
        raise RuntimeError(f"required command is missing: {arguments[0]}")
    result = subprocess.run(
        [executable, *arguments[1:]],
        cwd=cwd,
        env=environment,
        check=False,
        capture_output=True,
        text=True,
        encoding="utf-8",
        errors="replace",
        timeout=timeout,
    )
    if result.returncode != 0:
        output = "\n".join(part for part in (result.stdout, result.stderr) if part).strip()
        raise RuntimeError(
            f"{' '.join(arguments)} failed with exit code {result.returncode}: {output}"
        )
    return result.stdout


def normalized_relative_path(value: object) -> str | None:
    if not isinstance(value, str) or not value or "\\" in value:
        return None
    path = PurePosixPath(value)
    if path.is_absolute() or any(part in {"", ".", ".."} for part in path.parts):
        return None
    return path.as_posix()


def validate_policy(root: Path, policy: dict[str, object]) -> None:
    import tomllib

    toolchains = tomllib.loads((root / "toolchains.toml").read_text(encoding="utf-8"))
    sing_box = toolchains["sing_box"]
    exact = {
        "schema_version": 2,
        "hosting_model": "signed-orange-managed-sing-box-host",
        "go_module": sing_box["go_module"],
        "go_package": "orange.dev/native/dataplane/cmd/orange-data-plane",
        "version": sing_box["version"],
        "runtime_relative_path": "orange-data-plane.exe",
        "service_executable": "orange-service.exe",
        "runtime_download_allowed": False,
    }
    for field, expected in exact.items():
        if policy.get(field) != expected:
            raise RuntimeError(f"Windows Data Plane policy has invalid {field}")
    go_version = run_checked(["go", "version"]).strip()
    expected_go_version = f"go version go{toolchains['go']['recommended']} windows/amd64"
    if go_version != expected_go_version:
        raise RuntimeError(
            f"Windows Data Plane requires the exact artifact toolchain: {expected_go_version}"
        )
    if policy.get("target") != {"goos": "windows", "goarch": "amd64", "cgo_enabled": False}:
        raise RuntimeError("Windows Data Plane policy must target windows/amd64 without CGO")
    tags = policy.get("build_tags")
    if tags != ["with_quic"]:
        raise RuntimeError("Windows Data Plane must use only the with_quic feature tag")
    forbidden_tags = policy.get("forbidden_build_tags")
    if not isinstance(forbidden_tags, list) or set(forbidden_tags) != EXPECTED_FORBIDDEN_TAGS:
        raise RuntimeError("Windows Data Plane forbidden build tags are incomplete")
    if set(tags) & set(forbidden_tags):
        raise RuntimeError("Windows Data Plane enables a forbidden feature tag")
    artifact_path = normalized_relative_path(policy.get("artifact_path"))
    if artifact_path != "artifacts/data-plane/windows-amd64/orange-data-plane.exe":
        raise RuntimeError("Windows Data Plane artifact path is not fixed")
    if policy.get("runtime_relative_path") != "orange-data-plane.exe":
        raise RuntimeError("Windows Data Plane runtime name is not fixed")
    if policy.get("control_protocol") != {
        "schema_version": 1,
        "transport": "inherited-stdio",
        "max_frame_bytes": 4096,
        "max_concurrent_probes": 8,
        "commands": [
            "cancel_probe",
            "probe_delay",
            "read_selected_node",
            "select_node",
            "traffic",
        ],
        "delay_target": "sing-box-default-https-204",
    }:
        raise RuntimeError("Windows Data Plane control protocol is not fixed and bounded")
    if policy.get("registered_capabilities") != {
        "inbounds": ["mixed", "tun"],
        "outbounds": ["direct", "hysteria2", "selector", "shadowsocks", "trojan"],
        "dns_transports": ["local"],
        "network_control_listeners": [],
    }:
        raise RuntimeError("Windows Data Plane registered capabilities differ")
    release = policy.get("release")
    if not isinstance(release, dict) or release.get("require_authenticode") is not True:
        raise RuntimeError("Windows Data Plane release must require Authenticode")
    thumbprints = release.get("allowed_signer_sha1_thumbprints")
    if not isinstance(thumbprints, list) or not all(
        isinstance(value, str) and re.fullmatch(r"[0-9A-F]{40}", value) for value in thumbprints
    ):
        raise RuntimeError("release signer thumbprints must be uppercase SHA-1 values")
    if len(thumbprints) != len(set(thumbprints)):
        raise RuntimeError("release signer thumbprints must be unique")

    go_mod = (root / "native" / "dataplane" / "go.mod").read_text(encoding="utf-8")
    expected_requirement = re.search(
        rf"^\s*{re.escape(str(sing_box['go_module']))}\s+v{re.escape(str(sing_box['version']))}\s*$",
        go_mod,
        re.MULTILINE,
    )
    if expected_requirement is None or "replace " in go_mod:
        raise RuntimeError("Data Plane build module does not pin the official sing-box module")
    run_checked(["go", "mod", "verify"], cwd=root / "native" / "dataplane")
    run_checked(
        ["go", "test", "-tags", "with_quic", "./..."],
        cwd=root / "native" / "dataplane",
    )
    validate_managed_host(root, policy)


def validate_managed_host(root: Path, policy: dict[str, object]) -> None:
    sources = {path: (root / path).read_text(encoding="utf-8") for path in MANAGED_HOST_SOURCES}
    runtime = sources["native/dataplane/runtime.go"]
    protocol = sources["native/dataplane/protocol.go"]
    main = sources["native/dataplane/cmd/orange-data-plane/main.go"]
    required_runtime = (
        "group.RegisterSelector(outboundRegistry)",
        "selector.SelectOutbound(nodeID)",
        "selector.Now() != nodeID",
        'urltest.URLTest(ctx, "", node)',
        "instance.Router().AppendTracker(tracker)",
        "bufio.NewCounterConn(",
        "bufio.NewCounterPacketConn(",
    )
    if any(marker not in runtime for marker in required_runtime):
        raise RuntimeError("Orange Data Plane runtime is missing an authoritative core operation")
    registered = sorted(
        re.findall(r"^\s*([a-zA-Z0-9]+)\.Register(?:Inbound|Outbound|Selector|Transport)\(", runtime, re.MULTILINE)
    )
    if registered != ["direct", "group", "hysteria2", "local", "mixed", "shadowsocks", "trojan", "tun"]:
        raise RuntimeError("Orange Data Plane runtime registry differs from the capability policy")
    protocol_commands = set(re.findall(r'"(cancel_probe|probe_delay|read_selected_node|select_node|traffic)"', protocol))
    expected_commands = set(policy["control_protocol"]["commands"])
    if protocol_commands != expected_commands:
        raise RuntimeError("Orange Data Plane protocol command surface differs from policy")
    protocol_markers = (
        "MaxFrameBytes       = 4 << 10",
        "MaxConcurrentProbes = 8",
        "request.ID <= s.lastID",
        "duplicate := fields[name]",
        "validPublicID(request.SelectorID)",
        "request.TargetRequestID >= request.ID",
    )
    if any(marker not in protocol for marker in protocol_markers):
        raise RuntimeError("Orange Data Plane protocol bounds are incomplete")
    main_markers = ('arguments[0] == "version"', 'case "check":', 'case "run":')
    if any(marker not in main for marker in main_markers):
        raise RuntimeError("Orange Data Plane executable command surface differs")
    production = "\n".join(sources.values())
    forbidden = (
        "include.Context(",
        "experimental/clashapi",
        "experimental/v2rayapi",
        "http.ListenAndServe(",
        "net.Listen(",
        "exec.Command(",
    )
    if any(marker in production for marker in forbidden):
        raise RuntimeError("Orange Data Plane host contains a forbidden capability")
    tests = "\n".join(
        path.read_text(encoding="utf-8")
        for path in (root / "native" / "dataplane").glob("*_test.go")
    )
    if tests.count("func Test") < 11:
        raise RuntimeError("Orange Data Plane host coverage dropped below eleven tests")


def build_environment() -> dict[str, str]:
    environment = os.environ.copy()
    environment.update(
        {
            "GOOS": "windows",
            "GOARCH": "amd64",
            "CGO_ENABLED": "0",
            "GOWORK": "off",
        }
    )
    return environment


def build_artifact(policy: dict[str, object], output: Path) -> None:
    version = str(policy["version"])
    tags = ",".join(str(tag) for tag in policy["build_tags"])
    ldflags = " ".join(
        (
            f"-X main.version={version}",
            f"-X github.com/sagernet/sing-box/constant.Version={version}",
            "-X internal/godebug.defaultGODEBUG=multipathtcp=0",
            "-checklinkname=0",
            "-s",
            "-w",
            "-buildid=",
        )
    )
    output.parent.mkdir(parents=True, exist_ok=True)
    run_checked(
        [
            "go",
            "build",
            "-mod=readonly",
            "-trimpath",
            "-buildvcs=false",
            "-tags",
            tags,
            "-ldflags",
            ldflags,
            "-o",
            str(output),
            str(policy["go_package"]),
        ],
        cwd=MODULE_DIR,
        environment=build_environment(),
    )


def sha256_path(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def verify_binary_metadata(artifact: Path, policy: dict[str, object]) -> dict[str, object]:
    metadata = run_checked(["go", "version", "-m", str(artifact)])
    module = str(policy["go_module"])
    version = str(policy["version"])
    package = str(policy["go_package"])
    expected_lines = (
        f"\tpath\t{package}",
        "\tmod\torange.dev/native/dataplane\t(devel)\t",
        f"\tdep\t{module}\tv{version}\t",
        "\tbuild\t-tags=with_quic",
        "\tbuild\tCGO_ENABLED=0",
        "\tbuild\tGOARCH=amd64",
        "\tbuild\tGOOS=windows",
    )
    for expected in expected_lines:
        if expected not in metadata:
            raise RuntimeError(f"Windows Data Plane binary metadata is missing {expected.strip()}")
    included_forbidden = [name for name in FORBIDDEN_BINARY_MODULES if f"\tdep\t{name}\t" in metadata]
    if included_forbidden:
        raise RuntimeError(f"forbidden modules compiled into Data Plane: {included_forbidden}")
    dependency_count = sum(1 for line in metadata.splitlines() if line.startswith("\tdep\t"))
    return {"dependency_count": dependency_count, "forbidden_modules": included_forbidden}


def version_output(artifact: Path) -> str:
    return run_checked([str(artifact), "version"], timeout=15.0)


def verify_version_output(output: str, policy: dict[str, object]) -> None:
    version = re.search(r"^sing-box version ([^\s]+)$", output, re.MULTILINE)
    environment = re.search(r"^Environment: \S+ ([^\s]+)$", output, re.MULTILINE)
    tags = re.search(r"^Tags: (.+)$", output, re.MULTILINE)
    cgo = re.search(r"^CGO: (.+)$", output, re.MULTILINE)
    if version is None or version.group(1) != policy["version"]:
        raise RuntimeError("Data Plane version handshake failed")
    if environment is None or environment.group(1) != "windows/amd64":
        raise RuntimeError("Data Plane platform handshake failed")
    if tags is None or tags.group(1).split(",") != policy["build_tags"]:
        raise RuntimeError("Data Plane feature-tag handshake failed")
    if cgo is None or cgo.group(1) != "disabled":
        raise RuntimeError("Data Plane CGO handshake failed")


def authenticode_info(artifact: Path) -> SignatureInfo:
    powershell = shutil.which("powershell") or shutil.which("pwsh")
    if powershell is None:
        raise RuntimeError("PowerShell is required for Authenticode verification")
    environment = os.environ.copy()
    environment["ORANGE_DATA_PLANE_ARTIFACT"] = str(artifact)
    script = (
        "$s = Get-AuthenticodeSignature -LiteralPath $env:ORANGE_DATA_PLANE_ARTIFACT; "
        "$t = if ($null -eq $s.SignerCertificate) { '' } else { $s.SignerCertificate.Thumbprint }; "
        "$n = if ($null -eq $s.SignerCertificate) { '' } else { $s.SignerCertificate.Subject }; "
        "[ordered]@{status=[string]$s.Status;thumbprint=$t;subject=$n} | ConvertTo-Json -Compress"
    )
    result = subprocess.run(
        [powershell, "-NoLogo", "-NoProfile", "-NonInteractive", "-Command", script],
        env=environment,
        check=False,
        capture_output=True,
        text=True,
        encoding="utf-8",
        errors="replace",
        timeout=30,
    )
    if result.returncode != 0:
        raise RuntimeError(f"Authenticode inspection failed: {result.stderr.strip()}")
    value = json.loads(result.stdout)
    return SignatureInfo(
        str(value.get("status", "")),
        str(value.get("thumbprint", "")).replace(" ", "").upper(),
        str(value.get("subject", "")),
    )


def signature_classification(
    signature: SignatureInfo, policy: dict[str, object], *, release_requested: bool
) -> tuple[str, bool]:
    release = policy["release"]
    assert isinstance(release, dict)
    allowed = release["allowed_signer_sha1_thumbprints"]
    assert isinstance(allowed, list)
    if signature.status == "NotSigned":
        classification = "unsigned-debug"
        release_allowed = False
    elif signature.status == "Valid":
        release_allowed = bool(signature.thumbprint and signature.thumbprint in allowed)
        classification = (
            "verified-release-signature" if release_allowed else "debug-signature-untrusted"
        )
    else:
        raise RuntimeError(f"Data Plane Authenticode status is invalid: {signature.status}")
    if release_requested and not release_allowed:
        raise RuntimeError("Data Plane artifact is not signed by an approved release certificate")
    return classification, release_allowed


def verify_handshake(
    artifact: Path,
    expected_sha256: str,
    output: str,
    signature: SignatureInfo,
    policy: dict[str, object],
    *,
    release_requested: bool,
) -> tuple[str, bool]:
    classification = verify_file_handshake(
        artifact,
        expected_sha256,
        signature,
        policy,
        release_requested=release_requested,
    )
    verify_version_output(output, policy)
    return classification


def verify_file_handshake(
    artifact: Path,
    expected_sha256: str,
    signature: SignatureInfo,
    policy: dict[str, object],
    *,
    release_requested: bool,
) -> tuple[str, bool]:
    if re.fullmatch(r"[0-9a-f]{64}", expected_sha256) is None:
        raise RuntimeError("expected Data Plane SHA-256 is malformed")
    if not hmac.compare_digest(sha256_path(artifact), expected_sha256):
        raise RuntimeError("Data Plane SHA-256 handshake failed")
    return signature_classification(signature, policy, release_requested=release_requested)


class SmokeHandler(http.server.BaseHTTPRequestHandler):
    body = b"orange-win-g0-001-loopback"

    def do_GET(self) -> None:  # noqa: N802
        if self.path != "/orange-win-g0-001":
            self.send_error(404)
            return
        self.send_response(200)
        self.send_header("Content-Type", "text/plain")
        self.send_header("Content-Length", str(len(self.body)))
        self.end_headers()
        self.wfile.write(self.body)

    def log_message(self, _format: str, *args: object) -> None:
        del args


def receive_all(connection: socket.socket) -> bytes:
    chunks: list[bytes] = []
    while True:
        chunk = connection.recv(4096)
        if not chunk:
            return b"".join(chunks)
        chunks.append(chunk)


def assert_http_response(response: bytes) -> None:
    if b" 200 OK\r\n" not in response or not response.endswith(SmokeHandler.body):
        raise RuntimeError("mixed proxy returned an unexpected loopback HTTP response")


def http_proxy_smoke(proxy_port: int, target_port: int) -> None:
    with socket.create_connection(("127.0.0.1", proxy_port), timeout=3) as connection:
        request = (
            f"GET http://127.0.0.1:{target_port}/orange-win-g0-001 HTTP/1.1\r\n"
            f"Host: 127.0.0.1:{target_port}\r\nConnection: close\r\n\r\n"
        ).encode("ascii")
        connection.sendall(request)
        assert_http_response(receive_all(connection))


def socks_proxy_smoke(proxy_port: int, target_port: int) -> None:
    with socket.create_connection(("127.0.0.1", proxy_port), timeout=3) as connection:
        connection.sendall(b"\x05\x01\x00")
        if connection.recv(2) != b"\x05\x00":
            raise RuntimeError("mixed proxy rejected SOCKS5 no-auth negotiation")
        connection.sendall(
            b"\x05\x01\x00\x01" + socket.inet_aton("127.0.0.1") + struct.pack("!H", target_port)
        )
        response = connection.recv(10)
        if len(response) < 2 or response[:2] != b"\x05\x00":
            raise RuntimeError("mixed proxy rejected the SOCKS5 loopback connection")
        connection.sendall(
            f"GET /orange-win-g0-001 HTTP/1.1\r\nHost: 127.0.0.1:{target_port}\r\n"
            "Connection: close\r\n\r\n".encode("ascii")
        )
        assert_http_response(receive_all(connection))


def unused_loopback_port() -> int:
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as listener:
        listener.bind(("127.0.0.1", 0))
        return int(listener.getsockname()[1])


def write_control_frame(stream: object, value: dict[str, object]) -> None:
    payload = json.dumps(value, separators=(",", ":")).encode("utf-8")
    if not payload or len(payload) > 4096:
        raise RuntimeError("Data Plane control request exceeds the fixed frame bound")
    stream.write(struct.pack("!I", len(payload)) + payload)
    stream.flush()


def read_control_frame(stream: object) -> dict[str, object]:
    header = stream.read(4)
    if len(header) != 4:
        raise RuntimeError("Data Plane control response ended before its header")
    size = struct.unpack("!I", header)[0]
    if size == 0 or size > 4096:
        raise RuntimeError("Data Plane control response exceeds the fixed frame bound")
    payload = stream.read(size)
    if len(payload) != size:
        raise RuntimeError("Data Plane control response ended before its payload")
    value = json.loads(payload)
    if not isinstance(value, dict):
        raise RuntimeError("Data Plane control response is not an object")
    return value


def expect_control_response(stream: object, request_id: int) -> dict[str, object]:
    value = read_control_frame(stream)
    if value.get("version") != 1 or value.get("kind") != "response" or value.get("id") != request_id:
        raise RuntimeError("Data Plane control response correlation failed")
    return value


def wait_for_listener(process: subprocess.Popen[bytes], port: int) -> None:
    deadline = time.monotonic() + 8
    while time.monotonic() < deadline:
        if process.poll() is not None:
            stdout, stderr = process.communicate()
            raise RuntimeError(
                "sing-box exited before mixed readiness: "
                + (stdout + stderr).decode("utf-8", errors="replace").strip()
            )
        try:
            with socket.create_connection(("127.0.0.1", port), timeout=0.2):
                return
        except OSError:
            time.sleep(0.05)
    raise RuntimeError("sing-box mixed listener did not become ready")


def mixed_smoke(artifact: Path) -> dict[str, object]:
    server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), SmokeHandler)
    server_thread = threading.Thread(target=server.serve_forever, daemon=True)
    server_thread.start()
    proxy_port = unused_loopback_port()
    config = {
        "log": {"disabled": True},
        "inbounds": [
            {
                "type": "mixed",
                "tag": "orange-mixed",
                "listen": "127.0.0.1",
                "listen_port": proxy_port,
            }
        ],
        "outbounds": [
            {"type": "direct", "tag": "node-a"},
            {"type": "direct", "tag": "node-b"},
            {
                "type": "selector",
                "tag": "proxy",
                "outbounds": ["node-a", "node-b"],
                "default": "node-a",
            },
        ],
        "route": {"final": "proxy", "auto_detect_interface": False},
    }
    artifacts_root = ROOT / "artifacts"
    artifacts_root.mkdir(parents=True, exist_ok=True)
    process: subprocess.Popen[bytes] | None = None
    forced = False
    with tempfile.TemporaryDirectory(prefix="orange-win-g0-001-", dir=artifacts_root) as temporary:
        config_path = Path(temporary) / "mixed.json"
        config_path.write_text(json.dumps(config), encoding="utf-8")
        environment = os.environ.copy()
        for name in ("HTTP_PROXY", "HTTPS_PROXY", "ALL_PROXY", "http_proxy", "https_proxy", "all_proxy"):
            environment.pop(name, None)
        environment["NO_PROXY"] = "*"
        environment["no_proxy"] = "*"
        try:
            process = subprocess.Popen(
                [str(artifact), "run", "-c", str(config_path)],
                cwd=temporary,
                env=environment,
                stdin=subprocess.PIPE,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
            )
            if process.stdin is None or process.stdout is None:
                raise RuntimeError("Data Plane control pipes were not created")
            ready = read_control_frame(process.stdout)
            if ready != {"version": 1, "kind": "ready"}:
                raise RuntimeError("Data Plane control readiness handshake failed")
            wait_for_listener(process, proxy_port)
            target_port = int(server.server_address[1])
            http_proxy_smoke(proxy_port, target_port)
            socks_proxy_smoke(proxy_port, target_port)
            write_control_frame(
                process.stdin,
                {
                    "version": 1,
                    "kind": "request",
                    "id": 1,
                    "command": "select_node",
                    "selectorId": "proxy",
                    "nodeId": "node-b",
                },
            )
            if expect_control_response(process.stdout, 1).get("result") != "ok":
                raise RuntimeError("Data Plane selector update failed")
            write_control_frame(
                process.stdin,
                {
                    "version": 1,
                    "kind": "request",
                    "id": 2,
                    "command": "read_selected_node",
                    "selectorId": "proxy",
                },
            )
            selected = expect_control_response(process.stdout, 2)
            if selected.get("result") != "ok" or selected.get("selectedNodeId") != "node-b":
                raise RuntimeError("Data Plane selector readback failed")
            write_control_frame(
                process.stdin,
                {"version": 1, "kind": "request", "id": 3, "command": "traffic"},
            )
            traffic = expect_control_response(process.stdout, 3)
            if (
                traffic.get("result") != "ok"
                or not isinstance(traffic.get("uploadBytesTotal"), int)
                or not isinstance(traffic.get("downloadBytesTotal"), int)
                or traffic["uploadBytesTotal"] <= 0
                or traffic["downloadBytesTotal"] <= 0
            ):
                raise RuntimeError("Data Plane traffic counters were not authoritative")
            if process.poll() is not None:
                raise RuntimeError("Orange Data Plane exited during mixed smoke")
        finally:
            if process is not None and process.poll() is None:
                if process.stdin is not None:
                    process.stdin.close()
                try:
                    process.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    forced = True
                    process.terminate()
                    try:
                        process.wait(timeout=5)
                    except subprocess.TimeoutExpired:
                        process.kill()
                        process.wait(timeout=5)
            server.shutdown()
            server.server_close()
            server_thread.join(timeout=5)
    if process is None or process.poll() is None:
        raise RuntimeError("mixed smoke left a residual sing-box process")
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as listener:
        listener.bind(("127.0.0.1", proxy_port))
    return {
        "offline": True,
        "target": "loopback-only",
        "http_proxy": "passed",
        "socks5_proxy": "passed",
        "process_reaped": True,
        "listener_released": True,
        "forced_cleanup": forced,
    }


def write_manifest(
    root: Path,
    output: Path,
    artifact: Path,
    digest: str,
    signature_class: str,
    release_allowed: bool,
    policy: dict[str, object],
) -> None:
    import tomllib

    toolchains = tomllib.loads((root / "toolchains.toml").read_text(encoding="utf-8"))
    manifest = {
        "schema_version": 1,
        "artifacts": [
            {
                "id": "windows-data-plane:windows-amd64:orange-data-plane.exe",
                "path": artifact.relative_to(root).as_posix(),
                "sha256": digest,
                "kind": "windows-data-plane-sidecar",
                "source": "native/dataplane/build-policy.json",
                "version": str(policy["version"]),
                "license": str(toolchains["sing_box"]["license"]),
                "platform": "windows-amd64",
                "signature": signature_class,
                "release_allowed": release_allowed,
            }
        ],
    }
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8")
    errors = validate_artifact_manifest(root, output)
    if errors:
        raise RuntimeError("; ".join(errors))


def main() -> int:
    parser = argparse.ArgumentParser(description="Audit the Windows sing-box Data Plane sidecar")
    parser.add_argument("--verify-existing", action="store_true")
    parser.add_argument("--release", action="store_true")
    parser.add_argument("--manifest", type=Path, default=DEFAULT_MANIFEST)
    parser.add_argument("--report", type=Path, default=DEFAULT_REPORT)
    args = parser.parse_args()
    try:
        if platform.system() != "Windows":
            raise RuntimeError("Windows Data Plane runtime audit must run on Windows")
        policy = read_json(POLICY_PATH)
        validate_policy(ROOT, policy)
        artifact_value = normalized_relative_path(policy["artifact_path"])
        assert artifact_value is not None
        artifact = ROOT / Path(artifact_value)
        reproducible_digest: str | None = None
        if args.release and not args.verify_existing:
            raise RuntimeError("release verification must not overwrite the signed artifact")
        if args.verify_existing:
            if not artifact.is_file():
                raise RuntimeError(f"Windows Data Plane artifact is missing: {artifact_value}")
        else:
            build_artifact(policy, artifact)
            with tempfile.TemporaryDirectory(prefix="orange-data-plane-rebuild-") as temporary:
                rebuilt = Path(temporary) / "orange-data-plane.exe"
                build_artifact(policy, rebuilt)
                reproducible_digest = sha256_path(rebuilt)
            if sha256_path(artifact) != reproducible_digest:
                raise RuntimeError("locked Windows Data Plane builds are not reproducible")
        digest = sha256_path(artifact)
        binary = verify_binary_metadata(artifact, policy)
        signature = authenticode_info(artifact)
        # Hash and trust are checked before any untrusted PE code is executed.
        signature_class, release_allowed = verify_file_handshake(
            artifact,
            digest,
            signature,
            policy,
            release_requested=args.release,
        )
        output = version_output(artifact)
        verify_version_output(output, policy)
        if not hmac.compare_digest(sha256_path(artifact), digest):
            raise RuntimeError("Data Plane artifact changed during version handshake")
        smoke = mixed_smoke(artifact)
        manifest = args.manifest if args.manifest.is_absolute() else ROOT / args.manifest
        report = args.report if args.report.is_absolute() else ROOT / args.report
        write_manifest(
            ROOT,
            manifest,
            artifact,
            digest,
            signature_class,
            release_allowed,
            policy,
        )
        result = {
            "schema_version": 1,
            "passed": True,
            "hosting_model": policy["hosting_model"],
            "artifact": artifact_value,
            "artifact_bytes": artifact.stat().st_size,
            "artifact_sha256": digest,
            "reproducible_sha256": reproducible_digest,
            "go_module": policy["go_module"],
            "go_version": run_checked(["go", "version"]).strip().split()[2].removeprefix("go"),
            "version": policy["version"],
            "build_tags": policy["build_tags"],
            "binary": binary,
            "authenticode": {
                "status": signature.status,
                "signer_thumbprint": signature.thumbprint,
                "signer_subject": signature.subject,
                "classification": signature_class,
            },
            "release_allowed": release_allowed,
            "runtime_download_allowed": False,
            "control_protocol": policy["control_protocol"],
            "registered_capabilities": policy["registered_capabilities"],
            "managed_host_tests": sum(
                path.read_text(encoding="utf-8").count("func Test")
                for path in MODULE_DIR.glob("*_test.go")
            ),
            "mixed_smoke": smoke,
            "artifact_manifest": manifest.relative_to(ROOT).as_posix(),
        }
        report.parent.mkdir(parents=True, exist_ok=True)
        report.write_text(json.dumps(result, indent=2) + "\n", encoding="utf-8")
    except (json.JSONDecodeError, OSError, RuntimeError, subprocess.SubprocessError) as error:
        print(f"ERROR: {error}")
        return 1
    print(
        "Windows Data Plane core passed: "
        f"sing-box {result['version']}, {result['artifact_sha256']}, "
        f"signature={result['authenticode']['status']}, release_allowed={result['release_allowed']}"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
