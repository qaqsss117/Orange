# 模块 03：Bootstrap Control Plane

## 模块目标

应用启动后，用 Rust 从包体内存解密最小 sing-box 节点配置，启动只有 outbound 的 Control Plane，并通过 direct-dial 承载所有业务 API；不监听本地 HTTP/SOCKS/TCP/UDP 端口，不允许裸连回退。

## 核心数据流

```text
bootstrap.enc
  -> Rust AEAD decrypt + schema/expiry validation
  -> Control Plane sing-box router/outbounds
  -> BootstrapTransport direct-dial
  -> fixed business commands
  -> HTTPS API allowlist
```

## BOOT-G0-001：Bootstrap 包格式与构建加密

**目标**：定义并生成可版本化、可轮换的加密 bootstrap 资源。

**依赖**：`ARC-G0-001`、`SEC-G0-004`。

**交付物**：bootstrap schema、CI 加密工具、`bootstrap.enc`、非敏感 manifest。

**验收规则**：

1. 明文只包含候选 outbound、故障切换、启动 DNS、API host、版本和到期时间，不含用户 token。
2. 使用 XChaCha20-Poly1305 或 AES-256-GCM，nonce 不复用，解密前验证认证标签。
3. 构建 key 只从 CI secret 注入；仓库、lockfile、日志和产物资源区找不到 key 明文。
4. 每渠道/版本可生成不同密文和凭据；相同明文重复构建不会因固定 nonce 产生相同密文。
5. manifest 记录 schema、算法、密文哈希、渠道、版本、过期时间，不泄漏节点。
6. 错 key、截断、篡改、旧 schema 和过期 fixture 均被拒绝。

**非目标**：包内加密不承诺抵御专业逆向；节点安全依赖短期凭据、限流和轮换。

## BOOT-G0-002：Rust 内存解密与清零

**目标**：明文只在受控内存中短暂存在。

**依赖**：`BOOT-G0-001`。

**交付物**：Rust decryptor、secret buffer、schema validator、内存/日志测试。

**验收规则**：

1. Rust 使用受控 secret buffer，解密结果不转换成会被随意 clone 的普通全局 String。
2. 明文不写入临时文件、数据库、WebView state、panic message、日志或 crash report。
3. 传给 libbox/Go bridge 后 Rust 缓冲立即 zeroize；实例销毁时释放原生副本。
4. panic/错误路径也执行清零；测试覆盖验证失败和启动失败路径。
5. 构建产物 strings/资源扫描不直接出现完整节点 URI、密码或私钥。
6. 调试功能不能导出 bootstrap 明文。

**非目标**：不实现远程更新。

## BOOT-G0-003：无端口 sing-box Direct-Dial PoC

**目标**：证明 sing-box outbound 能在没有网络 inbound 的情况下代理 HTTPS GET/POST。

**依赖**：`BOOT-G0-002`、固定 sing-box 版本。

**交付物**：PoC、direct-dial/窄 Go bridge 接口、端口扫描和抓包报告。

**验收规则**：

1. Control Plane 配置不含 HTTP、SOCKS、mixed、TUN、redirect 或其他网络 inbound。
2. 通过 outbound 成功访问境外测试 API 的 GET 和带 body 的 POST，TLS 验证正常。
3. Windows `Get-NetTCPConnection`/等价工具、Linux/macOS `lsof/ss`、移动平台 socket 审计均没有 Control Plane TCP/UDP listener。
4. 若 libbox 无稳定 DialContext，窄 Go bridge 只接受结构化 HTTPS 请求，并通过 stdio/UDS/Named Pipe 通信，仍不开放网络端口。
5. 阻断代理节点后请求失败，抓包证明没有直接连接 API IP/host。
6. 并发、超时、取消、DNS 失败、TLS 失败和响应上限均有测试。

**非目标**：不启动系统 VPN 或系统代理。

**实现基线**：PoC 固定 `github.com/sagernet/sing-box v1.13.14`。`native/controlplane` 直接调用 sing-box outbound 的 `DialContext`，只通过长度前缀 stdio 接收结构化 `init/request/cancel` 帧；`init` 必须携带 bootstrap `startupDns`，首项作为代理节点域名解析器，支持 UDP/TCP/DoT。Control Plane 配置不注册 inbound，且 `route.final` 固定为唯一代理 outbound。最终平台嵌入/sidecar 形态仍由各平台 G0 切片决定。

## BOOT-P0-004：BootstrapTransport 与业务请求强制路由

**目标**：所有业务 HTTP 只能经 Control Plane 发出。

**依赖**：`BOOT-G0-003`、`SEC-G0-003`、`ARC-G0-002`。

**交付物**：BootstrapTransport、业务 command client、host/redirect validator。

**验收规则**：

1. 登录、注册、config、订阅、账户、套餐、订单、邀请、工单和更新 command 全部注入同一 BootstrapTransport。
2. 代码库不存在第二套可访问生产后端的 direct HTTP client；CI 静态规则覆盖常见客户端构造点。
3. URL scheme 只允许 HTTPS，host 必须在 command 对应 allowlist，重定向每跳重新校验。
4. 前端不能传完整 URL、Authorization 或 bootstrap route；token 由 Rust 安全层注入。
5. 请求/响应大小、连接/总超时、并发和重试次数有上限。
6. fixture 测试覆盖所有业务端点和错误映射。

**非目标**：用户经 VPN 访问的网站不经过此业务 client。

## BOOT-P0-005：节点故障切换与 Fail-Closed

**目标**：境内网络下可靠访问后端，同时严格禁止裸连降级。

**依赖**：`BOOT-P0-004`。

**交付物**：候选节点 selector、健康检查、熔断/退避、Control Plane degraded UI 状态。

**验收规则**：

1. 至少支持两个 bootstrap 候选，单节点失败可在限定时间切换。
2. 故障节点进入熔断，重试使用有抖动退避，不形成请求风暴。
3. 所有节点失败时 API command 返回明确 bootstrap-unavailable，不尝试 DIRECT。
4. 节点恢复后用户可主动重试，或在安全上限内自动恢复。
5. 健康检查只访问批准的轻量端点，不包含用户数据。
6. Data Plane 在线、切换、停止和崩溃期间 Control Plane 故障切换仍可工作且不发生路由环。

**非目标**：不向用户展示 bootstrap 节点明文或允许用户将其用于系统代理。

## BOOT-P1-006：签名更新、轮换与防回滚

**目标**：无需发版更新 bootstrap 节点，同时防篡改和旧版本回滚。

**依赖**：`BOOT-P0-005`、`ARC-P1-004`。

**交付物**：Ed25519 签名 envelope、更新 client、设备加密存储、回滚槽位。

**验收规则**：

1. 更新包含版本、生成/到期时间、目标渠道、密文哈希和 Ed25519 签名。
2. 客户端只内置验签公钥，签名私钥不进入任何客户端或普通 CI 日志。
3. 版本低于已接受版本、签名错误、渠道错误、过期或 schema 不兼容均拒绝。
4. 新配置旁路启动并完成 API 健康检查后才原子激活；失败保留上一可用配置。
5. 落盘更新使用设备随机 key 加密并由系统安全存储保护 key。
6. 轮换测试覆盖当前/下一验签公钥过渡以及全部节点凭据失效场景。

**非目标**：不通过此机制更新可执行代码或 sing-box 二进制。
