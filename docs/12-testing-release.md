# 模块 12：测试、质量与发布

## 模块目标

建立可重复的自动化与人工证据，使每个切片和五平台发布都经过安全、功能、恢复、性能、签名和供应链门禁。

**当前执行方式（2026-08-01）**：GitHub Actions 当前运行五平台签名打包、构建缓存和
聚焦的 `workspace-quality`；后者执行 34 项 CI 脚本合同测试、Tauri capability 精确
基线、TypeScript 格式/ESLint/Vitest/build、Rust fmt/clippy/test、双 Go 模块
fmt/vet/test，并由前端与五平台壳构建执行资源清单门禁。Tauri 门禁以结构化 JSON/TOML
解析锁定 5 个 capability 和 17 份命令权限定义，报告作为独立 artifact 留存。Android
release 打包后以固定版本、URL 与 SHA-256
的 ktlint 1.8.0 检查 9 个项目自有 Kotlin 文件，验证 4 份生成副本与受控源一致，并
执行 lint 和 4 项 Kotlin unit/contract tests；格式日志、测试 XML/HTML 与 lint 报告
随 APK/AAB artifact 留存。macOS
runner 以项目格式配置执行严格 `swift format lint`，并以 warnings-as-errors 运行固定
4 项 XCTest，日志随 artifact 留存。已签名 macOS/iOS 包的窄权限后验仍保留。工作流
使用官方 Node 24 Action 主版本，并在 iOS 临时 runner 上移除未使用且未受信任的
`aws/tap`；商店上传仅在版本标签或手动明确选择时执行。`package #61` 六个 job 全绿、
产生 6 个 artifact 且运行页无注解。已删除的通用安全、覆盖率、SBOM 等门禁没有整体
恢复，既有通用 evidence 仍只作历史记录。

## QA-G0-001：CI 基础门禁

**目标**：每次变更自动执行最低质量和安全检查。

**依赖**：`ARC-G0-001`、`SEC-G0-004`。

**交付物**：CI matrix、lint/test/build/security jobs、artifact retention。

**验收规则**：

1. TypeScript、Rust、Go、Kotlin/Swift 在对应变更时运行格式、lint、unit/contract tests。
2. 至少构建当前可用 runner 的平台；Apple/移动构建有专用 runner 或明确队列。
3. 权限差异、SBOM、依赖 denylist、资源 manifest、secret scan 失败会阻断合并。
4. CI 不打印 secret、签名密码、bootstrap key 或解密后的配置。
5. 测试报告与产物按 commit 保留，失败步骤能定位模块/切片。
6. branch protection 要求 G0 jobs 全绿，不能仅靠人工跳过。

**非目标**：不要求每个提交跑全部真机矩阵。

**当前差距（2026-08-01）**：`package #61` 的六个 job 全绿；除 TypeScript、Rust、
双 Go 模块质量检查和资源 manifest 外，Tauri capability、Android/Apple 包审计、
Android 原生质量、资源 manifest 与工具链检查器的 34 项 Python 合同测试也已进入 CI，
并继承五平台构建、Android release APK/AAB、macOS/iOS 签名包窄权限后验及 artifact
留存。Android lint
和固定 4 项 Kotlin unit/contract tests 已闭环，Action runtime 已升级到 Node 24 且
当前运行无注解。固定 ktlint 1.8.0 的 Android Kotlin 格式、Swift 严格格式/lint、
warnings-as-errors 和固定 4 项 XCTest 也已闭环；Tauri capability 当前态已有独立报告，
但 Windows/Linux 包快照等其余权限差异、SBOM、denylist、secret scan 与 branch
protection 仍未恢复，本切片保持 `review`。

## QA-P0-002：单元、契约与故障注入

**目标**：覆盖共享逻辑和关键失败路径。

**依赖**：各模块 `G0/P0` 实现。

**交付物**：Vitest、Rust tests、Go/platform tests、fixture library、coverage report。

**验收规则**：

1. 双状态机、DTO、错误映射、AEAD、验签、防回滚、配置净化和原子写入有单元测试。
2. API 所有端点用脱敏 fixture 覆盖成功、4xx/5xx、超时、无效 JSON 和 schema 漂移。
3. 故障注入覆盖进程 kill、端口冲突、磁盘满、规则损坏、代理节点阻断和网络切换。
4. 测试不得依赖生产账号、真实 token 或不稳定公共服务。
5. 风险模块新增分支必须有测试；覆盖率只作辅助，不以单个百分比替代验收。
6. flaky test 必须修复或隔离并登记，不能无限重跑掩盖。

**非目标**：单元测试不替代真实 VPN 验收。

**验收结果（2026-07-31）**：209 项 Python 安全/变异测试及双状态机、DTO/错误、AEAD、验签、防回滚、配置净化与原子写入的固定回归测试通过；11 个业务 API 操作和 6 类失败响应由脱敏 fixture 覆盖。进程退出、端口冲突、磁盘满、规则损坏、代理阻断及网络切换六类故障注入逐项通过，测试只使用本地或 `.invalid` 数据且质量入口不重跑失败用例。`pnpm coverage` 生成前端、Rust workspace 与两套 Go module 的可追溯覆盖率报告，百分比仅作为规则级证据的补充。验收证据见 `docs/evidence/QA-P0-002-fault-injection-coverage-2026-07-31.md`，本切片为 `done`。

## QA-P0-003：端到端与视觉回归

**目标**：验证用户主流程和跨尺寸界面。

**依赖**：UI/API/VPN `P0`、平台 `P0`。

**交付物**：Playwright、Maestro/平台 UI 测试、视觉基线、E2E 报告。

**验收规则**：

1. 覆盖冷启动、Control Plane、登录、订阅、VPN 授权、连接、切节点、断开、退出。
2. 商业流程覆盖套餐、创建订单、支付跳转校验、订单刷新；支持流程覆盖工单创建/回复/关闭。
3. 360×800、412×915、平板、1366×768、1440×900 截图没有阻断级差异。
4. 动态 Banner、时间、流量和延迟在视觉测试中使用固定 fixture。
5. 键盘、触控、返回手势、窗口缩放和字体放大各有至少一条主流程。
6. E2E 失败保存截图、日志和状态，但先执行脱敏。

**非目标**：浏览器 E2E 不作为原生 VPN 成功证据。

## QA-G0-004：安全、隐私、端口与出网专项

**目标**：验证最关键负面要求“没有做什么”。

**依赖**：`SEC-G0-001`、`SEC-G0-002`、`SEC-G0-003`、`SEC-G0-004`、`BOOT-G0-003`、平台可运行产物。

**交付物**：权限、端口、抓包、诱饵文件、产物字符串和 crash dump 报告。

**验收规则**：

1. 五平台包权限与白名单一致，无照片/OCR/相机/麦克风等禁用能力。
2. Control Plane 无 TCP/UDP listener；仅受限 stdio/UDS/Named Pipe IPC 可存在。
3. 阻断 bootstrap 后业务 API fail closed，抓包无后端直连。
4. 图片诱饵文件在全流程后无访问记录，网络请求无图片/OCR/助记词数据。
5. 产物和 SBOM 不含禁用依赖、旧 geo 数据、Clash/mihomo 或未登记 executable。
6. 日志/crash dump 不含 bootstrap、token、订阅、支付参数和用户文件内容。

**非目标**：不解密检查用户主动的隧道内容。

**当前权限证据（2026-08-01）**：`package #43` 的 Android release APK 与 AAB 均仅含 INTERNET、应用私有签名权限和 DUMP 组件守卫，无显式硬件 feature/shared user ID；同一提交再次通过 macOS PKG 与 iOS IPA 的照片、相机、麦克风、通讯录、位置和录屏声明后验，APK/AAB/PKG/IPA 四项 JSON 路径均纳入对应 artifact。Windows、Linux 与其余专项仍缺当前包审计，不能把规则 1 或本切片标记完成。

## REL-P1-005：五平台签名与安装包

**目标**：生成可安装、可验证、可追溯的 release 产物。

**依赖**：所有 `G0/P0/P1`、`QA-G0-004`。

**交付物**：AAB/APK、IPA/TestFlight、DMG/PKG、Windows installer、deb/rpm/AppImage 决策、checksums。

**验收规则**：

1. 每个平台产物通过官方签名/验证工具；macOS 完成 notarization，Apple extension 签名一致。
2. 产物内所有 executable/library/data 与 resources manifest 一致。
3. 安装、新装启动、覆盖升级、失败回滚和卸载在平台矩阵通过。
4. release 不含 debug URL、测试账号、开发证书、source map secret 或 bootstrap 明文。
5. SBOM、许可证/Notices、checksums、symbol/mapping 和隐私说明随版本归档。
6. 发布版本号在 UI、Tauri、service/helper、extension 和 core 握手中一致或兼容。

**非目标**：不在缺少签名/entitlement 时生产伪 release。

## REL-P1-006：异常恢复、升级与卸载门禁

**目标**：确保 VPN 产品不会在故障后破坏系统网络。

**依赖**：各平台生命周期切片、`REL-P1-005`。

**交付物**：recovery matrix、kill/reboot/upgrade/uninstall scripts、结果报告。

**验收规则**：

1. 分别 kill UI、Control Plane、Data Plane、service/helper 后，状态和恢复符合设计。
2. 系统代理、DNS、route 在正常停止、crash、系统重启、升级失败和卸载后恢复。
3. 新配置/规则/core 不可用时回滚上一版本，不出现半更新。
4. 睡眠唤醒、网络切换、系统时间变化、证书过期和磁盘满有结果。
5. 修复工具只操作 Orange 拥有且带标记的设置，不覆盖用户后来修改。
6. 任一平台存在 P0 网络残留问题时阻断该平台发布。

**非目标**：不以“重启电脑可恢复”作为唯一修复方案。

## REL-P2-007：商店与渠道发布

**目标**：按渠道满足审核、更新和隐私要求。

**依赖**：`REL-P1-005`、`REL-P1-006`。

**交付物**：Google Play/App Store/Mac App Store/Microsoft Store/下载站 checklist。

**验收规则**：

1. 每渠道 package ID、签名、entitlement、隐私问卷和 VPN 声明一致。
2. 渠道差异只影响允许的功能/资源，不绕过 G0 安全门禁。
3. 自动更新包签名、版本回滚、防降级和 helper/core 兼容性有测试。
4. 隐私政策明确控制面、VPN 隧道、日志和不采集照片/OCR/助记词。
5. 发布链接、hash、审核版本和已知问题归档。

**非目标**：渠道审核时间不计入工程完成时间。
