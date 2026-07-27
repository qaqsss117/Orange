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
