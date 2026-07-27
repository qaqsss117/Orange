$ErrorActionPreference = "Stop"

$requirements = [ordered]@{
    node = "22.22.0"
    pnpm = "11.9.0"
    rustc = "1.95.0"
    cargo = "1.95.0"
    go = "1.25.0"
    java = "17.0.17"
}

function Get-NumericVersion([string]$Text) {
    $match = [regex]::Match($Text, "\d+\.\d+\.\d+")
    if (-not $match.Success) {
        throw "Cannot parse version from '$Text'"
    }
    return [version]$match.Value
}

function Assert-Tool([string]$Name, [string]$Command, [string[]]$Arguments, [string]$Minimum) {
    if (-not (Get-Command $Command -ErrorAction SilentlyContinue)) {
        throw "$Name is not installed"
    }
    $previousErrorAction = $ErrorActionPreference
    $ErrorActionPreference = "Continue"
    $output = (& $Command @Arguments 2>&1 | Out-String).Trim()
    $exitCode = $LASTEXITCODE
    $ErrorActionPreference = $previousErrorAction
    if ($exitCode -ne 0) {
        throw "$Name version command failed with exit code $exitCode`: $output"
    }
    $actual = Get-NumericVersion $output
    if ($actual -lt [version]$Minimum) {
        throw "$Name $actual is older than required $Minimum"
    }
    Write-Host "$Name $actual"
}

Assert-Tool "Node.js" "node" @("--version") $requirements.node
Assert-Tool "pnpm" "pnpm" @("--version") $requirements.pnpm
Assert-Tool "Rust" "rustc" @("--version") $requirements.rustc
Assert-Tool "Cargo" "cargo" @("--version") $requirements.cargo
Assert-Tool "Go" "go" @("version") $requirements.go
Assert-Tool "Java" "java" @("-version") $requirements.java

if (-not $env:ANDROID_HOME -or -not (Test-Path -LiteralPath $env:ANDROID_HOME)) {
    throw "ANDROID_HOME is missing or invalid"
}

$requiredAndroidPaths = @(
    "platforms\android-36",
    "build-tools\36.0.0",
    "ndk\29.0.14206865",
    "cmake\3.22.1",
    "platform-tools"
)
foreach ($relative in $requiredAndroidPaths) {
    $path = Join-Path $env:ANDROID_HOME $relative
    if (-not (Test-Path -LiteralPath $path)) {
        throw "Missing Android component: $relative"
    }
    Write-Host "Android component $relative"
}

$sdkManager = Join-Path $env:ANDROID_HOME "cmdline-tools\latest\bin\sdkmanager.bat"
if (-not (Test-Path -LiteralPath $sdkManager)) {
    Write-Warning "Android command-line tools are missing; SDK updates and license automation are unavailable."
}

$requiredRustTargets = @(
    "aarch64-linux-android",
    "armv7-linux-androideabi",
    "i686-linux-android",
    "x86_64-linux-android",
    "x86_64-pc-windows-msvc"
)
$installedTargets = @(rustup target list --installed)
foreach ($target in $requiredRustTargets) {
    if ($target -notin $installedTargets) {
        throw "Missing Rust target: $target"
    }
    Write-Host "Rust target $target"
}

Write-Host "Toolchain preflight passed with one optional warning at most."
