# Orange 开发进度

> 更新日期：2026-07-27
> 产品切片：69  
> 已完成：1
> 当前阶段：`SEC-G0-002` in_progress；建立跨平台权限白名单与构建快照

状态定义见 [docs/README.md](docs/README.md)。没有验收证据的切片不得标记 `done`。

## 1. 总览

| 模块 | 切片数 | done | 当前状态 | 文档 |
| --- | ---: | ---: | --- | --- |
| 安全与隐私 | 5 | 1 | in_progress | [01](docs/01-security-privacy.md) |
| 共享架构 | 5 | 0 | review | [02](docs/02-shared-architecture.md) |
| Bootstrap Control Plane | 6 | 0 | in_progress | [03](docs/03-bootstrap-control-plane.md) |
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
| 3 | `SEC-G0-004` 供应链与资源清单 | review | 本地 28 项测试和 727 组件 SBOM 通过；等待 `ARC-G0-001` 前置证据 |
| 4 | `ARC-G0-002` DTO、错误与命令边界 | review | 14 项契约/命令测试与全量门禁通过；等待 `ARC-G0-001` 前置证据 |
| 5 | `BOOT-G0-001` Bootstrap 包格式 | review | 本地信封、CLI 和 9 项测试通过；等待生产 secrets 生成正式资源 |
| 6 | `BOOT-G0-002` Rust 内存解密与清零 | review | 本地 13 项测试、原位清零、真实 Go handoff、产物泄漏扫描和全量门禁通过；待生产 bootstrap 资源 |
| 7 | `BOOT-G0-003` 无端口 sing-box Direct-Dial PoC | in_progress | 本机 direct-dial、startup DNS、Rust sidecar 宿主、三桌面打包注册/哈希校验、live API、fail-closed、Windows 与 Linux WSL2 无监听审计及全量门禁通过；待抓包、macOS/移动端运行审计、生产代理和正式签名安装包 |
| 8 | `SEC-G0-003` 控制面出网与敏感数据 | in_progress | 十类开发端点策略、出网/日志审计、桌面系统密钥存储、Android 内部 Rust/Tauri/Keystore 后端和 iOS 内部 Rust/Tauri/Keychain 后端已落地；Android API 36、Windows Credential Manager 与隔离 Linux Secret Service 真实往返及全门禁通过；待生产 command 接线、Android 真机/API 矩阵、Apple 运行期、Linux 包装应用图形会话集成与真实抓包 |
| 9 | `SEC-G0-002` 跨平台权限白名单 | in_progress | 机器可读开发壳白名单、权限声明发现、硬禁止隐私权限、Tauri capability 和 Android 合并 APK 快照门禁已落地；Windows/Linux 21 步、Android 8 步及 API 36 四项设备回归通过；待 Apple 包、Windows 服务 ACL、Linux helper 与单文件临时授权证据 |

## 3. 切片明细

### 安全与隐私

| ID | 摘要 | 状态 | 证据/备注 |
| --- | --- | --- | --- |
| `SEC-G0-001` | 不可信源隔离 | done | `SECURITY.md`、`docs/migration-inventory.md`、508 项资源清单；扫描/测试通过，独立副本日志无原工程路径 |
| `SEC-G0-002` | 跨平台权限白名单 | in_progress | `security/platform-permissions.yml`、跨平台声明/构建快照门禁、7 项故障测试、Android 实际 APK 精确权限审计及 API 36 四项真实启动/密钥存储回归通过；证据见 `docs/evidence/SEC-G0-002-permission-baseline-2026-07-27.md`；待 Apple 包、Windows 服务 ACL、Linux helper/polkit/systemd 与单文件临时授权证据 |
| `SEC-G0-003` | 控制面出网与敏感数据 | in_progress | 不可发布的开发端点/出网/日志门禁、固定 token key、自动清零、平台注销覆写、三桌面系统密钥存储及 Android/iOS 内部 Rust/Tauri/native 后端已落地；Windows Credential Manager、隔离 Linux Secret Service 与 Android API 36 真实生命周期/跨语言往返、iOS Keychain 静态边界和干净 Android 重建通过；证据见 `docs/evidence/SEC-G0-003-control-egress-2026-07-27.md`；待生产 command 接线、Android 真机/API 矩阵、Apple 运行期、Linux 包装应用图形会话集成与真实抓包 |
| `SEC-G0-004` | 供应链、SBOM 与资源签名 | review | 727 组件、53 资源、7 生态、原生产物 manifest 与 28 项测试通过；证据见 `docs/evidence/SEC-G0-004-supply-chain-2026-07-27.md` |
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
| `BOOT-G0-002` | Rust 内存解密与清零 | review | 生产 `decrypt`、受控 `SecretBuffer`、schema/过期校验、consume/consume_in_place/Drop/panic 清零、真实 Go handoff、Debug 脱敏、产物泄漏扫描与 13 项测试；证据见 `docs/evidence/BOOT-G0-002-memory-decrypt-2026-07-27.md` |
| `BOOT-G0-003` | 无端口 sing-box Direct-Dial PoC | in_progress | 固定 sing-box `v1.13.14`、stdio 窄桥、startup DNS、Rust sidecar 宿主、Tauri 单实例状态、三桌面 `externalBin`/运行时哈希校验、live GET/POST、fail-closed、Windows 与 Linux WSL2 无监听、18 组 Go 测试、7 项宿主进程测试及双系统全量 19 步门禁通过；证据见 `docs/evidence/BOOT-G0-003-direct-dial-2026-07-27.md` 和 `docs/evidence/BOOT-G0-003-linux-runtime-2026-07-27.md`；待管理员抓包、macOS/移动端运行审计、生产代理和正式签名安装包 |
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
