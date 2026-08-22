# Bootstrap、OSS、TXT 与更新发布手册

本文说明 Orange 的 bootstrap 回退和 Android 自更新如何配置 GitHub Actions、
OSS 与 Cloudflare DNS，以及发布前如何做故障测试。

## 配置放在哪里

GitHub 仓库进入 `Settings -> Secrets and variables -> Actions`：

- 所有配置均放在 `Variables`，包括地址、版本、公钥、bootstrap 明文、加密密钥、
  Ed25519 私钥和平台签名材料。
- GitHub Variables 不具备 Secrets 的加密存储和日志自动脱敏能力；必须限制仓库管理
  权限与 workflow 修改权限，且 workflow 不得输出这些敏感值。
- OSS/TXT URL 使用分号分隔，不要使用 JSON 数组，也不要在分号两侧加引号。
- URL 必须是 HTTPS 443，不允许跳转、用户信息、IP 私网地址或 URL fragment。
- TXT 名称只填写完整 DNS 名称，不包含 `https://`，也不要使用下划线。

Cloudflare TXT 的实际内容不放进 GitHub Variable。Actions 会生成
`bootstrap.txt` 和 `android-update.txt`，发布人员将文件中的完整单行内容分别
写入 Cloudflare TXT 记录。

## GitHub Actions Variables

### Bootstrap 发现与签名

| Variable | 示例格式 | 用途 |
| --- | --- | --- |
| `ORANGE_BOOTSTRAP_CHANNEL` | `production` | 远程配置渠道，客户端严格匹配 |
| `ORANGE_BOOTSTRAP_PRODUCT_VERSION` | `0.1.0` | 本次构建版本，必须与 Cargo 应用版本一致 |
| `ORANGE_BOOTSTRAP_KEY_ID` | `bootstrap-enc-2026-01` | XChaCha20 加密密钥的非敏感 ID |
| `ORANGE_BOOTSTRAP_SIGNING_KEY_ID` | `bootstrap-sign-2026-01` | 当前 Ed25519 签名密钥 ID |
| `ORANGE_BOOTSTRAP_SIGNING_PUBLIC_KEYS` | `currentId=BASE64URL;nextId=BASE64URL` | 当前及下一把 Ed25519 公钥，必须为 2 至 4 把且互不重复 |
| `ORANGE_BOOTSTRAP_MINIMUM_CLIENT_VERSION` | `0.1.0` | 接受远程配置的最低客户端 SemVer |
| `ORANGE_BOOTSTRAP_MANIFEST_URLS` | `https://oss-a.example.com/orange/bootstrap/v42/manifest.json;https://oss-b.example.net/orange/bootstrap/v42/manifest.json` | 编译进客户端的 2 至 4 个首层 OSS manifest 地址 |
| `ORANGE_BOOTSTRAP_ENVELOPE_URLS` | `https://oss-a.example.com/orange/bootstrap/v42/bootstrap.enc;https://oss-b.example.net/orange/bootstrap/v42/bootstrap.enc` | 与 manifest URL 按顺序一一对应的密文地址 |
| `ORANGE_BOOTSTRAP_TXT_NAMES` | `bootstrap-a.example.com;bootstrap-b.example.net` | 编译进客户端的 2 至 4 个 Cloudflare TXT 查询名 |
| `ORANGE_BOOTSTRAP_TXT_MANIFEST_URLS` | 一至四个 rescue OSS manifest URL | 写入签名 TXT 的第二层 OSS 地址，必须与包内 URL 不同 |
| `ORANGE_BOOTSTRAP_TXT_ENVELOPE_URLS` | 一至四个 rescue OSS 密文 URL | 与 TXT manifest URL 按顺序一一对应 |
| `ORANGE_BOOTSTRAP_TXT_SEQUENCE` | `42` | TXT 防回滚序号，每次改变 TXT 内容必须递增 |
| `ORANGE_BOOTSTRAP_TXT_EXPIRES_AT_UNIX` | `1798761600` | TXT 到期时间，Unix 秒 |

包内 manifest/envelope 两组变量数量必须相同，TXT manifest/envelope 两组变量
数量也必须相同。包内与 TXT rescue manifest URL 必须完全不同，否则首层 OSS 被
封后第二层没有回退价值。每份 manifest 的签名都绑定其对应密文 URL。

### Windows 与 Android

| Variable | 示例格式 | 用途 |
| --- | --- | --- |
| `ORANGE_WINDOWS_STORE_PRODUCT_ID` | Microsoft Store Product ID | Windows 设置页跳转目标；正式 Store 构建必填 |
| `ORANGE_WINDOWS_STORE_IDENTITY_NAME` | Partner Center package identity name | MSIX `Identity Name`；正式 Store 构建必填 |
| `ORANGE_WINDOWS_STORE_PUBLISHER` | Partner Center publisher subject | MSIX `Identity Publisher`；必须与 Partner Center 完全一致 |
| `ORANGE_WINDOWS_STORE_DISPLAY_NAME` | Store display name | MSIX 清单展示名称 |
| `ORANGE_WINDOWS_MSIX_VERSION` | `1.2.3.0` | MSIX 四段版本；不填时从 `v*` tag 推导 |
| `ORANGE_WINDOWS_STORE_TENANT_ID` | Entra tenant ID | Store Developer CLI 认证 |
| `ORANGE_WINDOWS_STORE_SELLER_ID` | Partner Center seller ID | Store Developer CLI 认证 |
| `ORANGE_WINDOWS_STORE_CLIENT_ID` | Entra application/client ID | Store Developer CLI 认证 |
| `ORANGE_WINDOWS_STORE_CLIENT_SECRET` | Entra client secret | Store Developer CLI 认证 |
| `ORANGE_ANDROID_PACKAGE_ID` | `com.example.orange` | 固定生产包名，发布后不可更换 |
| `ORANGE_ANDROID_VERSION_CODE` | `42` | APK/AAB 递增整数版本 |
| `ORANGE_ANDROID_VERSION_NAME` | `0.1.0` | Android 展示版本 |
| `ORANGE_ANDROID_SIGNING_CERT_SHA256` | 64 位十六进制 | 固定生产签名证书 SHA-256 摘要 |
| `ORANGE_ANDROID_UPDATE_MANIFEST_URLS` | 两至四个 HTTPS URL，以分号分隔 | 编译进 Android 客户端的更新 manifest 地址 |
| `ORANGE_ANDROID_UPDATE_TXT_NAMES` | `android-update-a.example.com;android-update-b.example.net` | Android 更新的 TXT 查询名 |
| `ORANGE_ANDROID_UPDATE_TXT_MANIFEST_URLS` | 一至四个 HTTPS rescue URL | 写入 Android 更新 TXT，必须与包内 manifest URL 不同 |
| `ORANGE_ANDROID_APK_MIRROR_URLS` | 两至四个 HTTPS APK URL，以分号分隔 | APK 下载镜像；所有镜像必须提供同一个 APK |
| `ORANGE_ANDROID_UPDATE_EXPIRES_AT_UNIX` | Unix 秒 | Android 更新 manifest 和 TXT 的到期时间 |
| `ORANGE_ANDROID_UPDATE_TXT_SEQUENCE` | `42` | Android 更新 TXT 防回滚序号，修改 TXT 时递增 |

bootstrap TXT 和 Android 更新 TXT 是两套独立记录，不能混用。两者当前都使用
`orange-bootstrap-v1:` 格式，但签名内容中的 URL 集合不同。

## 敏感 GitHub Actions Variables

本功能直接使用以下敏感 Variables：

| Variable | 内容 |
| --- | --- |
| `ORANGE_BOOTSTRAP_BUILD_KEY_HEX` | 32 字节 XChaCha20 密钥，编码为 64 位十六进制 |
| `ORANGE_BOOTSTRAP_CONFIG_JSON` | 完整 bootstrap v2 明文，至少含 2 个代理候选和 2 个 API host |
| `ORANGE_BOOTSTRAP_SIGNING_KEY_HEX` | 当前 Ed25519 私钥种子，32 字节、64 位十六进制；必须匹配当前公钥 |

Android 正式构建还需要现有的 `ANDROID_KEYSTORE_BASE64`、
`ANDROID_KEYSTORE_PASSWORD`、`ANDROID_KEY_ALIAS` 和 `ANDROID_KEY_PASSWORD`。
Windows MSIX 正式构建不需要 Windows PFX 或 Authenticode thumbprint，微软商店
会对提交的 MSIX 包重新签名。Tauri 包签名使用 `TAURI_SIGNING_PRIVATE_KEY` 和
`TAURI_SIGNING_PRIVATE_KEY_PASSWORD`；Apple 发布还需要 `APPLE_API_PRIVATE_KEY`、
macOS 应用/安装器证书及其密码。这些值虽然按当前仓库策略放入 Variables，仍不得
写入仓库文件、Actions artifact、OSS 或 workflow 日志。

## OSS 对象布局与上传映射

建议所有对象使用不可变版本目录，例如 `v42`，禁止在 CDN/OSS 上通过 301/302
跳转到新版本。一次 Android Actions 构建会产出：

| 本地/Actions 产物 | 发布目标 |
| --- | --- |
| `artifacts/bootstrap/release/bootstrap.enc` | 上传到包内与 TXT 两组 envelope URL 中的每个 URL |
| `bootstrap.remote.manifest.hardcoded.N.json` | 上传到第 N 个 `ORANGE_BOOTSTRAP_MANIFEST_URLS` |
| `bootstrap.remote.manifest.rescue.N.json` | 上传到第 N 个 `ORANGE_BOOTSTRAP_TXT_MANIFEST_URLS` |
| `artifacts/android/android-update-manifest.json` | 上传到包内与 TXT rescue 两组 Android manifest URL 中的每个 URL |
| 签名 release APK | 上传到每个 `ORANGE_ANDROID_APK_MIRROR_URLS` |

`.github/workflows/package.yml` 当前负责生成并保存这些 Actions artifact，但不掌握
具体云厂商凭据，因此不会自动上传 OSS 或修改 Cloudflare。OSS 上传应由发布系统
或单独的受保护部署 job 完成，并使用对应云厂商的短期身份凭据。

OSS 必须返回对象本身，不得跳转；不能开启需要 Cookie、Referer、签名查询参数或
登录态的防盗链。客户端会校验 TLS、响应大小、签名、密文 SHA-256 和 APK
SHA-256，因此 OSS 压缩或内容改写也应关闭。

## Cloudflare TXT 发布

先上传并验证所有 OSS 对象，再发布 TXT：

1. 在每个 Cloudflare zone 中添加 `TXT` 记录。
2. 记录名使用 `ORANGE_BOOTSTRAP_TXT_NAMES` 中对应的完整域名。
3. 记录值使用 `artifacts/bootstrap/release/bootstrap.txt` 的完整单行内容；其中只
   包含 `ORANGE_BOOTSTRAP_TXT_MANIFEST_URLS` 的 rescue 地址。
4. Android 更新记录名使用 `ORANGE_ANDROID_UPDATE_TXT_NAMES`。
5. Android 更新记录值使用 `artifacts/android/android-update.txt`。
6. 同一组中的多个 TXT 名称写入相同值，TTL 建议 60 至 300 秒。
7. Cloudflare 控制台可能自动添加引号或拆分长 TXT，这是允许的；不要手工删除
   `orange-bootstrap-v1:` 前缀，也不要添加额外空格或说明文字。

TXT 只负责在包内 OSS 地址全部失败时发现新 manifest URL。它不承载代理明文、
API token、私钥或 APK。

可用固定 resolver 检查解析结果：

```powershell
curl.exe --fail --silent --show-error `
  --resolve cloudflare-dns.com:443:1.1.1.1 `
  -H "accept: application/dns-json" `
  "https://cloudflare-dns.com/dns-query?name=bootstrap-a.example.com&type=TXT"
```

## 正确发布顺序

1. 提高 bootstrap `configurationVersion`，设置所有到期时间和 TXT sequence。
2. 触发 `package` workflow，确认所有平台构建和资源门禁通过。
3. 从 `orange-android` Actions artifact 取得远程 bootstrap、Android manifest、TXT
   和签名 APK；不要使用工作站上历史生成的文件。
4. 上传所有 `bootstrap.enc`、逐一对应的 bootstrap manifest、APK 和 Android
   update manifest。
5. 从外网确认所有 URL 为 HTTPS 443、200、无跳转且对象哈希一致。
6. 最后更新两组 Cloudflare TXT；TXT sequence 必须高于客户端已接受的旧值。
7. 完成下述故障测试后，再提交 Windows Store/AAB 或向 APK 用户开放更新。

更新失败时不要覆盖旧对象。使用新的版本目录重新生成、上传、验证，最后切换
TXT；包内硬编码 URL 只能随下一版 Windows Store 或 Android 安装包更新。

## 测试流程

### 1. 静态和构建测试

```powershell
cargo test --workspace
Push-Location native/controlplane
go test ./...
Pop-Location
pnpm build
git diff --check
```

使用 staging 的 GitHub Variables 执行一次完整 `workflow_dispatch`，确认：

- 至少生成 2 个 `bootstrap.remote.manifest.hardcoded.N.json` 和 1 个 rescue manifest；
- 生成 `bootstrap.txt`、`android-update-manifest.json` 和 `android-update.txt`；
- Windows 缺失 Store Product ID 时构建失败；
- APK 中包含嵌入 bootstrap，APK 包名、versionCode 和证书摘要与变量一致。

### 2. 发布物检查

对每个 OSS URL 执行真实 GET，不能使用会掩盖跳转的 `-L`：

```powershell
curl.exe --fail --show-error --max-redirs 0 --output downloaded.bin `
  "https://oss-a.example.com/orange/bootstrap/v42/bootstrap.enc"
Get-FileHash downloaded.bin -Algorithm SHA256
```

两个 OSS 上的 `bootstrap.enc` 哈希必须相同；多个 APK 镜像的 APK 哈希也必须
相同。查询两组 TXT，确认 DNS 返回值与 Actions 生成文件完全一致。

### 3. 四层回退故障注入

每个场景都应使用干净的 staging 用户和可区分的 OSS 访问日志：

| 场景 | 注入方式 | 预期结果 |
| --- | --- | --- |
| 首层 OSS 正常 | TXT 指向不同 rescue URL，包内 OSS 可访问 | 从硬编码 OSS 启动并在健康检查后写入 LKG |
| 单 OSS 失败 | 阻断第 1 个 manifest/envelope，只保留第 2 个 | 4 秒预算内由第 2 个 OSS 启动 |
| 包内 OSS 全失败 | 阻断所有硬编码 URL，TXT 指向可用 rescue OSS | 经固定 DoH 获取签名 TXT，再从 rescue OSS 启动 |
| 远端全失败、缓存正常 | 先成功启动一次，再阻断 OSS、rescue OSS 和 DoH | 使用未过期 LKG 启动 |
| 仅内置可用 | 清空应用数据/缓存并阻断所有远端 | 使用安装包内置配置启动 |
| 全部失效 | 远端全阻断、清空缓存、令内置代理或 API 健康检查失败 | 显示“服务连接不可用”，不得直连 API |

Windows 清缓存测试应只删除测试用户应用数据中的 bootstrap cache；Android 使用
`adb shell pm clear <staging-package-id>` 清除 staging 包数据。不要对生产用户或
生产包执行清除操作。

### 4. 代理与 API 轮询

1. 配置代理 A 失败、代理 B 正常，确认业务 GET 经 B 成功。
2. 恢复 A，等待熔断冷却，确认 A 能重新进入候选池。
3. 配置 API A 失败、API B 正常，确认在全局超时及 `maxAttempts` 内切换。
4. 对 GET 在连接前、TLS、请求写入后分别断开，确认允许的传输失败会切换。
5. 对下单、兑换、提交工单等 POST 在服务端收到请求后断开响应，服务端计数必须
   仍为 1，客户端不得向另一组合重复提交。
6. 抓包确认业务 API 只出现在 bootstrap outbound 内；代理池全失效时不能看到
   后端 API 的裸 TLS 连接。

### 5. 签名和回滚测试

分别发布到 staging rescue 路径并确认客户端拒绝：

- 修改 manifest/TXT 任意字节但保留原签名；
- 修改 `bootstrap.enc` 或 APK 任意字节；
- 降低 configuration version 或 TXT sequence；
- 同 version 发布不同密文；
- 使用错误渠道、已过期时间或过高 minimum client version；
- 使用 HTTPS 非 443、重定向、私网 IP、localhost 或带用户信息的 URL；
- 使用未知签名 key ID，再使用预置的下一把公钥完成一次正常密钥轮换。

### 6. 平台验收

- Windows：无 Product ID 的正式 Store 构建必须失败；有 Product ID 时设置页只能
  打开正确 Store 产品页，不出现 GitHub/Tauri 自动更新。
- Android API 24、29、33、35：测试正常下载、未知来源授权、用户取消、安装成功、
  错误 SHA-256、错误证书、错误包名和降级 versionCode；取消、失败或 30 分钟超时
  后缓存 APK 必须被清理。
- Android 自动检查：首次启动检查一次，24 小时内重启不重复后台检查；设置页手动
  检查不受频率限制。
- 最终对两端执行端口扫描和崩溃日志检查，确认 Control Plane 没有监听端口，日志
  不包含代理凭据、token 或完整 bootstrap 配置。
