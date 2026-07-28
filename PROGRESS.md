# Orange 开发进度

> 更新日期：2026-07-28
> 产品切片：69  
> 已完成：1
> 当前阶段：`VPN-P0-004` in_progress；Windows 原生生命周期/流量事件已进入有界 hub，下一检查点为生产订阅 backend/激活源与明确授权的 WebView 事件消费/UI

状态定义见 [docs/README.md](docs/README.md)。没有验收证据的切片不得标记 `done`。

## 1. 总览

| 模块 | 切片数 | done | 当前状态 | 文档 |
| --- | ---: | ---: | --- | --- |
| 安全与隐私 | 5 | 1 | in_progress | [01](docs/01-security-privacy.md) |
| 共享架构 | 5 | 0 | in_progress | [02](docs/02-shared-architecture.md) |
| Bootstrap Control Plane | 6 | 0 | in_progress | [03](docs/03-bootstrap-control-plane.md) |
| sing-box Data Plane | 6 | 0 | in_progress | [04](docs/04-singbox-data-plane.md) |
| 业务 API | 6 | 0 | in_progress | [05](docs/05-business-api.md) |
| UI 与资产 | 8 | 0 | in_progress | [06](docs/06-ui-assets.md) |
| Android | 5 | 0 | not_started | [07](docs/07-platform-android.md) |
| Apple | 6 | 0 | not_started | [08](docs/08-platform-apple.md) |
| Windows | 5 | 0 | not_started | [09](docs/09-platform-windows.md) |
| Linux | 5 | 0 | not_started | [10](docs/10-platform-linux.md) |
| 规则与地理数据 | 5 | 0 | not_started | [11](docs/11-rules-geo-data.md) |
| 测试与发布 | 7 | 0 | not_started | [12](docs/12-testing-release.md) |

## 2. 当前队列

| 顺序 | 切片 | 状态 | 下一检查点 |
| ---: | --- | --- | --- |
| 1 | `SEC-G0-001` 不可信源隔离 | done | 扫描、独立副本和迁移清单证据已登记 |
| 2 | `ARC-G0-001` 五平台 Workspace | blocked | Gitee Go 适配文件已完成；等待推送后的远端运行链接及 macOS/iOS runner 证据 |
| 3 | `SEC-G0-004` 供应链与资源清单 | review | 本地 28 项测试和 727 组件 SBOM 通过；等待 `ARC-G0-001` 前置证据 |
| 4 | `ARC-G0-002` DTO、错误与命令边界 | review | 版本化双命令契约、双向 fixture 与全量门禁通过；等待 `ARC-G0-001` 前置证据 |
| 5 | `BOOT-G0-001` Bootstrap 包格式 | review | 本地信封、CLI 和 9 项测试通过；等待生产 secrets 生成正式资源 |
| 6 | `BOOT-G0-002` Rust 内存解密与清零 | review | 本地 13 项测试、原位清零、真实 Go handoff、产物泄漏扫描和全量门禁通过；待生产 bootstrap 资源 |
| 7 | `BOOT-G0-003` 无端口 sing-box Direct-Dial PoC | in_progress | 本机 direct-dial、startup DNS、Rust sidecar 宿主、三桌面打包注册/哈希校验、live API、fail-closed、Windows 与 Linux WSL2 无监听审计及全量门禁通过；待抓包、macOS/移动端运行审计、生产代理和正式签名安装包 |
| 8 | `SEC-G0-003` 控制面出网与敏感数据 | in_progress | 十类开发端点策略、出网/日志审计、桌面系统密钥存储、Android 内部 Rust/Tauri/Keystore 后端和 iOS 内部 Rust/Tauri/Keychain 后端已落地；Android API 36、Windows Credential Manager 与隔离 Linux Secret Service 真实往返及全门禁通过；待生产 command 接线、Android 真机/API 矩阵、Apple 运行期、Linux 包装应用图形会话集成与真实抓包 |
| 9 | `SEC-G0-002` 跨平台权限白名单 | in_progress | 机器可读开发壳白名单、权限声明发现、硬禁止隐私权限、Tauri capability、Android 合并 APK 快照和 Windows 原生 Named Pipe ACL/身份门禁已落地；待 Apple 包、Windows 安装后跨用户/低完整性独立进程证据、Linux helper 与单文件临时授权证据 |
| 10 | `ARC-G0-003` 双平面状态机与 Adapter | review | 双状态机、平台 adapter、实例/序列防回退、只读状态命令和故障 mock 已落地；Windows/Linux 21 步、双桌面启动与 Android 8 步/API 36 回归通过；等待 `ARC-G0-002` 正式前置收口 |
| 11 | `ARC-P1-004` 持久化、迁移与回滚 | in_progress | 强类型非敏感设置、v1→v2 migration、原子代次文件、损坏恢复、future-schema 拒绝、Data Plane revision 回滚账本和三项用户凭据注销已落地；待五平台安装/卸载残留后验及正式前置收口 |
| 12 | `ARC-P1-005` 事件、任务与可观测性 | in_progress | Windows Data Plane 状态/流量生产者、统一序列、有界原生 hub 与可取消后台 task 已接线；待 Control Plane/其他平台生产者、WebView 消费、UI 预览导出和正式前置收口 |
| 13 | `BOOT-P0-004` BootstrapTransport 强制路由 | in_progress | 十类固定业务路由、单一 client、Rust 安全存储 token 注入、桌面 stdio/Go Bearer 接线与跨三平台门禁已完成；待生产策略、真实业务 command 和移动端嵌入式实现 |
| 14 | `API-G0-001` 接口契约与脱敏 Fixture | in_progress | 开发 v1 等价 schema、全端点 wire/public DTO、结构化脱敏 fixture、失败矩阵与静态门禁已完成，三平台验证通过；待获批生产 OpenAPI/后端联调与正式前置收口 |
| 15 | `API-P0-002` 动态配置、登录与注册 | in_progress | 原生 config/auth service、四个桌面固定命令、三态登录态、双端表单校验、原子 token 生命周期、401/离线场景与 URL/ACL 门禁已完成，三平台验证通过；待生产 API/host、移动 transport、真实后端 E2E 与正式依赖收口 |
| 16 | `API-P0-003` 账户与订阅 | in_progress | 固定账户/订阅刷新、原生凭据隔离、401 清理、共享 guard 及停止 Data Plane 优先的桌面注销已落地；待获批订阅配置契约、生产 Data Plane 接线、产品 UI、真实后端与跨平台验证 |
| 17 | `VPN-G0-001` 纯 sing-box 配置模型与净化 | in_progress | 闭合 v1 schema、Rust 内部模型/净化器、客户端 TUN/DNS/TLS/route 模板、sing-box 1.13.14 严格兼容测试和产物泄漏门禁已完成，三平台验证通过；待获批生产订阅 fixture、真实 Data Plane 接线、macOS/iOS 验证与正式依赖收口 |
| 18 | `VPN-P0-003` 订阅预启动与原子切换 | in_progress | 原生候选事务、三项健康契约、持久化 revision journal、崩溃恢复、commit 后节点 runtime 交接、20 项 Rust 测试和静态门禁已落地；待生产 backend、真实旁路拨号/防环探测、订阅下载契约、应用接线、UI 与五平台验证 |
| 19 | `UI-G0-001` 设计 Token 与页面基线 | in_progress | 亮暗主题、命名 Token、移动/平板/桌面分层布局和五视口截图已落地；待原生平台截图、设计审批与正式品牌资产 |
| 20 | `UI-G0-002` 资产白名单与转换 | in_progress | 严格白名单、PNG/JPEG/WebP 元数据清洗、Lottie 拒绝规则、许可证记录和全目录资源门禁已落地；待正式品牌、第三方 Banner 授权与专有图形清单 |
| 21 | `UI-P0-003` App Shell、认证与通用状态 | in_progress | Hash 路由、启动恢复、严格认证守卫、登录/注册、五项导航、退出确认和通用状态已落地；待真实后端、移动原生 handler、macOS/iOS 与正式依赖收口 |
| 22 | `VPN-P0-004` Selector、测速与流量 | in_progress | provisioned Windows host 每 500 ms 回读权威生命周期/流量，以统一序列进入 64 项原生 hub，后台 task 可取消并在退出时 join；生产 backend/激活源仍未接线；待 WebView/UI、真实切换抓包和 Linux/macOS/iOS 证据 |

## 3. 切片明细

### 安全与隐私

| ID | 摘要 | 状态 | 证据/备注 |
| --- | --- | --- | --- |
| `SEC-G0-001` | 不可信源隔离 | done | `SECURITY.md`、`docs/migration-inventory.md`、508 项资源清单；扫描/测试通过，独立副本日志无原工程路径 |
| `SEC-G0-002` | 跨平台权限白名单 | in_progress | `security/platform-permissions.yml`、跨平台声明/构建快照门禁、Android 实际 APK 精确权限审计，以及 Windows SYSTEM/service SID/安装用户 DACL、medium integrity label 与 PID/令牌/映像复核通过；证据见 `docs/evidence/SEC-G0-002-permission-baseline-2026-07-27.md` 和 `docs/evidence/WIN-P0-002-windows-service-ipc-2026-07-28.md`；待 Apple 包、Windows 安装后跨用户/低完整性独立进程、Linux helper/polkit/systemd 与单文件临时授权证据 |
| `SEC-G0-003` | 控制面出网与敏感数据 | in_progress | 不可发布的开发端点/出网/日志门禁、固定 token key、自动清零、平台注销覆写、三桌面系统密钥存储及 Android/iOS 内部 Rust/Tauri/native 后端已落地；Windows Credential Manager、隔离 Linux Secret Service 与 Android API 36 真实生命周期/跨语言往返、iOS Keychain 静态边界和干净 Android 重建通过；证据见 `docs/evidence/SEC-G0-003-control-egress-2026-07-27.md`；待生产 command 接线、Android 真机/API 矩阵、Apple 运行期、Linux 包装应用图形会话集成与真实抓包 |
| `SEC-G0-004` | 供应链、SBOM 与资源签名 | review | 727 组件、53 资源、7 生态、原生产物 manifest 与 28 项测试通过；证据见 `docs/evidence/SEC-G0-004-supply-chain-2026-07-27.md` |
| `SEC-P1-005` | 运行时隐私专项 | not_started | 发布前执行 |

### 共享架构

| ID | 摘要 | 状态 | 证据/备注 |
| --- | --- | --- | --- |
| `ARC-G0-001` | 五平台 Workspace 与工具链 | blocked | Windows/Linux/Android 空壳构建和启动通过；供应商无关 CI 入口与国内镜像验证见 `docs/evidence/ARC-G0-001-ci-portability-2026-07-27.md`；缺少 macOS 构建机、iOS 模拟器和有运行链接的远端 CI |
| `ARC-G0-002` | DTO、错误与命令边界 | review | 版本化 schema、9 类脱敏错误、固定命令 ACL、Rust/TypeScript 双向 fixture；证据见 `docs/evidence/ARC-G0-002-contract-boundary-2026-07-27.md` |
| `ARC-G0-003` | 双平面状态机与 Adapter | review | Control/Data 独立状态机、共享 Control 状态、`PlatformVpnAdapter`、幂等控制器、权威快照恢复、实例/序列防回退、只读 `get_plane_state` 和故障 mock 已落地；Windows/Linux 全门禁、双桌面 8 秒启动、Android 构建与 API 36 当前 x86_64 二进制回归通过；证据见 `docs/evidence/ARC-G0-003-dual-plane-state-2026-07-27.md`；正式依赖 `ARC-G0-002` 仍为 `review` |
| `ARC-P1-004` | 持久化、迁移与回滚 | in_progress | 强类型非敏感设置、v1→v2 migration、原子代次文件、损坏恢复、future-schema 拒绝、Data Plane revision 回滚账本和三项用户凭据注销已落地；无新增 WebView command/capability；证据见 `docs/evidence/ARC-P1-004-persistence-2026-07-27.md`；待五平台安装/卸载残留后验及正式前置收口 |
| `ARC-P1-005` | 事件、任务与可观测性 | in_progress | 版本化 envelope、旧实例/乱序过滤、单待发流量节流、有限 task registry、分类诊断与确认式 bundle 已落地；Windows host 已用同一原生 client 将生命周期/流量按统一序列写入 64/256 有界 hub，500 ms monitor 持有可取消后台 lease 并在退出时 join；无 WebView emitter/新 command/capability；证据见 `docs/evidence/ARC-P1-005-observability-2026-07-27.md`；待 Control Plane/其他平台生产者、WebView 消费、UI 预览导出和正式前置收口 |

### Bootstrap Control Plane

| ID | 摘要 | 状态 | 证据/备注 |
| --- | --- | --- | --- |
| `BOOT-G0-001` | Bootstrap 包格式与构建加密 | review | 严格 schema、随机 XChaCha20-Poly1305、zeroize CLI、manifest、开发 `bootstrap.enc` 与失败测试；证据见 `docs/evidence/BOOT-G0-001-bootstrap-envelope-2026-07-27.md` |
| `BOOT-G0-002` | Rust 内存解密与清零 | review | 生产 `decrypt`、受控 `SecretBuffer`、schema/过期校验、consume/consume_in_place/Drop/panic 清零、真实 Go handoff、Debug 脱敏、产物泄漏扫描与 13 项测试；证据见 `docs/evidence/BOOT-G0-002-memory-decrypt-2026-07-27.md` |
| `BOOT-G0-003` | 无端口 sing-box Direct-Dial PoC | in_progress | 固定 sing-box `v1.13.14`、stdio 窄桥、startup DNS、Rust sidecar 宿主、Tauri 单实例状态、三桌面 `externalBin`/运行时哈希校验、live GET/POST、fail-closed、Windows 与 Linux WSL2 无监听、18 组 Go 测试、7 项宿主进程测试及双系统全量 19 步门禁通过；证据见 `docs/evidence/BOOT-G0-003-direct-dial-2026-07-27.md` 和 `docs/evidence/BOOT-G0-003-linux-runtime-2026-07-27.md`；待管理员抓包、macOS/移动端运行审计、生产代理和正式签名安装包 |
| `BOOT-P0-004` | BootstrapTransport 强制路由 | in_progress | 十类固定业务路由、单一 transport client、Rust 安全存储 token 注入、1 MiB 上限、稳定错误、桌面 stdio/Go Bearer 接线与原始请求静态门禁完成；无新增 WebView 网络能力，Windows/Linux/Android 验证通过；证据见 `docs/evidence/BOOT-P0-004-bootstrap-transport-2026-07-27.md`；待生产策略、真实业务 command、移动端嵌入式实现和正式前置收口 |
| `BOOT-P0-005` | 节点故障切换与 Fail-Closed | not_started |  |
| `BOOT-P1-006` | 签名更新、轮换与防回滚 | not_started |  |

### sing-box Data Plane

| ID | 摘要 | 状态 | 证据/备注 |
| --- | --- | --- | --- |
| `VPN-G0-001` | 纯 sing-box 配置模型与净化 | in_progress | 仅接受 Shadowsocks/Trojan/Hysteria2/selector 与有界 route 引用的闭合 v1 JSON；Rust 先转内部模型再生成固定 TUN、本地 DNS、TLS 最低版本和 route action，敏感缓冲区清零，字段级脱敏错误、sing-box 1.13.14 Go 严格解析和 18 项应用产物禁入标记扫描通过；Windows/Linux 24 步、双桌面启动、Android 8 步及 API 36 回归通过；证据见 `docs/evidence/VPN-G0-001-data-plane-config-2026-07-28.md`；待获批生产订阅 fixture、真实 Data Plane 接线、macOS/iOS 验证与正式依赖收口 |
| `VPN-P0-002` | Data Plane 生命周期 | in_progress | 原生监管器按配置版本/实例号提供 preflight、start/stop/restart、就绪探测、弱引用后台崩溃监控、2 秒检测策略上限、启动/停止超时、强制回收和幂等资源 cleanup；权威快照显式记录真实活动实例，13 项监管 Rust 测试含 20 轮重复启停、故障恢复、Control Plane 隔离、消费者重建与真实子进程崩溃；Windows 应用已按同目录 installer 身份条件注入 `NamedPipeClient`，缺失/非法时显式回退未配置；静态门禁阻断生产层任意可执行路径/参数/shell；证据见 `docs/evidence/VPN-P0-002-data-plane-lifecycle-2026-07-28.md`；待真实 installer/ACL 与 SCM、活动净化配置、真实 TUN/权限/路由/DNS/端口恢复、系统事件桥、其他平台后端及 macOS/iOS 验证 |
| `VPN-P0-003` | 订阅预启动与原子切换 | in_progress | 仅接收已净化配置的原生 pipeline 按 candidate journal -> stage -> 旁路启动 -> core/outbound/DNS 健康 -> 原子 activate -> active 回读 -> commit -> 节点 runtime 交接执行；stage 后立即清零原始配置，commit 后只传公开目录；安装失败清理旧 runtime，同 revision 可重试，恢复会对账 runtime revision；18 项 pipeline 与 2 项文件 journal Rust 测试、机器可读事务顺序门禁通过；审计明确生产 backend/激活源仍为 false；证据见 `docs/evidence/VPN-P0-003-subscription-pipeline-2026-07-28.md`；待生产 backend、受保护 revision 写入、真实旁路拨号/DNS 防环、获批订阅下载/激活契约、应用启动接线、产品 UI、真实后端与五平台验证 |
| `VPN-P0-004` | Selector、测速与流量 | in_progress | 净化目录、回读/补偿/持久化、64 项/8 并发测速和单调节流流量核心已完成；Windows host 只接收 commit 后公开目录，并把权威生命周期/流量以统一序列写入有界原生 hub，停止清 pending，后台 monitor 可取消且 join；22 项 runtime、4 项 event-source 测试和 16 项变异门禁通过；证据见 `docs/evidence/VPN-P0-004-node-runtime-2026-07-28.md` 与 `docs/evidence/VPN-P0-004-windows-managed-host-2026-07-28.md`；待生产订阅 backend/激活源、WebView/UI、真实 TUN 切换抓包与 Linux/macOS/iOS 证据 |
| `VPN-P1-005` | 桌面 Mixed 与系统代理契约 | not_started |  |
| `VPN-P1-006` | 双平面隔离与路由防环 | not_started |  |

### 业务 API

| ID | 摘要 | 状态 | 证据/备注 |
| --- | --- | --- | --- |
| `API-G0-001` | 接口契约与脱敏 Fixture | in_progress | 不可发布的 clean-room v1 等价 schema、Rust 敏感 wire DTO、TypeScript 严格公开 DTO、九条字段映射、结构化脱敏与六类失败 fixture 已落地；Windows/Linux 22 步、双桌面启动、Android 8 步及 API 36 回归通过；证据见 `docs/evidence/API-G0-001-business-contract-2026-07-27.md`；待获批生产契约、真实后端联调和正式前置收口 |
| `API-P0-002` | 动态配置、登录与注册 | in_progress | Control Plane ready 等待、严格动态 config、四个桌面固定命令、三态登录态、双端表单校验、重复提交 guard、原子凭据替换/回滚及认证 401 清理已落地；公开 DTO 无 URL/token，移动端命令 fail closed；Windows/Linux 22 步、双桌面启动、Android 8 步及 API 36 回归通过；证据见 `docs/evidence/API-P0-002-authentication-2026-07-27.md`；待生产 API/host、移动 transport、真实后端 E2E 与正式依赖收口 |
| `API-P0-003` | 账户与订阅 | in_progress | 固定 `account`/`subscription` 原生刷新、用量/过期策略、带回滚的订阅凭据隔离、401 全用户 secret/会话清理、共享并发 guard 和三个桌面严格命令已落地；注销按权威回读 -> 停止 Data Plane -> 再回读确认 -> 删除三类凭据 -> 清缓存执行，失败可重试；`VPN-P0-003` 平台无关事务核心已建立但尚未接线；证据见 `docs/evidence/API-P0-003-account-subscription-2026-07-28.md` 与 `docs/evidence/API-P0-003-logout-2026-07-28.md`；待获批凭据到节点配置契约、生产 Data Plane backend/激活、产品 UI、真实后端、移动/macOS/iOS 验证和正式依赖收口 |
| `API-P1-004` | 套餐、订单与支付 | not_started |  |
| `API-P1-005` | 邀请与工单 | not_started |  |
| `API-P2-006` | 缓存、离线与恢复 | not_started |  |

### UI 与资产

| ID | 摘要 | 状态 | 证据/备注 |
| --- | --- | --- | --- |
| `UI-G0-001` | 设计 Token 与页面基线 | in_progress | 颜色/字号/间距/圆角/阴影/状态/安全区 Token、180px 移动横幅、连接中心、模式/节点入口及 1024px 桌面侧栏断点已落地；360×800、412×915、768×1024、1366×768、1440×900 浏览器基线覆盖亮暗主题、130% 字体和减少动画，图片进入资源哈希审计；证据见 `docs/evidence/UI-G0-001-design-baseline-2026-07-28.md`；待 Android/iOS/macOS 原生截图、正式设计审批与正式品牌资产 |
| `UI-G0-002` | 资产白名单与转换 | in_progress | `docs/asset-allowlist.yml`、严格 schema、确定性 PNG/JPEG/WebP 清洗、Lottie URL/脚本/隐藏二进制/图片拒绝、512 KiB 上限、资源清单交叉校验与许可证记录已落地；当前仅开发标识获准且不可发布，待正式品牌、第三方 Banner 授权及明确专有图形后才能完成；证据见 `docs/evidence/UI-G0-002-asset-pipeline-2026-07-28.md` |
| `UI-P0-003` | App Shell、认证与通用状态 | in_progress | `HashRouter`、启动 loading/error/retry、三态会话守卫、登录/注册校验与提交锁、五项受保护导航、退出 Dialog、Toast、空态及安全 ErrorBoundary 已接通桌面固定命令；浏览器固定模式、15 项 React 测试和 UI 壳静态/突变门禁已落地；证据见 `docs/evidence/UI-P0-003-app-shell-2026-07-28.md`；待真实后端 E2E、Android/iOS 原生 handler、macOS/iOS 运行证据及正式依赖收口 |
| `UI-P0-004` | 首页与连接主流程 | not_started |  |
| `UI-P0-005` | 订阅、节点与配置页面 | not_started |  |
| `UI-P1-006` | 账户、商业与支持页面 | not_started |  |
| `UI-P1-007` | 响应式、可访问性与多语言 | not_started |  |
| `UI-P2-008` | Android TV 与大屏增强 | not_started | 可延后 |

### Android

| ID | 摘要 | 状态 | 证据/备注 |
| --- | --- | --- | --- |
| `AND-G0-001` | libbox 与 Kotlin 插件 PoC | not_started |  |
| `AND-P0-002` | VpnService、权限与前台生命周期 | not_started |  |
| `AND-P0-003` | TUN、Socket Protect 与网络切换 | not_started |  |
| `AND-P1-004` | 分应用、Tile 与开机恢复 | not_started |  |
| `AND-P1-005` | 打包、升级与隐私验收 | not_started |  |

### Apple

| ID | 摘要 | 状态 | 证据/备注 |
| --- | --- | --- | --- |
| `APL-G0-001` | Entitlement、构建机与签名 PoC | not_started | 外部前置条件 |
| `APL-G0-002` | libbox XCFramework 与版本握手 | not_started |  |
| `IOS-P0-003` | Packet Tunnel Data Plane | not_started |  |
| `IOS-P0-004` | Control Plane 与 Tunnel 防环 | not_started |  |
| `MAC-P0-005` | macOS Extension 与系统代理 | not_started |  |
| `APL-P1-006` | Keychain、签名与发布 | not_started |  |

### Windows

| ID | 摘要 | 状态 | 证据/备注 |
| --- | --- | --- | --- |
| `WIN-G0-001` | 产物与核心宿主决策 | in_progress | ADR-0002 固定受签名 `orange-data-plane.exe` 单一路径；宿主组合而不 fork 官方 sing-box 1.13.14，仅注册 TUN/mixed、三节点协议、selector/direct/local DNS，并以继承 stdio 提供窄控制面；36 依赖编译图、双构建 SHA-256、metadata、版本/标签/CGO/Authenticode、标准 manifest、离线 mixed selector 回读/流量 smoke 及 service 签名/哈希链已完成；证据见 `docs/evidence/VPN-P0-004-windows-managed-host-2026-07-28.md`；待正式签名证书及获准指纹、受保护安装和 Win10 22H2/Win11 兼容证据 |
| `WIN-P0-002` | Service、Named Pipe 与双平面 | in_progress | 独立 SCM/受限 Named Pipe 与原生 client 已接入共享 supervisor 和固定 `orange-data-plane.exe` backend；应用复用同一 client 驱动生命周期、节点 runtime 和 500 ms 原生事件 monitor，事件只进有界 native hub；静态门禁保持 WebView emitter、生产订阅 backend/激活源、SCM 安装和 release 均为 false；证据见 `docs/evidence/WIN-P0-002-windows-service-ipc-2026-07-28.md` 与 `docs/evidence/VPN-P0-004-windows-managed-host-2026-07-28.md`；待真实 installer/ACL、生产订阅激活、WebView/UI、获准签名/真实 TUN、恢复、SCM 生命周期、低权限/跨用户及 Win10/Win11 矩阵 |
| `WIN-P0-003` | WinINET 系统代理与恢复 | not_started |  |
| `WIN-P1-004` | Windows TUN/Wintun | not_started |  |
| `WIN-P1-005` | 托盘、安装、升级与卸载 | not_started |  |

### Linux

| ID | 摘要 | 状态 | 证据/备注 |
| --- | --- | --- | --- |
| `LNX-G0-001` | Helper 权限与 IPC PoC | not_started |  |
| `LNX-P0-002` | Mixed 与桌面系统代理 | not_started |  |
| `LNX-P1-003` | TUN、DNS 与路由恢复 | not_started |  |
| `LNX-P1-004` | 发行版打包与生命周期 | not_started |  |
| `LNX-P2-005` | 更多桌面与无特权模式 | not_started | 可延后 |

### 规则与地理数据

| ID | 摘要 | 状态 | 证据/备注 |
| --- | --- | --- | --- |
| `GEO-G0-001` | 可信上游、许可证与生成链 | not_started |  |
| `GEO-G0-002` | 资源 Manifest 与路径沙箱 | not_started |  |
| `GEO-P0-003` | 最小离线规则集打包 | not_started |  |
| `GEO-P1-004` | 签名更新、替换与回滚 | not_started |  |
| `GEO-P2-005` | Country/ASN UI 数据 | not_started | 可选 |

### 测试与发布

| ID | 摘要 | 状态 | 证据/备注 |
| --- | --- | --- | --- |
| `QA-G0-001` | CI 基础门禁 | not_started |  |
| `QA-P0-002` | 单元、契约与故障注入 | not_started |  |
| `QA-P0-003` | 端到端与视觉回归 | not_started |  |
| `QA-G0-004` | 安全、隐私、端口与出网专项 | not_started |  |
| `REL-P1-005` | 五平台签名与安装包 | not_started |  |
| `REL-P1-006` | 异常恢复、升级与卸载 | not_started |  |
| `REL-P2-007` | 商店与渠道发布 | not_started |  |

## 4. 外部决策与阻塞风险

| 项目 | 当前状态 | 解除条件/决定 |
| --- | --- | --- |
| Apple Network Extension entitlement | 未确认 | 提供 Developer Team，在 Mac 真机完成 `APL-G0-001` |
| Mac 构建机与 iOS 真机 | blocked | 配置 macOS CI/开发机与 iOS 模拟器/测试设备，运行五平台构建任务 |
| CI 承载与远端授权 | blocked | 推送并启用 `.workflow` 的 Gitee Go 流水线，保留运行链接；完整平台门禁还需自有 runner |
| Windows 核心宿主 | 已决定 | 仅使用受签名 `orange-data-plane.exe` 受管 sidecar，组合锁定官方 sing-box 核心并以继承 stdio 暴露窄协议；Rust client 与受限 Named Pipe 已接通生产节点 backend，签名和双系统证据继续由 `VPN-P0-004`、`WIN-G0-001`、`WIN-P0-002` 收口 |
| 后端 sing-box JSON | 未确认 | 提供测试 API/fixture，决定是否需要转换层 |
| Bootstrap 节点与密钥系统 | 需配置 | 提供生产节点/API host/到期策略，并在 Gitee Go 配置 `ORANGE_BOOTSTRAP_*` secrets 后运行 `bootstrap-release` |
| API/支付/Banner allowlist | 未确认 | 提供生产/测试完整 host 和重定向规则 |
| `.srs`/MMDB 上游 | 未决定 | `GEO-G0-001` 完成许可证与兼容性审核 |
| 产品名/包名/签名 | 未决定 | 确认 Orange/UUVPN、各平台 identifier 与旧包升级要求 |

## 5. 进度更新模板

每次状态变化追加一行：

| 日期 | 切片 | 旧状态 -> 新状态 | 结果/证据 | 阻塞/下一步 |
| --- | --- | --- | --- | --- |
| 2026-07-26 | DOC baseline | in_progress -> done | `DEVELOPMENT_PLAN.md`、`docs/*` | 开始 `SEC-G0-001` |
| 2026-07-26 | `SEC-G0-001` | not_started -> in_progress | 负责人：Codex；目标交付：`SECURITY.md`、`docs/migration-inventory.md`、禁止规则和 CI 扫描 | 本地扫描通过后进入 review |
| 2026-07-26 | `SEC-G0-001` | in_progress -> done | `python scripts/security/check_source_isolation.py`；3 项门禁测试；独立副本扫描通过且日志匹配 0；`docs/reference-assets.csv` 508 项 | 持续由 CI 阻断回归 |
| 2026-07-26 | `ARC-G0-001` | not_started -> in_progress | 负责人：Codex；目标交付：五平台 workspace、国内镜像、工具链预检和基础 CI | 先完成 Windows 空壳与 Android 环境检查 |
| 2026-07-26 | `ARC-G0-001` | in_progress -> in_progress | Windows EXE 与 Android API 36 APK 构建/启动通过；哈希、权限和截图见 `docs/evidence/ARC-G0-001-windows-android-2026-07-26.md`；五平台 CI 构建任务、Go 检查和 53 项资源一一对应门禁已加入 | 需要 Linux/macOS runner 与 iOS 模拟器产生真实 CI 证据，暂不标记 `done` |
| 2026-07-26 | `ARC-G0-001` | in_progress -> blocked | Ubuntu 24.04.4 WSL2 冷构建、全量检查和 8 秒启动通过，见 `docs/evidence/ARC-G0-001-linux-2026-07-26.md` | 当前仅剩 macOS/iOS 真实 runner；远端为 Gitee，需决定启用 Gitee CI 或增加可运行 GitHub Actions 的镜像 |
| 2026-07-27 | `ARC-G0-001` | blocked -> blocked | 新增供应商无关 `scripts/ci/run.py`；Windows `quality`/desktop 与隔离 Android 冷构建通过，见 `docs/evidence/ARC-G0-001-ci-portability-2026-07-27.md` | 本地工作已收口；等待远端 CI 授权和 macOS/iOS runner 配置 |
| 2026-07-27 | `ARC-G0-001` | blocked -> blocked | 新增 Gitee Go `.workflow` 主干/分支/PR 流水线与国内镜像云端入口 | 等待推送后启用 Gitee Go、取得运行链接，并配置 macOS/iOS runner |
| 2026-07-27 | `SEC-G0-004` | not_started -> review | 国内镜像 fail-closed、677 组件 SBOM、许可证/资源一致性、Windows/Android 调试产物 manifest 与 21 项测试通过 | 正式前置 `ARC-G0-001` 仍等待后置的 Apple 与远端 CI 证据 |
| 2026-07-27 | `ARC-G0-002` | not_started -> in_progress | 负责人：Codex；目标交付：版本化 DTO schema、错误码表、固定命令注册和双向 fixture 契约测试 | 纯契约与 mock 命令可独立验证；不依赖 `ARC-G0-001` 尚缺的 Apple runner 证据 |
| 2026-07-27 | `ARC-G0-002` | in_progress -> review | schema、9 类错误、`get_runtime_info` DTO/handler/ACL、Rust 8 项与 TypeScript 5 项契约测试、全量门禁通过 | 正式前置 `ARC-G0-001` 仍等待后置的 Apple 与远端 CI 证据 |
| 2026-07-27 | `BOOT-G0-001` | not_started -> in_progress | 负责人：Codex；目标交付：严格明文 schema、随机 XChaCha20-Poly1305 信封、环境密钥 CLI、manifest 与失败测试 | 生产节点、渠道凭据和 Gitee secret 注入在本地工具验证后作为最终配置点 |
| 2026-07-27 | `SEC-G0-004` | review -> review | 加密工具依赖纳入锁文件；SBOM 刷新为 690 组件，22 项安全测试通过，并新增 GOPROXY `direct` 回退门禁 | 正式前置 `ARC-G0-001` 状态不变 |
| 2026-07-27 | `BOOT-G0-001` | in_progress -> review | XChaCha20-Poly1305 信封、严格 schema、环境密钥 CLI、开发 `bootstrap.enc`、manifest、9 项测试与全量质量门禁通过 | 等待生产节点与 Gitee secrets；正式前置切片仍为 review/blocked |
| 2026-07-27 | `BOOT-G0-002` | not_started -> in_progress | 负责人：Codex；目标交付：生产 decryptor、受控 secret buffer、schema/过期校验、清零与泄漏门禁 | 定向 Rust 与 bootstrap CI 全部通过后进入 review |
| 2026-07-27 | `BOOT-G0-002` | in_progress -> review | 生产 `decrypt`、受控 `SecretBuffer`、consume/Drop/panic 清零、Debug 脱敏、产物泄漏扫描门禁、13 项测试与全量 15 步门禁通过 | Go/libbox handoff 与原生副本释放随 `BOOT-G0-003` 落地；生产资源仍等待 Gitee secrets |
| 2026-07-27 | `BOOT-G0-003` | not_started -> in_progress | 负责人：Codex；目标交付：固定版本 sing-box 无监听 direct-dial 窄桥、结构化 HTTPS GET/POST、端口审计和故障注入测试 | 本机 PoC 与全量门禁通过后进入 review；生产节点仍通过 Gitee secrets 注入 |
| 2026-07-27 | `BOOT-G0-003` | in_progress -> in_progress | sing-box `v1.13.14` direct-dial、长度前缀 stdio、live 境外 API GET/POST、代理阻断不裸连、Windows TCP/UDP 无新增监听、15 组 Go 测试、21.6 MB 脱敏产物与全量 16 步门禁通过 | `pktmon` 因当前进程无管理员权限未执行；仍需 Linux/macOS/移动端实机审计、Rust 宿主交接和生产代理验证 |
| 2026-07-27 | `BOOT-G0-003` | in_progress -> in_progress | bootstrap `startupDns` 接入 stdio 和 sing-box UDP/TCP/DoT；真实 DNS fixture 解析代理域名，18 组顶层测试与产物审计通过 | 其余抓包、目标平台实机审计、Rust 宿主交接和生产代理验证缺口不变 |
| 2026-07-27 | `BOOT-G0-003` | in_progress -> in_progress | Rust sidecar 宿主、Tauri 单实例状态、`SecretBuffer` 原位清零、7 项真实子进程测试、生产 Go sidecar handoff 审计与全量 17 步门禁通过 | 仍需管理员抓包、目标平台实机审计、生产代理验证及固定签名打包资源 |
| 2026-07-27 | `BOOT-G0-003` | in_progress -> in_progress | Windows/Linux/macOS 固定 `externalBin`、目标感知 Go 构建、应用同目录解析、构建期嵌入/运行时复验 SHA-256、标准产物 manifest 与全量 19 步门禁通过 | 仍需管理员抓包、目标平台原生运行、生产代理验证和正式签名安装包审计 |
| 2026-07-27 | `BOOT-G0-003` | in_progress -> in_progress | Ubuntu 24.04.4 WSL2 无 `.git` 隔离环境下 Linux 全量 19 步门禁、原生 TCP/UDP 无监听审计、sidecar 字节/哈希一致性和 8 秒 Xvfb/D-Bus 启动通过；证据见 `docs/evidence/BOOT-G0-003-linux-runtime-2026-07-27.md` | 仍需管理员抓包、macOS/移动端原生运行、生产代理验证和正式签名安装包审计 |
| 2026-07-27 | `SEC-G0-003` | not_started -> in_progress | 负责人：Codex；本轮交付端点策略、固定命令/HTTPS/重定向约束、直连客户端静态审计和 secret storage adapter 契约 | 依赖 `BOOT-G0-003` 与 `ARC-G0-002` 尚未完成；平台安全存储实现和真实控制面抓包仍是验收缺口 |
| 2026-07-27 | `SEC-G0-003` | in_progress -> in_progress | 十类开发端点策略、HTTPS/443/禁止重定向、WebView CSP、IPC 敏感字段、第二 HTTP client、运行时日志门禁和 secret storage adapter 契约通过 Windows/Linux 全量 20 步门禁；证据见 `docs/evidence/SEC-G0-003-control-egress-2026-07-27.md` | 待批准生产端点与类型化业务 command、四平台安全存储实现、管理员抓包及前置切片收口 |
| 2026-07-27 | `SEC-G0-003` | in_progress -> in_progress | 接入 Windows Credential Manager、macOS Keychain 与 Linux Secret Service 桌面 adapter；Windows 真实覆盖/读取/注销测试、Linux 全量 20 步门禁和 Android 交叉编译通过 | WSL2 无 Secret Service，仍待 macOS/Linux 真实桌面运行期、Android Keystore/iOS Keychain、生产 command、管理员抓包及前置切片收口 |
| 2026-07-27 | `SEC-G0-003` | in_progress -> in_progress | Android Keystore 不可导出 AES-256-GCM key、私有密文存储、token-key AAD、防篡改/清零/注销实现及受控生成链落地；API 36 模拟器 3 项真实测试和 Android lint 通过 | 尚待类型化登录桥接、Android 真机/API 矩阵、iOS Keychain、macOS/Linux 运行期、生产 command、管理员抓包及前置切片收口 |
| 2026-07-27 | `SEC-G0-003` | in_progress -> in_progress | Android 内部 Tauri mobile plugin、Rust `SecretStoreBackend`、固定版本/key/Base64 协议和平台注销覆写落地；无 WebView handler/capability，API 36 x86_64 真实 Rust/Kotlin/Keystore 往返与 4 项测试通过 | 尚待类型化登录 command 接线、Android 真机/API 矩阵、iOS Keychain、macOS/Linux 运行期、生产端点、管理员抓包及前置切片收口 |
| 2026-07-27 | `SEC-G0-003` | in_progress -> in_progress | iOS 内部 Tauri plugin、Rust `SecretStoreBackend`、固定 Keychain generic-password/`ThisDeviceOnly`/非同步策略和共享移动协议落地；无 WebView handler/capability，Windows 全量 20 步与干净 Android 7 步门禁通过 | Windows 缺少 Xcode/SDK，尚待 iOS Swift/package 编译与模拟器/真机生命周期、类型化登录 command、其余平台运行期、生产端点、管理员抓包及前置切片收口 |
| 2026-07-27 | `SEC-G0-003` | in_progress -> in_progress | 跨桌面原生生命周期测试与隔离 Linux runner 落地；Ubuntu 24.04.4 WSL2 的真实 GNOME Keyring 完成覆盖写入、读取、调用方清零、注销及外部无残留后验 | 尚待 macOS Keychain 生命周期、Linux 包装应用图形会话集成、类型化登录 command、生产端点、管理员抓包及前置切片收口 |
| 2026-07-27 | `SEC-G0-002` | not_started -> in_progress | 机器可读五平台权限策略、Tauri capability、Android 生成 Manifest/合并 APK 精确快照和 Apple plist/entitlement 解析落地；移除未配置的目录范围 FileProvider，Windows/Linux 21 步、Android 8 步及 API 36 四项设备回归通过 | 尚待 Apple 包、Windows 服务 ACL、Linux helper 沙箱和单文件临时授权真实证据，不能标记 `done` |
| 2026-07-27 | `ARC-G0-003` | not_started -> review | Control/Data 独立状态机、共享 sidecar 状态、最小 `PlatformVpnAdapter`、幂等 start/stop/restart、实例/序列防回退、权威快照恢复和只读 WebView 查询完成；故障 mock、Windows/Linux 全门禁、双桌面启动、Android 8 步及 API 36 当前 x86_64 二进制回归通过 | 正式依赖 `ARC-G0-002` 仍为 `review`；具体 TUN adapter 由平台切片实现，不能标记 `done` |
| 2026-07-27 | `ARC-P1-004` | not_started -> in_progress | 强类型非敏感设置、v1→v2 migration、原子代次文件、损坏恢复、future-schema 拒绝、Data Plane revision 回滚账本和三项用户凭据注销落地；无新增 WebView command/capability | 五平台签名安装器尚未落地，无法验证升级/卸载后的 app-data 与安全存储残留；正式依赖 `ARC-G0-002` 仍为 `review` |
| 2026-07-27 | `ARC-P1-005` | not_started -> in_progress | 负责人：Codex；本轮交付版本化事件 envelope、旧实例/乱序过滤、流量节流、有限 task registry、分类环形诊断与确认式 debug bundle | 依赖 `ARC-G0-003` 仍为 `review`；真实事件源/后台任务接线和用户预览导出 UI 尚未实现，不能进入 review |
| 2026-07-27 | `ARC-P1-005` | in_progress -> in_progress | Rust/JSON Schema/TypeScript 事件契约、单待发样本节流、任务取消/期限/RAII 清理、有限分类诊断与二次审计确认式 bundle 完成；Windows/Linux 全门禁、双桌面启动、Android 8 步及 API 36 当前 x86_64 二进制回归通过 | 真实事件源、生产长任务和用户预览导出尚未接线；依赖 `ARC-G0-003` 仍为 `review` |
| 2026-07-27 | `BOOT-P0-004` | not_started -> in_progress | 负责人：Codex；本轮交付十类固定业务路由、统一 BootstrapTransport client、Rust 安全存储 token 注入、响应/错误上限和桌面 Control Plane 接线 | 依赖切片尚未正式收口；生产 host/API fixture、真实业务 command 和移动端嵌入式 Control Plane 仍缺失，不能进入 review |
| 2026-07-27 | `BOOT-P0-004` | in_progress -> in_progress | 十路由契约/策略交叉校验、单一 Rust client、安全存储 token、stdio 窄字段、Go-only Bearer 构造、全宿主错误映射与静态逃逸门禁完成；Windows/Linux 21 步、双桌面启动、Android 8 步及 API 36 当前 x86_64 回归通过 | 生产 host/API DTO、真实业务 command、移动端嵌入式 transport、macOS/抓包/签名包和正式依赖尚未收口 |
| 2026-07-27 | `API-G0-001` | not_started -> in_progress | 负责人：Codex；本轮建立 clean-room 开发契约、Rust 敏感 wire/TypeScript 公开 DTO 分层、全端点脱敏 fixture、字段策略与失败矩阵 | 获批生产 OpenAPI/后端样本不可用；依赖 `ARC-G0-002`、`SEC-G0-003` 尚未正式完成，不能声称生产契约冻结 |
| 2026-07-27 | `API-G0-001` | in_progress -> in_progress | 十一项开发契约、Rust 零化 wire DTO、TypeScript 严格公开 DTO、九条字段映射、六类失败 fixture 与 CI 静态门禁完成；Windows/Linux 22 步、双桌面启动、Android 8 步及 API 36 当前 x86_64 回归通过 | 获批生产 OpenAPI、真实脱敏后端样本、错误码确认与联调仍缺失；正式依赖 `ARC-G0-002`、`SEC-G0-003` 未收口，不能进入 review |
| 2026-07-27 | `API-P0-002` | not_started -> in_progress | 负责人：Codex；本轮交付原生动态配置/认证服务、固定 Tauri command、登录态、表单校验、token 生命周期和六类主流程场景门禁 | 依赖 `API-G0-001`、`BOOT-P0-004`、`ARC-P1-004` 均未正式收口；生产 API/host、移动端 transport 和真实后端联调缺失，不能进入 review |
| 2026-07-27 | `API-P0-002` | in_progress -> in_progress | Control Plane ready 等待、严格 config URL/公开投影、四个桌面固定命令、三态会话、双端表单校验、重复提交 guard、原子凭据替换/回滚及认证 401 清理完成；Windows/Linux/Android 全门禁和双桌面/API 36 运行回归通过 | 生产 API/host、移动端嵌入式 transport、真实后端与产品级 E2E、macOS/iOS 运行期和正式依赖仍缺失，不能进入 review |
| 2026-07-28 | `WIN-G0-001` | not_started -> in_progress | 负责人：Codex；固定受签名官方 sing-box sidecar 单一路径，交付独立锁定构建、制品 manifest、签名/版本握手、最小标签和离线 mixed PoC | 无正式签名证书、Win10 22H2 测试机与生产 service adapter，不能进入 review |
| 2026-07-28 | `WIN-G0-001` | in_progress -> in_progress | ADR、`v1.13.14`/`with_quic` 独立构建锁、双构建同哈希、48 个实际编译依赖、版本/SHA-256/Authenticode 失败关闭和 loopback mixed HTTP/SOCKS5 smoke 通过；Windows 26 步、Linux 隔离 25 步、双桌面与 Android/API 36 回归通过 | 开发制品未签名且不可发布；待正式证书/指纹、原生 `WinVerifyTrust` 接线和 Win10/Win11 兼容矩阵 |
| 2026-07-28 | `WIN-P0-002` | not_started -> in_progress | 负责人：Codex；独立 Windows SCM 宿主、v1 固定 DTO、4 KiB 帧、受限 Named Pipe ACL、客户端 PID/令牌/完整性/固定映像校验、原生 client adapter、12 项 Rust 测试和专用权限/静态门禁落地 | 生产后端仍故意使用 `UnconfiguredVpnAdapter`；待签名 sidecar 接线、安装生命周期、崩溃恢复、独立低权限/跨用户测试及 Win10/Win11 证据，不能进入 review |
| 2026-07-28 | `WIN-P0-002` | in_progress -> in_progress | service 接入共享 supervisor、嵌入式运行 manifest、固定 revision store、SHA-256/原生 `WinVerifyTrust`/证书指纹、精确版本与配置握手、spawn 前 TOCTOU 复验、固定 `run -c` 和 kill-on-close Job Object；23 项 Rust 测试覆盖签名失败、路径逃逸、篡改、握手超时、崩溃与强制回收 | 签名者白名单为空、开发 sidecar 未签名且动态净化配置尚无受保护安装路径；真实 TUN readiness、系统设置恢复、SCM 生命周期、跨用户/低完整性和 Win10/Win11 证据未齐，不能进入 review |
| 2026-07-28 | `WIN-P0-002` | in_progress -> in_progress | 用 `GetAdaptersAddresses` 将临时进程存活 readiness 替换为固定 `orange-tun` Up/双栈地址契约；preflight/spawn 残留拒绝与回收后有界消失验证落地，29 项 Rust 测试覆盖延迟/错误/Down 状态、竞态和 cleanup 失败 | 尚无获准签名 sidecar 与真实 TUN 端到端证据；listener、代理/路由/DNS 恢复、受保护安装、SCM 生命周期、跨用户/低完整性和 Win10/Win11 证据未齐，不能进入 review |
| 2026-07-28 | `VPN-P0-003` | not_started -> in_progress | 负责人：Codex；仅接收已净化配置的原生候选事务、三项健康契约、原子 revision journal、失败补偿、无健康回退清空 ownership、未知 active 清理、16 项 Rust 测试和机器可读顺序门禁落地；Windows 29 步/Linux 25 步全门禁及双桌面 8 秒启动通过 | 无生产 backend、获批订阅下载契约、受保护 revision 写入、真实旁路拨号/DNS 防环、平台 ownership 切换、应用接线、产品 UI 和五平台证据，不能进入 review |
| 2026-07-28 | `VPN-P0-004` | not_started -> in_progress | 公开 selector DTO、backend 回读确认/补偿、受限测速、停止清零流量会话、设置 v3 选择账本、17 项 Rust 与 7 项变异测试落地；Windows 34 步全门禁/桌面启动及 Android 8 步/API 36 回归通过 | 无生产 sing-box backend、生命周期/Tauri/UI 接线、真实节点切换抓包和 Linux/macOS/iOS 证据，不能进入 review |
| 2026-07-28 | `VPN-P0-004` | in_progress -> in_progress | ADR-0002 与受管 `orange-data-plane.exe` 落地；无网络 listener 的 4 KiB stdio 协议直接驱动 sing-box selector、固定 URL 测速/取消和 TCP/UDP 统计，Go 故障测试、双构建及离线 mixed 切换/回读/流量 smoke 通过 | Rust sidecar client、Named Pipe/Tauri/UI、真实 TUN 抓包和跨平台证据未齐，不能进入 review |
| 2026-07-28 | `VPN-P0-004` | in_progress -> in_progress | 严格有界 Rust stdio client 接入 Windows 生产 `DataPlaneNodeBackend`，按请求 ID 分发乱序响应，关联取消并绑定 revision/instance/PID；真实 Rust/Go 进程完成切换、回读、流量和 EOF 回收 | 外层 Named Pipe/共享 runtime、生命周期事件、Tauri/UI、真实签名 TUN 抓包和跨平台证据未齐，不能进入 review |

## 6. 变更记录

| 日期 | 变更 |
| --- | --- |
| 2026-07-26 | 建立模块化文档、69 个功能切片、逐切片验收和初始进度台账。 |
| 2026-07-27 | 完成 `SEC-G0-004` 本地实现并进入 review；Go 模块下载禁止 direct fallback。 |
| 2026-07-27 | 完成 `ARC-G0-002` 本地实现并进入 review；Tauri 自有命令改为版本化 DTO 与最小 ACL。 |
| 2026-07-27 | 完成 `BOOT-G0-001` 本地实现并进入 review；生产资源生成保留为 Gitee secrets 配置点。 |
| 2026-07-27 | 完成 `BOOT-G0-002` 本地实现并进入 review；Rust 受控内存解密、全路径清零与产物泄漏扫描门禁落地。 |
| 2026-07-27 | 推进 `BOOT-G0-003` 本机 PoC；固定 sing-box、无监听 direct-dial、stdio 窄桥、故障注入、Go SBOM/许可证和 CI 审计落地，因抓包与目标平台证据缺失保持 `in_progress`。 |
| 2026-07-27 | 接入桌面 Rust sidecar 宿主和 Tauri 单实例状态；真实 Go handoff、全路径取消/回收及原位清零审计落地，`BOOT-G0-003` 状态不变。 |
| 2026-07-27 | 注册三桌面固定 Control Plane sidecar，接入目标构建、应用内嵌哈希与启动前完整性复验；正式签名和实机证据未齐，`BOOT-G0-003` 状态不变。 |
| 2026-07-27 | 完成 Linux WSL2 Control Plane 无监听、宿主/sidecar、桌面启动和无 Git 元数据全门禁审计；macOS/移动端及发布证据未齐，`BOOT-G0-003` 状态不变。 |
| 2026-07-27 | 开工 `SEC-G0-003`；完成开发端点策略、控制面出网/日志静态门禁和跨平台 secret storage 契约，因生产端点、平台后端与抓包未齐保持 `in_progress`。 |
| 2026-07-27 | 为 `SEC-G0-003` 接入三桌面系统密钥存储；Windows 原生生命周期、Linux 编译和 Android 依赖隔离通过，移动端与非 Windows 运行期证据未齐，状态不变。 |
| 2026-07-27 | 为 `SEC-G0-003` 接入 Android Keystore 密文存储原语和可再生测试链；API 36 模拟器生命周期/篡改/注销后验通过，主链桥接与其余平台证据未齐，状态不变。 |
| 2026-07-27 | 为 `SEC-G0-003` 接入无 WebView 暴露的 Android Rust/Tauri secret backend；API 36 模拟器真实跨语言存取/注销及 4 项测试通过，类型化业务 command 与其余平台证据未齐，状态不变。 |
| 2026-07-27 | 为 `SEC-G0-003` 接入无 WebView 暴露的 iOS Rust/Tauri/Keychain 后端并下沉共享移动协议；Windows 与干净 Android 门禁通过，Apple 编译和生命周期证据未齐，状态不变。 |
| 2026-07-27 | 为 `SEC-G0-003` 补齐隔离 Linux Secret Service 生命周期 runner；真实 GNOME Keyring 存取、覆盖、清零、注销及无残留验证通过，macOS 和 Linux 包装应用图形会话证据未齐，状态不变。 |
| 2026-07-27 | 开工 `SEC-G0-002`；建立跨平台权限白名单、CI 构建快照和硬禁止隐私权限门禁，Android 实际 APK 精确审计通过；其余平台特权实现未齐，保持 `in_progress`。 |
| 2026-07-27 | 完成 `ARC-G0-003` 本地实现并进入 review；双平面状态机、共享权威状态、最小平台 adapter、幂等与事件防回退门禁落地。 |
| 2026-07-27 | 开工 `ARC-P1-004`；完成强类型设置、schema migration、原子代次存储、损坏恢复、revision rollback 与三项用户凭据注销，因五平台卸载残留证据缺失保持 `in_progress`。 |
| 2026-07-27 | 开工 `ARC-P1-005`；先建立事件、任务、诊断和 debug bundle 的原生核心，不开放远程遥测或 WebView 文件能力。 |
| 2026-07-27 | 完成 `ARC-P1-005` 事件、任务与本地诊断基线；三平台验证通过，因生产接线、用户预览导出与正式依赖未收口保持 `in_progress`。 |
| 2026-07-27 | 开工 `BOOT-P0-004`；先建立共享固定路由与桌面强制 Control Plane transport，不开放任意 URL、Authorization 或前端网络能力。 |
| 2026-07-27 | 完成 `BOOT-P0-004` 固定业务路由与桌面强制传输基线；三平台验证通过，因生产 API、真实 command、移动端 transport 和正式依赖未收口保持 `in_progress`。 |
| 2026-07-27 | 开工 `API-G0-001`；先建立不可发布的 clean-room 等价契约和敏感 wire/公开 DTO 分层，不猜测或复制未获批生产模型。 |
| 2026-07-27 | 完成 `API-G0-001` 开发等价契约与脱敏基线；三平台验证通过，因获批生产契约、真实联调和正式依赖未收口保持 `in_progress`。 |
| 2026-07-27 | 开工 `API-P0-002`；先建立 Rust 原生动态配置与认证服务、固定命令和 token 生命周期，不向 WebView 暴露 URL、凭据或任意网络能力。 |
| 2026-07-27 | 完成 `API-P0-002` 动态配置与认证开发基线；三平台静态/构建门禁、双桌面启动和 API 36 回归通过，因生产 API、移动 transport、真实 E2E 与正式依赖未收口保持 `in_progress`。 |
| 2026-07-28 | 开工 `VPN-G0-001`；先建立单一闭合 sing-box JSON 输入、Rust 内部模型与客户端固定模板，不接收完整上游配置、Clash YAML 或本地执行能力。 |
| 2026-07-28 | 完成 `VPN-G0-001` 配置净化开发基线；sing-box 1.13.14 严格兼容、三平台门禁和应用产物泄漏扫描通过，因生产订阅样本、真实 Data Plane 接线、macOS/iOS 证据与正式依赖未收口保持 `in_progress`。 |
| 2026-07-28 | 推进 `WIN-G0-001`；固定受签名官方 sing-box sidecar 单一路径，独立构建/SBOM、manifest、版本/哈希/签名握手及离线 mixed PoC 落地，因签名证书、生产 service 接线和 Win10/Win11 证据未齐保持 `in_progress`。 |
| 2026-07-28 | 开工 `WIN-P0-002`；落地独立 SCM 服务入口、固定版本 DTO、受限 Named Pipe ACL 和客户端原生身份复核，因生产 sidecar backend、安装/恢复流程及双系统证据未齐保持 `in_progress`。 |
| 2026-07-28 | 推进 `WIN-P0-002`；固定 sidecar backend 接入共享 supervisor，完成嵌入式 manifest、revision store、原生签名/哈希/版本校验、固定进程命令和 Job Object 回收，因正式签名/安装/readiness/恢复及双系统证据未齐保持 `in_progress`。 |
| 2026-07-28 | 推进 `WIN-P0-002`；原生 adapter table 固定 TUN readiness、启动前残留拒绝和退出后有界清理验证落地，因真实签名 TUN 全链路、listener、系统设置恢复、安装生命周期及双系统证据未齐保持 `in_progress`。 |
| 2026-07-28 | 开工 `API-P0-003`；先建立账户与公开订阅刷新、溢出安全用量策略、原生订阅凭据隔离和桌面固定命令，不向 React 暴露凭据，也不猜测尚未获批的订阅配置下载契约。 |
| 2026-07-28 | 完成 `API-P0-003` 账户与订阅刷新开发基线；固定原生路由、公开 DTO、订阅凭据回滚/清理、401 与并发门禁及桌面最小权限落地，因 Data Plane 契约、产品 UI、真实后端和跨平台证据未齐保持 `in_progress`。 |
| 2026-07-28 | 开工并推进 `VPN-P0-003`；原生候选事务、三项健康检查契约、原子 revision journal、崩溃恢复与静态门禁落地，因生产 backend、真实旁路探测、应用接线、UI 和五平台证据未齐保持 `in_progress`。 |
| 2026-07-28 | 推进 `API-P0-003`；接通停止 Data Plane 优先的原生注销、严格桌面命令、最小 capability、失败重试与顺序门禁，因生产 adapter、移动业务 handler、产品 UI、真实后端和五平台证据未齐保持 `in_progress`。 |
| 2026-07-28 | 开工并推进 `UI-G0-001`；建立亮暗主题设计 Token、移动/平板/桌面静态连接首页、五视口截图和资源/词表/布局门禁，因原生平台截图与正式设计审批未齐保持 `in_progress`。 |
| 2026-07-28 | 开工并推进 `UI-G0-002`；建立显式源路径资产白名单、确定性元数据清洗、Lottie/字体/大资源拒绝、许可证台账和 CI 资源交叉审计，因正式品牌与第三方授权资产未齐保持 `in_progress`。 |
| 2026-07-28 | 开工 `UI-P0-003`；接入 Hash 路由、启动恢复和认证守卫，复用原生固定业务命令，不新增浏览器网络、存储或移动权限面。 |
| 2026-07-28 | 推进 `UI-P0-003`；完成登录/注册、五项导航、退出确认、Toast、异步状态、安全 ErrorBoundary 与静态突变门禁，因真实后端、移动原生 handler、Apple 运行证据和正式依赖未齐保持 `in_progress`。 |
| 2026-07-28 | 开工并推进 `VPN-P0-004`；建立仅含 selector 成员的公开目录、强制 backend 回读选择、受限测速、停止清零流量会话和设置 v3 选择账本，因生产 sing-box backend、生命周期/UI、真实抓包及三平台证据未齐保持 `in_progress`。 |
| 2026-07-28 | 推进 `VPN-P0-004` 与 Windows 宿主；以组合官方 sing-box 1.13.14 的受管 sidecar 取代无控制面的上游 CLI，完成窄 stdio selector/测速/流量协议、最小注册表、双构建和真实 mixed 流量 smoke；因 Rust client、TUN 抓包、产品接线和跨平台证据未齐保持 `in_progress`。 |
| 2026-07-28 | 继续推进 `VPN-P0-004`；Windows service 以严格 Rust stdio client 绑定当前受管宿主并实现生产节点 backend，真实 Rust/Go 互操作通过；因外层 Named Pipe/共享 runtime、生命周期事件、Tauri/UI、真实签名 TUN 与跨平台证据未齐保持 `in_progress`。 |
| 2026-07-28 | 继续推进 `VPN-P0-004`；受限 Windows Named Pipe 以 10 个固定命令接通节点选择/回读、begin/poll/cancel 测速和权威流量，完成取消竞态收口与真实管道往返；因共享 runtime、生命周期事件、Tauri/UI、真实签名 TUN 与跨平台证据未齐保持 `in_progress`。 |
| 2026-07-28 | 继续推进 `VPN-P0-004`；建立共享节点 runtime 的原子安装/清除边界，候选 revision 恢复成功后才发布，失败保留旧 runtime，并以读写锁阻止活动测速与重配置交错；因 Windows 应用尚未注入 installation ID/活动配置，生命周期事件、Tauri/UI、真实签名 TUN 与跨平台证据未齐保持 `in_progress`。 |
| 2026-07-28 | 继续推进 `VPN-P0-004`；Windows 应用只从固定同目录 installer 身份文件建立原生 `NamedPipeClient`，同一 client 同时进入生命周期 adapter 与共享节点 runtime host，缺失/非法身份保持未配置；因真实 installer/ACL、活动净化配置 handoff、生命周期事件、Tauri/UI、真实签名 TUN 与跨平台证据未齐保持 `in_progress`。 |
| 2026-07-28 | 继续推进 `VPN-P0-003`/`VPN-P0-004`；订阅事务在 revision journal commit 后只向 `ActiveDataPlaneNodeRuntime` 交接公开 selector 目录，Windows host 已实现 sink，安装失败清理旧 runtime、同 revision 重试并在恢复时对账 revision；34 步 quality、8 步 Android、4 步桌面及独立 8 秒无残留检查通过；生产 backend/获批激活源、生命周期事件、Tauri/UI、真实签名 TUN 与跨平台证据未齐，状态保持 `in_progress`。 |
| 2026-07-28 | 继续推进 `VPN-P0-004`/`ARC-P1-005`；Windows host 从同一受限 Named Pipe 回读生命周期、从已安装 runtime 读取流量，以统一实例序列写入 64/256 有界原生 hub；500 ms monitor 使用可取消 Data/background task，退出唤醒并 join；无 WebView emitter/新 command/capability，因生产订阅源、UI、真实签名 TUN 与跨平台证据未齐保持 `in_progress`。 |
