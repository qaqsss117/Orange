from __future__ import annotations

import argparse
import base64
import datetime as dt
import hashlib
import json
import os
import secrets
import shutil
import string
import subprocess
import tempfile
from pathlib import Path

from cryptography import x509
from cryptography.hazmat.primitives import hashes, serialization
from cryptography.hazmat.primitives.asymmetric import ed25519


ROOT = Path(__file__).resolve().parents[2]
PRIVATE_ROOT = ROOT / "artifacts" / "private"
PASSWORD_ALPHABET = string.ascii_letters + string.digits
GENERATED_VALIDITY_DAYS = 180


def random_password(length: int = 40) -> str:
    while True:
        value = "".join(secrets.choice(PASSWORD_ALPHABET) for _ in range(length))
        if (
            any(character.islower() for character in value)
            and any(character.isupper() for character in value)
            and any(character.isdigit() for character in value)
        ):
            return value


def base64url_no_padding(value: bytes) -> str:
    return base64.urlsafe_b64encode(value).rstrip(b"=").decode("ascii")


def executable(name: str) -> str:
    candidates = [f"{name}.cmd", f"{name}.exe", name] if os.name == "nt" else [name]
    for candidate in candidates:
        resolved = shutil.which(candidate)
        if resolved:
            return resolved
    raise RuntimeError(f"required executable is unavailable: {name}")


def run_checked(command: list[str], *, environment: dict[str, str] | None = None) -> None:
    result = subprocess.run(
        command,
        cwd=ROOT,
        env=environment,
        stdin=subprocess.DEVNULL,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
        encoding="utf-8",
        errors="replace",
        check=False,
    )
    if result.returncode != 0:
        detail = (result.stderr or result.stdout).strip()
        raise RuntimeError(f"command failed: {Path(command[0]).name}: {detail}")


def write_private_bytes(path: Path, contents: bytes) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open("xb") as output:
        output.write(contents)
    try:
        path.chmod(0o600)
    except OSError:
        pass


def write_private_text(path: Path, contents: str) -> None:
    write_private_bytes(path, contents.encode("utf-8"))


def generate_ed25519_key() -> tuple[str, str]:
    private_key = ed25519.Ed25519PrivateKey.generate()
    seed = private_key.private_bytes(
        serialization.Encoding.Raw,
        serialization.PrivateFormat.Raw,
        serialization.NoEncryption(),
    )
    public_key = private_key.public_key().public_bytes(
        serialization.Encoding.Raw,
        serialization.PublicFormat.Raw,
    )
    return seed.hex(), base64url_no_padding(public_key)


def generate_tauri_key(output_directory: Path, password: str) -> tuple[Path, Path, str, str]:
    private_path = output_directory / "tauri-signing.key"
    public_path = Path(f"{private_path}.pub")
    environment = os.environ.copy()
    environment["CI"] = "true"
    run_checked(
        [
            executable("pnpm"),
            "tauri",
            "signer",
            "generate",
            "--password",
            password,
            "--write-keys",
            str(private_path),
            "--ci",
        ],
        environment=environment,
    )
    if not private_path.is_file() or not public_path.is_file():
        raise RuntimeError("Tauri signer did not create both private and public key files")
    try:
        private_path.chmod(0o600)
    except OSError:
        pass

    private_value = private_path.read_text(encoding="utf-8").strip()
    public_value = public_path.read_text(encoding="utf-8").strip()
    if not private_value or not public_value:
        raise RuntimeError("Tauri signer created an empty key")

    with tempfile.TemporaryDirectory(prefix="orange-tauri-key-check-") as directory:
        verification_file = Path(directory) / "verification.txt"
        verification_file.write_text("Orange Tauri signing key verification\n", encoding="utf-8")
        verification_environment = environment.copy()
        verification_environment["TAURI_SIGNING_PRIVATE_KEY_PATH"] = str(private_path)
        verification_environment["TAURI_SIGNING_PRIVATE_KEY_PASSWORD"] = password
        run_checked(
            [executable("pnpm"), "tauri", "signer", "sign", str(verification_file)],
            environment=verification_environment,
        )
    return private_path, public_path, private_value, public_value


def generate_android_keystore(
    output_directory: Path,
    alias: str,
    store_password: str,
    key_password: str,
) -> tuple[Path, str, str]:
    keystore_path = output_directory / "android-release.jks"
    certificate_path = output_directory / "android-release.cer"
    environment = os.environ.copy()
    environment["ORANGE_GENERATED_STORE_PASSWORD"] = store_password
    environment["ORANGE_GENERATED_KEY_PASSWORD"] = key_password
    common_arguments = [
        "-keystore",
        str(keystore_path),
        "-storetype",
        "JKS",
        "-storepass:env",
        "ORANGE_GENERATED_STORE_PASSWORD",
    ]
    run_checked(
        [
            executable("keytool"),
            "-genkeypair",
            *common_arguments,
            "-alias",
            alias,
            "-keypass:env",
            "ORANGE_GENERATED_KEY_PASSWORD",
            "-keyalg",
            "RSA",
            "-keysize",
            "4096",
            "-sigalg",
            "SHA256withRSA",
            "-validity",
            "10000",
            "-dname",
            "CN=Orange VPN Android Release, OU=Release, O=Orange VPN, L=Shanghai, ST=Shanghai, C=CN",
            "-noprompt",
        ],
        environment=environment,
    )
    run_checked(
        [
            executable("keytool"),
            "-exportcert",
            *common_arguments,
            "-alias",
            alias,
            "-file",
            str(certificate_path),
        ],
        environment=environment,
    )
    try:
        keystore_path.chmod(0o600)
    except OSError:
        pass
    certificate = x509.load_der_x509_certificate(certificate_path.read_bytes())
    fingerprint = certificate.fingerprint(hashes.SHA256()).hex()
    encoded_keystore = base64.b64encode(keystore_path.read_bytes()).decode("ascii")
    return keystore_path, encoded_keystore, fingerprint


def fenced(value: str, language: str = "text") -> str:
    return f"```{language}\n{value}\n```"


def variable_section(name: str, value: str, note: str) -> str:
    return f"### `{name}`\n\n{note}\n\n{fenced(value)}\n"


def bootstrap_template(configuration_version: int, expires_at_unix: int) -> dict[str, object]:
    return {
        "schemaVersion": 2,
        "configurationVersion": configuration_version,
        "expiresAtUnix": expires_at_unix,
        "candidates": [
            {
                "id": "proxy-1",
                "protocol": "trojan",
                "server": "<PROXY_1_HOST>",
                "port": 443,
                "credential": "<PROXY_1_PASSWORD>",
                "tlsServerName": "<PROXY_1_TLS_NAME>",
            },
            {
                "id": "proxy-2",
                "protocol": "trojan",
                "server": "<PROXY_2_HOST>",
                "port": 443,
                "credential": "<PROXY_2_PASSWORD>",
                "tlsServerName": "<PROXY_2_TLS_NAME>",
            },
        ],
        "failover": {
            "connectTimeoutMs": 3000,
            "requestTimeoutMs": 15000,
            "maxAttempts": 4,
            "backoffBaseMs": 300,
        },
        "startupDns": [
            {
                "protocol": "tls",
                "server": "1.1.1.1",
                "port": 853,
                "tlsServerName": "cloudflare-dns.com",
            },
            {
                "protocol": "tls",
                "server": "8.8.8.8",
                "port": 853,
                "tlsServerName": "dns.google",
            },
        ],
        "apiHosts": ["<API_1_HOST>", "<API_2_HOST>"],
    }


def build_report(
    *,
    generated_at: dt.datetime,
    output_directory: Path,
    values: dict[str, str],
    current_private_seed: str,
    next_private_seed: str,
    next_key_id: str,
    tauri_public_key: str,
    android_keystore_path: Path,
    apple_app_password: str,
    apple_installer_password: str,
    configuration_template: dict[str, object],
    expires_at: dt.datetime,
) -> str:
    sections = [
        "# Orange GitHub Actions Variables 新密钥材料",
        "",
        "> **极敏感文件：包含私钥、keystore 和全部密码。** 即使仓库是私有的，"
        "GitHub Actions Variables 也不会像 Secrets 一样加密存储或自动遮蔽日志。"
        "本文件位于 Git 忽略的 `artifacts/` 下，但仍是磁盘上的明文；录入 GitHub 后应离线加密备份并删除工作副本。",
        "",
        f"- 生成时间（UTC）：`{generated_at.isoformat()}`",
        f"- 自动到期时间（UTC）：`{expires_at.isoformat()}`",
        f"- 输出目录：`{output_directory}`",
        "- 用途：建立一套全新的发布信任周期；未执行兼容迁移前不要覆盖已有线上值。",
        "",
        "## 上线前必须理解的兼容限制",
        "",
        "1. **Android keystore 不可对已发布应用直接更换。** Google Play、现有 APK 安装和 Android 系统升级都要求签名证书连续；如果已有用户安装旧签名版本，请继续使用旧 keystore。",
        "2. **bootstrap 加密密钥不是可透明轮换的远端密钥。** 旧客户端只嵌入旧密钥；新旧客户端必须使用各自不可变的 OSS 版本目录和密文，不能直接覆盖共用对象。",
        "3. **Ed25519 必须按 current/next 桥接轮换。** 旧客户端只有在上一版已嵌入这里的新 current 公钥时才会信任它；否则只能通过应用商店/安装包发布新的信任集合，同时保留旧客户端资源。",
        "4. **Tauri 签名密钥更换会切断旧客户端更新。** 使用新私钥前必须让客户端配置信任对应的新公钥；Windows Store 渠道不依赖该应用内更新密钥，但其他桌面渠道可能依赖。",
        "5. **Windows MSIX 不生成本地 PFX。** Microsoft Store 在接收提交后负责包签名；Store 身份和 Partner Center 认证参数必须由外部系统提供。",
        "",
        "## 可直接创建的 GitHub Variables",
        "",
    ]

    generated_notes = {
        "ORANGE_BOOTSTRAP_BUILD_KEY_HEX": "新 XChaCha20-Poly1305 32 字节密钥。仅用于新客户端/新 OSS 版本目录。",
        "ORANGE_BOOTSTRAP_CHANNEL": "发布渠道。",
        "ORANGE_BOOTSTRAP_PRODUCT_VERSION": "从 `src-tauri/tauri.conf.json` 读取的当前应用版本。",
        "ORANGE_BOOTSTRAP_KEY_ID": "新 bootstrap 加密密钥 ID。",
        "ORANGE_BOOTSTRAP_SIGNING_KEY_HEX": "当前 Ed25519 私钥种子。",
        "ORANGE_BOOTSTRAP_SIGNING_KEY_ID": "当前 Ed25519 签名密钥 ID。",
        "ORANGE_BOOTSTRAP_SIGNING_PUBLIC_KEYS": "当前和下一把 Ed25519 公钥；客户端会嵌入这组信任锚。",
        "ORANGE_BOOTSTRAP_MINIMUM_CLIENT_VERSION": "允许读取本次远端配置的最低客户端版本。",
        "ORANGE_BOOTSTRAP_TXT_SEQUENCE": "以生成时 Unix 秒作为新的防回滚序号；部署前仍需确认大于线上已发布值。",
        "ORANGE_BOOTSTRAP_TXT_EXPIRES_AT_UNIX": "自动生成的 TXT 到期 Unix 秒。",
        "ORANGE_ANDROID_PACKAGE_ID": "按仓库发布文档使用固定生产包名；正式发布后不可更换。",
        "ORANGE_ANDROID_VERSION_CODE": "由当前 SemVer 推导的 Tauri Android versionCode；每次发布必须严格递增。",
        "ORANGE_ANDROID_VERSION_NAME": "Android 展示版本。",
        "ORANGE_ANDROID_SIGNING_CERT_SHA256": "由新 Android keystore 中证书计算得出。",
        "ORANGE_ANDROID_UPDATE_EXPIRES_AT_UNIX": "自动生成的 Android 更新 manifest/TXT 到期 Unix 秒。",
        "ORANGE_ANDROID_UPDATE_TXT_SEQUENCE": "以生成时 Unix 秒作为 Android 更新 TXT 防回滚序号。",
        "ANDROID_KEYSTORE_BASE64": "完整 Android JKS 的单行 Base64。",
        "ANDROID_KEYSTORE_PASSWORD": "新 Android JKS 存储密码。",
        "ANDROID_KEY_ALIAS": "新 Android 发布密钥别名。",
        "ANDROID_KEY_PASSWORD": "新 Android 私钥密码。",
        "TAURI_SIGNING_PRIVATE_KEY": "新 Tauri/minisign 私钥文件的完整内容。",
        "TAURI_SIGNING_PRIVATE_KEY_PASSWORD": "新 Tauri 私钥密码。",
        "ORANGE_WINDOWS_MSIX_VERSION": "MSIX 四段版本，默认由应用版本补齐 `.0`。",
        "ORANGE_WINDOWS_STORE_DISPLAY_NAME": "MSIX 展示名称。",
    }
    for name in generated_notes:
        sections.append(variable_section(name, values[name], generated_notes[name]))

    sections.extend(
        [
            "## 密钥轮换备用材料（不是当前 workflow 变量）",
            "",
            "### 下一把 Ed25519 私钥",
            "",
            f"密钥 ID：`{next_key_id}`。必须离线保存；未来轮换时把它设为 `ORANGE_BOOTSTRAP_SIGNING_KEY_HEX`，并同时把签名 key ID 改为该 ID。",
            "",
            fenced(next_private_seed),
            "",
            "### 当前 Ed25519 私钥备份",
            "",
            "与当前 workflow variable 相同，单独列出用于离线密钥台账核对。",
            "",
            fenced(current_private_seed),
            "",
            "### Tauri 公钥",
            "",
            "新 Tauri 私钥对应的公钥。使用这套密钥前，必须把应用配置中的 updater 公钥更新为此值并发布新客户端。",
            "",
            fenced(tauri_public_key),
            "",
            "## 需要外部系统提供或人工确认的 Variables",
            "",
            "以下内容无法在本机凭空生成。不要把示例域名或占位符直接录入生产 Variables。",
            "",
            "| Variable | 取得方式 |",
            "| --- | --- |",
            "| `ORANGE_BOOTSTRAP_CONFIG_JSON` | 用真实代理节点凭据和真实 API host 完成下方模板 |",
            "| `ORANGE_BOOTSTRAP_MANIFEST_URLS` | 2–4 个包内硬编码 OSS HTTPS 443 manifest URL |",
            "| `ORANGE_BOOTSTRAP_ENVELOPE_URLS` | 与上述 manifest 一一对应的密文 URL |",
            "| `ORANGE_BOOTSTRAP_TXT_NAMES` | 2–4 个 Cloudflare TXT 完整记录名 |",
            "| `ORANGE_BOOTSTRAP_TXT_MANIFEST_URLS` | 1–4 个与包内地址不同的 rescue manifest URL |",
            "| `ORANGE_BOOTSTRAP_TXT_ENVELOPE_URLS` | 与 rescue manifest 一一对应的密文 URL |",
            "| `ORANGE_WINDOWS_STORE_PRODUCT_ID` | Microsoft Partner Center 产品 ID |",
            "| `ORANGE_WINDOWS_STORE_IDENTITY_NAME` | Partner Center package identity name |",
            "| `ORANGE_WINDOWS_STORE_PUBLISHER` | Partner Center publisher subject |",
            "| `ORANGE_WINDOWS_STORE_TENANT_ID` | Microsoft Entra tenant ID |",
            "| `ORANGE_WINDOWS_STORE_SELLER_ID` | Partner Center seller ID |",
            "| `ORANGE_WINDOWS_STORE_CLIENT_ID` | Microsoft Entra application/client ID |",
            "| `ORANGE_WINDOWS_STORE_CLIENT_SECRET` | Microsoft Entra client secret |",
            "| `ORANGE_ANDROID_UPDATE_MANIFEST_URLS` | 2–4 个 Android 更新 manifest URL |",
            "| `ORANGE_ANDROID_UPDATE_TXT_NAMES` | 2–4 个 Android 更新 TXT 记录名 |",
            "| `ORANGE_ANDROID_UPDATE_TXT_MANIFEST_URLS` | 1–4 个不同于包内地址的更新 rescue URL |",
            "| `ORANGE_ANDROID_APK_MIRROR_URLS` | 2–4 个提供同一签名 APK 的镜像 URL |",
            "| `APPLE_DEVELOPMENT_TEAM` | Apple Developer Team ID |",
            "| `APPLE_API_ISSUER` | App Store Connect API Issuer ID |",
            "| `APPLE_API_KEY` | App Store Connect API Key ID |",
            "| `APPLE_API_PRIVATE_KEY` | 只能在 App Store Connect 创建并下载一次的 `.p8` 私钥 |",
            "| `MACOS_APP_CERTIFICATE` | Apple 签发的 Developer ID Application P12，再做 Base64 |",
            "| `MACOS_APP_CERTIFICATE_PASSWORD` | 导出上述 P12 时实际设置的密码 |",
            "| `MACOS_INSTALLER_CERTIFICATE` | Apple 签发的 Developer ID Installer P12，再做 Base64 |",
            "| `MACOS_INSTALLER_CERTIFICATE_PASSWORD` | 导出上述 P12 时实际设置的密码 |",
            "",
            "### `ORANGE_BOOTSTRAP_CONFIG_JSON` 待填写模板",
            "",
            "所有 `<...>` 都必须替换；模板故意使用非法 host 占位符，以防未经填写就通过生产构建。`configurationVersion` 和 `expiresAtUnix` 已生成，但部署前仍需确认严格高于/晚于线上配置。",
            "",
            fenced(json.dumps(configuration_template, ensure_ascii=False, indent=2), "json"),
            "",
            "### Apple P12 导出密码建议",
            "",
            "这些密码尚未绑定任何证书。以后从 Keychain 导出对应 P12 时可以采用，然后把实际 P12 Base64 和密码一起录入 Variables。",
            "",
            variable_section(
                "MACOS_APP_CERTIFICATE_PASSWORD（建议值）",
                apple_app_password,
                "仅在你用该密码导出 Developer ID Application P12 后才有效。",
            ),
            variable_section(
                "MACOS_INSTALLER_CERTIFICATE_PASSWORD（建议值）",
                apple_installer_password,
                "仅在你用该密码导出 Developer ID Installer P12 后才有效。",
            ),
            "## 生成的辅助文件",
            "",
            f"- Android keystore：`{android_keystore_path}`",
            "- Tauri 私钥/公钥位于同一输出目录。",
            "",
            "## 建议录入顺序",
            "",
            "1. 先保存 Android、bootstrap、Tauri 的旧线上材料；不要直接删除。",
            "2. 补齐 OSS/TXT、代理/API、Apple 与 Store 外部值。",
            "3. 在 staging Variables 中验证完整 `workflow_dispatch`。",
            "4. 按上述兼容限制决定是新应用首次发布，还是分版本/分端点渐进轮换。",
            "5. 确认发布物可验证后再更新生产 Variables；随后离线加密备份本目录并删除工作副本。",
            "",
        ]
    )
    return "\n".join(sections)


def semver_android_version_code(version: str) -> int:
    components = version.split("-", 1)[0].split(".")
    if len(components) != 3 or any(not component.isdigit() for component in components):
        raise RuntimeError(f"cannot derive Android versionCode from version {version!r}")
    major, minor, patch = (int(component) for component in components)
    if major > 2100 or minor > 999 or patch > 999:
        raise RuntimeError("application SemVer cannot be represented as Android versionCode")
    value = major * 1_000_000 + minor * 1_000 + patch
    return max(value, 1)


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Generate a fresh Orange release trust epoch and a private Markdown handoff."
    )
    parser.add_argument(
        "--output-root",
        type=Path,
        default=PRIVATE_ROOT,
        help="parent directory for the unique private output directory",
    )
    arguments = parser.parse_args()

    generated_at = dt.datetime.now(dt.timezone.utc).replace(microsecond=0)
    timestamp = generated_at.strftime("%Y%m%dT%H%M%SZ")
    suffix = secrets.token_hex(3)
    output_directory = arguments.output_root.resolve() / f"release-variables-{timestamp}-{suffix}"
    output_directory.mkdir(parents=True, exist_ok=False)
    try:
        output_directory.chmod(0o700)
    except OSError:
        pass

    application_config = json.loads((ROOT / "src-tauri/tauri.conf.json").read_text(encoding="utf-8"))
    application_version = str(application_config["version"])
    sequence = int(generated_at.timestamp())
    expires_at = generated_at + dt.timedelta(days=GENERATED_VALIDITY_DAYS)
    expires_at_unix = int(expires_at.timestamp())
    key_suffix = f"{generated_at:%Y%m%d}-{suffix}"
    encryption_key_id = f"bootstrap-enc-{key_suffix}"
    current_key_id = f"bootstrap-sign-current-{key_suffix}"
    next_key_id = f"bootstrap-sign-next-{key_suffix}"

    current_private_seed, current_public_key = generate_ed25519_key()
    next_private_seed, next_public_key = generate_ed25519_key()
    bootstrap_build_key = secrets.token_hex(32)

    tauri_password = random_password()
    _, _, tauri_private_key, tauri_public_key = generate_tauri_key(
        output_directory,
        tauri_password,
    )

    android_store_password = random_password()
    android_key_password = random_password()
    android_alias = f"orange-release-{key_suffix}"
    android_keystore_path, android_keystore_base64, android_fingerprint = (
        generate_android_keystore(
            output_directory,
            android_alias,
            android_store_password,
            android_key_password,
        )
    )

    values = {
        "ORANGE_BOOTSTRAP_BUILD_KEY_HEX": bootstrap_build_key,
        "ORANGE_BOOTSTRAP_CHANNEL": "production",
        "ORANGE_BOOTSTRAP_PRODUCT_VERSION": application_version,
        "ORANGE_BOOTSTRAP_KEY_ID": encryption_key_id,
        "ORANGE_BOOTSTRAP_SIGNING_KEY_HEX": current_private_seed,
        "ORANGE_BOOTSTRAP_SIGNING_KEY_ID": current_key_id,
        "ORANGE_BOOTSTRAP_SIGNING_PUBLIC_KEYS": (
            f"{current_key_id}={current_public_key};{next_key_id}={next_public_key}"
        ),
        "ORANGE_BOOTSTRAP_MINIMUM_CLIENT_VERSION": application_version,
        "ORANGE_BOOTSTRAP_TXT_SEQUENCE": str(sequence),
        "ORANGE_BOOTSTRAP_TXT_EXPIRES_AT_UNIX": str(expires_at_unix),
        "ORANGE_ANDROID_PACKAGE_ID": "com.orangevpn.cn",
        "ORANGE_ANDROID_VERSION_CODE": str(semver_android_version_code(application_version)),
        "ORANGE_ANDROID_VERSION_NAME": application_version,
        "ORANGE_ANDROID_SIGNING_CERT_SHA256": android_fingerprint,
        "ORANGE_ANDROID_UPDATE_EXPIRES_AT_UNIX": str(expires_at_unix),
        "ORANGE_ANDROID_UPDATE_TXT_SEQUENCE": str(sequence),
        "ANDROID_KEYSTORE_BASE64": android_keystore_base64,
        "ANDROID_KEYSTORE_PASSWORD": android_store_password,
        "ANDROID_KEY_ALIAS": android_alias,
        "ANDROID_KEY_PASSWORD": android_key_password,
        "TAURI_SIGNING_PRIVATE_KEY": tauri_private_key,
        "TAURI_SIGNING_PRIVATE_KEY_PASSWORD": tauri_password,
        "ORANGE_WINDOWS_MSIX_VERSION": f"{application_version}.0",
        "ORANGE_WINDOWS_STORE_DISPLAY_NAME": "Orange VPN",
    }
    report = build_report(
        generated_at=generated_at,
        output_directory=output_directory,
        values=values,
        current_private_seed=current_private_seed,
        next_private_seed=next_private_seed,
        next_key_id=next_key_id,
        tauri_public_key=tauri_public_key,
        android_keystore_path=android_keystore_path,
        apple_app_password=random_password(),
        apple_installer_password=random_password(),
        configuration_template=bootstrap_template(sequence, expires_at_unix),
        expires_at=expires_at,
    )
    report_path = output_directory / "github-actions-variables.md"
    write_private_text(report_path, report)

    manifest = {
        "generatedAtUtc": generated_at.isoformat(),
        "report": report_path.name,
        "files": {
            path.name: hashlib.sha256(path.read_bytes()).hexdigest()
            for path in sorted(output_directory.iterdir())
            if path.is_file() and path != report_path
        },
    }
    manifest_path = output_directory / "checksums.json"
    write_private_text(
        manifest_path,
        json.dumps(manifest, ensure_ascii=False, indent=2) + "\n",
    )
    print(f"Generated private release material: {report_path}")
    print(f"Generated supporting files: {len(manifest['files'])}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
