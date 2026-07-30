[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [ValidateSet("preflight", "build", "install", "proxy", "tun", "crash", "upgrade", "uninstall", "verify-clean")]
    [string]$Phase,

    [string]$BaselinePackage,
    [string]$CandidatePackage,
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
    foreach ($name in @("Orange_0.0.9_x64-setup.exe", "Orange_0.1.0_x64-setup.exe")) {
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

function Invoke-Installer([string]$Path) {
    $process = Start-Process -FilePath $Path -ArgumentList "/S" -Wait -PassThru
    if ($process.ExitCode -ne 0) {
        throw "installer failed with exit code $($process.ExitCode)"
    }
}

function Get-InstalledFileHashes {
    $files = @("orange-app.exe", "orange-control-plane.exe", "orange-service.exe", "orange-installer.exe", "orange-data-plane.exe")
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

function Write-MergedTauriConfig([string]$Repository, [string]$Version, [string]$Destination) {
    $source = Join-Path $Repository "src-tauri\tauri.windows.test.conf.json"
    $config = Get-Content -LiteralPath $source -Raw -Encoding UTF8 | ConvertFrom-Json
    $config | Add-Member -NotePropertyName version -NotePropertyValue $Version -Force
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

function Build-TestPackage([string]$Repository, [string]$Version, [string]$Destination) {
    $previousProductVersion = [Environment]::GetEnvironmentVariable("ORANGE_BOOTSTRAP_PRODUCT_VERSION")
    try {
        Set-WorkspacePackageVersion $Repository $Version
        $env:ORANGE_BOOTSTRAP_PRODUCT_VERSION = $Version
        Invoke-Checked $Repository "pnpm" @("install", "--frozen-lockfile")
        Invoke-Checked $Repository "python" @("scripts/ci/run.py", "bootstrap-release")
        Invoke-Checked $Repository "python" @("scripts/ci/run.py", "windows-data-plane")
        Invoke-Checked $Repository "pnpm" @("prepare:windows-test")
        $configPath = Join-Path $Repository "artifacts\windows-acceptance-tauri-config.json"
        Write-MergedTauriConfig $Repository $Version $configPath
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
    $uninstaller = Join-Path $Script:InstallRoot "uninstall.exe"
    if (-not (Test-Path -LiteralPath $uninstaller -PathType Leaf)) {
        throw "Orange uninstaller is missing"
    }
    $process = Start-Process -FilePath $uninstaller -ArgumentList "/S" -Wait -PassThru
    if ($process.ExitCode -ne 0) {
        throw "uninstaller failed with exit code $($process.ExitCode)"
    }
    Wait-Until { -not (Test-Path -LiteralPath $Script:InstallRoot) } 30 "Orange install root remains after uninstall"
    $clean = Assert-Clean
    Write-PhaseReport "uninstall" $clean
}

function Invoke-VerifyClean {
    $clean = Assert-Clean
    Write-PhaseReport "verify-clean" $clean
}

Assert-Windows
switch ($Phase) {
    "preflight" { Invoke-Preflight }
    "build" { Invoke-Build }
    "install" { Invoke-Install }
    "proxy" { Invoke-ProxyAcceptance }
    "tun" { Invoke-TunAcceptance }
    "crash" { Invoke-CrashAcceptance }
    "upgrade" { Invoke-Upgrade }
    "uninstall" { Invoke-Uninstall }
    "verify-clean" { Invoke-VerifyClean }
}
