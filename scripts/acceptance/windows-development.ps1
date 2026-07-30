[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [ValidateSet("preflight", "build", "install", "ipc-boundary", "proxy", "tun", "crash", "upgrade-failure", "upgrade", "uninstall", "verify-clean")]
    [string]$Phase,

    [string]$BaselinePackage,
    [string]$CandidatePackage,
    [string]$FailurePackage,
    [string]$OutputDirectory = "artifacts/acceptance/windows-development",

    [ValidateSet("ui", "control-plane", "data-plane", "service")]
    [string]$CrashTarget = "ui",

    [switch]$AllowSystemChanges
)

$ErrorActionPreference = "Stop"
$ProgressPreference = "SilentlyContinue"

$Script:Root = (Resolve-Path (Join-Path $PSScriptRoot "..\..")).Path
$Script:PinnedGoBin = Join-Path ([Environment]::GetFolderPath("UserProfile")) "sdk\go1.25.5\bin"
if (Test-Path -LiteralPath (Join-Path $Script:PinnedGoBin "go.exe") -PathType Leaf) {
    $env:Path = $Script:PinnedGoBin + ";" + $env:Path
}
$Script:OutputRoot = if ([IO.Path]::IsPathRooted($OutputDirectory)) {
    [IO.Path]::GetFullPath($OutputDirectory)
} else {
    [IO.Path]::GetFullPath((Join-Path $Script:Root $OutputDirectory))
}
$Script:InstallRoot = "C:\Program Files\Orange"
$Script:ServiceName = "OrangeDataPlane"
$Script:FirewallRuleName = "Orange Data Plane TUN"
$Script:ProxyPort = 24836
$Script:ProxyRegistryPath = "HKCU:\Software\Microsoft\Windows\CurrentVersion\Internet Settings"
$Script:RecoveryRegistryPath = "HKCU:\Software\Orange\Recovery"
$Script:RunOnceRegistryPath = "HKCU:\Software\Microsoft\Windows\CurrentVersion\RunOnce"
$Script:AppDataDirectory = Join-Path ([Environment]::GetFolderPath("ApplicationData")) "com.orange.vpn.dev"
$Script:LocalAppDataDirectory = Join-Path ([Environment]::GetFolderPath("LocalApplicationData")) "com.orange.vpn.dev"
$Script:UninstallRetentionMarker = ".orange-uninstall-retention-acceptance.v1"
$Script:RequiredSecrets = @(
    "ORANGE_BOOTSTRAP_BUILD_KEY_HEX",
    "ORANGE_BOOTSTRAP_CONFIG_JSON",
    "ORANGE_BOOTSTRAP_CHANNEL",
    "ORANGE_BOOTSTRAP_PRODUCT_VERSION",
    "ORANGE_BOOTSTRAP_KEY_ID",
    "ORANGE_E2E_EMAIL",
    "ORANGE_E2E_PASSWORD"
)

function Assert-Windows {
    if ($env:OS -ne "Windows_NT") {
        throw "windows-development acceptance requires Windows"
    }
}

function Assert-Administrator {
    $identity = [Security.Principal.WindowsIdentity]::GetCurrent()
    $principal = New-Object Security.Principal.WindowsPrincipal($identity)
    if (-not $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
        throw "this phase requires an elevated administrator session"
    }
}

function Assert-SystemChangesAllowed {
    if (-not $AllowSystemChanges) {
        throw "this phase requires -AllowSystemChanges"
    }
    Assert-Administrator
}

function Assert-RequiredEnvironment {
    $missing = @($Script:RequiredSecrets | Where-Object {
        [string]::IsNullOrWhiteSpace([Environment]::GetEnvironmentVariable($_))
    })
    if ($missing.Count -ne 0) {
        throw "required acceptance environment is incomplete: $($missing -join ', ')"
    }
    if ($env:ORANGE_BOOTSTRAP_CHANNEL -ne "production") {
        throw "ORANGE_BOOTSTRAP_CHANNEL must be production"
    }
    if ($env:ORANGE_BOOTSTRAP_PRODUCT_VERSION -ne "0.1.0") {
        throw "ORANGE_BOOTSTRAP_PRODUCT_VERSION must be 0.1.0"
    }
}

function Assert-ExitProbeConfiguration {
    $value = [Environment]::GetEnvironmentVariable("ORANGE_E2E_IP_CHECK_URL")
    if ([string]::IsNullOrWhiteSpace($value)) {
        throw "ORANGE_E2E_IP_CHECK_URL is required for live network phases"
    }
    $uri = $null
    if (-not [Uri]::TryCreate($value, [UriKind]::Absolute, [ref]$uri) -or
        $uri.Scheme -ne "https" -or
        $uri.Port -ne 443 -or
        -not [string]::IsNullOrEmpty($uri.UserInfo) -or
        -not [string]::IsNullOrEmpty($uri.Fragment)) {
        throw "ORANGE_E2E_IP_CHECK_URL must be a closed HTTPS/443 URL"
    }
    return $value
}

function ConvertTo-Sha256([string]$Value) {
    $algorithm = [Security.Cryptography.SHA256]::Create()
    try {
        $bytes = [Text.Encoding]::UTF8.GetBytes($Value)
        return ([BitConverter]::ToString($algorithm.ComputeHash($bytes))).Replace("-", "").ToLowerInvariant()
    } finally {
        $algorithm.Dispose()
    }
}

function Get-FileSha256([string]$Path) {
    return (Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash.ToLowerInvariant()
}

function Get-RelativeEvidencePath([string]$Path) {
    $full = [IO.Path]::GetFullPath($Path)
    $rootPrefix = $Script:Root.TrimEnd('\') + '\'
    if ($full.StartsWith($rootPrefix, [StringComparison]::OrdinalIgnoreCase)) {
        return $full.Substring($rootPrefix.Length).Replace('\', '/')
    }
    return $full
}

function Write-PhaseReport([string]$Key, [Collections.IDictionary]$Observations) {
    New-Item -ItemType Directory -Force -Path $Script:OutputRoot | Out-Null
    $phasePath = Join-Path $Script:OutputRoot "phase-$Key.json"
    $report = [ordered]@{
        schema_version = 1
        phase = $Key
        status = "passed"
        recorded_at_utc = [DateTime]::UtcNow.ToString("o")
        context = Get-AcceptanceContext
        packages = Get-KnownPackageState
        postcondition = Get-SystemState
        observations = $Observations
    }
    $report | ConvertTo-Json -Depth 12 | Set-Content -LiteralPath $phasePath -Encoding UTF8

    $items = @()
    Get-ChildItem -LiteralPath $Script:OutputRoot -Filter "phase-*.json" -File |
        Sort-Object Name |
        ForEach-Object {
            $phaseReport = Get-Content -LiteralPath $_.FullName -Raw -Encoding UTF8 | ConvertFrom-Json
            $items += [ordered]@{
                phase = $phaseReport.phase
                status = $phaseReport.status
                evidence_path = Get-RelativeEvidencePath $_.FullName
                sha256 = Get-FileSha256 $_.FullName
            }
        }
    $resultPath = Join-Path $Script:OutputRoot "result.json"
    [ordered]@{
        schema_version = 1
        status = "passed"
        updated_at_utc = [DateTime]::UtcNow.ToString("o")
        items = $items
    } | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath $resultPath -Encoding UTF8
    [ordered]@{
        schema_version = 1
        last_completed = $Key
        updated_at_utc = [DateTime]::UtcNow.ToString("o")
        result_path = Get-RelativeEvidencePath $resultPath
    } | ConvertTo-Json -Depth 4 | Set-Content -LiteralPath (Join-Path $Script:OutputRoot "checkpoint.json") -Encoding UTF8
    Write-Host "Acceptance phase '$Key' passed."
}

function Read-PhaseReport([string]$Key) {
    $path = Join-Path $Script:OutputRoot "phase-$Key.json"
    if (-not (Test-Path -LiteralPath $path -PathType Leaf)) {
        throw "required acceptance phase has not completed: $Key"
    }
    return Get-Content -LiteralPath $path -Raw -Encoding UTF8 | ConvertFrom-Json
}

function Invoke-Checked([string]$WorkingDirectory, [string]$Command, [string[]]$Arguments) {
    Push-Location $WorkingDirectory
    try {
        & $Command @Arguments
        if ($LASTEXITCODE -ne 0) {
            throw "$Command failed with exit code $LASTEXITCODE"
        }
    } finally {
        Pop-Location
    }
}

function Invoke-ProductionSecretStoreProbe(
    [ValidateSet("complete", "empty")]
    [string]$ExpectedState
) {
    $name = "ORANGE_ACCEPTANCE_EXPECTED_PRODUCTION_SECRET_STATE"
    $previous = [Environment]::GetEnvironmentVariable($name)
    try {
        [Environment]::SetEnvironmentVariable($name, $ExpectedState)
        Invoke-Checked $Script:Root "cargo" @(
            "test",
            "-p", "orange-platform",
            "desktop_secret_store::native_tests::native_production_secret_store_matches_acceptance_state",
            "--", "--ignored", "--exact"
        )
    } finally {
        [Environment]::SetEnvironmentVariable($name, $previous)
    }
}

function Initialize-UninstallRetentionMarkers {
    foreach ($directory in @($Script:AppDataDirectory, $Script:LocalAppDataDirectory)) {
        New-Item -ItemType Directory -Force -Path $directory | Out-Null
        Set-Content -LiteralPath (Join-Path $directory $Script:UninstallRetentionMarker) `
            -Value "orange-uninstall-retention-acceptance-v1" -Encoding ASCII -NoNewline
    }
}

function Assert-AppDataRetained {
    foreach ($directory in @($Script:AppDataDirectory, $Script:LocalAppDataDirectory)) {
        $marker = Join-Path $directory $Script:UninstallRetentionMarker
        if (-not (Test-Path -LiteralPath $marker -PathType Leaf)) {
            throw "default uninstall did not preserve the fixed application-data directory"
        }
    }
}

function Assert-AppDataRemoved {
    foreach ($directory in @($Script:AppDataDirectory, $Script:LocalAppDataDirectory)) {
        if (Test-Path -LiteralPath $directory) {
            throw "explicit application-data deletion left a fixed Orange directory"
        }
    }
}

function Get-ToolVersion([string]$Command, [string[]]$Arguments) {
    $resolved = Get-Command $Command -ErrorAction Stop
    $value = (& $resolved.Source @Arguments 2>&1 | Out-String).Trim()
    if ($LASTEXITCODE -ne 0) {
        throw "$Command version check failed"
    }
    return $value
}

function Get-OptionalToolVersion([string]$Command, [string[]]$Arguments) {
    try {
        return Get-ToolVersion $Command $Arguments
    } catch {
        return $null
    }
}

function Get-AcceptanceContext {
    $os = Get-CimInstance Win32_OperatingSystem
    $revision = (& git -C $Script:Root rev-parse HEAD 2>$null | Out-String).Trim()
    if ($LASTEXITCODE -ne 0 -or $revision -notmatch '^[0-9a-f]{40}$') {
        $revision = $null
    }
    $worktreeStatus = (& git -C $Script:Root status --porcelain 2>$null | Out-String).Trim()
    $worktreeDirty = $LASTEXITCODE -eq 0 -and -not [string]::IsNullOrWhiteSpace($worktreeStatus)
    return [ordered]@{
        os_caption = [string]$os.Caption
        os_version = [string]$os.Version
        os_build = [string]$os.BuildNumber
        architecture = [string]$os.OSArchitecture
        git_revision = $revision
        git_dirty = $worktreeDirty
        node = Get-OptionalToolVersion "node" @("--version")
        pnpm = Get-OptionalToolVersion "pnpm" @("--version")
        rustc = Get-OptionalToolVersion "rustc" @("--version")
        cargo = Get-OptionalToolVersion "cargo" @("--version")
        go = Get-OptionalToolVersion "go" @("version")
    }
}

function Get-GitSourceProvenance([string]$Repository) {
    $trackedDiff = (& git -c core.autocrlf=false -C $Repository diff --binary --no-ext-diff HEAD -- 2>$null | Out-String)
    if ($LASTEXITCODE -ne 0) {
        throw "cannot read tracked source diff"
    }
    $untrackedPaths = @(& git -c core.autocrlf=false -C $Repository ls-files --others --exclude-standard 2>$null | Sort-Object)
    if ($LASTEXITCODE -ne 0) {
        throw "cannot read untracked source paths"
    }
    return [ordered]@{
        tracked_diff_sha256 = ConvertTo-Sha256 $trackedDiff
        untracked_paths_sha256 = ConvertTo-Sha256 ($untrackedPaths -join "`n")
        untracked_path_count = $untrackedPaths.Count
    }
}

function Get-KnownPackageState {
    $packages = [ordered]@{}
    foreach ($name in @(
        "Orange_0.0.9_x64-setup.exe",
        "Orange_0.1.0_x64-setup.exe",
        "Orange_0.1.0_x64-upgrade-failure-setup.exe"
    )) {
        $path = Join-Path (Join-Path $Script:OutputRoot "packages") $name
        if (Test-Path -LiteralPath $path -PathType Leaf) {
            $packages[$name] = [ordered]@{
                evidence_path = Get-RelativeEvidencePath $path
                sha256 = Get-FileSha256 $path
            }
        }
    }
    return $packages
}

function Assert-ToolVersion([string]$Name, [string]$Actual, [string]$ExpectedPattern) {
    if ($Actual -notmatch $ExpectedPattern) {
        throw "$Name does not match the pinned acceptance toolchain"
    }
}

function Get-ProxyState {
    $settings = Get-ItemProperty -LiteralPath $Script:ProxyRegistryPath
    $recovery = Get-ItemProperty -LiteralPath $Script:RecoveryRegistryPath -ErrorAction SilentlyContinue
    $recoveryPresent = $null -ne $recovery -and
        $null -ne $recovery.PSObject.Properties["SystemProxyV1"]
    $runOnce = Get-ItemProperty -LiteralPath $Script:RunOnceRegistryPath -ErrorAction SilentlyContinue
    $runOncePresent = $null -ne $runOnce -and
        $null -ne $runOnce.PSObject.Properties["OrangeSystemProxyRecovery"]
    return [ordered]@{
        enabled = ([int]$settings.ProxyEnable -eq 1)
        orange_server = ([string]$settings.ProxyServer -eq "127.0.0.1:$($Script:ProxyPort)")
        recovery_present = $recoveryPresent
        run_once_present = $runOncePresent
    }
}

function Get-NormalizedWindowsPath([string]$Path) {
    if ([string]::IsNullOrWhiteSpace($Path)) {
        return $null
    }
    $binaryPath = $Path.Trim()
    if ($binaryPath.StartsWith("\\?\", [StringComparison]::Ordinal)) {
        $binaryPath = $binaryPath.Substring(4)
    }
    try {
        return [IO.Path]::GetFullPath($binaryPath)
    } catch {
        return $null
    }
}

function Get-ServiceBinaryPath([string]$PathName) {
    $value = $PathName.Trim()
    if ([string]::IsNullOrWhiteSpace($value) -or -not $value.StartsWith('"')) {
        return $null
    }
    $closingQuote = $value.IndexOf('"', 1)
    if ($closingQuote -le 1) {
        return $null
    }
    return Get-NormalizedWindowsPath ($value.Substring(1, $closingQuote - 1))
}

function Get-ServiceState {
    $service = Get-Service -Name $Script:ServiceName -ErrorAction SilentlyContinue
    if ($null -eq $service) {
        return [ordered]@{
            present = $false
            running = $false
            start_type = "absent"
            binary_under_install_root = $false
            binary_matches_expected = $false
            account_local_system = $false
            service_sid_unrestricted = $false
        }
    }
    $configuration = Get-CimInstance Win32_Service -Filter "Name='$($Script:ServiceName)'"
    $serviceSid = Get-ItemPropertyValue -LiteralPath "HKLM:\SYSTEM\CurrentControlSet\Services\$($Script:ServiceName)" -Name ServiceSidType -ErrorAction SilentlyContinue
    $binaryPath = if ($null -eq $configuration) { $null } else { Get-ServiceBinaryPath ([string]$configuration.PathName) }
    $expectedBinaryPath = Join-Path $Script:InstallRoot "orange-service.exe"
    return [ordered]@{
        present = $true
        running = ($service.Status -eq "Running")
        start_type = [string]$service.StartType
        binary_under_install_root = $null -ne $binaryPath -and
            $binaryPath.StartsWith($Script:InstallRoot + "\", [StringComparison]::OrdinalIgnoreCase)
        binary_matches_expected = $null -ne $binaryPath -and
            $binaryPath.Equals($expectedBinaryPath, [StringComparison]::OrdinalIgnoreCase)
        account_local_system = $null -ne $configuration -and [string]$configuration.StartName -eq "LocalSystem"
        service_sid_unrestricted = ([int]$serviceSid -eq 1)
    }
}

function Get-OrangeProcesses {
    $names = @("orange-app.exe", "orange-control-plane.exe", "orange-data-plane.exe", "orange-service.exe")
    return @(Get-CimInstance Win32_Process | Where-Object {
        $_.Name -in $names -and $_.ExecutablePath
    } | ForEach-Object {
        $executablePath = Get-NormalizedWindowsPath ([string]$_.ExecutablePath)
        [ordered]@{
            name = $_.Name
            process_id = [int]$_.ProcessId
            inside_install_root = $null -ne $executablePath -and
                $executablePath.StartsWith($Script:InstallRoot + "\", [StringComparison]::OrdinalIgnoreCase)
            executable_path_sha256 = if ($null -eq $executablePath) {
                $null
            } else {
                ConvertTo-Sha256 ($executablePath.ToLowerInvariant())
            }
        }
    })
}

function Get-TunState {
    $adapter = Get-NetAdapter -ErrorAction SilentlyContinue | Where-Object {
        $_.Name -eq "orange-tun" -or $_.InterfaceDescription -eq "orange-tun"
    } | Select-Object -First 1
    if ($null -eq $adapter) {
        return [ordered]@{ present = $false; up = $false; ipv4 = $false; ipv6 = $false }
    }
    $addresses = @(Get-NetIPAddress -InterfaceIndex $adapter.ifIndex -ErrorAction SilentlyContinue)
    return [ordered]@{
        present = $true
        up = ([string]$adapter.Status -eq "Up")
        ipv4 = ($null -ne ($addresses | Where-Object { $_.IPAddress -eq "172.19.0.1" }))
        ipv6 = ($null -ne ($addresses | Where-Object { $_.IPAddress -eq "fdfe:dcba:9876::1" }))
    }
}

function Test-ProxyListener {
    return $null -ne (Get-NetTCPConnection -State Listen -LocalPort $Script:ProxyPort -ErrorAction SilentlyContinue | Select-Object -First 1)
}

function Test-FirewallRule {
    return $null -ne (Get-NetFirewallRule -DisplayName $Script:FirewallRuleName -ErrorAction SilentlyContinue | Select-Object -First 1)
}

function Get-FirewallState {
    $rule = Get-NetFirewallRule -DisplayName $Script:FirewallRuleName -ErrorAction SilentlyContinue | Select-Object -First 1
    if ($null -eq $rule) {
        return [ordered]@{ present = $false; enabled = $false; allow = $false; program_under_install_root = $false }
    }
    $application = $rule | Get-NetFirewallApplicationFilter -ErrorAction SilentlyContinue | Select-Object -First 1
    return [ordered]@{
        present = $true
        enabled = ([string]$rule.Enabled -eq "True")
        allow = ([string]$rule.Action -eq "Allow")
        program_under_install_root = $null -ne $application -and
            ([string]$application.Program).StartsWith($Script:InstallRoot + "\", [StringComparison]::OrdinalIgnoreCase)
    }
}

function Get-DnsState {
    $entries = @(Get-DnsClientServerAddress -ErrorAction SilentlyContinue)
    $servers = @($entries | ForEach-Object { $_.ServerAddresses } | Where-Object { -not [string]::IsNullOrWhiteSpace($_) } | Sort-Object -Unique)
    $tunServers = @($entries | Where-Object { $_.InterfaceAlias -eq "orange-tun" } | ForEach-Object { $_.ServerAddresses } | Where-Object { -not [string]::IsNullOrWhiteSpace($_) } | Sort-Object -Unique)
    return [ordered]@{
        server_count = $servers.Count
        servers_sha256 = if ($servers.Count -eq 0) { $null } else { ConvertTo-Sha256 ($servers -join "`n") }
        tun_server_count = $tunServers.Count
        tun_servers_sha256 = if ($tunServers.Count -eq 0) { $null } else { ConvertTo-Sha256 ($tunServers -join "`n") }
    }
}

function Get-RouteState {
    $routes = @(Get-NetRoute -ErrorAction SilentlyContinue)
    $tunRoutes = @($routes | Where-Object { $_.InterfaceAlias -eq "orange-tun" })
    $normalized = @($tunRoutes | ForEach-Object {
        "$($_.AddressFamily)|$($_.DestinationPrefix)|$($_.NextHop)|$($_.RouteMetric)"
    } | Sort-Object -Unique)
    return [ordered]@{
        default_route_count = @($routes | Where-Object { $_.DestinationPrefix -in @("0.0.0.0/0", "::/0") }).Count
        tun_route_count = $tunRoutes.Count
        tun_routes_sha256 = if ($normalized.Count -eq 0) { $null } else { ConvertTo-Sha256 ($normalized -join "`n") }
    }
}

function Get-SystemState {
    return [ordered]@{
        service = Get-ServiceState
        proxy = Get-ProxyState
        tun = Get-TunState
        dns = Get-DnsState
        routes = Get-RouteState
        firewall = Get-FirewallState
        proxy_listener_ready = Test-ProxyListener
        processes = @(Get-OrangeProcesses)
    }
}

function Invoke-ExitProbe([string]$Mode) {
    $url = Assert-ExitProbeConfiguration
    $arguments = @("--fail", "--silent", "--show-error", "--connect-timeout", "10", "--max-time", "20")
    if ($Mode -eq "proxy") {
        $arguments += @("--proxy", "http://127.0.0.1:$($Script:ProxyPort)")
    } else {
        $arguments += @("--noproxy", "*")
    }
    $arguments += $url
    $value = (& curl.exe @arguments 2>$null | Out-String).Trim()
    if ($LASTEXITCODE -ne 0 -or [string]::IsNullOrWhiteSpace($value) -or $value.Length -gt 256) {
        throw "exit probe failed or returned an invalid bounded response"
    }
    return ConvertTo-Sha256 $value
}

function Wait-Until([scriptblock]$Condition, [int]$Seconds, [string]$Failure) {
    $deadline = [DateTime]::UtcNow.AddSeconds($Seconds)
    while ([DateTime]::UtcNow -lt $deadline) {
        if (& $Condition) {
            return
        }
        Start-Sleep -Milliseconds 250
    }
    throw $Failure
}

function Assert-Package([string]$Path, [string]$Name) {
    if ([string]::IsNullOrWhiteSpace($Path)) {
        throw "$Name package path is required"
    }
    $resolved = (Resolve-Path -LiteralPath $Path).Path
    $signature = Get-AuthenticodeSignature -LiteralPath $resolved
    if ($signature.Status -ne "NotSigned") {
        throw "$Name package must be the explicitly non-releaseable unsigned test package"
    }
    return $resolved
}

function Get-PipeProbeCommand([string]$PipeName, [switch]$RequireLowIntegrity) {
    $integrityCheck = if ($RequireLowIntegrity) {
        '$g=& whoami.exe /groups|Out-String;if($g-notmatch"S-1-16-4096"){exit 42};'
    } else {
        ""
    }
    # Keep the encoded child command below CreateProcessWithLogonW's 1,024-character limit.
    $source = '$ErrorActionPreference="Stop";__INTEGRITY_CHECK__try{$p=[IO.Pipes.NamedPipeClientStream]::new(".","__PIPE_NAME__");$p.Connect(1500);$p.Dispose();exit 41}catch [UnauthorizedAccessException]{exit 0}catch [TimeoutException]{exit 0}catch{if($_.Exception.HResult-eq -2147024891){exit 0};exit 43}'
    $source = $source.Replace("__INTEGRITY_CHECK__", $integrityCheck)
    $source = $source.Replace("__PIPE_NAME__", $PipeName)
    return [Convert]::ToBase64String([Text.Encoding]::Unicode.GetBytes($source))
}

function Invoke-ProcessAsLocalUser(
    [string]$UserName,
    [Security.SecureString]$Password,
    [string]$EncodedCommand
) {
    if ($null -eq ("OrangeAcceptanceUserLauncher" -as [type])) {
        Add-Type -TypeDefinition @'
using System;
using System.ComponentModel;
using System.Runtime.InteropServices;
using System.Text;

public static class OrangeAcceptanceUserLauncher {
    const uint CreateNoWindow = 0x08000000;
    const uint StartUseShowWindow = 0x00000001;

    [StructLayout(LayoutKind.Sequential, CharSet = CharSet.Unicode)]
    struct StartupInfo {
        public uint cb;
        public string lpReserved;
        public string lpDesktop;
        public string lpTitle;
        public uint dwX;
        public uint dwY;
        public uint dwXSize;
        public uint dwYSize;
        public uint dwXCountChars;
        public uint dwYCountChars;
        public uint dwFillAttribute;
        public uint dwFlags;
        public short wShowWindow;
        public short cbReserved2;
        public IntPtr lpReserved2;
        public IntPtr hStdInput;
        public IntPtr hStdOutput;
        public IntPtr hStdError;
    }

    [StructLayout(LayoutKind.Sequential)]
    struct ProcessInformation {
        public IntPtr hProcess;
        public IntPtr hThread;
        public uint dwProcessId;
        public uint dwThreadId;
    }

    [DllImport("advapi32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
    static extern bool LogonUserW(
        string userName, string domain, IntPtr password, int logonType,
        int logonProvider, out IntPtr token
    );
    [DllImport("advapi32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
    static extern bool CreateProcessWithLogonW(
        string userName, string domain, IntPtr password, uint logonFlags,
        string application, StringBuilder commandLine, uint flags,
        IntPtr environment, string currentDirectory,
        ref StartupInfo startup, out ProcessInformation process
    );
    [DllImport("advapi32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
    static extern bool CreateProcessAsUserW(
        IntPtr token, string application, StringBuilder commandLine,
        IntPtr processAttributes, IntPtr threadAttributes, bool inheritHandles,
        uint flags, IntPtr environment, string currentDirectory,
        ref StartupInfo startup, out ProcessInformation process
    );
    [DllImport("advapi32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
    static extern bool CreateProcessWithTokenW(
        IntPtr token, uint logonFlags, string application, StringBuilder commandLine,
        uint flags, IntPtr environment, string currentDirectory,
        ref StartupInfo startup, out ProcessInformation process
    );
    [DllImport("kernel32.dll", SetLastError = true)] static extern bool CloseHandle(IntPtr handle);
    [DllImport("kernel32.dll", SetLastError = true)] static extern uint WaitForSingleObject(IntPtr handle, uint milliseconds);
    [DllImport("kernel32.dll", SetLastError = true)] static extern bool GetExitCodeProcess(IntPtr process, out uint exitCode);
    [DllImport("kernel32.dll", SetLastError = true)] static extern bool TerminateProcess(IntPtr process, uint exitCode);

    public static int Run(
        string userName, string domain, IntPtr password,
        string application, string arguments
    ) {
        IntPtr token = IntPtr.Zero;
        ProcessInformation process = new ProcessInformation();
        try {
            var startup = new StartupInfo {
                cb = (uint)Marshal.SizeOf(typeof(StartupInfo)),
                dwFlags = StartUseShowWindow,
                wShowWindow = 0
            };
            var command = new StringBuilder("\"" + application + "\" " + arguments);
            bool started = CreateProcessWithLogonW(
                userName, domain, password, 0, application, command, CreateNoWindow,
                IntPtr.Zero, @"C:\Windows\Temp", ref startup, out process
            );
            int withLogonError = started ? 0 : Marshal.GetLastWin32Error();
            if (started) return WaitForProcess(process);

            if (!LogonUserW(userName, domain, password, 2, 0, out token))
                throw new Win32Exception(
                    Marshal.GetLastWin32Error(), "acceptance user logon failed"
                );
            startup = new StartupInfo { cb = (uint)Marshal.SizeOf(typeof(StartupInfo)) };
            command = new StringBuilder("\"" + application + "\" " + arguments);
            started = CreateProcessAsUserW(
                token, application, command, IntPtr.Zero, IntPtr.Zero, false,
                CreateNoWindow, IntPtr.Zero, @"C:\Windows\Temp", ref startup, out process
            );
            int asUserError = started ? 0 : Marshal.GetLastWin32Error();
            if (!started) {
                command = new StringBuilder("\"" + application + "\" " + arguments);
                started = CreateProcessWithTokenW(
                    token, 0, application, command, CreateNoWindow, IntPtr.Zero,
                    @"C:\Windows\Temp", ref startup, out process
                );
            }
            if (!started) throw new Win32Exception(
                Marshal.GetLastWin32Error(),
                "acceptance user process creation failed; CreateProcessWithLogon=" +
                withLogonError + "; CreateProcessAsUser=" + asUserError
            );
            return WaitForProcess(process);
        } finally {
            if (process.hThread != IntPtr.Zero) CloseHandle(process.hThread);
            if (process.hProcess != IntPtr.Zero) CloseHandle(process.hProcess);
            if (token != IntPtr.Zero) CloseHandle(token);
        }
    }

    static int WaitForProcess(ProcessInformation process) {
            if (WaitForSingleObject(process.hProcess, 15000) == 0x00000102) {
                TerminateProcess(process.hProcess, 44);
                WaitForSingleObject(process.hProcess, 5000);
                throw new TimeoutException("different-user acceptance process timed out");
            }
            uint exitCode;
            if (!GetExitCodeProcess(process.hProcess, out exitCode))
                throw new Win32Exception(Marshal.GetLastWin32Error());
            return unchecked((int)exitCode);
    }
}
'@
    }
    $passwordPointer = [Runtime.InteropServices.Marshal]::SecureStringToGlobalAllocUnicode($Password)
    try {
        $arguments = "-NoProfile -NonInteractive -EncodedCommand $EncodedCommand"
        if ($arguments.Length -ge 1024) {
            throw "different-user acceptance command exceeds CreateProcessWithLogonW limit"
        }
        return [OrangeAcceptanceUserLauncher]::Run(
            $UserName,
            [Environment]::MachineName,
            $passwordPointer,
            (Join-Path $PSHOME "powershell.exe"),
            $arguments
        )
    } finally {
        [Runtime.InteropServices.Marshal]::ZeroFreeGlobalAllocUnicode($passwordPointer)
    }
}

function Invoke-DifferentUserPipeProbe([string]$EncodedCommand) {
    if ($null -eq (Get-Command New-LocalUser -ErrorAction SilentlyContinue) -or
        $null -eq (Get-Command Remove-LocalUser -ErrorAction SilentlyContinue)) {
        throw "local user management commands are unavailable"
    }
    $userName = "OraAcc" + [Guid]::NewGuid().ToString("N").Substring(0, 8)
    $random = New-Object byte[] 24
    $generator = [Security.Cryptography.RandomNumberGenerator]::Create()
    try {
        $generator.GetBytes($random)
    } finally {
        $generator.Dispose()
    }
    $password = [Convert]::ToBase64String($random) + "aA1!"
    [Array]::Clear($random, 0, $random.Length)
    $securePassword = ConvertTo-SecureString $password -AsPlainText -Force
    $created = $false
    $userSid = $null
    try {
        $user = New-LocalUser -Name $userName -Password $securePassword `
            -AccountNeverExpires -PasswordNeverExpires
        $created = $true
        $userSid = [string]$user.SID.Value
        $exitCode = Invoke-ProcessAsLocalUser $userName $securePassword $EncodedCommand
        if ($exitCode -ne 0) {
            throw "different-user pipe probe returned $exitCode"
        }
    } finally {
        $password = $null
        $securePassword = $null
        if (-not [string]::IsNullOrWhiteSpace($userSid)) {
            $profile = Get-CimInstance Win32_UserProfile -ErrorAction SilentlyContinue |
                Where-Object { [string]$_.SID -eq $userSid } |
                Select-Object -First 1
            if ($null -ne $profile -and -not [bool]$profile.Loaded) {
                Remove-CimInstance -InputObject $profile
            }
        }
        if ($created) {
            Remove-LocalUser -Name $userName
        }
        if ($null -ne (Get-LocalUser -Name $userName -ErrorAction SilentlyContinue)) {
            throw "temporary acceptance user was not removed"
        }
    }
    return $true
}

function Invoke-LowIntegrityPipeProbe([string]$EncodedCommand) {
    if ($null -eq ("OrangeAcceptanceLowIntegrityLauncher" -as [type])) {
        Add-Type -TypeDefinition @'
using System;
using System.ComponentModel;
using System.Runtime.InteropServices;
using System.Text;

public static class OrangeAcceptanceLowIntegrityLauncher {
    const uint TokenAccess = 0x0001 | 0x0002 | 0x0008 | 0x0080 | 0x0100;
    const uint IntegrityAttribute = 0x20;
    const uint CreateNoWindow = 0x08000000;

    [StructLayout(LayoutKind.Sequential)]
    struct SidAndAttributes { public IntPtr Sid; public uint Attributes; }

    [StructLayout(LayoutKind.Sequential)]
    struct TokenMandatoryLabel { public SidAndAttributes Label; }

    [StructLayout(LayoutKind.Sequential, CharSet = CharSet.Unicode)]
    struct StartupInfo {
        public uint cb;
        public string lpReserved;
        public string lpDesktop;
        public string lpTitle;
        public uint dwX;
        public uint dwY;
        public uint dwXSize;
        public uint dwYSize;
        public uint dwXCountChars;
        public uint dwYCountChars;
        public uint dwFillAttribute;
        public uint dwFlags;
        public short wShowWindow;
        public short cbReserved2;
        public IntPtr lpReserved2;
        public IntPtr hStdInput;
        public IntPtr hStdOutput;
        public IntPtr hStdError;
    }

    [StructLayout(LayoutKind.Sequential)]
    struct ProcessInformation {
        public IntPtr hProcess;
        public IntPtr hThread;
        public uint dwProcessId;
        public uint dwThreadId;
    }

    [DllImport("kernel32.dll")] static extern IntPtr GetCurrentProcess();
    [DllImport("kernel32.dll", SetLastError = true)] static extern bool CloseHandle(IntPtr handle);
    [DllImport("kernel32.dll", SetLastError = true)] static extern uint WaitForSingleObject(IntPtr handle, uint milliseconds);
    [DllImport("kernel32.dll", SetLastError = true)] static extern bool GetExitCodeProcess(IntPtr process, out uint exitCode);
    [DllImport("kernel32.dll", SetLastError = true)] static extern bool TerminateProcess(IntPtr process, uint exitCode);
    [DllImport("kernel32.dll")] static extern IntPtr LocalFree(IntPtr memory);
    [DllImport("advapi32.dll", SetLastError = true)] static extern bool OpenProcessToken(IntPtr process, uint access, out IntPtr token);
    [DllImport("advapi32.dll", SetLastError = true)] static extern bool DuplicateTokenEx(IntPtr existing, uint access, IntPtr attributes, int impersonationLevel, int tokenType, out IntPtr token);
    [DllImport("advapi32.dll", SetLastError = true)] static extern bool SetTokenInformation(IntPtr token, int informationClass, ref TokenMandatoryLabel information, uint length);
    [DllImport("advapi32.dll", CharSet = CharSet.Unicode, SetLastError = true)] static extern bool ConvertStringSidToSid(string value, out IntPtr sid);
    [DllImport("advapi32.dll", SetLastError = true)] static extern uint GetLengthSid(IntPtr sid);
    [DllImport("advapi32.dll", CharSet = CharSet.Unicode, SetLastError = true)] static extern bool CreateProcessAsUser(IntPtr token, string application, StringBuilder commandLine, IntPtr processAttributes, IntPtr threadAttributes, bool inheritHandles, uint flags, IntPtr environment, string currentDirectory, ref StartupInfo startup, out ProcessInformation process);
    [DllImport("advapi32.dll", CharSet = CharSet.Unicode, SetLastError = true)] static extern bool CreateProcessWithTokenW(IntPtr token, uint logonFlags, string application, StringBuilder commandLine, uint flags, IntPtr environment, string currentDirectory, ref StartupInfo startup, out ProcessInformation process);

    public static int Run(string application, string arguments) {
        IntPtr source = IntPtr.Zero;
        IntPtr token = IntPtr.Zero;
        IntPtr sid = IntPtr.Zero;
        ProcessInformation process = new ProcessInformation();
        try {
            if (!OpenProcessToken(GetCurrentProcess(), TokenAccess, out source))
                throw new Win32Exception(Marshal.GetLastWin32Error());
            if (!DuplicateTokenEx(source, TokenAccess, IntPtr.Zero, 2, 1, out token))
                throw new Win32Exception(Marshal.GetLastWin32Error());
            if (!ConvertStringSidToSid("S-1-16-4096", out sid))
                throw new Win32Exception(Marshal.GetLastWin32Error());
            var label = new TokenMandatoryLabel {
                Label = new SidAndAttributes { Sid = sid, Attributes = IntegrityAttribute }
            };
            uint length = (uint)Marshal.SizeOf(typeof(TokenMandatoryLabel)) + GetLengthSid(sid);
            if (!SetTokenInformation(token, 25, ref label, length))
                throw new Win32Exception(Marshal.GetLastWin32Error());

            var startup = new StartupInfo { cb = (uint)Marshal.SizeOf(typeof(StartupInfo)) };
            var command = new StringBuilder("\"" + application + "\" " + arguments);
            bool started = CreateProcessAsUser(
                token, application, command, IntPtr.Zero, IntPtr.Zero, false,
                CreateNoWindow, IntPtr.Zero, @"C:\Windows\Temp", ref startup, out process
            );
            if (!started) {
                command = new StringBuilder("\"" + application + "\" " + arguments);
                started = CreateProcessWithTokenW(
                    token, 0, application, command, CreateNoWindow, IntPtr.Zero,
                    @"C:\Windows\Temp", ref startup, out process
                );
            }
            if (!started) throw new Win32Exception(Marshal.GetLastWin32Error());
            if (WaitForSingleObject(process.hProcess, 15000) == 0x00000102) {
                TerminateProcess(process.hProcess, 44);
                WaitForSingleObject(process.hProcess, 5000);
                throw new TimeoutException("low-integrity acceptance process timed out");
            }
            uint exitCode;
            if (!GetExitCodeProcess(process.hProcess, out exitCode))
                throw new Win32Exception(Marshal.GetLastWin32Error());
            return unchecked((int)exitCode);
        } finally {
            if (process.hThread != IntPtr.Zero) CloseHandle(process.hThread);
            if (process.hProcess != IntPtr.Zero) CloseHandle(process.hProcess);
            if (sid != IntPtr.Zero) LocalFree(sid);
            if (token != IntPtr.Zero) CloseHandle(token);
            if (source != IntPtr.Zero) CloseHandle(source);
        }
    }
}
'@
    }
    $arguments = "-NoProfile -NonInteractive -EncodedCommand $EncodedCommand"
    $exitCode = [OrangeAcceptanceLowIntegrityLauncher]::Run(
        (Join-Path $PSHOME "powershell.exe"),
        $arguments
    )
    if ($exitCode -ne 0) {
        throw "low-integrity pipe probe returned $exitCode"
    }
    return $true
}

function Invoke-Installer([string]$Path) {
    $process = Start-Process -FilePath $Path -ArgumentList "/S" `
        -WindowStyle Hidden -Wait -PassThru
    if ($process.ExitCode -ne 0) {
        throw "installer failed with exit code $($process.ExitCode)"
    }
}

function Get-InstalledFileHashes {
    $files = @(
        "orange-app.exe",
        "orange-control-plane.exe",
        "orange-service.exe",
        "orange-installer.exe",
        "orange-data-plane.exe",
        "uninstall.exe"
    )
    $hashes = [ordered]@{}
    foreach ($name in $files) {
        $path = Join-Path $Script:InstallRoot $name
        if (-not (Test-Path -LiteralPath $path -PathType Leaf)) {
            throw "installed package is missing $name"
        }
        $hashes[$name] = Get-FileSha256 $path
    }
    return $hashes
}

function Get-InstallationIdentityHash {
    $path = Join-Path $Script:InstallRoot "orange-installation-id.v1"
    Get-InstallationId | Out-Null
    return Get-FileSha256 $path
}

function Get-InstallationId {
    $path = Join-Path $Script:InstallRoot "orange-installation-id.v1"
    if (-not (Test-Path -LiteralPath $path -PathType Leaf)) {
        throw "installation identity is missing"
    }
    $identity = (Get-Content -LiteralPath $path -Raw -Encoding ASCII).Trim()
    if ($identity -notmatch '^[0-9a-f]{32}$') {
        throw "installation identity is invalid"
    }
    return $identity
}

function Get-AclPolicyState([string]$Path) {
    if (-not (Test-Path -LiteralPath $Path)) {
        return [ordered]@{ present = $false; protected = $false; broad_write_absent = $false; sddl_sha256 = $null }
    }
    $acl = Get-Acl -LiteralPath $Path
    $broadPrincipals = @("S-1-1-0", "S-1-5-11", "S-1-5-32-545")
    $writeMask = [int][Security.AccessControl.FileSystemRights]::Write -bor
        [int][Security.AccessControl.FileSystemRights]::Modify -bor
        [int][Security.AccessControl.FileSystemRights]::FullControl
    $broadWrite = @($acl.Access | Where-Object {
        $_.AccessControlType -eq [Security.AccessControl.AccessControlType]::Allow -and
        $_.IdentityReference.Value -in $broadPrincipals -and
        (([int]$_.FileSystemRights -band $writeMask) -ne 0)
    })
    $sddl = $acl.GetSecurityDescriptorSddlForm([Security.AccessControl.AccessControlSections]::Access)
    return [ordered]@{
        present = $true
        protected = [bool]$acl.AreAccessRulesProtected
        broad_write_absent = ($broadWrite.Count -eq 0)
        sddl_sha256 = ConvertTo-Sha256 $sddl
    }
}

function Get-InstallationPolicyState {
    $identity = Get-InstallationId
    $pipeName = "Orange.DataPlane.$identity.v1"
    $pipePresent = $null -ne (Get-ChildItem -LiteralPath "\\.\pipe\" -ErrorAction SilentlyContinue | Where-Object { $_.Name -eq $pipeName } | Select-Object -First 1)
    return [ordered]@{
        identity_acl = Get-AclPolicyState (Join-Path $Script:InstallRoot "orange-installation-id.v1")
        runtime_acl = Get-AclPolicyState (Join-Path $Script:InstallRoot "data-plane")
        named_pipe_present = $pipePresent
    }
}

function Assert-Installed {
    $service = Get-ServiceState
    if (-not $service.present -or -not $service.running -or $service.start_type -ne "Automatic" -or
        -not $service.binary_under_install_root -or -not $service.binary_matches_expected -or
        -not $service.account_local_system -or
        -not $service.service_sid_unrestricted) {
        throw "OrangeDataPlane service does not match the fixed SCM policy"
    }
    $firewall = Get-FirewallState
    if (-not $firewall.present -or -not $firewall.enabled -or -not $firewall.allow -or
        -not $firewall.program_under_install_root) {
        throw "Orange Data Plane firewall rule does not match the fixed program policy"
    }
    Get-InstalledFileHashes | Out-Null
    Get-InstallationIdentityHash | Out-Null
    $policy = Get-InstallationPolicyState
    if (-not $policy.identity_acl.protected -or -not $policy.identity_acl.broad_write_absent -or
        -not $policy.runtime_acl.protected -or -not $policy.runtime_acl.broad_write_absent -or
        -not $policy.named_pipe_present) {
        throw "Orange installation ACL or Named Pipe policy is incomplete"
    }
    if (Test-Path -LiteralPath (Join-Path $Script:InstallRoot ".orange-upgrade-backup")) {
        throw "Orange installation contains an uncommitted upgrade backup"
    }
}

function Assert-Clean {
    $service = Get-ServiceState
    $proxy = Get-ProxyState
    $tun = Get-TunState
    $dns = Get-DnsState
    $routes = Get-RouteState
    $processes = @(Get-OrangeProcesses)
    if ($service.present -or (Test-Path -LiteralPath $Script:InstallRoot) -or
        $processes.Count -ne 0 -or $proxy.enabled -or $proxy.recovery_present -or
        $proxy.run_once_present -or $tun.present -or (Test-FirewallRule) -or
        $dns.tun_server_count -ne 0 -or $routes.tun_route_count -ne 0 -or
        (Test-ProxyListener)) {
        throw "Orange system state is not clean"
    }
    return [ordered]@{
        service_absent = $true
        install_root_absent = $true
        processes_absent = $true
        proxy_restored = $true
        recovery_absent = $true
        run_once_absent = $true
        tun_absent = $true
        tun_dns_absent = $true
        tun_routes_absent = $true
        firewall_absent = $true
        proxy_listener_absent = $true
    }
}

function Write-MergedTauriConfig(
    [string]$Repository,
    [string]$Version,
    [string]$Destination,
    [switch]$InjectUpgradeFailure
) {
    $source = Join-Path $Repository "src-tauri\tauri.windows.test.conf.json"
    $config = Get-Content -LiteralPath $source -Raw -Encoding UTF8 | ConvertFrom-Json
    $config | Add-Member -NotePropertyName version -NotePropertyValue $Version -Force
    if ($InjectUpgradeFailure) {
        $config.bundle.windows.nsis.installerHooks = "windows/installer-hooks-upgrade-failure.nsh"
    }
    $config | ConvertTo-Json -Depth 20 | Set-Content -LiteralPath $Destination -Encoding UTF8
}

function Set-WorkspacePackageVersion([string]$Repository, [string]$Version) {
    if ($Version -notmatch '^\d+\.\d+\.\d+$') {
        throw "acceptance package version is invalid"
    }
    $manifestPath = Join-Path $Repository "Cargo.toml"
    $document = Get-Content -LiteralPath $manifestPath -Raw -Encoding UTF8
    $section = [regex]::Match($document, '(?ms)^\[workspace\.package\]\r?\n(?<body>.*?)(?=^\[|\z)')
    if (-not $section.Success) {
        throw "Cargo workspace package section is missing"
    }
    $versionMatches = [regex]::Matches($section.Groups["body"].Value, '(?m)^version = "(?<version>\d+\.\d+\.\d+)"\r?$')
    if ($versionMatches.Count -ne 1) {
        throw "Cargo workspace package version is missing or ambiguous"
    }
    $versionMatch = $versionMatches[0]
    $current = $versionMatch.Groups["version"].Value
    if ($current -eq $Version) {
        return
    }
    $absoluteStart = $section.Groups["body"].Index + $versionMatch.Groups["version"].Index
    $updated = $document.Remove($absoluteStart, $current.Length).Insert($absoluteStart, $Version)
    $updated | Set-Content -LiteralPath $manifestPath -Encoding UTF8
}

function Build-TestPackage(
    [string]$Repository,
    [string]$Version,
    [string]$Destination,
    [switch]$InjectUpgradeFailure
) {
    $previousProductVersion = [Environment]::GetEnvironmentVariable("ORANGE_BOOTSTRAP_PRODUCT_VERSION")
    try {
        Set-WorkspacePackageVersion $Repository $Version
        $env:ORANGE_BOOTSTRAP_PRODUCT_VERSION = $Version
        Invoke-Checked $Repository "pnpm" @("install", "--frozen-lockfile")
        Invoke-Checked $Repository "python" @("scripts/ci/run.py", "bootstrap-release")
        Invoke-Checked $Repository "python" @("scripts/ci/run.py", "windows-data-plane")
        Invoke-Checked $Repository "pnpm" @("prepare:windows-test")
        $configName = if ($InjectUpgradeFailure) {
            "windows-acceptance-upgrade-failure-tauri-config.json"
        } else {
            "windows-acceptance-tauri-config.json"
        }
        $configPath = Join-Path $Repository "artifacts\$configName"
        Write-MergedTauriConfig $Repository $Version $configPath -InjectUpgradeFailure:$InjectUpgradeFailure
        Invoke-Checked $Repository "pnpm" @(
            "tauri", "build", "--bundles", "nsis", "--features", "unsigned-test-runtime",
            "--config", $configPath, "--no-sign", "--ci"
        )
        $package = Join-Path $Repository "target\release\bundle\nsis\Orange_$($Version)_x64-setup.exe"
        if (-not (Test-Path -LiteralPath $package -PathType Leaf)) {
            throw "expected Tauri package was not produced: $package"
        }
        New-Item -ItemType Directory -Force -Path (Split-Path -Parent $Destination) | Out-Null
        Copy-Item -LiteralPath $package -Destination $Destination -Force
    } finally {
        $env:ORANGE_BOOTSTRAP_PRODUCT_VERSION = $previousProductVersion
    }
}

function Invoke-Preflight {
    Assert-RequiredEnvironment
    $os = Get-CimInstance Win32_OperatingSystem
    $clean = Assert-Clean
    $nodeVersion = Get-ToolVersion "node" @("--version")
    $pnpmVersion = Get-ToolVersion "pnpm" @("--version")
    $rustVersion = Get-ToolVersion "rustc" @("--version")
    $cargoVersion = Get-ToolVersion "cargo" @("--version")
    $goVersion = Get-ToolVersion "go" @("version")
    Assert-ToolVersion "Node.js" $nodeVersion '^v22\.23\.1$'
    Assert-ToolVersion "pnpm" $pnpmVersion '^11\.9\.0$'
    Assert-ToolVersion "Rust" $rustVersion '^rustc 1\.95\.0 '
    Assert-ToolVersion "Cargo" $cargoVersion '^cargo 1\.95\.0 '
    Assert-ToolVersion "Go" $goVersion '^go version go1\.25\.5 windows/amd64$'
    $exitHash = $null
    $exitProbeConfigured = -not [string]::IsNullOrWhiteSpace($env:ORANGE_E2E_IP_CHECK_URL)
    if ($exitProbeConfigured) {
        $exitHash = Invoke-ExitProbe "direct"
    }
    Write-PhaseReport "preflight" ([ordered]@{
        os_caption = [string]$os.Caption
        os_version = [string]$os.Version
        os_build = [string]$os.BuildNumber
        architecture = [string]$os.OSArchitecture
        node = $nodeVersion
        pnpm = $pnpmVersion
        rustc = $rustVersion
        cargo = $cargoVersion
        go = $goVersion
        secrets_present = $true
        exit_probe_configured = $exitProbeConfigured
        direct_exit_sha256 = $exitHash
        clean_state = $clean
    })
}

function Invoke-Build {
    Assert-RequiredEnvironment
    Read-PhaseReport "preflight" | Out-Null
    $packages = Join-Path $Script:OutputRoot "packages"
    New-Item -ItemType Directory -Force -Path $packages | Out-Null
    $baselineDestination = Join-Path $packages "Orange_0.0.9_x64-setup.exe"
    $candidateDestination = Join-Path $packages "Orange_0.1.0_x64-setup.exe"
    $failureDestination = Join-Path $packages "Orange_0.1.0_x64-upgrade-failure-setup.exe"
    $candidateSource = Get-GitSourceProvenance $Script:Root
    $temporaryBase = [IO.Path]::GetFullPath([IO.Path]::GetTempPath()).TrimEnd('\') + '\'
    $baselineRoot = Join-Path ([IO.Path]::GetTempPath()) ("orange-baseline-" + [Guid]::NewGuid().ToString("N"))
    $baselineFull = [IO.Path]::GetFullPath($baselineRoot)
    if (-not $baselineFull.StartsWith($temporaryBase, [StringComparison]::OrdinalIgnoreCase)) {
        throw "baseline worktree escaped the temporary directory"
    }
    try {
        Invoke-Checked $Script:Root "git" @(
            "-c", "core.autocrlf=false", "-c", "core.longpaths=true",
            "worktree", "add", "--detach", $baselineRoot, "6b23686"
        )
        Build-TestPackage $baselineRoot "0.0.9" $baselineDestination
        Build-TestPackage $Script:Root "0.1.0" $candidateDestination
        Build-TestPackage $Script:Root "0.1.0" $failureDestination -InjectUpgradeFailure
    } finally {
        $previousErrorActionPreference = $ErrorActionPreference
        try {
            $ErrorActionPreference = "Continue"
            & git -c core.longpaths=true -C $Script:Root worktree remove --force $baselineRoot 2>$null
        } finally {
            $ErrorActionPreference = $previousErrorActionPreference
        }
        if (Test-Path -LiteralPath $baselineRoot) {
            $longBaselineRoot = "\\?\$baselineFull"
            [IO.Directory]::Delete($longBaselineRoot, $true)
        }
        if (Test-Path -LiteralPath $baselineRoot) {
            throw "baseline worktree cleanup failed"
        }
    }
    Write-PhaseReport "build" ([ordered]@{
        baseline_revision = "6b23686"
        baseline_version = "0.0.9"
        baseline_path = Get-RelativeEvidencePath $baselineDestination
        baseline_sha256 = Get-FileSha256 $baselineDestination
        candidate_revision = (& git -C $Script:Root rev-parse HEAD).Trim()
        candidate_source = $candidateSource
        candidate_version = "0.1.0"
        candidate_path = Get-RelativeEvidencePath $candidateDestination
        candidate_sha256 = Get-FileSha256 $candidateDestination
        failure_injection_path = Get-RelativeEvidencePath $failureDestination
        failure_injection_sha256 = Get-FileSha256 $failureDestination
        failure_injection = "post-payload-pre-service-install"
        signature = "unsigned-test"
        release_allowed = $false
    })
}

function Invoke-Install {
    Assert-SystemChangesAllowed
    $package = Assert-Package $BaselinePackage "baseline"
    Assert-Clean | Out-Null
    Invoke-Installer $package
    Wait-Until { (Get-ServiceState).running } 20 "OrangeDataPlane did not start after installation"
    Assert-Installed
    $identityHash = Get-InstallationIdentityHash
    $fileHashes = Get-InstalledFileHashes
    Start-Process -FilePath (Join-Path $Script:InstallRoot "orange-app.exe") | Out-Null
    Write-PhaseReport "install" ([ordered]@{
        package_sha256 = Get-FileSha256 $package
        identity_sha256 = $identityHash
        service = Get-ServiceState
        installation_policy = Get-InstallationPolicyState
        firewall = Get-FirewallState
        installed_files = $fileHashes
        app_launched = $true
    })
}

function Invoke-IpcBoundaryAcceptance {
    Assert-SystemChangesAllowed
    Read-PhaseReport "install" | Out-Null
    Assert-Installed
    $installationId = Get-InstallationId
    $pipeName = "Orange.DataPlane.$installationId.v1"
    $differentUserCommand = Get-PipeProbeCommand $pipeName
    $lowIntegrityCommand = Get-PipeProbeCommand $pipeName -RequireLowIntegrity
    $differentUserDenied = Invoke-DifferentUserPipeProbe $differentUserCommand
    $lowIntegrityDenied = Invoke-LowIntegrityPipeProbe $lowIntegrityCommand
    Assert-Installed
    Write-PhaseReport "ipc-boundary" ([ordered]@{
        different_user_process = "independent-local-user"
        different_user_denied = $differentUserDenied
        temporary_user_removed = $true
        low_integrity_process = "low-mandatory-level"
        low_integrity_denied = $lowIntegrityDenied
        service_remained_running = (Get-ServiceState).running
        policy = Get-InstallationPolicyState
    })
}

function Invoke-ProxyAcceptance {
    Read-PhaseReport "install" | Out-Null
    $preflight = Read-PhaseReport "preflight"
    if ([string]::IsNullOrWhiteSpace([string]$preflight.observations.direct_exit_sha256)) {
        throw "preflight must record the approved exit probe before proxy acceptance"
    }
    $proxy = Get-ProxyState
    if (-not $proxy.enabled -or -not $proxy.orange_server -or -not (Test-ProxyListener)) {
        throw "system proxy mode is not active on the fixed loopback listener"
    }
    $exitHash = Invoke-ExitProbe "proxy"
    if ($exitHash -eq $preflight.observations.direct_exit_sha256) {
        throw "system proxy mode did not change the observed exit"
    }
    Write-PhaseReport "proxy" ([ordered]@{
        proxy = $proxy
        listener_ready = $true
        exit_changed = $true
        exit_sha256 = $exitHash
        service = Get-ServiceState
        process_count = @(Get-OrangeProcesses).Count
    })
}

function Invoke-TunAcceptance {
    Read-PhaseReport "install" | Out-Null
    $preflight = Read-PhaseReport "preflight"
    if ([string]::IsNullOrWhiteSpace([string]$preflight.observations.direct_exit_sha256)) {
        throw "preflight must record the approved exit probe before TUN acceptance"
    }
    $proxy = Get-ProxyState
    $tun = Get-TunState
    $routes = Get-RouteState
    if ($proxy.enabled -or -not $tun.present -or -not $tun.up -or -not $tun.ipv4 -or -not $tun.ipv6 -or
        $routes.tun_route_count -eq 0) {
        throw "TUN mode does not have the fixed interface contract or still owns WinINET proxy"
    }
    $exitHash = Invoke-ExitProbe "direct"
    if ($exitHash -eq $preflight.observations.direct_exit_sha256) {
        throw "TUN mode did not change the observed exit"
    }
    Write-PhaseReport "tun" ([ordered]@{
        proxy_disabled = $true
        tun = $tun
        dns = Get-DnsState
        routes = $routes
        exit_changed = $true
        exit_sha256 = $exitHash
        service = Get-ServiceState
        process_count = @(Get-OrangeProcesses).Count
    })
}

function Invoke-CrashAcceptance {
    Assert-SystemChangesAllowed
    Read-PhaseReport "install" | Out-Null
    $name = switch ($CrashTarget) {
        "ui" { "orange-app.exe" }
        "control-plane" { "orange-control-plane.exe" }
        "data-plane" { "orange-data-plane.exe" }
        "service" { "orange-service.exe" }
    }
    $targets = @(Get-CimInstance Win32_Process | Where-Object {
        if ($_.Name -ne $name -or -not $_.ExecutablePath) {
            return $false
        }
        $executablePath = Get-NormalizedWindowsPath ([string]$_.ExecutablePath)
        return $null -ne $executablePath -and
            $executablePath.StartsWith($Script:InstallRoot + "\", [StringComparison]::OrdinalIgnoreCase)
    })
    if ($targets.Count -eq 0) {
        throw "crash target is not running: $CrashTarget"
    }
    $targets | ForEach-Object { Stop-Process -Id $_.ProcessId -Force }
    Start-Sleep -Seconds 5
    $proxy = Get-ProxyState
    $tun = Get-TunState
    $listener = Test-ProxyListener
    if ($proxy.enabled -and -not $listener) {
        throw "crash left an enabled system proxy without a listener"
    }
    if ($CrashTarget -eq "ui" -and $proxy.enabled) {
        throw "UI crash watchdog did not restore the system proxy"
    }
    if ($tun.present) {
        Invoke-ExitProbe "direct" | Out-Null
    }
    Write-PhaseReport ("crash-" + $CrashTarget) ([ordered]@{
        target = $CrashTarget
        terminated_processes = $targets.Count
        proxy = $proxy
        proxy_listener_ready = $listener
        tun = $tun
        service = Get-ServiceState
        remaining_process_count = @(Get-OrangeProcesses).Count
        network_state_safe = $true
    })
}

function Stop-InstalledUserProcesses {
    $targets = @(Get-CimInstance Win32_Process | Where-Object {
        if ($_.Name -notin @("orange-app.exe", "orange-control-plane.exe") -or
            -not $_.ExecutablePath) {
            return $false
        }
        $path = Get-NormalizedWindowsPath ([string]$_.ExecutablePath)
        return $null -ne $path -and
            $path.StartsWith($Script:InstallRoot + "\", [StringComparison]::OrdinalIgnoreCase)
    })
    $targets | ForEach-Object { Stop-Process -Id $_.ProcessId -Force }
    if ($targets.Count -ne 0) {
        Wait-Until { @(Get-OrangeProcesses | Where-Object {
            $_.name -in @("orange-app.exe", "orange-control-plane.exe")
        }).Count -eq 0 } 10 "installed Orange user processes did not exit"
    }
    return $targets.Count
}

function Repair-BaselineInstallation([string]$Package, [Collections.IDictionary]$ExpectedFiles) {
    Invoke-Installer $Package
    Wait-Until { (Get-ServiceState).running } 20 "OrangeDataPlane did not restart during baseline repair"
    $backup = Join-Path $Script:InstallRoot ".orange-upgrade-backup"
    if (Test-Path -LiteralPath $backup) {
        $resolved = [IO.Path]::GetFullPath($backup)
        $expected = [IO.Path]::GetFullPath(
            (Join-Path $Script:InstallRoot ".orange-upgrade-backup")
        )
        if (-not $resolved.Equals($expected, [StringComparison]::OrdinalIgnoreCase)) {
            throw "upgrade backup cleanup escaped the fixed installation path"
        }
        [IO.Directory]::Delete($resolved, $true)
    }
    $actualFiles = Get-InstalledFileHashes
    foreach ($name in $ExpectedFiles.Keys) {
        if ($actualFiles[$name] -ne $ExpectedFiles[$name]) {
            throw "baseline repair did not restore $name"
        }
    }
    Assert-Installed
}

function Invoke-UpgradeFailureAcceptance {
    Assert-SystemChangesAllowed
    Read-PhaseReport "install" | Out-Null
    $baselinePackage = Assert-Package $BaselinePackage "baseline"
    $failurePackage = Assert-Package $FailurePackage "upgrade failure injection"
    Assert-Installed
    $filesBefore = Get-InstalledFileHashes
    $identityBefore = Get-InstallationIdentityHash
    $revisionPath = Join-Path $Script:InstallRoot "data-plane\revisions\active-revision.v1"
    $revisionBefore = if (Test-Path -LiteralPath $revisionPath -PathType Leaf) {
        Get-FileSha256 $revisionPath
    } else {
        $null
    }
    $displayVersionBefore = [string](Get-ItemPropertyValue `
        -LiteralPath "HKLM:\Software\Microsoft\Windows\CurrentVersion\Uninstall\Orange" `
        -Name DisplayVersion)
    $stoppedUserProcesses = Stop-InstalledUserProcesses
    $process = $null
    try {
        $process = Start-Process -FilePath $failurePackage -ArgumentList "/S" `
            -WindowStyle Hidden -PassThru
        if (-not $process.WaitForExit(120000)) {
            Stop-Process -Id $process.Id -Force -ErrorAction SilentlyContinue
            $process.WaitForExit(5000) | Out-Null
            throw "upgrade failure injection installer timed out"
        }
        if ($process.ExitCode -eq 0) {
            throw "upgrade failure injection unexpectedly succeeded"
        }
        Wait-Until { (Get-ServiceState).running } 20 "previous service was not restored after upgrade failure"
        Assert-Installed
        $filesAfter = Get-InstalledFileHashes
        foreach ($name in $filesBefore.Keys) {
            if ($filesAfter[$name] -ne $filesBefore[$name]) {
                throw "upgrade failure rollback did not restore $name"
            }
        }
        $identityAfter = Get-InstallationIdentityHash
        if ($identityAfter -ne $identityBefore) {
            throw "upgrade failure rollback replaced the installation identity"
        }
        $revisionAfter = if (Test-Path -LiteralPath $revisionPath -PathType Leaf) {
            Get-FileSha256 $revisionPath
        } else {
            $null
        }
        if ($revisionAfter -ne $revisionBefore) {
            throw "upgrade failure rollback changed the active revision marker"
        }
        $displayVersionAfter = [string](Get-ItemPropertyValue `
            -LiteralPath "HKLM:\Software\Microsoft\Windows\CurrentVersion\Uninstall\Orange" `
            -Name DisplayVersion)
        if ($displayVersionAfter -ne $displayVersionBefore) {
            throw "upgrade failure rollback changed the installed display version"
        }
    } catch {
        Repair-BaselineInstallation $baselinePackage $filesBefore
        throw
    }
    Start-Process -FilePath (Join-Path $Script:InstallRoot "orange-app.exe") | Out-Null
    Write-PhaseReport "upgrade-failure" ([ordered]@{
        package_sha256 = Get-FileSha256 $failurePackage
        injection_point = "post-payload-pre-service-install"
        installer_failed = $true
        installer_exit_code = $process.ExitCode
        previous_files_restored = $true
        identity_preserved = $true
        active_revision_preserved = $true
        display_version_preserved = $true
        service_restored = $true
        upgrade_backup_removed = $true
        stopped_user_processes = $stoppedUserProcesses
        release_allowed = $false
    })
}

function Invoke-Upgrade {
    Assert-SystemChangesAllowed
    $install = Read-PhaseReport "install"
    $package = Assert-Package $CandidatePackage "candidate"
    $identityBefore = Get-InstallationIdentityHash
    if ($identityBefore -ne $install.observations.identity_sha256) {
        throw "installation identity changed before upgrade"
    }
    $revisionPath = Join-Path $Script:InstallRoot "data-plane\revisions\active-revision.v1"
    $revisionBefore = if (Test-Path -LiteralPath $revisionPath -PathType Leaf) { Get-FileSha256 $revisionPath } else { $null }
    Invoke-Installer $package
    Wait-Until { (Get-ServiceState).running } 20 "OrangeDataPlane did not start after upgrade"
    Assert-Installed
    $identityAfter = Get-InstallationIdentityHash
    if ($identityAfter -ne $identityBefore) {
        throw "upgrade replaced the installation identity"
    }
    $revisionAfter = if (Test-Path -LiteralPath $revisionPath -PathType Leaf) { Get-FileSha256 $revisionPath } else { $null }
    if ($null -ne $revisionBefore -and $revisionAfter -ne $revisionBefore) {
        throw "upgrade did not preserve the active revision marker"
    }
    Start-Process -FilePath (Join-Path $Script:InstallRoot "orange-app.exe") | Out-Null
    Write-PhaseReport "upgrade" ([ordered]@{
        package_sha256 = Get-FileSha256 $package
        identity_preserved = $true
        active_revision_preserved = ($revisionBefore -eq $revisionAfter)
        service = Get-ServiceState
        firewall_present = Test-FirewallRule
        installed_files = Get-InstalledFileHashes
        release_allowed = $false
    })
}

function Invoke-Uninstall {
    Assert-SystemChangesAllowed
    $build = Read-PhaseReport "build"
    $candidateInput = if ([string]::IsNullOrWhiteSpace($CandidatePackage)) {
        $recorded = [string]$build.observations.candidate_path
        if ([IO.Path]::IsPathRooted($recorded)) { $recorded } else { Join-Path $Script:Root $recorded }
    } else {
        $CandidatePackage
    }
    $candidate = Assert-Package $candidateInput "candidate"
    if ((Get-FileSha256 $candidate) -ne [string]$build.observations.candidate_sha256) {
        throw "candidate package does not match the completed build phase"
    }
    Invoke-ProductionSecretStoreProbe "complete"
    Initialize-UninstallRetentionMarkers

    $uninstaller = Join-Path $Script:InstallRoot "uninstall.exe"
    if (-not (Test-Path -LiteralPath $uninstaller -PathType Leaf)) {
        throw "Orange uninstaller is missing"
    }
    $process = Start-Process -FilePath $uninstaller -ArgumentList "/S" -Wait -PassThru
    if ($process.ExitCode -ne 0) {
        throw "uninstaller failed with exit code $($process.ExitCode)"
    }
    Wait-Until { -not (Test-Path -LiteralPath $Script:InstallRoot) } 30 "Orange install root remains after uninstall"
    $defaultClean = Assert-Clean
    Assert-AppDataRetained
    Invoke-ProductionSecretStoreProbe "empty"

    Invoke-Installer $candidate
    Wait-Until { (Get-ServiceState).running } 20 "OrangeDataPlane did not start after retention reinstall"
    Assert-Installed
    $uninstaller = Join-Path $Script:InstallRoot "uninstall.exe"
    $process = Start-Process -FilePath $uninstaller -ArgumentList "/S /DELETEAPPDATA" -Wait -PassThru
    if ($process.ExitCode -ne 0) {
        throw "explicit application-data uninstaller failed with exit code $($process.ExitCode)"
    }
    Wait-Until { -not (Test-Path -LiteralPath $Script:InstallRoot) } 30 "Orange install root remains after explicit application-data uninstall"
    Assert-AppDataRemoved
    Invoke-ProductionSecretStoreProbe "empty"
    $explicitClean = Assert-Clean
    Write-PhaseReport "uninstall" ([ordered]@{
        package_sha256 = Get-FileSha256 $candidate
        default_preserved_roaming_app_data = $true
        default_preserved_local_app_data = $true
        explicit_delete_removed_roaming_app_data = $true
        explicit_delete_removed_local_app_data = $true
        credentials_removed_when_settings_retained = $true
        candidate_reinstalled_between_choices = $true
        default_cleanup = $defaultClean
        explicit_cleanup = $explicitClean
    })
}

function Invoke-VerifyClean {
    $clean = Assert-Clean
    Assert-AppDataRemoved
    Invoke-ProductionSecretStoreProbe "empty"
    Write-PhaseReport "verify-clean" $clean
}

Assert-Windows
switch ($Phase) {
    "preflight" { Invoke-Preflight }
    "build" { Invoke-Build }
    "install" { Invoke-Install }
    "ipc-boundary" { Invoke-IpcBoundaryAcceptance }
    "proxy" { Invoke-ProxyAcceptance }
    "tun" { Invoke-TunAcceptance }
    "crash" { Invoke-CrashAcceptance }
    "upgrade-failure" { Invoke-UpgradeFailureAcceptance }
    "upgrade" { Invoke-Upgrade }
    "uninstall" { Invoke-Uninstall }
    "verify-clean" { Invoke-VerifyClean }
}
