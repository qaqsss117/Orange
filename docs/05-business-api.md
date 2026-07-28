# 模块 05：业务 API

## 模块目标

在 Rust 中实现类型化业务 client，所有境外 API 请求强制经 BootstrapTransport，向 React 暴露固定命令，不暴露任意 URL、token 或原始网络能力。

## API-G0-001：接口契约与脱敏 Fixture

**目标**：冻结后端契约，避免直接复制原 Retrofit 模型和乱码行为。

**依赖**：`ARC-G0-002`、`SEC-G0-003`。

**交付物**：OpenAPI/等价 schema、成功/失败 fixture、字段映射、错误码表。

**验收规则**：

1. 覆盖 config、登录、注册、用户、订阅、套餐、订单、支付、邀请、工单端点。
2. fixture 删除真实邮箱、token、订阅 URL、订单号和支付链接，保留协议结构。
3. nullable、时间戳、金额单位、状态枚举和未知字段策略有明确说明。
4. 服务端 2xx 空 body、4xx、5xx、非 JSON、超时和 schema 漂移均有 fixture。
5. Rust/TypeScript DTO 通过契约测试，不能用 `any`/无界 map 绕过。
6. 生产 base URL 不硬编码到前端，动态 config 也必须通过 allowlist。

**非目标**：不在此切片实现页面。

### 2026-07-27 开发基线

本轮已在 `contracts/business-api/` 建立 clean-room v1 等价契约，固定 config、
login、register、account、subscription、plans、orders、payment、invite、tickets、
update 十一项语义操作。该 schema 明确标记为 `development`、
`releaseAllowed: false`，不是获批生产 OpenAPI，也不能作为生产契约冻结的证据。

Rust `orange-domain` 负责完整 wire DTO。登录/注册输入、认证凭据、订阅凭据和支付
跳转值使用零化类型及脱敏 `Debug`；wire fixture 保留协议结构，但所有敏感值均为
显式 `<redacted:...>` 标记。TypeScript 只定义 Rust 投影后的公开响应，逐层校验精确
键集合、显式 nullable、安全整数、三位大写货币和数组上限，不定义 token、密码、
订阅凭据或支付 URL。未知结构字段拒绝，未知状态字符串统一映射为类型化 `unknown`。

`failures.v1.json` 固定空 2xx、4xx、5xx、非 JSON、超时和 schema 漂移六类结果。
`check_business_api_contract.py` 在 CI 中交叉检查十一项操作、闭合对象、状态表、九条
字段映射、失败矩阵、fixture 脱敏和 TypeScript 敏感字段/无界 DTO 禁令。Windows、
隔离 Linux 与 Android 门禁及双桌面启动已通过，证据见
`docs/evidence/API-G0-001-business-contract-2026-07-27.md`。

当前已取得 config、登录、账户、订阅元数据和订阅下载的真实去敏联调证据，但仍缺获批生产 OpenAPI、注册及其余业务端点样本和完整错误码语义确认；正式依赖
`ARC-G0-002`、`SEC-G0-003` 也未完成。因此本切片保持 `in_progress`，后续不能通过
猜测生产字段或把开发 fixture 改名来替代这些输入。

## API-P0-002：动态配置、登录与注册

**目标**：境内首次启动能够经 bootstrap 完成认证。

**依赖**：`API-G0-001`、`BOOT-P0-004`、`ARC-P1-004`。

**交付物**：config/auth commands、登录态、表单错误、token storage。

**验收规则**：

1. 未登录冷启动先等待 Control Plane ready，再拉动态 config；不会 direct 请求后端。
2. 登录/注册校验邮箱、密码和服务端要求字段，重复提交被禁用或幂等处理。
3. token 仅由 Rust 写入安全存储，React state/localStorage/日志看不到 token。
4. 401 清理失效登录态但不删除非用户设置；网络失败保留可重试输入且不记录密码。
5. config 中的 API、支付、客服、Banner URL 全部通过 scheme/host 校验。
6. 新安装、已有有效 token、token 过期、bootstrap 不可用和离线场景都有 E2E。

**非目标**：不在注册流程加入图片验证码 OCR 或设备指纹。

### 2026-07-27 开发基线

`orange-platform` 已建立单一 `BusinessApiService`。桌面初始化先通过共享 Control Plane
状态的条件变量等待 ready，再使用固定 `config` 路由；没有 direct HTTP fallback。
wire config 严格解析 API、支付、客服和 Banner URL，公开响应在校验后丢弃全部 URL。
API origin 的 host 必须命中已解密 bootstrap allowlist，并由原生宿主选择 bootstrap
primary host；应用和路由代码不编译 API host 明文。其他三类 URL 只允许机器策略登记的
不可路由开发 host，统一要求 HTTPS/443、无凭据、query 和 fragment，API/payment origin
还必须为根路径。

认证基线在桌面 Tauri 新增 `initialize_business`、`login`、`register` 和
`get_auth_session` 四个固定命令，并通过独立 capability 限定为 Linux、macOS、Windows
主窗口。Android/iOS
构建清单仍只登记原有两个基础命令，移动端调用这些业务命令会 fail closed。Rust 与
TypeScript 对邮箱、密码、邀请码和 schema version 做一致的有界校验；共享原子 guard
拒绝同时登录/注册。公开登录态固定为 `signed_out`、`authenticated`、`unverified`，
离线或 bootstrap 不可用时保留完整凭据但不伪称已认证，partial 凭据会被清理。

access/refresh token 只由 Rust 解析并交给平台安全存储。两项凭据以 best-effort 原子方式
替换，失败时恢复 access、refresh 和订阅凭据；成功认证会清除旧订阅凭据。认证路由的
401 会清除全部用户凭据，但不触碰非用户设置。IPC/TypeScript 公开响应不定义 token、
URL 或 Authorization 字段，前端失败测试证明输入对象不被修改，也不会写入 storage 或
日志。请求、wire 响应和凭据缓冲使用自动清零及脱敏 `Debug`。

Windows 与隔离 Ubuntu 24.04.4 WSL2 的 22 步质量门禁、双桌面构建/8 秒启动、Android
8 步构建及 Android 16 / API 36 x86_64 四项 Rust/Kotlin/Keystore 回归均已通过，证据见
`docs/evidence/API-P0-002-authentication-2026-07-27.md`。

2026-07-28 的真实桌面探针已经通过生产 config 和登录路由完成认证，并用服务端接受的 Bearer 凭据回读账户；严格 production envelope/DTO 映射和 `app_url` Bootstrap host 校验均已覆盖。生产注册路由没有契约证据，因此注册在 production config 下明确返回 unavailable，绝不发送猜测请求。

本基线仍不是生产完成态：Android/iOS 尚无嵌入式
Control Plane transport，生产注册、新安装/离线矩阵的产品级 UI 流程以及正式依赖
`API-G0-001`、`BOOT-P0-004`、`ARC-P1-004` 都未收口。因此切片保持 `in_progress`，
不能以开发 `.invalid` fixture、mock 场景或桌面命令代替这些验收输入。

## API-P0-003：账户与订阅

**目标**：获取用户、套餐状态和 sing-box 订阅，并驱动 Data Plane。

**依赖**：`API-P0-002`、`VPN-P0-003`。

**交付物**：account/subscription commands、用量计算、过期状态、刷新逻辑。

**验收规则**：

1. 账户邮箱、余额、套餐、过期时间、已用/总流量与 fixture/后端一致。
2. 流量加法使用溢出安全整数；总量为 null/0、时间单位异常时不崩溃。
3. 订阅内容不返回 React，直接进入 Rust Data Plane pipeline。
4. 套餐过期或流量耗尽时阻止新启动，并允许已连接状态按产品规则明确处理。
5. 手动刷新有 loading/error/success 状态，重复刷新有并发控制。
6. 注销先停止 Data Plane，再删除 token/订阅和用户缓存；Control Plane 可继续支持重新登录。

**非目标**：不让前端复制含 secret 的完整订阅 URL。

### 2026-07-28 开发基线

`orange-platform` 已通过现有单一 `BusinessCommandClient` 接通固定 `account` 和
`subscription` 路由。两条路由必须在动态配置初始化且原生会话为 `authenticated` 后
执行；登录、注册、账户刷新和订阅刷新共享同一原子操作 guard，重复刷新或与认证并发
时不会发出第二个请求。账户刷新成功后更新 Rust 内的权威用户，认证路由返回 401 时
清除 access、refresh、订阅凭据、原生登录态和订阅缓存。

订阅 wire DTO 仍负责接收敏感 `subscriptionCredential`，该字段在 Rust 中立即从响应
取出并进入平台 secret backend，不进入公开 DTO、Tauri 返回值、React、browser
storage 或日志。有效 `trial`/`active` 凭据使用带回滚的原子替换；过期、耗尽、无套餐
和未知状态会删除旧订阅凭据。替换失败恢复上一凭据，调用方 secret buffer 在成功和
失败路径均清零。

公开订阅策略使用 JavaScript 安全整数：加法超过 `2^53 - 1` 返回失败，剩余流量使用
饱和减法。到期时间小于等于当前时间映射为 `expired`，总量为 0 或已用量达到总量映射
为 `exhausted`；仅有效的 `trial`/`active` 允许未来的新 Data Plane 启动。总量为 null
保持无限量语义，未知状态默认阻止启动。

桌面 IPC 提供严格无参数的 `refresh_account`、`refresh_subscription` 与 `logout`，
请求只能包含 schema version，注入 URL、token、订阅凭据或额外字段会在
Rust/TypeScript 边界被拒绝。独立 capability 只向 Linux、macOS、Windows 主窗口授予
这三条命令；移动端 handler、网络、文件和 shell 权限均未增加。公开响应由 TypeScript
再次按精确键集合解析。

注销与登录、注册及两类刷新共享同一原子 operation guard。`BusinessApiService` 必须先
调用原生 `LogoutDataPlane`；桌面实现先回读权威 Data Plane 状态、停止实例、再次回读并
确认 `unconfigured`，之后才依次删除 access、refresh、订阅凭据，最后清除 Rust 会话与
订阅缓存。停止或回读失败不会开始删除凭据；secret backend 会尝试全部三项删除，任一项
失败则保留内存登录态供用户重试，避免把部分清理误报为成功。四项 Rust 故障测试和机器
可读顺序门禁固定成功、停止失败、部分删除失败重试及并发注销场景；证据见
`docs/evidence/API-P0-003-logout-2026-07-28.md`。

2026-07-28 的真实桌面探针已确认订阅元数据路由、敏感下载 URL 的 HTTPS/443 与 Bootstrap host 白名单边界，以及 Base64 UTF-8 正文中的 18 条一致 VLESS Reality/TCP/Vision URI。探针只输出结构、计数和布尔结论，不记录响应正文、凭据、URL 或节点秘密；Rust 映射继续只把下载 URL 存入原生 secret backend。`BusinessCommandClient` 现可在原生层重新读取该 URL，拒绝 userinfo、fragment、非 443 端口、非 allowlist host 和异常 path/query，再通过同一桌面 Control Plane 下载自动清零的正文；该能力没有新增 WebView command。

本基线仍保持 `in_progress`。Windows 登录与显式刷新已在原生层依次执行订阅元数据刷新、安全下载、VLESS 净化、单调 revision 和 pipeline 激活；公开命令仍只返回原订阅 DTO，不返回正文、URL 或节点目录。`VPN-P0-003` 已有
平台无关的候选事务、三项健康契约、原子 revision journal 与崩溃恢复核心，但尚未接入
生产 Data Plane backend；当前桌面注销已完成顺序接线，但生产平台
adapter、移动端业务 handler、产品 UI 的 loading/error/success 状态和
macOS/iOS 运行证据仍待后续输入与实现。

## API-P1-004：套餐、订单与支付

**目标**：完成购买、订单查询和受控支付跳转。

**依赖**：`API-P0-003`、`UI-P1-006`。

**交付物**：plan/order/payment commands、金额格式、幂等保护、页面数据。

**验收规则**：

1. 套餐周期、价格、流量、说明按服务端单位正确显示，金额不使用浮点随意计算。
2. 创建订单有 idempotency/按钮锁，网络重试不会无提示生成重复订单。
3. 订单状态映射覆盖待支付、已支付、取消/关闭、退款和未知状态。
4. 支付 URL 必须为 HTTPS 且 host 在支付 allowlist，用户看到明确外部跳转目标。
5. 支付返回/重新进入应用后主动刷新订单，不只相信 URL 回调参数。
6. 空支付方式、订单失效、服务端错误和跨 host 重定向被正确处理。

**非目标**：客户端不保存银行卡或支付密码。

## API-P1-005：邀请与工单

**目标**：完成邀请信息及工单生命周期。

**依赖**：`API-P0-003`、`UI-P1-006`。

**交付物**：invite/ticket commands、分页/刷新、创建/回复/关闭。

**验收规则**：

1. 邀请码、佣金、邀请记录字段与 fixture 一致；复制行为不自动读取剪贴板。
2. 工单列表/详情支持空状态、刷新和服务端排序，不重复消息。
3. 新建/回复对标题、正文、长度和空白进行校验，重复点击不重复提交。
4. 关闭工单需确认，关闭后回复入口禁用且状态从后端回读。
5. 工单正文作为纯文本/严格净化内容渲染，不能执行远程 HTML/脚本。
6. 不支持附件上传；未来新增附件必须建立独立安全切片，不能获得相册全库权限。

**非目标**：不复制原工程任何图片上传能力。

## API-P2-006：缓存、离线与恢复

**目标**：弱网下提供可预测体验而不使用过期敏感数据。

**依赖**：`API-P0-003`、`ARC-P1-004`。

**交付物**：cache policy、TTL、stale UI、清理策略。

**验收规则**：

1. 动态 config、账户、订阅、套餐、订单分别定义 TTL，不能共用一个模糊时间戳。
2. 离线只能展示明确标记的上次数据，不能离线创建订单、支付或回复工单。
3. token 过期后缓存不能维持“已登录可操作”假状态。
4. 缓存不保存密码、完整订阅、bootstrap 明文或支付敏感参数。
5. 系统时间回拨/跳变不会无限延长缓存有效期。
6. 用户可清理业务缓存而不破坏应用所需最小 bootstrap 资源。

**非目标**：不实现后台隐蔽数据同步。
