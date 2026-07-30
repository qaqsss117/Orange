from __future__ import annotations

import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[3]
SCRIPT = ROOT / "scripts/acceptance/windows-development.ps1"


class WindowsDevelopmentAcceptanceTests(unittest.TestCase):
    def setUp(self) -> None:
        self.source = SCRIPT.read_text(encoding="utf-8")

    def test_phase_contract_is_complete(self) -> None:
        for phase in (
            "preflight",
            "build",
            "install",
            "ipc-boundary",
            "proxy",
            "tun",
            "crash",
            "upgrade-failure",
            "upgrade",
            "uninstall",
            "verify-clean",
        ):
            self.assertIn(f'"{phase}"', self.source)
        self.assertIn('schema_version = 1', self.source)
        self.assertIn('checkpoint.json', self.source)
        self.assertIn('result.json', self.source)

    def test_secrets_are_environment_only(self) -> None:
        parameter_block = self.source.split(")\n\n$ErrorActionPreference", 1)[0]
        for marker in ("Password", "Email", "BuildKey", "ConfigJson"):
            self.assertNotIn(f"[string]${marker}", parameter_block)
        for name in (
            "ORANGE_BOOTSTRAP_BUILD_KEY_HEX",
            "ORANGE_BOOTSTRAP_CONFIG_JSON",
            "ORANGE_E2E_EMAIL",
            "ORANGE_E2E_PASSWORD",
        ):
            self.assertIn(f'"{name}"', self.source)
        self.assertNotRegex(self.source, r"observations\s*=.*ORANGE_E2E_(EMAIL|PASSWORD)")

    def test_system_changes_require_explicit_guard(self) -> None:
        for function in (
            "Invoke-Install",
            "Invoke-IpcBoundaryAcceptance",
            "Invoke-CrashAcceptance",
            "Invoke-UpgradeFailureAcceptance",
            "Invoke-Upgrade",
            "Invoke-Uninstall",
        ):
            body = between(self.source, f"function {function} {{", "\n}\n")
            self.assertIn("Assert-SystemChangesAllowed", body)
        self.assertIn("-AllowSystemChanges", self.source)
        self.assertIn("Assert-Administrator", self.source)

    def test_release_boundary_cannot_be_weakened(self) -> None:
        self.assertIn('"unsigned-test-runtime"', self.source)
        self.assertIn('release_allowed = $false', self.source)
        self.assertIn('signature = "unsigned-test"', self.source)
        self.assertIn('Status -ne "NotSigned"', self.source)
        self.assertNotIn("release_allowed = $true", self.source)

    def test_ipc_boundary_uses_independent_restricted_processes(self) -> None:
        body = between(
            self.source,
            "function Invoke-IpcBoundaryAcceptance",
            "function Invoke-ProxyAcceptance",
        )
        for marker in (
            "Invoke-DifferentUserPipeProbe",
            "Invoke-LowIntegrityPipeProbe",
            'different_user_process = "independent-local-user"',
            'low_integrity_process = "low-mandatory-level"',
            "temporary_user_removed = $true",
        ):
            self.assertIn(marker, body)
        self.assertIn('ConvertStringSidToSid("S-1-16-4096"', self.source)
        self.assertIn("Remove-LocalUser -Name $userName", self.source)
        self.assertIn("[Environment]::MachineName", self.source)
        self.assertIn(
            "userName, domain, password, 0, application, command, CreateNoWindow",
            self.source,
        )
        self.assertIn("if ($arguments.Length -ge 1024)", self.source)
        self.assertNotRegex(body, r"user(Name|Sid)|password|credential")

    def test_upgrade_failure_package_and_rollback_are_required(self) -> None:
        build = between(self.source, "function Invoke-Build", "function Invoke-Install")
        self.assertIn("Orange_0.1.0_x64-upgrade-failure-setup.exe", build)
        self.assertIn("-InjectUpgradeFailure", build)
        body = between(
            self.source,
            "function Invoke-UpgradeFailureAcceptance",
            "function Invoke-Upgrade {",
        )
        for marker in (
            'injection_point = "post-payload-pre-service-install"',
            "upgrade failure rollback did not restore $name",
            "upgrade failure rollback replaced the installation identity",
            "upgrade failure rollback changed the active revision marker",
            "upgrade failure rollback changed the installed display version",
            "Repair-BaselineInstallation",
        ):
            self.assertIn(marker, body)

    def test_toolchain_versions_are_exact(self) -> None:
        for version in ("22\\.23\\.1", "11\\.9\\.0", "1\\.95\\.0", "1\\.25\\.5"):
            self.assertIn(version, self.source)
        self.assertIn("Assert-ToolVersion", self.source)

    def test_native_shell_escape_hatches_are_absent(self) -> None:
        lowered = self.source.lower()
        for marker in ("invoke-expression", "sc.exe", "netsh.exe", "cmd.exe /c"):
            self.assertNotIn(marker, lowered)
        self.assertIn("StartsWith($Script:InstallRoot", self.source)
        self.assertIn("[StringComparison]::OrdinalIgnoreCase", self.source)
        self.assertIn("Get-NormalizedWindowsPath", self.source)
        self.assertIn('$binaryPath.StartsWith("\\\\?\\"', self.source)
        self.assertIn("baseline worktree escaped the temporary directory", self.source)
        self.assertIn('"core.autocrlf=false"', self.source)
        self.assertIn('"core.longpaths=true"', self.source)
        self.assertIn("[IO.Directory]::Delete($longBaselineRoot, $true)", self.source)

    def test_clean_state_checks_recovery_values_not_empty_keys(self) -> None:
        self.assertIn('$recovery.PSObject.Properties["SystemProxyV1"]', self.source)
        self.assertNotIn(
            "recovery_present = (Test-Path -LiteralPath $Script:RecoveryRegistryPath)",
            self.source,
        )

    def test_every_phase_records_reproducible_redacted_context(self) -> None:
        report = between(self.source, "function Write-PhaseReport", "function Read-PhaseReport")
        for marker in (
            "context = Get-AcceptanceContext",
            "packages = Get-KnownPackageState",
            "postcondition = Get-SystemState",
            "observations = $Observations",
        ):
            self.assertIn(marker, report)
        for marker in (
            "git_revision = $revision",
            "os_build = [string]$os.BuildNumber",
            "go = Get-OptionalToolVersion",
            '"sdk\\go1.25.5\\bin"',
        ):
            self.assertIn(marker, self.source)

    def test_install_and_clean_postconditions_cover_native_state(self) -> None:
        installed = between(self.source, "function Assert-Installed", "function Assert-Clean")
        for marker in (
            "binary_under_install_root",
            "binary_matches_expected",
            "account_local_system",
            "service_sid_unrestricted",
            "identity_acl.protected",
            "runtime_acl.protected",
            "named_pipe_present",
        ):
            self.assertIn(marker, installed)
        clean = between(self.source, "function Assert-Clean", "function Write-MergedTauriConfig")
        for marker in (
            "Get-DnsState",
            "Get-RouteState",
            "tun_server_count",
            "tun_route_count",
            "tun_dns_absent",
            "tun_routes_absent",
        ):
            self.assertIn(marker, clean)

    def test_uninstall_exercises_retention_deletion_and_native_credentials(self) -> None:
        body = between(self.source, "function Invoke-Uninstall", "function Invoke-VerifyClean")
        for marker in (
            "Initialize-UninstallRetentionMarkers",
            "Assert-AppDataRetained",
            'ArgumentList "/S /DELETEAPPDATA"',
            "Assert-AppDataRemoved",
            'Invoke-ProductionSecretStoreProbe "complete"',
            'Invoke-ProductionSecretStoreProbe "empty"',
            "credentials_removed_when_settings_retained = $true",
            "candidate_reinstalled_between_choices = $true",
        ):
            self.assertIn(marker, body)
        self.assertIn('"com.orange.vpn.dev"', self.source)
        self.assertNotIn('Remove-Item -LiteralPath $Script:AppDataDirectory', self.source)

    def test_baseline_and_candidate_versions_reach_rust_and_bootstrap(self) -> None:
        build = between(self.source, "function Set-WorkspacePackageVersion", "function Invoke-Preflight")
        for marker in (
            '"Cargo.toml"',
            "[workspace\\.package\\]",
            "Set-WorkspacePackageVersion $Repository $Version",
            "$env:ORANGE_BOOTSTRAP_PRODUCT_VERSION = $Version",
            "$env:ORANGE_BOOTSTRAP_PRODUCT_VERSION = $previousProductVersion",
            '"unsigned-test-runtime"',
        ):
            self.assertIn(marker, build)

    def test_dirty_candidate_source_is_recorded_only_as_hashes(self) -> None:
        provenance = between(
            self.source, "function Get-GitSourceProvenance", "function Get-KnownPackageState"
        )
        for marker in (
            "core.autocrlf=false -C $Repository diff --binary --no-ext-diff HEAD",
            "ls-files --others --exclude-standard",
            "tracked_diff_sha256 = ConvertTo-Sha256",
            "untracked_paths_sha256 = ConvertTo-Sha256",
        ):
            self.assertIn(marker, provenance)
        build = between(self.source, "function Invoke-Build", "function Invoke-Install")
        self.assertIn("candidate_source = $candidateSource", build)


def between(source: str, start: str, end: str) -> str:
    start_index = source.index(start)
    end_index = source.index(end, start_index)
    return source[start_index:end_index]


if __name__ == "__main__":
    unittest.main()
