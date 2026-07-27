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

**实现基线**：`security/control-endpoints.yml` 以不可发布的开发策略登记十类固定业务 command、HTTPS/443、禁止重定向和请求资源上限，并与加密 bootstrap fixture 的 API host/超时保持一致。`scripts/security/check_control_egress.py` 阻断 WebView 网络逃逸、IPC 敏感字段、第二套 HTTP client、未审计 socket 构造和生产运行时日志出口；唯一批准的网络实现是 sing-box direct-dial Go bridge。`orange-platform` 定义固定 token key、自动清零且 Debug 脱敏的 `SecretValue`、稳定错误和平台 secret store backend 契约，shared wrapper 保证写入成功或失败后都清零调用方缓冲，并在注销时尝试删除全部用户 token。桌面 `DesktopSecretStore` 通过精确固定、禁用默认特性的 `keyring 4.1.5` 分别接入 Windows Credential Manager、macOS Keychain 和 Linux Secret Service，生产 service/key 不允许调用方注入，第三方错误细节不会越过 adapter；Windows 真实覆盖写入、覆盖、读取与注销生命周期已通过，Linux 后端已编译但 WSL2 没有可用 Secret Service。Android 目标确认不链接桌面依赖。生产端点、Android Keystore/iOS Keychain、macOS/Linux 真实桌面运行期验证和真实抓包仍是本切片验收缺口。

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
