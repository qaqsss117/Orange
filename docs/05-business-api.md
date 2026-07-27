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

仍缺获批生产 OpenAPI、真实脱敏后端样本、错误码语义确认和联调结果；正式依赖
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
