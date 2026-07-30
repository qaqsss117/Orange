# Orange 开发进度

> 更新日期：2026-07-31
> 产品切片：69  
> 已完成：10
> 状态统计：done 10 / review 4 / in_progress 17 / blocked 1 / not_started 37
> 当前阶段：10 个切片已按自身验收规则闭环；本轮生产参数补齐后，Windows 10 22H2 未签名开发包已完成真实后端登录/订阅、受限 Service IPC、Data Plane 生命周期、首页主流程、系统代理、TUN、四类崩溃、跨用户/低完整性拒绝、升级失败回滚、正常升级，以及卸载保留/删除配置与原生凭据清理。正式签名、真实重启、Win11、远端 CI 和其他平台实现继续由对应切片跟踪

状态定义见 [docs/README.md](docs/README.md)。没有验收证据的切片不得标记 `done`。

## 1. 总览

| 模块 | 切片数 | done | 当前状态 | 文档 |
| --- | ---: | ---: | --- | --- |
| 安全与隐私 | 5 | 2 | in_progress | [01](docs/01-security-privacy.md) |
| 共享架构 | 5 | 2 | in_progress | [02](docs/02-shared-architecture.md) |
| Bootstrap Control Plane | 6 | 1 | in_progress | [03](docs/03-bootstrap-control-plane.md) |
| sing-box Data Plane | 6 | 2 | in_progress | [04](docs/04-singbox-data-plane.md) |
| 业务 API | 6 | 1 | in_progress | [05](docs/05-business-api.md) |
| UI 与资产 | 8 | 1 | in_progress | [06](docs/06-ui-assets.md) |
| Android | 5 | 0 | not_started | [07](docs/07-platform-android.md) |
| Apple | 6 | 0 | not_started | [08](docs/08-platform-apple.md) |
| Windows | 5 | 1 | in_progress | [09](docs/09-platform-windows.md) |
| Linux | 5 | 0 | not_started | [10](docs/10-platform-linux.md) |
| 规则与地理数据 | 5 | 0 | not_started | [11](docs/11-rules-geo-data.md) |
| 测试与发布 | 7 | 0 | in_progress | [12](docs/12-testing-release.md) |

## 2. 当前队列

| 顺序 | 切片 | 状态 | 下一检查点 |
| ---: | --- | --- | --- |
| 1 | `SEC-G0-001` 不可信源隔离 | done | 扫描、独立副本和迁移清单证据已登记 |
| 2 | `ARC-G0-001` 五平台 Workspace | blocked | Gitee Go 适配文件已完成；等待推送后的远端运行链接及 macOS/iOS runner 证据 |
| 3 | `SEC-G0-004` 供应链与资源清单 | done | 810 组件、59 资源、836 项依赖/7 生态的锁定、许可证、来源、哈希、签名状态和禁用依赖门禁已通过；后续新增平台产物由同一策略强制登记 |
| 4 | `ARC-G0-002` DTO、错误与命令边界 | done | 版本化双命令契约、九类脱敏错误、双向 fixture、默认拒绝和最小 capability 已逐条验收 |
| 5 | `BOOT-G0-001` Bootstrap 包格式 | review | 严格 VLESS Reality schema、生产密文构建注入和认证后嵌入边界已落地；正式密文仍只保存在忽略产物目录，待批准 CI secret 注入 |
| 6 | `BOOT-G0-002` Rust 内存解密与清零 | done | 生产密文解密、原位清零、panic/error 清零、真实 Go handoff、桌面启动接线和泄漏门禁已逐条验收；CI secret 配置继续归 `BOOT-G0-001` |
| 7 | `BOOT-G0-003` 无端口 sing-box Direct-Dial PoC | in_progress | VLESS Reality/uTLS 与 Windows 最小 `SystemRoot` sidecar 环境已落地；轮换后的真实密文经 audited sidecar 访问既有 API 主机返回 HTTP 200，待正式 API 契约、抓包和跨平台发布证据 |
| 8 | `SEC-G0-003` 控制面出网与敏感数据 | in_progress | 四条生产业务 command 与订阅下载已通过桌面 Control Plane 去敏验证；Windows 卸载复用原生 `DesktopSecretStore` 清空三项生产凭据，即使保留普通设置也不残留；待其余生产 command、Android 真机/API 矩阵、Apple 运行期、Linux 包装应用图形会话集成与真实抓包 |
| 9 | `SEC-G0-002` 跨平台权限白名单 | in_progress | 机器可读开发壳白名单、权限声明发现、硬禁止隐私权限、Tauri capability、Android 合并 APK 快照，以及 Windows 原生 Named Pipe ACL/身份门禁和安装后跨用户/低完整性独立进程拒绝已通过；待 Apple 包、正式签名 Windows/Win11、Linux helper 与单文件临时授权证据 |
| 10 | `ARC-G0-003` 双平面状态机与 Adapter | done | 双状态机、平台 adapter、实例/序列防回退、只读状态命令和故障 mock 已逐条验收；具体平台 TUN 明确不在本切片范围 |
| 11 | `ARC-P1-004` 持久化、迁移与回滚 | in_progress | Windows 10 已证明默认卸载保留普通设置、显式删除移除两处固定 app-data，且两条路径均清空三项生产凭据；待正式签名 Windows 及 Linux/Android/iOS/macOS 安装卸载后验 |
| 12 | `ARC-P1-005` 事件、任务与可观测性 | in_progress | Windows Data Plane 状态/流量生产者、统一序列、有界原生 hub、可取消后台 task 与 WebView 严格消费已接线；待 Control Plane/其他平台生产者、UI 预览导出和正式前置收口 |
| 13 | `BOOT-P0-004` BootstrapTransport 强制路由 | in_progress | 生产 config/login/account/subscription 与敏感订阅下载均经桌面 Rust/Go Control Plane 验证，安全下载边界已接入 Rust client；注册及其余路由不猜测，待完整生产契约和移动端嵌入式实现 |
| 14 | `API-G0-001` 接口契约与脱敏 Fixture | in_progress | 开发 v1 等价 schema、全端点 wire/public DTO、结构化脱敏 fixture、失败矩阵与静态门禁已完成，三平台验证通过；待获批生产 OpenAPI/后端联调与正式前置收口 |
| 15 | `API-P0-002` 动态配置、登录与注册 | in_progress | 生产 config/login/account 严格 DTO 和真实桌面联调通过；生产注册未验证并 fail closed，待注册契约、移动 transport、安装/离线 E2E 与正式依赖收口 |
| 16 | `API-P0-003` 账户与订阅 | done | 生产账户/订阅、原生正文下载与激活、溢出安全用量、失效订阅启动门禁、刷新状态、注销顺序及同 Control Plane 重新登录已逐条验收 |
| 17 | `VPN-G0-001` 纯 sing-box 配置模型与净化 | done | 闭合 JSON/Base64 VLESS 输入、内部模型重建、危险能力拒绝、字段级脱敏错误、SBOM/产物禁入及 sing-box 1.13.14 严格解析均通过，安装应用 mixed/TUN 真实出网补齐运行证据 |
| 18 | `VPN-P0-003` 订阅预启动与原子切换 | in_progress | 安装应用已完成真实登录/刷新、VLESS 净化、候选探测、revision 激活、模式切换、正常升级保留和失败升级回滚；待无中断切换与五平台验证 |
| 19 | `UI-G0-001` 设计 Token 与页面基线 | in_progress | 亮暗主题、命名 Token、移动/平板/桌面分层布局和五视口截图已落地；待原生平台截图、设计审批与正式品牌资产 |
| 20 | `UI-G0-002` 资产白名单与转换 | in_progress | 严格白名单、PNG/JPEG/WebP 元数据清洗、Lottie 拒绝规则、许可证记录和全目录资源门禁已落地；待正式品牌、第三方 Banner 授权与专有图形清单 |
| 21 | `UI-P0-003` App Shell、认证与通用状态 | in_progress | Hash 路由、启动恢复、严格认证守卫、登录/注册、五项导航、退出确认和通用状态已落地；待真实后端、移动原生 handler、macOS/iOS 与正式依赖收口 |
| 22 | `VPN-P0-004` Selector、测速与流量 | in_progress | Windows 生产 18 节点已完成 8 并发测速、选择/core 回读、重启恢复、删除节点回退及安装态 TUN 切换抓包，切换后 Control Plane 在线；待 Linux/macOS/iOS backend 和运行证据 |
| 23 | `UI-P0-004` 首页与连接主流程 | done | 生产订阅/激活驱动真实 mixed/TUN；权威状态、八种生命周期、到期/耗尽提示、双锁、流量归零、本地 Banner 与五视口基线逐条通过 |
| 24 | `UI-P0-005` 订阅、节点与配置页面 | in_progress | 安装应用已由用户输入真实账号并完成订阅刷新、节点目录和系统代理/TUN 切换；待节点选择持久化专项、完整生产测速和跨平台验收 |
| 25 | `WIN-P0-003` WinINET 系统代理与恢复 | review | 固定 mixed 监听、国内/海外 HTTPS、出口变化及 UI/Data Plane/Service 崩溃后的安全代理恢复通过；实现完成，验收规则 4 的真实系统重启仍待执行 |
| 26 | `WIN-P1-004` Windows TUN/Wintun | in_progress | 固定接口/双栈地址、严格路由、DoT DNS、国内/海外 HTTPS、出口变化、安装态节点切换抓包、Control Plane 防环和停止清理通过；待正式组件签名、真实重启、睡眠/唤醒、网卡切换、VPN 冲突、mixed 回退及 Win11 |
| 27 | `WIN-P1-005` 托盘、安装、升级与卸载 | in_progress | 未签名基线/候选完成 build/install/ipc-boundary/proxy/tun/四类 crash/upgrade-failure/upgrade；卸载已实际覆盖默认保留、原生凭据清空、重装后显式删除和最终 verify-clean；待正式签名、真实重启及 Win11 |
| 28 | `QA-G0-001` CI 基础门禁 | review | Windows 10 固定工具链下 35 步本地 quality 通过；等待远端 CI 运行链接和其他平台 runner 证据 |
| 29 | `QA-P0-002` 单元、契约与故障注入 | in_progress | 203 项 Python 安全/变异、54 项前端、Rust workspace、两套 Go、真实生产链、安装态四类进程故障、跨用户/低完整性、升级回滚及卸载保留/删除变异通过；待真实重启及其他平台证据 |

## 3. 切片明细

### 安全与隐私

| ID | 摘要 | 状态 | 证据/备注 |
| --- | --- | --- | --- |
| `SEC-G0-001` | 不可信源隔离 | done | `SECURITY.md`、`docs/migration-inventory.md`、508 项资源清单；扫描/测试通过，独立副本日志无原工程路径 |
| `SEC-G0-002` | 跨平台权限白名单 | in_progress | `security/platform-permissions.yml`、跨平台声明/构建快照门禁、Android 实际 APK 精确权限审计，以及 Windows SYSTEM/service SID/安装用户 DACL、medium integrity label、PID/令牌/映像复核与安装后跨用户/低完整性独立进程拒绝通过；证据见 `docs/evidence/SEC-G0-002-permission-baseline-2026-07-27.md`、`docs/evidence/WIN-P0-002-windows-service-ipc-2026-07-28.md` 和 `docs/evidence/WIN-P1-005-windows-development-acceptance-2026-07-30.md`；待 Apple 包、正式签名 Windows/Win11、Linux helper/polkit/systemd 与单文件临时授权证据 |
| `SEC-G0-003` | 控制面出网与敏感数据 | in_progress | 固定 token key、自动清零、平台注销覆写、三桌面系统密钥存储及 Android/iOS native 后端已落地；生产 config/login/account/subscription 和订阅正文下载经桌面 Rust/Go Control Plane 去敏验证，Windows Credential Manager、隔离 Linux Secret Service 与 Android API 36 往返通过；Windows 原生卸载 helper 复用生产 `DesktopSecretStore` 清空三项固定凭据，安装态只读状态探针通过；证据见 `docs/evidence/SEC-G0-003-control-egress-2026-07-27.md`、`docs/evidence/API-P0-003-production-business-vless-2026-07-28.md` 与 `docs/evidence/WIN-P1-005-windows-development-acceptance-2026-07-30.md`；待其余生产 command、移动/Apple 运行期、Linux 图形会话集成与真实抓包 |
| `SEC-G0-004` | 供应链、SBOM 与资源签名 | done | 810 组件、59 资源、7 生态的 SBOM 及 836 项依赖策略通过，当前全部依赖、资源和产物均由来源/许可证/哈希/签名状态门禁覆盖；原生产物 manifest 继续保持不可发布，正式发布签名由 `REL-P1-005` 验收；证据见 `docs/evidence/SEC-G0-004-supply-chain-2026-07-27.md` 与 `docs/evidence/QA-G0-001-windows-quality-2026-07-30.md` |
| `SEC-P1-005` | 运行时隐私专项 | not_started | 发布前执行 |

### 共享架构

| ID | 摘要 | 状态 | 证据/备注 |
| --- | --- | --- | --- |
| `ARC-G0-001` | 五平台 Workspace 与工具链 | blocked | Windows/Linux/Android 空壳构建和启动通过；供应商无关 CI 入口与国内镜像验证见 `docs/evidence/ARC-G0-001-ci-portability-2026-07-27.md`；缺少 macOS 构建机、iOS 模拟器和有运行链接的远端 CI |
| `ARC-G0-002` | DTO、错误与命令边界 | done | 版本化 schema、9 类脱敏错误、固定命令 ACL、未知字段/enum 策略、Rust/TypeScript 双向 fixture 和默认拒绝均通过；证据见 `docs/evidence/ARC-G0-002-contract-boundary-2026-07-27.md` |
| `ARC-G0-003` | 双平面状态机与 Adapter | done | Control/Data 独立状态机、共享 Control 状态、`PlatformVpnAdapter`、幂等控制器、权威快照恢复、实例/序列防回退、只读 `get_plane_state` 和六类故障 mock 已逐条验收；具体平台 TUN 属于平台切片；证据见 `docs/evidence/ARC-G0-003-dual-plane-state-2026-07-27.md` |
| `ARC-P1-004` | 持久化、迁移与回滚 | in_progress | 强类型设置、原子代次文件、migration/损坏/future-schema、revision 回滚账本和注销已落地；Windows 10 NSIS 已实际证明默认保留与显式删除两处固定 app-data，并在两条路径清空三项生产凭据；无新增 WebView command/capability；证据见 `docs/evidence/ARC-P1-004-persistence-2026-07-27.md` 与 `docs/evidence/WIN-P1-005-windows-development-acceptance-2026-07-30.md`；待正式签名 Windows 及 Linux/Android/iOS/macOS 安装卸载后验 |
| `ARC-P1-005` | 事件、任务与可观测性 | in_progress | 版本化 envelope、旧实例/乱序过滤、单待发流量节流、有限 task registry、分类诊断与确认式 bundle 已落地；Windows host 将生命周期/流量写入 64/256 有界 hub，桌面主窗口通过最小只读 capability 每 500 ms 拉取严格快照；保持无 WebView emitter、文件权限或遥测；证据见 `docs/evidence/ARC-P1-005-observability-2026-07-27.md`；待 Control Plane/其他平台生产者、UI 预览导出和正式前置收口 |

### Bootstrap Control Plane

| ID | 摘要 | 状态 | 证据/备注 |
| --- | --- | --- | --- |
| `BOOT-G0-001` | Bootstrap 包格式与构建加密 | review | 严格 schema、VLESS Reality、随机 XChaCha20-Poly1305、zeroize CLI、manifest、生产密文构建注入与静态/变异门禁；证据见 `docs/evidence/BOOT-G0-001-bootstrap-envelope-2026-07-27.md` 和 `docs/evidence/BOOT-G0-003-production-bootstrap-2026-07-28.md` |
| `BOOT-G0-002` | Rust 内存解密与清零 | done | 生产 `decrypt`、受控 `SecretBuffer`、schema/过期校验、consume/Drop/panic/error 清零、桌面生产嵌入/启动、真实 Go handoff、Debug 脱敏与产物泄漏扫描均通过；证据见 `docs/evidence/BOOT-G0-002-memory-decrypt-2026-07-27.md` 和 `docs/evidence/BOOT-G0-003-production-bootstrap-2026-07-28.md` |
| `BOOT-G0-003` | 无端口 sing-box Direct-Dial PoC | in_progress | 固定 sing-box `v1.13.14`、VLESS Reality/uTLS、stdio 窄桥、startup DNS、Rust sidecar 宿主、Windows 最小 `SystemRoot` 环境、三桌面 `externalBin`/哈希校验与 fail-closed 已落地；轮换后的真实密文完成 Rust host/Go sidecar/Reality 链路并返回 HTTP 200；证据见 `docs/evidence/BOOT-G0-003-direct-dial-2026-07-27.md`、`docs/evidence/BOOT-G0-003-linux-runtime-2026-07-27.md` 和 `docs/evidence/BOOT-G0-003-production-bootstrap-2026-07-28.md`；待正式 API 契约、抓包和跨平台发布审计 |
| `BOOT-P0-004` | BootstrapTransport 强制路由 | in_progress | 单一 transport client、Rust 安全存储 token 注入、1 MiB 上限、桌面 stdio/Go Bearer 接线与静态门禁完成；生产 config/login/account/subscription 和 allowlisted 订阅下载真实通过，无新增 WebView 网络能力；证据见 `docs/evidence/BOOT-P0-004-bootstrap-transport-2026-07-27.md` 与 `docs/evidence/API-P0-003-production-business-vless-2026-07-28.md`；待注册/其余生产路由、移动嵌入式实现和正式前置收口 |
| `BOOT-P0-005` | 节点故障切换与 Fail-Closed | not_started |  |
| `BOOT-P1-006` | 签名更新、轮换与防回滚 | not_started |  |

### sing-box Data Plane

| ID | 摘要 | 状态 | 证据/备注 |
| --- | --- | --- | --- |
| `VPN-G0-001` | 纯 sing-box 配置模型与净化 | done | 闭合 JSON/Base64 VLESS 均先进入内部模型；固定 DoT、sniff 后 DNS hijack、敏感缓冲清零、危险能力拒绝、字段级脱敏错误、Go 1.13.14 严格解析和产物禁入扫描通过，安装应用的 mixed/TUN 真实出网已验证；macOS/iOS 生命周期与正式签名分别由平台/发布切片验收；证据见 `docs/evidence/VPN-G0-001-data-plane-config-2026-07-28.md`、`docs/evidence/API-P0-003-production-business-vless-2026-07-28.md` 与 `docs/evidence/WIN-P1-005-windows-development-acceptance-2026-07-30.md` |
| `VPN-P0-002` | Data Plane 生命周期 | done | 原生监管器、20 轮重复启停、2 秒崩溃识别、WebView 重建回读、强制回收、幂等 cleanup 与 Control Plane 隔离通过；Windows 10 安装态生产 revision 的 mixed/TUN、四类进程故障及代理/路由/DNS/端口恢复补齐真实证据；其他平台 backend 归各平台切片，见 `docs/evidence/P0-production-slice-acceptance-2026-07-31.md` |
| `VPN-P0-003` | 订阅预启动与原子切换 | in_progress | 安装应用真实登录/刷新已完成安全下载、VLESS 净化、分块 revision、回环候选探测和 mixed/TUN 激活；正常候选升级保留安装身份与 active revision，专用故障包在 payload 替换后注入失败并完整恢复六个文件、服务、身份、revision 与显示版本；未签名测试特性不改变发布资格；证据见 `docs/evidence/VPN-P0-003-subscription-pipeline-2026-07-28.md`、`docs/evidence/VPN-P0-003-windows-activation-2026-07-28.md` 与 `docs/evidence/WIN-P1-005-windows-development-acceptance-2026-07-30.md`；待无中断切换和五平台验证 |
| `VPN-P0-004` | Selector、测速与流量 | in_progress | 净化目录、回读/补偿/持久化、64 项/8 并发测速和单调节流流量核心已完成；Windows 生产 18 节点真实通过有界测速、非默认选择/core 回读、进程重启恢复、删除节点默认回退，以及安装态 TUN 切换前后 HTTPS/流量、Wintun 组件抓包和切换后 Control Plane 请求；应用释放检查点后完整停止、清凭据并卸载；证据见 `docs/evidence/VPN-P0-004-production-node-acceptance-2026-07-31.md`、`docs/evidence/VPN-P0-004-windows-tun-node-switch-2026-07-31.md`、`docs/evidence/VPN-P0-004-windows-managed-host-2026-07-28.md`、`docs/evidence/UI-P0-004-connection-home-2026-07-28.md` 与 `docs/evidence/WIN-P1-005-windows-development-acceptance-2026-07-30.md`；待 Linux/macOS/iOS backend 和运行证据 |
| `VPN-P1-005` | 桌面 Mixed 与系统代理契约 | not_started |  |
| `VPN-P1-006` | 双平面隔离与路由防环 | not_started |  |

### 业务 API

| ID | 摘要 | 状态 | 证据/备注 |
| --- | --- | --- | --- |
| `API-G0-001` | 接口契约与脱敏 Fixture | in_progress | clean-room v1 契约、严格 DTO、脱敏 fixture 与失败矩阵已落地；生产 config/login/account/subscription 的 envelope、字段类型和真实链路已去敏验证；证据见 `docs/evidence/API-G0-001-business-contract-2026-07-27.md` 与 `docs/evidence/API-P0-003-production-business-vless-2026-07-28.md`；待获批完整 OpenAPI、注册/其余端点与错误语义收口 |
| `API-P0-002` | 动态配置、登录与注册 | in_progress | Control Plane ready、严格生产 config/login/account 映射、原子凭据替换/回滚及 401 清理已落地并完成真实桌面登录；生产注册未验证且 fail closed；证据见 `docs/evidence/API-P0-002-authentication-2026-07-27.md` 与 `docs/evidence/API-P0-003-production-business-vless-2026-07-28.md`；待注册契约、移动 transport、安装/离线 E2E 与正式依赖收口 |
| `API-P0-003` | 账户与订阅 | done | 生产账户/订阅字段、溢出安全用量、敏感正文原生 pipeline、失效订阅启动门禁、手动刷新三态/并发锁、停止 Data Plane 后注销及同 Control Plane 重新登录全部通过；移动/其他桌面接线归平台切片，见 `docs/evidence/P0-production-slice-acceptance-2026-07-31.md` |
| `API-P1-004` | 套餐、订单与支付 | not_started |  |
| `API-P1-005` | 邀请与工单 | not_started |  |
| `API-P2-006` | 缓存、离线与恢复 | not_started |  |

### UI 与资产

| ID | 摘要 | 状态 | 证据/备注 |
| --- | --- | --- | --- |
| `UI-G0-001` | 设计 Token 与页面基线 | in_progress | 颜色/字号/间距/圆角/阴影/状态/安全区 Token、180px 移动横幅、连接中心、模式/节点入口及 1024px 桌面侧栏断点已落地；360×800、412×915、768×1024、1366×768、1440×900 浏览器基线覆盖亮暗主题、130% 字体和减少动画，图片进入资源哈希审计；证据见 `docs/evidence/UI-G0-001-design-baseline-2026-07-28.md`；待 Android/iOS/macOS 原生截图、正式设计审批与正式品牌资产 |
| `UI-G0-002` | 资产白名单与转换 | in_progress | `docs/asset-allowlist.yml`、严格 schema、确定性 PNG/JPEG/WebP 清洗、Lottie URL/脚本/隐藏二进制/图片拒绝、512 KiB 上限、资源清单交叉校验与许可证记录已落地；当前仅开发标识获准且不可发布，待正式品牌、第三方 Banner 授权及明确专有图形后才能完成；证据见 `docs/evidence/UI-G0-002-asset-pipeline-2026-07-28.md` |
| `UI-P0-003` | App Shell、认证与通用状态 | in_progress | `HashRouter`、启动 loading/error/retry、三态会话守卫、登录/注册校验与提交锁、五项受保护导航、退出 Dialog、Toast、空态及安全 ErrorBoundary 已接通桌面固定命令；浏览器固定模式、15 项 React 测试和 UI 壳静态/突变门禁已落地；证据见 `docs/evidence/UI-P0-003-app-shell-2026-07-28.md`；待真实后端 E2E、Android/iOS 原生 handler、macOS/iOS 运行证据及正式依赖收口 |
| `UI-P0-004` | 首页与连接主流程 | done | 首页只采用原生权威状态与回读，覆盖八种生命周期和订阅到期/耗尽；双重操作锁、失败重试、非在线流量归零、本地白名单 Banner、五视口布局及 Windows 生产 mixed/TUN 主流程通过，见 `docs/evidence/P0-production-slice-acceptance-2026-07-31.md` |
| `UI-P0-005` | 订阅、节点与配置页面 | in_progress | 安装应用已由用户手动输入真实账号并完成订阅刷新、节点目录、候选探测及系统代理/TUN 模式切换；54 项前端测试和静态门禁通过，秘密未进入报告；待节点选择持久化专项、完整生产测速和跨平台验收；证据见 `docs/evidence/WIN-P1-005-windows-development-acceptance-2026-07-30.md` |
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
| `WIN-G0-001` | 产物与核心宿主决策 | review | ADR-0002 固定受签名 `orange-data-plane.exe` 单一路径；宿主组合官方 sing-box 1.13.14，仅注册 TUN/mixed、受限节点协议、selector/direct 及 local/TLS DNS，以继承 stdio 提供窄控制面；可复现哈希、metadata、版本/标签/CGO/Authenticode、manifest、mixed smoke、安装后哈希链和 Win10 22H2 通过；实现完成，验收规则 3/5 的正式签名证书/获准指纹和 Win11 结果仍待外部矩阵 |
| `WIN-P0-002` | Service、Named Pipe 与双平面 | done | 受限 Named Pipe 的跨用户/低完整性拒绝、固定 DTO、UI 重建权威回读、service crash 安全恢复、Control/Data listener 边界及 SCM 安装/升级/卸载无孤儿逐条通过；签名、Win11 和重启分别归 `WIN-G0-001`、`WIN-P0-003`、`REL-P1-005`，见 `docs/evidence/P0-production-slice-acceptance-2026-07-31.md` |
| `WIN-P0-003` | WinINET 系统代理与恢复 | review | 固定 `127.0.0.1:24836` 的真实国内/海外 HTTPS 与出口变化通过；UI/Data Plane/Service 强制终止、升级和卸载均恢复为安全网络状态，所有权保护与失败关闭有自动化覆盖；实现完成，验收规则 4 的真实系统重启仍待执行；证据见 `docs/evidence/WIN-P1-005-windows-development-acceptance-2026-07-30.md` |
| `WIN-P1-004` | Windows TUN/Wintun | in_progress | 固定双栈接口、auto/strict route、固定 DoT、sniff/DNS hijack、真实 DNS、国内/海外 HTTPS、出口变化及停止清理通过；生产 18 节点又完成 Wintun 组件抓包、非默认选择/core 回读、切换前后流量与切换后 Control Plane 请求；证据见 `docs/evidence/WIN-P1-005-windows-development-acceptance-2026-07-30.md` 与 `docs/evidence/VPN-P0-004-windows-tun-node-switch-2026-07-31.md`；待正式组件签名、真实重启、睡眠/唤醒、网卡切换、VPN 冲突、mixed 回退与 Win11 |
| `WIN-P1-005` | 托盘、安装、升级与卸载 | in_progress | schema-v1 可续跑阶段全部通过：未签名构包、安装策略、ipc-boundary、proxy、TUN、四类 crash、升级失败完整回滚与正常升级；卸载先以 `/S` 证明两处固定配置保留且三项凭据清空，再重装并以 `/S /DELETEAPPDATA` 证明显式删除，最终 clean-state 通过；候选/阶段哈希已登记，`release_allowed=false`；待正式签名、真实重启及 Win11；证据见 `docs/evidence/WIN-P1-005-windows-development-acceptance-2026-07-30.md` |

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
| `QA-G0-001` | CI 基础门禁 | review | Windows 10 22H2 上 Node 22.23.1、pnpm 11.9.0、Rust/Cargo 1.95.0、Go 1.25.5 的顶层 35 步 `quality` 通过；等待远端 CI 运行链接与非 Windows runner 证据，见 `docs/evidence/QA-G0-001-windows-quality-2026-07-30.md` |
| `QA-P0-002` | 单元、契约与故障注入 | in_progress | 203 项 Python 安全/变异、54 项前端、Rust workspace fmt/clippy/test/build、双 Go module、Windows Data Plane/Bootstrap、真实生产链、生产 18 节点选择/测速/恢复、安装态四类进程故障、独立跨用户/低完整性、升级回滚及卸载数据选择变异通过；待真实重启和其他平台验收；证据见 `docs/evidence/QA-G0-001-windows-quality-2026-07-30.md`、`docs/evidence/VPN-P0-004-production-node-acceptance-2026-07-31.md` 与 `docs/evidence/WIN-P1-005-windows-development-acceptance-2026-07-30.md` |
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
| 后端 sing-box JSON | 部分确认 | 真实 VLESS Reality 订阅已由转换层闭合净化；其余协议/字段仍需获批 fixture |
| Bootstrap 节点与密钥系统 | 本地已配置 | 本地忽略输入与未签名构建已验证；仍需在远端 CI 配置受管 `ORANGE_BOOTSTRAP_*` secrets 和正式轮换策略 |
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
| 2026-07-28 | `BOOT-G0-001`/`BOOT-G0-002`/`BOOT-G0-003` | review/review/in_progress -> review/review/in_progress | 严格 VLESS Reality、生产密文认证后桌面嵌入、Tauri 启动、Windows 最小 `SystemRoot` 环境和脱敏 release probe 落地；配置版本 2 经真实 sidecar 访问既有 API 主机返回 HTTP 200 | 正式面板 API 路由/DTO 未获批准，不能猜测；管理员抓包、macOS/移动端运行和签名发布证据仍待完成 |
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
| 2026-07-30 | `SEC-G0-004`、`ARC-G0-002`、`ARC-G0-003`、`BOOT-G0-002` | review -> done | 按各自六条验收规则复核现有代码、负向测试、产物/SBOM、平台无关契约及已登记证据，全部通过；不再把 `ARC-G0-001` 的远端/Apple 矩阵重复挂入这些基础切片 | 新增依赖、命令、capability、原生产物或密钥生命周期变化会重新打开对应切片 |
| 2026-07-30 | `VPN-G0-001` | in_progress -> done | 获批生产 VLESS 样本已进入闭合内部模型并重新生成配置，严格解析、危险能力拒绝、脱敏、SBOM/产物扫描和 Windows 安装态 mixed/TUN 运行证据齐全 | 其他平台生命周期及正式签名继续由平台/发布切片验收；配置字段扩展会重新打开本切片 |
| 2026-07-30 | `WIN-G0-001`、`WIN-P0-003` | in_progress -> review | 核心宿主/签名握手实现和 WinINET 所有权/恢复实现已完成，Windows 10 22H2 安装态链路通过 | `WIN-G0-001` 待正式签名与 Win11；`WIN-P0-003` 待真实系统重启，均有未通过的明确验收条款，不能标记 `done` |
| 2026-07-31 | `WIN-P1-005`、`ARC-P1-004`、`SEC-G0-003` | in_progress -> in_progress | NSIS 保留交互删除数据复选框，静默 `/S` 默认保留，显式 `/DELETEAPPDATA` 才删除两处固定目录；默认卸载、候选重装、显式删除和原生三凭据清理均通过安装态探针 | Windows 规则 5 已闭环；正式签名、Win11、真实重启和其他平台安装卸载矩阵仍未完成 |
| 2026-07-31 | `WIN-P0-002`、`VPN-P0-002`、`API-P0-003`、`UI-P0-004` | in_progress -> done | 逐条复核 24 条验收规则；生产 Windows 10 安装态 IPC/生命周期/账户订阅/首页 mixed/TUN 证据齐全，并补上失效订阅不能复用旧 revision、在线失效仍可明确断开和注销后同 Control Plane 重新登录门禁 | 签名、Win11、真实重启、其他平台 backend/原生截图仍由各平台与发布切片跟踪；相关共享边界或当前生产证据回归会重新打开对应切片 |

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
| 2026-07-28 | 开工 `UI-P0-004` 并继续推进 `VPN-P0-004`/`ARC-P1-005`；新增桌面专用只读事件快照命令和最小 capability，首页每 500 ms 并行读取权威状态与有界快照，以严格实例/序列游标显示八种状态和流量，非在线或失败时速度归零且连接按钮保持禁用；因启停、生产订阅/节点数据、真实 TUN 与跨平台证据未齐保持 `in_progress`。 |
| 2026-07-28 | 验证 `UI-P0-004` 首页只读状态增量；360×800、768×1024、1366×768 浏览器几何/截图无重叠、截断或告警，35 步 quality、8 步 Android、4 步桌面和独立 8 秒零残留后验通过；Android 真机未重复，启停、生产订阅/节点数据、真实 TUN 与五平台证据仍待完成。 |
| 2026-07-28 | 继续推进 `UI-P0-004`/`VPN-P0-002`；新增只允许 status/start/stop 的桌面控制命令，start revision 只读原生 Windows runtime、stop 可清理遗留活动实例，原子 guard 覆盖操作与权威回读并与前端 ref 双重拒绝重复 mutation，UI 只采用原生 canStart/canStop 和操作回读；360×800、768×1024、1366×768 无溢出/重叠/告警，35/35 quality（164 项安全/变异、45 项前端）、8/8 Android、4/4 桌面与独立 8 秒零残留后验通过；因生产 pipeline/激活源、订阅/节点数据、获准签名 TUN E2E 与五平台证据未齐保持 `in_progress`。 |
| 2026-07-28 | 推进生产 Bootstrap；VLESS Reality 与密文构建/嵌入链路已落地，修复 Windows sidecar 清空环境导致的 Winsock 初始化失败并保持最小环境；轮换后的加密生产候选经真实 sidecar 访问既有 API 主机返回 HTTP 200，未记录响应正文；正式 API 契约和跨平台发布证据仍未获批准，因此 `BOOT-G0-003` 保持 `in_progress`。 |
| 2026-07-28 | 推进 `BOOT-P0-004`/`API-P0-002`/`API-P0-003`/`VPN-G0-001`；真实账号仅以进程环境变量完成 production config/login/account/subscription 与 allowlisted 订阅下载，去敏确认 18 条 Reality/TCP/Vision VLESS；Rust 严格映射并闭合净化，Go 数据平面注册 VLESS；生产注册未验证并 fail closed。因应用内下载、Windows revision 激活、SCM installer、签名和真实 TUN 尚未完成，各切片保持 `in_progress`。 |
| 2026-07-28 | 推进 `BOOT-P0-004`/`API-P0-003`；Rust client 从原生安全存储读取订阅 URL，严格校验 HTTPS/443、userinfo、fragment、Bootstrap allowlist 与 path/query 后只经现有 Control Plane 下载自动清零正文；149 项平台测试、Windows Tauri 编译、严格 Clippy 与控制出网/订阅流水线审计通过。因应用刷新调用、Windows revision 激活、SCM installer、签名和真实 TUN 尚未完成，切片保持 `in_progress`。 |
| 2026-07-28 | 推进 `WIN-P0-002`/`VPN-P0-003`；保持 4 KiB Named Pipe 帧，以 2 KiB chunk 接通净化配置的 begin/chunk/commit，service 在固定 revision 根目录校验长度/SHA-256、拒绝 reparse/乱序/冲突并 flush 后原子 rename；`NamedPipeClient` 实现完整订阅 backend 命令映射，49 项 Windows service 测试与真实管道多帧往返通过。候选旁路/健康/激活仍 fail closed，状态保持 `in_progress`。 |
| 2026-07-28 | 推进 `WIN-P0-002`/`VPN-P0-003`；Windows service 从净化 revision 派生仅回环 mixed 候选，以同一受管 Go 核心执行真实默认节点延迟探测，闭合 local DNS 结构后激活 TUN，并支持失败恢复/清空 ownership；正式构建继续要求 Authenticode，独立未签名测试特性不能改变 `release_allowed=false`。50 项 Windows service 测试、27 项 IPC/权限变异测试通过；应用接线、SCM installer 和真实安装 E2E 未完成，状态保持 `in_progress`。 |
| 2026-07-28 | 推进 `API-P0-003`/`VPN-P0-003`/`VPN-P0-004`；Windows 原生登录和显式刷新在公开 DTO 返回前完成元数据刷新、安全正文下载、VLESS 净化、单调 revision 与 pipeline 激活，节点正文不进入 WebView；18 项 Tauri、149 项平台和 27 项订阅/节点变异测试通过。SCM installer、真实安装 TUN E2E、无中断刷新与跨平台 backend 未完成，状态保持 `in_progress`。 |
| 2026-07-28 | 推进 `WIN-P0-002`/`VPN-P0-002`；原生安装 helper 通过固定 `install`/`prepare-upgrade`/`uninstall` 动作和 per-machine NSIS 钩子管理 SCM，强制 `Program Files\\Orange`、自动启动、unrestricted service SID、随机安装身份及受保护 ACL；测试安装已验证 service 和文件边界。正式签名、升级包真实 TUN/卸载恢复、低完整性/跨用户与 Win10/Win11 矩阵未完成，状态保持 `in_progress`。 |
| 2026-07-30 | 修复 Go 净化 fixture 的五条闭合路由断言和拆分桌面 Tauri handler 静态审计；固定 Node 22.23.1、pnpm 11.9.0、Rust/Cargo 1.95.0、Go 1.25.5 后，189 项 Python、53 项前端、Rust workspace、双 Go module、Bootstrap、供应链、Windows Data Plane 与顶层 35 步 quality 全部通过。`QA-G0-001` 进入 `review` 等待远端 CI 链接，`QA-P0-002` 保持 `in_progress`。 |
| 2026-07-30 | 新增 Windows 开发验收脚本和 10 项合同测试，阶段覆盖 preflight/build/install/proxy/tun/crash/upgrade/uninstall/verify-clean，统一记录 OS/Git/工具链、包哈希及服务/代理/TUN/DNS/路由/防火墙/进程后验。首次运行仅验证 Windows 10 22H2 clean state；获批环境到位后从检查点续跑并完成全阶段，闭环结果见下一条。 |
| 2026-07-30 | 注入本地忽略的获批环境后完成 Windows 10 22H2 未签名开发闭环：真实登录/订阅、mixed/TUN 国内外 HTTPS、DNS/出口、四类 crash、候选升级、卸载与最终 clean-state 均通过；修复 mixed readiness 误判、TUN 缺少 sniff 导致的 DNS 黑洞及验收脚本 verbatim service 路径识别。候选包、phase/result 与脏工作树来源均以 SHA-256 登记，189 项 Python 与 35/35 quality 通过。正式签名、真实重启、Win11、跨用户/低完整性、升级失败回滚和远端 CI 仍未完成，各 Windows/UI/QA 切片保持 `in_progress` 或 `review`。 |
| 2026-07-30 | 补齐 Windows 10 安装态安全与失败升级验收：独立第二用户和 Low Mandatory Level 进程均无法连接受限 Named Pipe，服务保持运行且临时账号/profile 清理；专用未签名故障包在 payload 替换后、service install 前以 exit 32 失败，六个固定文件、旧服务、installation ID、active revision 与 DisplayVersion 全部恢复，备份目录清除。随后正常升级、卸载和 verify-clean 再次通过，固定工具链下 193 项 Python 与 35/35 quality 通过；正式签名、真实重启、Win11、远端 CI 和其他平台仍未完成。 |
| 2026-07-30 | 完成切片级验收重分类：`SEC-G0-004`、`ARC-G0-002`、`ARC-G0-003`、`BOOT-G0-002`、`VPN-G0-001` 进入 `done`；`WIN-G0-001` 与 `WIN-P0-003` 进入 `review`。状态统计由 done 1 调整为 done 6，未完成的签名、重启、Win11、远端 CI 和跨平台矩阵继续留在其真实所属切片。 |
| 2026-07-31 | 补齐 Windows 卸载配置选择与凭据残留验收：交互复选框、静默默认保留、显式 `/DELETEAPPDATA` 删除两处固定 app-data，原生 helper 在两条路径均清空三项生产凭据；更新模式固定改走 `prepare-upgrade` 以保留凭据。默认卸载、候选重装、显式删除和独立 verify-clean 全部通过，199 项 Python 安全/变异测试转绿。`WIN-P1-005`、`ARC-P1-004`、`SEC-G0-003` 因各自剩余矩阵保持 `in_progress`。 |
| 2026-07-31 | 完成第二轮切片级验收：`WIN-P0-002`、`VPN-P0-002`、`API-P0-003`、`UI-P0-004` 的 24 条规则分别以真实生产安装态、故障注入、严格契约、原生启动门禁和响应式 UI 证据闭环，状态统计由 done 6 调整为 done 10；不在规则内的签名、Win11、真实重启和其他平台实现继续留在其真实所属切片。 |
| 2026-07-31 | 第二轮切片收尾门禁在固定 Go 1.25.5 工具链下通过：201 项 Python、54 项前端、Rust workspace fmt/clippy/test/build、双 Go module、Bootstrap、供应链、SBOM 与 Windows Data Plane 均纳入顶层 35/35 quality；五视口截图以 SHA-256 绑定到去敏证据。 |
| 2026-07-31 | 继续推进 `VPN-P0-004`：Windows 生产 18 节点全部完成 8 并发有界测速，16 个可用、2 个不可用；非默认节点选择/core 回读、Control Plane 在线请求、Data Plane 重启持久化恢复和删除节点默认回退通过。安装态 TUN 节点切换抓包与 Linux/macOS/iOS 仍缺，状态保持 `in_progress`。 |
| 2026-07-31 | 继续推进 `VPN-P0-004`/`WIN-P1-004`：未签名 Windows 10 安装包经固定应用映像和受限 Named Pipe 激活生产 TUN；18 节点中 16 个可用，非默认选择/core 回读、切换前后 HTTPS/流量、切换后 Control Plane 请求及按 InterfaceIndex 绑定的 Wintun 组件抓包通过，2,235 个组件包按 ETL/PCAPNG 哈希登记；随后停止、清凭据、卸载和系统清理通过。跨平台 backend 及 Windows 重启/电源/网卡/冲突/回退矩阵仍缺，状态保持 `in_progress`。 |
