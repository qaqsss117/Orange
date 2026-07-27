param(
    [switch]$Persist
)

$ErrorActionPreference = "Stop"

$mirrorVariables = [ordered]@{
    NPM_CONFIG_REGISTRY = "https://registry.npmmirror.com/"
    COREPACK_NPM_REGISTRY = "https://registry.npmmirror.com/"
    NVM_NODEJS_ORG_MIRROR = "https://npmmirror.com/mirrors/node/"
    NVM_NPM_MIRROR = "https://npmmirror.com/mirrors/npm/"
    RUSTUP_DIST_SERVER = "https://rsproxy.cn"
    RUSTUP_UPDATE_ROOT = "https://rsproxy.cn/rustup"
    GOPROXY = "https://goproxy.cn,direct"
    GOSUMDB = "sum.golang.google.cn"
}

foreach ($entry in $mirrorVariables.GetEnumerator()) {
    Set-Item -Path "Env:$($entry.Key)" -Value $entry.Value
    if ($Persist) {
        [Environment]::SetEnvironmentVariable($entry.Key, $entry.Value, "User")
    }
}

go env -w GOPROXY=$env:GOPROXY GOSUMDB=$env:GOSUMDB
if ($LASTEXITCODE -ne 0) {
    throw "Unable to configure Go mirrors"
}

$gradleInitDirectory = Join-Path $HOME ".gradle\init.d"
New-Item -ItemType Directory -Force -Path $gradleInitDirectory | Out-Null
Copy-Item -LiteralPath "gradle\orange-domestic-mirrors.gradle" -Destination (Join-Path $gradleInitDirectory "orange-domestic-mirrors.gradle") -Force

$scope = if ($Persist) { "current process and current user" } else { "current process" }
Write-Host "Configured domestic mirrors for $scope."
Write-Host "Installed Gradle mirror init script at $gradleInitDirectory."
