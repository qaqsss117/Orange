# Orange IPC 契约

`orange-ipc.schema.json` 是 React、Rust 和后续原生 helper 之间的公开数据格式。命令只能使用 schema 中登记的 request/response DTO，不接受 URL、文件路径、shell 字符串或任意 JSON map。

## 兼容规则

- 所有 request/response/error 都携带 `schemaVersion`；当前版本固定为 `1`。
- 请求使用 fail-closed 策略：未知字段、未知 enum 和不支持的版本一律拒绝。
- 响应使用向前兼容策略：保留已知字段，忽略未知字段；未知 enum 仍拒绝。
- Rust 与 TypeScript 必须共同读取 `fixtures/`，并各自验证序列化和反序列化。
- 新命令必须同时进入 schema、`orange-domain` 注册表、Tauri handler、Tauri ACL 和前端类型化调用层。

## 错误码

| code | 用户消息 | 可重试 |
| --- | --- | --- |
| `validation` | 请求参数无效。 | 否 |
| `permission` | 当前操作未获授权。 | 否 |
| `network` | 网络请求失败，请稍后重试。 | 是 |
| `bootstrap` | 安全连接初始化失败。 | 是 |
| `subscription` | 订阅数据不可用。 | 否 |
| `service` | 系统服务暂不可用。 | 是 |
| `timeout` | 操作超时，请重试。 | 是 |
| `cancelled` | 操作已取消。 | 否 |
| `internal` | 发生内部错误。 | 否 |

命令响应只允许返回上述固定消息，不包含底层错误 detail、secret、token、节点或用户数据。诊断 detail 必须在后续可观测性切片中经过脱敏后写入受限 debug 日志，不能进入 IPC DTO。

## 本地设置契约

`settings/` 只定义 Rust 原生层持久化的非敏感应用设置和 Data Plane
revision 账本，不是 WebView IPC。v1 fixture 必须通过显式迁移生成 v2；未来
schema 由旧版本明确拒绝。该契约不能加入 token、订阅凭据、bootstrap、节点、
URL、主机或文件路径，这些敏感数据必须留在平台安全存储或受控内存中。

## 原生事件契约

`observability/` 定义原生层向未来 UI 消费者传递的版本化事件 envelope。每个事件
固定包含 schema version、非零实例 ID、单调序列号和 Unix 毫秒时间，只允许
Control/Data 状态或数值化流量样本。契约不允许任意消息、标签、URL、节点、域名、
路径或请求正文；旧实例和乱序事件必须由消费者游标丢弃。

## Control Plane 业务路由契约

`control-plane/fixtures/` 固定十类业务 command 的 Rust 侧 method、host、相对 path、
认证方式和 content type，并固定 HTTP/transport 错误映射。调用方只能选择 command 枚举
并提交类型化业务正文，不能提交 URL、host、Authorization、token 或 bootstrap route；
Rust client 与 `security/control-endpoints.yml` 的任一漂移都必须使契约测试失败。
