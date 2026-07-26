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
