# 模块 09：Windows 平台

## 模块目标

通过 `orange.exe`、受签名 `orange-service.exe` 和纯 sing-box 数据面实现 Control Plane、loopback mixed 系统代理、可选 TUN，以及可靠的设置恢复和安装升级。

## WIN-G0-001：Windows 产物与核心宿主决策

**目标**：固定可执行文件、DLL、IPC 和 sing-box 宿主方式。

**依赖**：`ARC-G0-001`、`BOOT-G0-003`、`SEC-G0-004`。

**交付物**：架构决策记录、PoC、产物清单、签名与版本握手方案。

**验收规则**：

1. 明确选择“sing-box 编译进 `orange-service.exe`”或“签名的独立 Data Plane sidecar”，不能同时遗留两套未维护路径。
2. 推荐方案能完成 Control Plane direct-dial 和 Data Plane mixed smoke test。
3. `orange.exe`、service、可选 sidecar/Wintun 的版本、哈希、签名和许可证进入 manifest。
4. 不从网络运行期下载或替换 EXE/DLL；哈希/签名不符时拒绝启动。
5. Windows 10 22H2、Windows 11 当前版有基础兼容结果。
6. 进程权限边界和安装所需管理员操作有文档。

**非目标**：不在此切片完成安装器。

**实现基线**：ADR-0002 已取代最初的官方 CLI 决策并固定唯一生产路径：由受签名
`orange-service.exe` 托管同目录、受 Authenticode 签名的 `orange-data-plane.exe`，
不保留“编进 service”或上游 CLI 的并行实现。宿主直接组合锁定的官方
`github.com/sagernet/sing-box@v1.13.14` 公共 API，不 fork 核心，仅启用 `with_quic,with_utls`；
只注册当前产品需要的 TUN/mixed、三种节点协议、selector、direct 和 local DNS。

受管宿主通过继承 stdio 暴露 4 KiB 固定协议，不建立网络控制 listener；只接受公开
selector/node ID、固定 URL 的有界测速、取消和数字流量总量。Windows 专项门禁执行 Go
协议测试、双构建哈希一致性、binary metadata、版本/标签/CGO、SHA-256、Authenticode
与发布证书白名单握手，并通过只访问 loopback 的 mixed HTTP/SOCKS5 smoke 真实验证
selector 切换/回读、非零流量和退出清理。开发制品为
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
入口和版本化 Named Pipe 边界。协议帧上限为 4 KiB，只接受生命周期、节点选择/回读、
异步测速 begin/poll/cancel 和流量这 10 个固定命令，Serde 严格拒绝未知字段。管道名绑定 32
位小写安装标识，禁止远程客户端且只保留一个实例；DACL 仅包含 SYSTEM、固定 service
SID 和安装用户 SID，并施加 medium integrity label。建立连接后，service 在读取 DTO 前
再次核对客户端 PID、主令牌用户 SID、完整性级别和固定同目录 `orange-app.exe` 映像。
真实 Windows 管道测试已覆盖状态往返、client 销毁/重建后回读 service 权威状态、节点
选择/回读与流量、跨连接测速取消，以及同 SID 错误映像拒绝。机器策略和证据分别见
`native/windows/service-ipc-policy.json` 与
`docs/evidence/WIN-P0-002-windows-service-ipc-2026-07-28.md`。

SCM 宿主现通过共享 `SupervisedVpnAdapter` 接入固定 sidecar backend。随 service 编译的
严格运行 manifest 固定同目录 `orange-data-plane.exe` 的 SHA-256、版本、Windows/amd64、
`with_quic,with_utls`、CGO 状态及签名者指纹，只从
`data-plane/revisions/<positive-u64>.json` 解析配置。每次启动先后执行 canonical path、
配置大小/哈希、`WinVerifyTrust`、签名证书 SHA-1、固定 `version` 与 `check -c`，握手后和
真正 spawn 前再次校验哈希；运行命令只有 `run -c <fixed-revision>`，子进程环境清空后仅
恢复由 `GetWindowsDirectoryW` 取得的可信 `SystemRoot`，并进入 `KILL_ON_JOB_CLOSE` Job
Object。共享 supervisor 负责状态、超时、崩溃检测和回收。
preflight 与 spawn 前均通过 IP Helper API 拒绝残留的同名 `orange-tun`；启动后只有原生
adapter table 中该接口处于 Up，且同时出现 `172.19.0.1/30` 和
`fdfe:dcba:9876::1/126` 时才进入 Online。子进程回收后，backend 还会有界等待该接口
消失，超时则返回 `CleanupFailed`。

service 现在通过继承 stdin/stdout 建立严格 Rust client：首帧必须是有界 `ready`，最多保留
32 项待处理请求，写锁内分配单调 ID，单 reader 按 ID 分发乱序响应；超时、未知/重复响应、
非法字段或帧会关闭 stdin 并失败所有待处理操作。client 关联取消测速，支持 selector 切换、
回读和权威流量，并由生产 `DataPlaneNodeBackend` 在调用前后绑定 revision、instance、PID
与同一 client 身份；supervisor 清理会使该绑定失活，正常停止以 EOF 优雅退出，Job Object
仍为强制回收后备。外层受限 Named Pipe 已增加严格节点 DTO，并用 begin/poll/cancel
避免单实例 service 被同步测速阻塞；探测限制为 8 项运行、32 条保留和 5 秒完成结果保留，
handler 销毁会取消仍在运行的探测。Windows 应用只从可执行文件同目录的固定
`orange-installation-id.v1` 读取 32 字节小写十六进制 ID；缺失、符号链接、目录逃逸或
非法内容保持未配置。合法 ID 建立的同一个 `NamedPipeClient` 同时供生命周期 adapter 与
共享节点 runtime host 使用；host 已实现 `ActiveDataPlaneNodeRuntime`，pipeline 只在
revision journal commit 后交接公开 selector 目录，安装失败会清除旧 runtime，恢复时也会
清除 revision 不匹配的 runtime。真实 installer/文件 ACL、生产订阅 backend 与获批激活源
尚未落地，Tauri 中也没有生产 pipeline 实例。installer 身份有效时，应用会启动受 task
registry 管理的 500 ms 原生监视器：从同一 Named Pipe 回读权威生命周期，从已安装节点
runtime 读取流量，以统一序列写入 64 项有界原生 hub；退出会唤醒并 join。桌面主窗口现有
闭合的 `control_data_plane` status/start/stop 命令；WebView 不能提供 revision，start 只读取
`WindowsNodeRuntimeHost.active_revision()`，stop 使用 adapter 权威实例且可清理无 revision 的
遗留在线实例。原子 guard 拒绝重叠 mutation，响应在操作后重新回读权威状态与能力。控制
capability 不进入 Android/iOS，也没有 WebView event emitter。当前签名者
白名单仍为空，开发 sidecar 未签名，
因此 start 会失败关闭；净化后的动态配置
也尚未由受保护安装流程写入 revision store。原生 TUN 状态已取代临时进程存活稳定期，
但尚未用获准签名 sidecar 证明真实启动/重启/崩溃/停止链路，也未探测 mixed listener。
权限策略保持 `production_backend_release_eligible: false`、
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
