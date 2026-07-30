# 模块 01：安全与隐私

## 模块目标

把原 Android 工程视为不可信输入，以权限白名单、出网白名单、供应链清单和运行时审计确保 Orange 不复制相册上传、OCR、助记词识别或其他隐蔽采集行为。

## 安全红线

- 不读取、枚举、缓存或上传相册、截图、图片库和用户目录。
- 不包含 OCR、BIP-39/助记词匹配、键盘/剪贴板监听或后台文件遍历。
- 不通过日志、工单、崩溃、更新、远程配置或统计 SDK 绕过限制。
- 不执行原工程的 APK/AAR/SO/JAR/DEX、未知脚本或运行期下载的可执行文件。
- 控制面只能访问 allowlist 域名，业务 API 禁止直连回退。

## SEC-G0-001：不可信源隔离

**目标**：确保原工程只能被静态参考，不能进入 Orange 构建或运行链。

**范围**：仓库边界、导入规则、CI 扫描、迁移清单。

**依赖**：无。

**交付物**：`SECURITY.md`、`docs/migration-inventory.md`、禁止路径/文件规则、CI 扫描任务。

**验收规则**：

1. Orange 的依赖树、构建脚本和产物中不包含原工程模块路径、包名、Clash/mihomo 库或原预编译文件。
2. CI 对新增 `.so/.dll/.dylib/.exe/.jar/.aar/.dex` 默认失败，只有资源 manifest 明确登记后才允许。
3. 从全新 clone 构建 Orange 时不需要访问或执行 `../Android-kotlin-Code`。
4. `migration-inventory.md` 对每个参考页面/接口/资产标明“参考、重写、拒绝迁移”之一。
5. 人工检查一次构建日志，确认没有从原工程目录读取文件。

**非目标**：本切片不迁移任何 UI 或业务代码。

## SEC-G0-002：跨平台权限白名单

**目标**：每个平台只申请 VPN、网络和明确用户操作所需的最小权限。

**范围**：Android Manifest、Apple entitlements/Info.plist、Windows capabilities、Linux helper 权限。

**依赖**：`SEC-G0-001`。

**交付物**：机器可读权限白名单、权限差异脚本、平台说明。

**验收规则**：

1. Android 不含 `READ_MEDIA_IMAGES`、照片全库、相机、麦克风、通讯录、短信和位置权限。
2. Apple 包不含 Photo Library、Camera、Microphone、Contacts、Location、Screen Recording 权限描述或 entitlement。
3. Windows 不声明图片库、摄像头、麦克风、输入监听能力；服务 ACL 仅允许当前安装的 Orange 客户端调用。
4. Linux helper 只获得 TUN/路由/DNS 所需能力，不获得 Home 全盘读权限或任意 root shell。
5. 用户主动导入配置时只获得单文件临时访问；取消选择后不存在目录级持久权限。
6. CI 保存每次构建权限快照，任何新增权限必须阻断并要求人工审批。

**非目标**：不在本切片实现平台 VPN。

**实现基线**：`security/platform-permissions.yml` 以不可发布的开发状态固定当前权限面，`scripts/security/check_platform_permissions.py` 使用结构化 JSON、XML、plist、TOML 和 Android `aapt` 数据阻断未登记声明。通用安全任务精确核对 Tauri capability、版本控制内的 Android/Apple/Windows/Linux 权限文件和文件导入依赖；Android 任务额外核对生成 Manifest 与合并 APK，iOS 任务要求并核对生成 Info.plist/entitlements。照片、媒体、相机、麦克风、通讯录、短信、位置和屏幕录制权限即使被同时加入策略仍会失败；`fs:`、`dialog:`、`shell:` WebView capability 也被硬阻断。当前开发 APK 只请求 INTERNET 和 AndroidX 私有动态接收器权限。Windows 已登记并精确核对 Named Pipe 的 SYSTEM/service SID/安装用户 DACL、medium integrity label、远程拒绝和 PID/令牌/固定映像复核策略；SCM 安装链路及安装后跨用户/低完整性独立进程拒绝已在 Windows 10 未签名开发包通过。Apple 生成包、正式签名 Windows 包与 Win11、Linux helper/polkit/systemd 以及单文件临时导入尚未验证，因此本切片保持 `in_progress`。

## SEC-G0-003：控制面出网与敏感数据策略

**目标**：限制应用自身的网络请求和敏感数据生命周期。

**范围**：域名 allowlist、HTTPS、重定向、日志、secret storage、抓包规则。

**依赖**：`ARC-G0-002`、`BOOT-G0-003`。

**交付物**：`control-endpoints.yml`、脱敏器、出网审计测试、secret storage adapter 契约。

**验收规则**：

1. React 不能提交任意 URL；每个 Tauri command 对应固定业务端点。
2. 所有控制面请求经 BootstrapTransport，所有 bootstrap 节点失败时不出现后端裸连。
3. HTTPS 证书错误、host 不在 allowlist、跨 host 重定向未批准时请求失败。
4. release 日志没有 HTTP body、Authorization、密码、token、订阅 URL、bootstrap 节点和本地路径。
5. token 分别进入 Keystore、Keychain、Credential Manager、Secret Service；退出账号后读取不到旧 token。
6. 抓包报告能区分应用控制面和用户隧道流量，控制面目的 host 与 allowlist 完全一致。

**非目标**：不实现业务接口页面。

**实现基线**：`security/control-endpoints.yml` 以不可发布的开发策略登记十类固定业务 command、HTTPS/443、禁止重定向和请求资源上限，并与加密 bootstrap fixture 的 API host/超时保持一致。`scripts/security/check_control_egress.py` 阻断 WebView 网络逃逸、IPC 敏感字段、第二套 HTTP client、未审计 socket/Swift 网络构造、生产运行时日志出口及 Android/iOS 密钥插件的 WebView 暴露、固定命令、三项固定用户凭据 key 和系统存储约束漂移；唯一批准的网络实现是 sing-box direct-dial Go bridge。`orange-platform` 定义固定 access/refresh/subscription credential key、自动清零且 Debug 脱敏的 `SecretValue`、稳定错误、共享移动 Base64 协议和平台 secret store backend 契约，shared wrapper 保证写入成功或失败后都清零调用方缓冲，并允许平台注销覆写。桌面 `DesktopSecretStore` 通过精确固定、禁用默认特性的 `keyring 4.1.5` 分别接入 Windows Credential Manager、macOS Keychain 和 Linux Secret Service，生产 service/key 不允许调用方注入，第三方错误细节不会越过 adapter；Windows Credential Manager 与 WSL2 隔离 GNOME Keyring 中的真实覆盖写入、读取和注销生命周期均已通过，Linux 包装应用的图形会话集成仍待验证。Android 目标不链接桌面依赖；受控 Kotlin 原语使用 Android Keystore 内不可导出 AES-256-GCM key 和应用私有密文存储，AAD 绑定固定凭据 key，Rust 通过无 WebView handler/无 capability 的内部 Tauri mobile plugin 调用固定协议，Android 注销同时删除密文和 key。API 36 x86_64 模拟器 4 项测试已覆盖真实 Rust/Kotlin 存取往返、生命周期、篡改、清零与注销后空存储。iOS 内部插件通过独立 Rust carrier 链接受控 Swift Package，只使用固定 service/account 的 Keychain generic-password、`AfterFirstUnlockThisDeviceOnly` 和禁用同步属性，不申请 access group；两侧仅开放固定 handshake/store/load/delete/logout 协议且无 WebView capability。生产端点、类型化登录 command 对内部后端的业务接线、Android 真机/API 矩阵、iOS 模拟器/真机生命周期、macOS Keychain 运行期、Linux 包装应用图形会话集成和真实抓包仍是本切片验收缺口。

`BOOT-P0-004` 进一步把十类开发业务 route 固定在共享 Rust command catalog 中，并用版本化 fixture 与同一端点策略逐项交叉验证。认证 command 只从 Rust 安全存储加载 access token，经窄版 stdio `accessToken` 字段送入 native bridge；协议不接受任意 header，只有 Go bridge 能构造 Bearer header，且两侧 token 缓冲均在使用后清零。桌面只管理一个共享 Control Plane transport，静态门禁阻断 adapter 外的原始请求构造；重定向、第二次尝试、超限 body/response 和未脱敏 transport 错误均 fail closed。前端 invoke ACL 未增加业务命令，仍不能传 URL、host、route、token 或 Authorization。生产端点与真实 command 尚未接线，移动端 transport 也尚未实现。

`ARC-P1-005` 的本地诊断只接受固定枚举和数值指标，保存在有硬上限的内存环形缓冲中；
不存在任意日志 message、节点、域名、URL、路径、请求正文、凭据或 token 字段，也不写入
日志 sink 或远程遥测。debug bundle 在序列化前递归复核敏感字段和值，并要求调用方先取得
预览，再用精确 confirmation ID 消费同一份待确认字节。当前未向 WebView 开放 bundle、
文件系统或网络能力，用户可见预览和导出必须在后续接线中继续保持这一确认边界。

## SEC-G0-004：供应链、SBOM 与资源签名

**目标**：所有代码依赖、可执行文件和数据资源来源可追溯。

**范围**：lockfile、SBOM、`resources-manifest.json`、哈希/签名、许可证。

**依赖**：`ARC-G0-001`。

**交付物**：依赖锁、SBOM 任务、资源 manifest schema、denylist。

**验收规则**：

1. Rust、Node、Go、Gradle、Swift Package 依赖全部锁定或记录不可锁定原因。
2. EXE/DLL/helper/appex/XCFramework/AAR/SRS/MMDB 都有来源、版本、平台、SHA-256、许可证和签名字段。
3. 构建时实际资源与 manifest 一一对应，多一个、少一个或哈希不符都失败。
4. SBOM 不含 Clash/mihomo、OCR、图像文字识别、BIP-39 扫描或未批准统计 SDK。
5. 构建过程不从未登记 URL 下载并执行二进制。
6. 许可证报告覆盖 sing-box、UI 依赖、规则集、图片和可选 MMDB。

**非目标**：不决定每个商店的法律文本。

**验收结果（2026-07-30）**：当前 810 个组件、59 个资源和 7 个生态的锁定/空生态原因、来源、许可证、SHA-256、签名状态、资源一一对应及禁用依赖检查均由 35 步质量门禁复核通过，本切片为 `done`。尚未产生的 Apple 原生产物和正式发布签名会在加入构建时被同一策略强制登记，其真机签名与发布资格由对应平台切片及 `REL-P1-005` 验收。

## SEC-P1-005：运行时隐私专项

**目标**：证明实际产物没有触碰用户图片或隐蔽上传。

**范围**：五平台诱饵文件、文件审计、端口/流量抓取、崩溃报告。

**依赖**：所有平台 `P0`、`QA-G0-004`。

**交付物**：每平台隐私测试报告和原始摘要。

**验收规则**：

1. 测试设备图片目录放置唯一诱饵文件，执行全部功能后无打开、读取、哈希或上传记录。
2. 系统权限历史中没有照片、相机、麦克风、通讯录和位置访问。
3. 控制面抓包不含图片字节、OCR 文本、文件列表、助记词或其派生指纹。
4. 端口扫描符合架构：Control Plane 无 TCP/UDP listener，Data Plane 只开放已登记 loopback 端口。
5. crash dump 与日志扫描不包含 bootstrap 明文、token、订阅和用户文件内容。
6. 任一失败将该平台发布状态改为 blocked，不允许风险接受后跳过。

**非目标**：不分析用户经过 VPN 主动访问的网络内容。
