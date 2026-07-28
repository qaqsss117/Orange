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

**实现基线**：PoC 固定 `github.com/sagernet/sing-box v1.13.14`。`native/controlplane` 直接调用 sing-box outbound 的 `DialContext`，支持严格 Shadowsocks、Trojan、Hysteria2 和 VLESS Reality；VLESS 固定 TCP、验证 TLS、uTLS Chrome 与 `xtls-rprx-vision`，Reality short ID 可选。bridge 只通过长度前缀 stdio 接收结构化 `init/request/cancel` 帧；`init` 必须携带 bootstrap `startupDns`，首项作为代理节点域名解析器，支持 UDP/TCP/DoT。Control Plane 配置不注册 inbound，且 `route.final` 固定为唯一代理 outbound。

桌面端由 `orange-control-plane-host` 从绝对、已规范化路径直接启动 sidecar，不经过 shell，并先清空宿主环境；Windows 只恢复 Winsock provider 初始化所需的非敏感 `SystemRoot`，`PATH` 等其余环境仍不继承，进程也不创建控制台窗口。宿主负责 `ready` 握手、并发请求分派、显式/超时/Drop 取消、退出广播、EOF 优雅关闭以及超时后的强制回收；稳定错误只暴露脱敏码。`SecretBuffer` 通过 `consume_in_place` 生成 `init` 帧后立即清零，sidecar 退出释放原生副本。Windows/Linux/macOS 平台配置通过固定 `externalBin` 注册目标三元组 sidecar，构建时把其 SHA-256 嵌入应用，运行时只解析应用同目录固定文件并在启动前复验。Tauri managed state 最多持有一个桌面宿主实例；Android/iOS 不注册也不编译桌面进程宿主，其嵌入式 native 形态仍由平台 G0 切片决定。

桌面生产构建仅在 `ORANGE_BOOTSTRAP_BUILD_KEY_HEX` 存在时启用嵌入：`build.rs` 先认证解密密文、验证 production channel 与应用版本，再把密文和非敏感 manifest 编入应用。没有构建密钥的开发包继续保持未配置；Android/iOS 若携带生产构建密钥则明确失败。真实密文和本地 DPAPI 包装的构建密钥只存在于忽略的 `artifacts/` 目录，不进入仓库或构建日志。离线解密所需的原始密钥会暂存在受保护构建环境和忽略的编译输出中，并最终存在于桌面二进制；密文边界不用于抵抗对已发布应用的逆向分析。

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

**实现基线**：`orange-platform` 以十个 `BusinessCommand` 固定开发 HTTPS host、method、path、认证方式和 content type，并由契约 fixture、`security/control-endpoints.yml` 与 Rust 测试三方锁定。`BusinessCommandClient` 只持有一个 `BootstrapTransport`；五个认证路由在 Rust 内部从平台安全存储读取 access token，缺失 token 时在调用 transport 前失败。订阅正文下载也由同一 client 从原生安全存储读取 URL，严格拒绝非 HTTPS/443、userinfo、fragment、非 allowlist host 和异常 path/query；只把已验证的 host 与 path/query 交给 transport，并以自动清零缓冲接收正文。请求和响应均限制为 1 MiB，重定向一律拒绝，每个 command 只执行一次，错误只映射为稳定脱敏码。

桌面 Tauri 壳把唯一 `Arc<ManagedControlPlane>` 同时用于状态管理和业务 client；adapter 只能把固定 route 转为 `ControlPlaneRequest`。stdio `request` 帧允许一个可选、Base64 编码且有字符和长度限制的 `accessToken` 字段，不接受任意 header；Go bridge 仅在 native 边界构造 `Authorization: Bearer ...`，Rust/Go 两侧在使用后清零 token 缓冲。安全门禁阻断 managed adapter 之外的原始 Control Plane 请求构造、第二套 HTTP client、WebView URL/host/token/Authorization 字段及运行时日志出口。

当前本地生产 bootstrap 已把批准的 API host 放入加密 allowlist。2026-07-28 的真实桌面探针确认 config、登录、账户和订阅元数据四条 `/api/v1` 路由均经 Rust host 与 Go sidecar 返回 HTTP 200，认证后的订阅正文下载也只经同一 allowlist 和 Control Plane 完成；对应安全下载边界现已进入生产 Rust client 和桌面 adapter。生产注册及套餐、订单、邀请、工单和更新路由尚未取得契约证据，继续保留开发路径；生产配置下注册明确 fail closed，不会尝试猜测端点。Android/iOS 嵌入式 Control Plane transport 与正式依赖仍未收口，因此切片保持 `in_progress`。证据见 `docs/evidence/API-P0-003-production-business-vless-2026-07-28.md`。

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
