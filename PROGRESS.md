# Orange 开发进度

> 更新日期：2026-07-27
> 产品切片：69  
> 已完成：1
> 当前阶段：`BOOT-G0-002` review；等待生产节点与 Gitee secrets 配置

状态定义见 [docs/README.md](docs/README.md)。没有验收证据的切片不得标记 `done`。

## 1. 总览

| 模块 | 切片数 | done | 当前状态 | 文档 |
| --- | ---: | ---: | --- | --- |
| 安全与隐私 | 5 | 1 | review | [01](docs/01-security-privacy.md) |
| 共享架构 | 5 | 0 | review | [02](docs/02-shared-architecture.md) |
| Bootstrap Control Plane | 6 | 0 | review | [03](docs/03-bootstrap-control-plane.md) |
| sing-box Data Plane | 6 | 0 | not_started | [04](docs/04-singbox-data-plane.md) |
| 业务 API | 6 | 0 | not_started | [05](docs/05-business-api.md) |
| UI 与资产 | 8 | 0 | not_started | [06](docs/06-ui-assets.md) |
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
| 3 | `SEC-G0-004` 供应链与资源清单 | review | 本地 22 项测试和 690 组件 SBOM 通过；等待 `ARC-G0-001` 前置证据 |
| 4 | `ARC-G0-002` DTO、错误与命令边界 | review | 14 项契约/命令测试与全量门禁通过；等待 `ARC-G0-001` 前置证据 |
| 5 | `BOOT-G0-001` Bootstrap 包格式 | review | 本地信封、CLI 和 9 项测试通过；等待生产 secrets 生成正式资源 |
| 6 | `BOOT-G0-002` Rust 内存解密与清零 | review | 本地 13 项测试、产物泄漏扫描和全量门禁通过；Go/libbox handoff 随 `BOOT-G0-003` 落地 |

## 3. 切片明细

### 安全与隐私

| ID | 摘要 | 状态 | 证据/备注 |
| --- | --- | --- | --- |
| `SEC-G0-001` | 不可信源隔离 | done | `SECURITY.md`、`docs/migration-inventory.md`、508 项资源清单；扫描/测试通过，独立副本日志无原工程路径 |
| `SEC-G0-002` | 跨平台权限白名单 | not_started | 依赖平台空壳 |
| `SEC-G0-003` | 控制面出网与敏感数据 | not_started | 依赖 direct-dial |
| `SEC-G0-004` | 供应链、SBOM 与资源签名 | review | 690 组件、53 资源、7 生态、原生产物 manifest 与 22 项测试通过；证据见 `docs/evidence/SEC-G0-004-supply-chain-2026-07-27.md` |
| `SEC-P1-005` | 运行时隐私专项 | not_started | 发布前执行 |

### 共享架构

| ID | 摘要 | 状态 | 证据/备注 |
| --- | --- | --- | --- |
| `ARC-G0-001` | 五平台 Workspace 与工具链 | blocked | Windows/Linux/Android 空壳构建和启动通过；供应商无关 CI 入口与国内镜像验证见 `docs/evidence/ARC-G0-001-ci-portability-2026-07-27.md`；缺少 macOS 构建机、iOS 模拟器和有运行链接的远端 CI |
| `ARC-G0-002` | DTO、错误与命令边界 | review | 版本化 schema、9 类脱敏错误、固定命令 ACL、Rust/TypeScript 双向 fixture；证据见 `docs/evidence/ARC-G0-002-contract-boundary-2026-07-27.md` |
| `ARC-G0-003` | 双平面状态机与 Adapter | not_started |  |
| `ARC-P1-004` | 持久化、迁移与回滚 | not_started |  |
| `ARC-P1-005` | 事件、任务与可观测性 | not_started |  |

### Bootstrap Control Plane

| ID | 摘要 | 状态 | 证据/备注 |
| --- | --- | --- | --- |
| `BOOT-G0-001` | Bootstrap 包格式与构建加密 | review | 严格 schema、随机 XChaCha20-Poly1305、zeroize CLI、manifest、开发 `bootstrap.enc` 与失败测试；证据见 `docs/evidence/BOOT-G0-001-bootstrap-envelope-2026-07-27.md` |
| `BOOT-G0-002` | Rust 内存解密与清零 | review | 生产 `decrypt`、受控 `SecretBuffer`、schema/过期校验、consume/Drop/panic 清零、Debug 脱敏、产物泄漏扫描与 13 项测试；证据见 `docs/evidence/BOOT-G0-002-memory-decrypt-2026-07-27.md` |
| `BOOT-G0-003` | 无端口 sing-box Direct-Dial PoC | not_started | 架构关键 PoC |
| `BOOT-P0-004` | BootstrapTransport 强制路由 | not_started |  |
| `BOOT-P0-005` | 节点故障切换与 Fail-Closed | not_started |  |
| `BOOT-P1-006` | 签名更新、轮换与防回滚 | not_started |  |

### sing-box Data Plane

| ID | 摘要 | 状态 | 证据/备注 |
| --- | --- | --- | --- |
| `VPN-G0-001` | 纯 sing-box 配置模型与净化 | not_started |  |
| `VPN-P0-002` | Data Plane 生命周期 | not_started |  |
| `VPN-P0-003` | 订阅预启动与原子切换 | not_started |  |
| `VPN-P0-004` | Selector、测速与流量 | not_started |  |
| `VPN-P1-005` | 桌面 Mixed 与系统代理契约 | not_started |  |
| `VPN-P1-006` | 双平面隔离与路由防环 | not_started |  |

### 业务 API

| ID | 摘要 | 状态 | 证据/备注 |
| --- | --- | --- | --- |
| `API-G0-001` | 接口契约与脱敏 Fixture | not_started |  |
| `API-P0-002` | 动态配置、登录与注册 | not_started |  |
| `API-P0-003` | 账户与订阅 | not_started |  |
| `API-P1-004` | 套餐、订单与支付 | not_started |  |
| `API-P1-005` | 邀请与工单 | not_started |  |
| `API-P2-006` | 缓存、离线与恢复 | not_started |  |

### UI 与资产

| ID | 摘要 | 状态 | 证据/备注 |
| --- | --- | --- | --- |
| `UI-G0-001` | 设计 Token 与页面基线 | not_started |  |
| `UI-G0-002` | 资产白名单与转换 | not_started |  |
| `UI-P0-003` | App Shell、认证与通用状态 | not_started |  |
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
| `WIN-G0-001` | 产物与核心宿主决策 | not_started | 需决定内嵌/sidecar |
| `WIN-P0-002` | Service、Named Pipe 与双平面 | not_started |  |
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
| Windows 核心宿主 | 未决定 | `WIN-G0-001` 比较内嵌 service 与 sidecar |
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

## 6. 变更记录

| 日期 | 变更 |
| --- | --- |
| 2026-07-26 | 建立模块化文档、69 个功能切片、逐切片验收和初始进度台账。 |
| 2026-07-27 | 完成 `SEC-G0-004` 本地实现并进入 review；Go 模块下载禁止 direct fallback。 |
| 2026-07-27 | 完成 `ARC-G0-002` 本地实现并进入 review；Tauri 自有命令改为版本化 DTO 与最小 ACL。 |
| 2026-07-27 | 完成 `BOOT-G0-001` 本地实现并进入 review；生产资源生成保留为 Gitee secrets 配置点。 |
| 2026-07-27 | 完成 `BOOT-G0-002` 本地实现并进入 review；Rust 受控内存解密、全路径清零与产物泄漏扫描门禁落地。 |
