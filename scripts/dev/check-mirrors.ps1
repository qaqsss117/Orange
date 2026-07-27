$ErrorActionPreference = "Stop"

$expectedEnvironment = [ordered]@{
    NPM_CONFIG_REGISTRY = "https://registry.npmmirror.com/"
    COREPACK_NPM_REGISTRY = "https://registry.npmmirror.com/"
    NVM_NODEJS_ORG_MIRROR = "https://npmmirror.com/mirrors/node/"
    NVM_NPM_MIRROR = "https://npmmirror.com/mirrors/npm/"
    RUSTUP_DIST_SERVER = "https://rsproxy.cn"
    RUSTUP_UPDATE_ROOT = "https://rsproxy.cn/rustup"
    GOPROXY = "https://goproxy.cn"
    GOSUMDB = "sum.golang.google.cn"
}

$errors = @()
foreach ($entry in $expectedEnvironment.GetEnumerator()) {
    $actual = [Environment]::GetEnvironmentVariable($entry.Key, "User")
    if ($actual -ne $entry.Value) {
        $errors += "$($entry.Key): expected '$($entry.Value)', got '$actual'"
    }
}

$projectRegistry = npm config get registry --location=project
if ($LASTEXITCODE -ne 0 -or $projectRegistry.Trim() -ne "https://registry.npmmirror.com/") {
    $errors += "npm project registry is not npmmirror"
}

$goProxy = go env GOPROXY
$goSumDb = go env GOSUMDB
if ($goProxy.Trim() -ne $expectedEnvironment.GOPROXY) {
    $errors += "Go GOPROXY is not goproxy.cn"
}
if ($goSumDb.Trim() -ne $expectedEnvironment.GOSUMDB) {
    $errors += "Go GOSUMDB is not sum.golang.google.cn"
}

$gradleInit = Join-Path $HOME ".gradle\init.d\orange-domestic-mirrors.gradle"
if (-not (Test-Path -LiteralPath $gradleInit)) {
    $errors += "Gradle domestic mirror init script is missing"
}

$endpoints = @(
    "https://registry.npmmirror.com/-/ping",
    "https://rsproxy.cn/index/config.json",
    "https://goproxy.cn/github.com/gorilla/websocket/@v/list",
    "https://maven.aliyun.com/repository/public/org/jetbrains/kotlin/kotlin-stdlib/maven-metadata.xml",
    "https://mirrors.cloud.tencent.com/gradle/"
)
foreach ($endpoint in $endpoints) {
    try {
        $response = Invoke-WebRequest -Uri $endpoint -Method Head -TimeoutSec 20 -UseBasicParsing
        if ($response.StatusCode -lt 200 -or $response.StatusCode -ge 400) {
            $errors += "Mirror returned HTTP $($response.StatusCode): $endpoint"
        }
    } catch {
        $errors += "Mirror unavailable: $endpoint ($($_.Exception.Message))"
    }
}

if ($errors.Count -gt 0) {
    $errors | ForEach-Object { Write-Error $_ }
    exit 1
}

Write-Host "Domestic mirror configuration verified."
