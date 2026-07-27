# 模块 09：Windows 平台

## 模块目标

通过 `orange.exe`、受签名 `orange-service.exe` 和纯 sing-box 数据面实现 Control Plane、loopback mixed 系统代理、可选 TUN，以及可靠的设置恢复和安装升级。

## WIN-G0-001：Windows 产物与核心宿主决策

**目标**：固定可执行文件、DLL、IPC 和 sing-box 宿主方式。

**依赖**：`ARC-G0-001`、`BOOT-G0-003`、`SEC-G0-004`。

**交付物**：架构决策记录、PoC、产物清单、签名与版本握手方案。

**验收规则**：

1. 明确选择“sing-box 编译进 `orange-service.exe`”或“签名 `sing-box.exe` sidecar”，不能同时遗留两套未维护路径。
2. 推荐方案能完成 Control Plane direct-dial 和 Data Plane mixed smoke test。
3. `orange.exe`、service、可选 sidecar/Wintun 的版本、哈希、签名和许可证进入 manifest。
4. 不从网络运行期下载或替换 EXE/DLL；哈希/签名不符时拒绝启动。
5. Windows 10 22H2、Windows 11 当前版有基础兼容结果。
6. 进程权限边界和安装所需管理员操作有文档。

**非目标**：不在此切片完成安装器。

**实现基线**：ADR `docs/adr/0001-windows-data-plane-sidecar.md` 已固定唯一生产路径：
由受签名 `orange-service.exe` 托管同目录、受 Authenticode 签名的官方
`sing-box.exe` sidecar，不保留“编进 service”的并行实现。`native/dataplane` 锁定
`github.com/sagernet/sing-box/cmd/sing-box@v1.13.14`，仅启用 `with_quic`；独立锁文件
和实际 Windows 编译图进入 SBOM。Windows 专项门禁执行双构建哈希一致性、Go binary
metadata、版本/标签/CGO、SHA-256、Authenticode 与发布证书白名单握手，并通过只访问
loopback 的 mixed HTTP/SOCKS5 smoke 验证退出后无残留进程或监听。开发制品为
`unsigned-debug`、`release_allowed: false`，运行期禁止下载/替换二进制。

当前 PoC 证据见 `docs/evidence/WIN-G0-001-windows-data-plane-core-2026-07-28.md`。
生产 service 已接入嵌入式固定 manifest、原生 `WinVerifyTrust`、证书指纹白名单和
SHA-256/版本二次校验；正式签名证书及获准指纹、受保护安装实证，以及 Windows 10
22H2 与 Windows 11 当前版兼容结果仍未齐，因此状态保持 `in_progress`。

## WIN-P0-002：Service、Named Pipe 与双平面宿主

**目标**：安全托管后台 sing-box，并让 UI 通过受限 IPC 控制。

**依赖**：`WIN-G0-001`、`ARC-G0-003`、`VPN-P0-002`。

**交付物**：Windows Service/helper、Named Pipe ACL、start/stop/status、crash recovery。

**验收规则**：

1. Named Pipe 只允许当前安装/用户上下文，其他普通用户或低权限进程调用被拒绝。
2. IPC 只接受固定 DTO，不接受 shell、任意路径、任意 URL、注册表路径或原始 sing-box 命令。
3. UI 关闭后按用户设置保持 Data Plane；重开 UI 能回读 service 真实状态。
4. service 崩溃被检测，系统代理/路由按模式恢复或进入明确修复流程。
5. Control Plane 无 TCP listener；Data Plane listener 只在 manifest/runtime state 登记的 loopback 端口。
6. 服务安装、启动、停止、删除不遗留孤儿进程或可写 service binary。

**非目标**：普通浏览器流量不经过 Control Plane。

**实现基线**：`crates/orange-windows-service` 已建立独立的 `orange-service.exe` SCM
入口和版本化 Named Pipe 边界。协议帧上限为 4 KiB，只接受
`status/start/stop/restart` 及配置版本/实例号，Serde 严格拒绝未知字段。管道名绑定 32
位小写安装标识，禁止远程客户端且只保留一个实例；DACL 仅包含 SYSTEM、固定 service
SID 和安装用户 SID，并施加 medium integrity label。建立连接后，service 在读取 DTO 前
再次核对客户端 PID、主令牌用户 SID、完整性级别和固定同目录 `orange-app.exe` 映像。
真实 Windows 管道测试已覆盖往返、UI client 销毁/重建后回读 service 权威状态，以及同
SID 错误映像拒绝。机器策略和证据分别见
`native/windows/service-ipc-policy.json` 与
`docs/evidence/WIN-P0-002-windows-service-ipc-2026-07-28.md`。

SCM 宿主现通过共享 `SupervisedVpnAdapter` 接入固定 sidecar backend。随 service 编译的
严格运行 manifest 固定同目录 `sing-box.exe` 的 SHA-256、版本、Windows/amd64、
`with_quic`、CGO 状态及签名者指纹，只从
`data-plane/revisions/<positive-u64>.json` 解析配置。每次启动先后执行 canonical path、
配置大小/哈希、`WinVerifyTrust`、签名证书 SHA-1、固定 `version` 与 `check -c`，握手后和
真正 spawn 前再次校验哈希；运行命令只有 `run -c <fixed-revision>`，环境清空且子进程
进入 `KILL_ON_JOB_CLOSE` Job Object。共享 supervisor 负责状态、超时、崩溃检测和回收。

当前签名者白名单仍为空，开发 sidecar 未签名，因此 start 会失败关闭；净化后的动态配置
也尚未由受保护安装流程写入 revision store。进程存活稳定期仅是临时 readiness，尚未
证明 TUN/listener 已就绪。权限策略保持 `production_backend_release_eligible: false`、
`service_configured: false` 和 `release_allowed: false`。SCM 安装/升级/删除、service crash
后代理/路由/DNS 恢复、独立低完整性/跨用户进程拒绝及 Windows 10/11 兼容结果未齐，
因此本切片保持 `in_progress`。

## WIN-P0-003：WinINET 系统代理设置与恢复

**目标**：安全设置当前用户代理，不覆盖用户的新设置。

**依赖**：`WIN-P0-002`、`VPN-P1-005`。

**交付物**：WinINET adapter、snapshot/ownership、refresh、startup repair。

**验收规则**：

1. mixed listener 成功后才保存并设置 `ProxyEnable/ProxyServer/ProxyOverride` 等价选项。
2. 使用 Windows 原生 API 设置/刷新，不执行 `reg.exe`、`netsh.exe`、PowerShell 或 shell。
3. 默认不修改机器级 WinHTTP；UI 明确区分 WinINET 与 WinHTTP。
4. 正常停止、UI crash、service crash、系统重启、升级和卸载均验证恢复。
5. 恢复前比较当前值/所有权标记，用户运行期间手动修改的新值不会被旧快照覆盖。
6. 端口冲突、设置 API 失败、快照损坏时 fail closed 并提供修复，不留下半设置。

**非目标**：P0 不支持 PAC、机器级 WinHTTP 和 LAN 代理。

## WIN-P1-004：Windows TUN/Wintun

**目标**：提供可选系统级 TUN 模式。

**依赖**：`WIN-P0-002`、`VPN-P1-006`。

**交付物**：TUN adapter、Wintun/选定组件、DNS/route、管理员流程。

**验收规则**：

1. TUN 组件版本、哈希、签名、架构和许可证登记并在启动前校验。
2. 连接后出口改变；停止、service crash、系统重启、升级和卸载后路由/DNS 恢复。
3. Control Plane 连接绕过 Data Plane 捕获，无环路。
4. IPv4 必测，IPv6 策略一致；睡眠/唤醒、网卡切换、VPN 冲突有结果。
5. 管理员权限只用于安装/必要网络操作，日常 UI 不以管理员运行。
6. TUN 不可用时可明确回退到用户选择的 mixed 模式，不静默切换。

**非目标**：不捆绑来源不明驱动。

## WIN-P1-005：托盘、开机启动、安装升级与卸载

**目标**：交付生产可维护 Windows 应用。

**依赖**：Windows `P0`、`REL-P1-005`。

**交付物**：tray、single instance、installer、service migration、uninstaller。

**验收规则**：

1. 托盘状态来自 service；退出 UI 与停止 VPN 是两个明确操作。
2. 开机启动/自动连接默认关闭或由用户明确设置，失败可诊断。
3. 安装器验证旧 service 停止、二进制替换、配置迁移和新 service 启动的原子性。
4. 升级失败可回滚，不留下新旧 EXE 混用。
5. 卸载停止服务、恢复代理/路由/DNS、删除 service/helper；用户是否保留配置有明确选项。
6. 安装器、EXE、service、DLL 均有发布签名、hash、SBOM 和 Win10/Win11 结果。

**非目标**：具体 MSI/MSIX/其他安装器在 G0 决策后固定。
