# 模块 04：sing-box Data Plane

## 模块目标

将后端订阅转换成受控 sing-box 配置，独立启动用户数据面，提供 TUN、桌面 mixed inbound、节点选择、测速、流量与配置更新；任何订阅内容都不能突破本地权限和文件边界。

## VPN-G0-001：纯 sing-box 配置模型与净化

**目标**：建立唯一、可验证的 Data Plane 配置输入。

**依赖**：`ARC-G0-002`、固定 sing-box 版本。

**交付物**：配置 schema、normalizer、sanitizer、兼容 fixture。

**验收规则**：

1. 运行时只生成 sing-box JSON，不加载 Clash YAML、Clash API client 或 mihomo 组件。
2. 后端原生 sing-box JSON必须先转换为内部模型再重新生成，不能原样信任完整配置。
3. sanitizer 拒绝 LAN 监听、任意本地路径、`file://`、未知 executable、远程代码、未经批准 DNS/控制端口和危险 route action。
4. inbound 由客户端模板生成，订阅只能提供允许的 outbound/selector/规则引用。
5. 解析失败返回字段级错误；任何 secret 不进入错误 message。
6. 依赖树、SBOM 和产物字符串检查不含 Clash/mihomo 核心。

**非目标**：本切片不负责 Clash YAML 转换；若必须支持，建立独立受测转换模块且输出仍经过本净化器。

**当前开发基线（2026-07-28）**：

## VPN-P0-002：Data Plane 生命周期

**目标**：可靠、幂等地启动/停止 sing-box 用户实例。

**依赖**：`VPN-G0-001`、`ARC-G0-003`、对应平台 `G0`。

**交付物**：start/stop/restart、真实状态查询、事件、超时和崩溃检测。

**验收规则**：

1. 无配置、无权限、配置无效时 start 明确失败且不留下进程、端口、路由或 DNS。
2. 连续重复 start/stop 各 20 次不生成重复实例、不泄漏句柄、不崩溃。
3. UI 进程/WebView 重建后能恢复真实状态；后台服务不依赖 React 生命周期。
4. helper/core 异常退出在 2 秒内被识别并进入 failed/rollback，不继续展示 online。
5. stop 有超时和强制清理策略，清理后系统代理、路由、DNS 与端口恢复。
6. Control Plane 在全部生命周期测试中保持可用。

**非目标**：不在此切片实现节点选择 UI。

**当前开发基线（2026-07-28）**：`orange-platform` 已提供由应用原生所有、可跨
WebView 消费者复用的 `SupervisedVpnAdapter`。平台后端只能按配置版本和单调实例号
执行无副作用 preflight、spawn、就绪探测、优雅停止、强制停止与幂等 cleanup，不能从
WebView 接收可执行路径、参数或 shell。监管线程以弱引用独立轮询，策略拒绝零间隔和
超过 2 秒的崩溃检测间隔；启动超时、就绪探测错误和异常退出都会撤销进程活动标记并
调用统一资源清理。stop 在超时后强制终止并等待回收，记录 graceful/forced 结果；清理
失败保持 failed，后续 stop 可重试恢复。权威快照现在显式区分状态与真实活动实例，操作
失败后 controller 会重新读取 adapter，因此 WebView 重建不会把已清理的 failed 尝试误报
为仍在运行。测试覆盖 20 轮重复启停、权限/配置/spawn 失败、启动超时、异常退出、强制
停止、清理失败恢复、restart、Control Plane 隔离、消费者重建和真实子进程崩溃。

桌面 Tauri 现在提供闭合的 `control_data_plane` 状态/start/stop 边界。请求不包含 revision；
Windows start 只从原生节点 runtime 读取已提交活动 revision，stop 则直接使用 adapter 的
权威活动实例并保持幂等。非 Windows 桌面当前没有活动 revision source，Android/iOS 也
没有该 handler。操作以原子 guard 串行化，完成后重新回读 adapter 再返回 canStart/canStop。

## VPN-P0-003：订阅拉取、预启动与原子切换

**目标**：从 BootstrapTransport 获得真实订阅并无中断地替换 Data Plane。

**依赖**：`BOOT-P0-004`、`VPN-P0-002`、`ARC-P1-004`。

**交付物**：subscription pipeline、候选槽位、健康检查、回滚。

**验收规则**：

1. 订阅只经 Control Plane 拉取，Authorization 由 Rust 注入。
2. 新订阅按“解析 -> 净化 -> 规则存在性 -> 旁路启动/检查 -> 激活”执行。
3. 健康检查至少验证 core 启动、目标 outbound 可拨号和 DNS 不自举死锁。
4. 任一步失败保留上一可用 Data Plane；首次安装失败则不修改系统代理/TUN。
5. 激活过程原子更新 active version；杀进程后能恢复到完整旧版或新版，不能处于半配置。
6. 过期、流量耗尽、空节点、未知协议和服务端错误均有 fixture 与 UI 状态。

**非目标**：不允许订阅替换 bootstrap 配置。

**当前开发基线（2026-07-28）**：`orange-platform` 新增纯原生
`SubscriptionPipeline`，输入只能是 `VPN-G0-001` 已净化的
`SanitizedDataPlaneConfig`，不接受 URL、Authorization、任意进程路径或 WebView 参数。
候选事务先原子持久化 candidate journal，再写候选槽位，并严格按“旁路启动 -> core
ready -> 目标 outbound 可拨号 -> Bootstrap DNS 独立 -> 原子激活 -> active revision
回读 -> journal commit”执行。重复 apply/recover 由同一原子 guard 拒绝，配置缓冲在 stage
返回后立即清零；清零前只复制不含连接材料的公开 selector 目录。journal commit 成功后
才把 revision 与该目录交给 `ActiveDataPlaneNodeRuntime`，安装失败会清除旧 runtime 并
返回显式 unavailable，清除失败则要求恢复；重复应用同一 committed revision 会重试安装。

`FileSettingsStore` 现在提供保留其他用户设置的原子 revision journal 操作。候选失败先
恢复完整 current revision，再幂等删除候选，最后清除 journal marker；持久化 commit
失败同样回滚。启动恢复会区分“candidate 已激活但尚未 commit”“candidate 尚未激活或
健康失败”“current 进程丢失”“平台已恢复 previous”和“首次安装存在意外 active”五类
状态，使中断点最终收敛到完整旧版或新版；没有健康 committed revision 时显式清空系统
ownership，完全未知的 active revision 在恢复后删除；runtime revision 与权威 backend/
journal 不一致时也会清除。18 项 pipeline Rust 测试和 2 项
文件 journal 测试覆盖三类健康失败、首次安装、激活/持久化失败、候选两侧崩溃窗口、
current 被杀、previous 已恢复、未知 ownership、无健康回退、幂等与并发拒绝；机器可读
静态门禁固定 commit 后 runtime 交接、失败清理和 revision 对账顺序，并阻止提前接入生产 Tauri。

本切片仍为 `in_progress`：生产订阅元数据、下载响应和 VLESS 参数形态已经过真实去敏
验证；Windows 原生登录和显式刷新会从安全存储经 allowlisted Control Plane 下载自动清零
的正文，完成 VLESS 净化、单调 revision、候选探测和 TUN 激活。受保护 revision store、
SCM 生命周期、安装 ACL、正常升级保留和 payload 替换后注入失败的六文件/服务/身份/
revision/显示版本回滚均已通过 Windows 10 安装态验收。已有活动 TUN 的刷新仍会先停止旧
实例再探测新候选，因此不宣称无中断原子切换；Linux/macOS/iOS 也尚无生产 pipeline
实例或 backend，这两项仍阻止进入 `review`。

## VPN-P0-004：Selector、节点、测速与流量

**目标**：提供用户可验证的节点管理和实时状态。

**依赖**：`VPN-P0-002`、`VPN-P0-003`。

**交付物**：group/node DTO、select、delay test、traffic events、持久化选择。

**验收规则**：

1. 只展示 selector 可选择的 outbound，内部/控制面 outbound 不出现在 UI。
2. 选择节点后从 sing-box 查询回读确认，不能只更新前端文本。
3. 单项/批量测速有并发上限、超时、取消和 unavailable 状态。
4. UI 收到的流量事件节流且单调合理；停止后不继续显示旧速度。
5. 重启 Data Plane 后恢复仍存在且有效的选择；节点已删除时回退到明确默认项。
6. 节点切换期间业务 API 继续通过 Control Plane，不随用户节点变化。

**非目标**：不记录或上传用户访问域名。

**当前开发基线（2026-07-28）**：净化配置现在同时生成不含 server、端口、凭据、
路由、内部 `orange-*` 对象和 Control Plane outbound 的公开 selector 目录，只暴露
selector ID、明确默认节点、成员节点 ID 与协议族。平台无关 `DataPlaneNodeRuntime`
将选择操作串行化，先回读旧值，再调用固定 backend 选择，并以第二次 backend 回读完全
相等作为成功条件；回读或持久化失败会补偿恢复旧值，补偿失败单独暴露。

单项和批量测速共用最多 64 项、并发最多 8、100～60000 ms 超时和共享取消令牌的
契约，结果只有 available/timed out/cancelled/unavailable。流量会话只接收当前实例的
单调总量，用单调时钟计算整数速度并复用单待发样本节流器；stop 会丢弃待发事件、清除
实例并把速度归零。Data Plane 事件桥把生命周期状态与流量放进同一实例递增序列，停止
用退役实例发布 `unconfigured`；默认 64 项、硬上限 256 项的原生 hub 只滚动保留最新事件。
设置 schema v3 仅原子持久化 revision 和最多 8 对 selector/node ID，
v1/v2 会迁移为空账本；重启或新 revision 只恢复仍有效的节点，删除节点回退到净化目录
中的明确默认项。

平台级 `SharedDataPlaneNodeRuntime` 只保存公开目录和活动 revision，以读锁包围选择、
测速、恢复和流量读取，以写锁串行化 install/clear。候选 runtime 必须先完成 backend
选择恢复和持久化才会原子发布；失败保留旧 runtime。`Arc` backend/storage 转发允许应用
复用同一个原生 client 和设置存储，不复制敏感配置 JSON。

22 项 runtime 与 4 项 event-source Rust 测试、闭合 JSON Schema/fixture、静态审计与
16 项变异测试通过。Windows
受管 `orange-data-plane.exe` 进一步直接组合 sing-box 1.13.14 公共 API，以无 listener 的
4 KiB stdio 协议提供 selector 切换/回读、固定 URL 测速/取消和 TCP/UDP 总流量；Go
测试与离线 mixed HTTP/SOCKS5 真实流量 smoke 已验证切换回读和非零统计。Windows Rust
client 以单 stdout reader 按请求 ID 分发乱序响应，限制 32 项待处理请求，并在协议失败时
关闭 stdin、失败全部待处理操作；生产 `DataPlaneNodeBackend` 在操作前后校验
configuration revision、supervisor instance、进程 PID 和同一 client 身份。真实 Rust/Go
进程互操作已完成选择、回读、权威流量读取与 EOF 优雅回收。

Windows 外层受限 Named Pipe 现在以 10 个固定命令暴露选择/回读、流量以及异步
begin/poll/cancel 测速；运行探测最多 8 项、记录最多保留 32 条，完成结果保留 5 秒后
失效。取消意图先于晚到成功结果生效，handler 销毁也会取消仍在运行的探测。真实管道
测试已验证 `NamedPipeClient` 可直接实现 `DataPlaneNodeBackend`，并跨独立连接完成节点
往返、流量读取与测速取消。

## VPN-P1-005：桌面 Mixed Inbound 与系统代理契约

**目标**：为 Windows/macOS/Linux 提供可控的系统代理入口。

**依赖**：`VPN-P0-002`、对应桌面平台 `P0`。

**交付物**：mixed inbound template、loopback port allocation、代理设置/恢复契约。

**验收规则**：

1. mixed 只监听 `127.0.0.1`/`::1`，禁止 `0.0.0.0`、LAN 地址和端口转发。
2. 只有 core 报告监听成功后才修改系统代理；设置失败立即停止该 inbound 或回滚。
3. 原代理配置带所有权标记保存；只在当前值仍由 Orange 管理时恢复。
4. 正常停止、core 崩溃、UI 崩溃、升级、重启修复和卸载都通过恢复测试。
5. 端口冲突会重新分配并验证，不覆盖其他进程。
6. 移动构建不包含或不启用 mixed listener。

**非目标**：机器级 WinHTTP、透明代理和 LAN 分享默认不在范围内。

## VPN-P1-006：双平面隔离与路由防环

**目标**：Control Plane 与 Data Plane 可独立故障、更新和恢复。

**依赖**：`BOOT-P0-005`、`VPN-P0-003`、平台 socket/route adapter。

**交付物**：socket protect/route exclusion、双实例测试、故障矩阵。

**验收规则**：

1. Data Plane 在线时连续请求账户/订阅，不出现 TUN 套娃、DNS 死锁或重复代理。
2. Data Plane restart/rollback/stop 不关闭、重建或泄漏 Control Plane。
3. Control Plane 节点切换不影响系统出口节点和 Data Plane 统计。
4. Android socket protect、Apple route/extension 策略及桌面 helper IPC 分别有平台测试证据。
5. 抓包能证明 API 流量进入 bootstrap outbound，普通系统流量只进入 Data Plane。
6. kill 任一实例后另一实例保持符合设计的状态，并能单独恢复。

**非目标**：不将 bootstrap 节点作为用户可选节点。
