# Orange AI 开发规范

> 适用于参与 Orange 的 AI 编码代理、自动化脚本和人工协作者。违反 G0 规则的变更不得合并。

## 1. 开始任务前

1. 阅读 [DEVELOPMENT_PLAN.md](DEVELOPMENT_PLAN.md)、[docs/README.md](docs/README.md) 和 [PROGRESS.md](PROGRESS.md)。
2. 阅读当前切片所属模块文档及全部前置切片。
3. 一次只认领一个主切片；需要并行时，每个子任务必须有独立文件边界和验收项。
4. 在修改代码前，把切片状态改为 `in_progress`，写清负责人/任务、预期交付物和下一检查点。
5. 如果需求没有对应切片，先补文档和验收规则，不直接写代码。

## 2. 不可信源规则

- `../Android-kotlin-Code` 是不可信只读参考，禁止复制其 Kotlin/Java/Go/C++ 模块、Manifest、后台服务、网络层或预编译二进制。
- 禁止执行原工程脚本、APK、AAR、SO、JAR、DEX 或未知工具。
- 只允许迁移 [UI 模块](docs/06-ui-assets.md) 白名单内、人工检查过的纯视觉资产。
- 从原工程观察到的 API/页面行为必须在 Orange 重新建模、重新实现和重新测试。
- 搜索不到恶意代码不代表安全；权限、依赖、产物和运行时出网必须独立证明。

## 3. 永久禁止项

- 不读取、枚举、缓存、上传用户相册、截图、图片目录或无关文件。
- 不实现 OCR、图像文字识别、助记词/BIP-39/seed phrase 检测。
- 不接入键盘、剪贴板、通讯录、短信、通话、相机、麦克风、位置或设备指纹采集。
- 不将 secret、token、订阅、节点、支付参数或用户内容写入日志、WebView、crash report、fixture 或提交。
- 不下载并执行远程代码，不通过 remote config 扩大权限或采集范围。
- 不增加 Clash、mihomo、Clash.Meta 代码、依赖、二进制或运行时配置模型。

发现任何禁止项时，立即停止相关实现，把切片标为 `blocked`，记录文件、依赖或行为证据，不尝试“保留但关闭”。

## 4. 双平面架构不变量

### Control Plane

- 应用启动后由 Rust 解密 `bootstrap.enc`，明文只在受控内存存在并及时 zeroize。
- 只包含 sing-box outbound/direct-dial，不得配置 HTTP/SOCKS/mixed/TUN/redirect 等网络 inbound。
- 所有业务 API 必须经 BootstrapTransport；代理失败时 fail closed，禁止 direct fallback。
- 前端只能调用固定业务 command，不能传任意 URL、Authorization、节点或路由配置。
- 桌面 helper 只用 stdio、UDS、Named Pipe 等受限 IPC，不开 TCP 控制端口。

### Data Plane

- 真实订阅先进入内部模型和 sanitizer，再生成 sing-box JSON；禁止原样执行服务端完整配置。
- 移动端使用系统 TUN/Packet Tunnel，不额外开放本地代理端口。
- 桌面 mixed inbound 只监听 loopback，监听成功后才能设置系统代理。
- 新配置/规则旁路验证后原子切换，失败保留上一可用版本。
- Data Plane 失败、停止、切换不能关闭或重建 Control Plane。

任何修改上述不变量的需求必须先更新架构文档、安全威胁模型和 G0 验收，不能在普通功能切片中顺手改变。

## 5. 代码边界

- React 只负责展示与用户交互，不持有 TUN、管理员权限、原始 token、bootstrap 或任意文件能力。
- Rust 是业务命令、DTO、状态机、配置净化、BootstrapTransport 和平台 adapter 的边界。
- Kotlin/Swift/Windows/Linux helper 只实现系统权限和固定原生能力，不承载可在共享层完成的业务逻辑。
- sing-box/libbox 通过窄版本化接口使用，不把内部结构直接暴露给 React。
- 所有 IPC/Tauri command 使用类型化 DTO；禁止任意 JSON map、shell string、注册表路径和文件路径接口。
- 新依赖必须说明用途、许可证、维护状态、体积和权限影响；可用标准库/现有依赖解决时不新增。

## 6. Secret、加密与配置

- 构建 key、签名 key、证书、token、测试账号只从批准的 secret store 注入。
- 包内 AEAD 加密是防简单提取的混淆层，不得把客户端静态 key 描述为不可提取安全边界。
- bootstrap 更新、规则 manifest 和可执行资源都要验签/校验 hash；远程配置不能更新 executable。
- secret 使用平台安全存储；普通设置与 secret 分开。
- 配置和规则写入使用候选文件、校验、原子替换和上一版本回滚。
- fixture 必须脱敏且使用不可登录的假数据。

## 7. 网络与系统设置

- 控制面 HTTPS scheme、host、重定向每跳都校验 allowlist。
- DNS 启动路径不能依赖尚未建立的同一代理；优先 bootstrap IP 或独立 resolver。
- 设置系统代理前先确认 mixed listener 成功并保存带所有权的快照。
- 恢复设置前比较当前值，不能覆盖用户运行期间手动修改的新值。
- Windows 使用原生 API，不调用 `reg.exe`、`netsh.exe`、PowerShell 或 shell。
- Linux helper 不拼接 shell；Apple 使用受支持的 Network/SystemConfiguration API。
- 每个平台都必须处理 stop、crash、reboot、upgrade、uninstall 的代理/DNS/route 恢复。

## 8. 文件与资源

- 手工编辑使用小范围补丁，不覆盖用户无关改动，不做无关格式化。
- 新 executable/library/SRS/MMDB/图片/动画必须先登记资源 manifest/资产 allowlist。
- 地理路由使用与固定 sing-box 兼容的 `.srs`；不复制原工程旧 geo 数据。
- MMDB 只做可选 UI/诊断，必须确认再分发许可证。
- 禁止订阅提供任意本地路径、路径穿越、符号链接逃逸和 `file://`。
- 运行期不得下载 executable；数据更新目录移除执行权限。

## 9. 实施流程

### 9.1 开工

- 确认依赖切片为 done；若不是，先做依赖或说明为什么可用 mock 独立推进。
- 写最小实现计划，列出将修改的文件和需要的测试。
- 检查工作区已有修改，保留不属于当前任务的用户改动。

### 9.2 开发

- 优先实现可测试的领域逻辑，再接平台/UI。
- 每次提交只解决当前切片或明确的子切片，不夹带重构。
- 失败路径与成功路径一起实现；不得以 TODO/假数据冒充验收。
- 对安全边界写简短解释性注释，普通代码不写重复叙述。

### 9.3 验证

- 逐条执行模块文档的验收规则；无法执行的项保持未完成并说明环境缺口。
- 运行受影响模块的 lint/unit/contract/build；网络/权限/平台切片增加真实系统测试。
- UI 用固定 fixture 截图；VPN 成功必须以真实出口/系统状态证明，浏览器 mock 不算。
- 安全切片必须保存端口、抓包、权限、SBOM 或文件审计证据。

### 9.4 收尾

- 把切片状态改为 `review`，附证据；只有全部验收通过后改 `done`。
- 更新 `PROGRESS.md` 的总数、模块汇总、切片行和变更记录。
- 若改变契约、权限、资源或平台行为，同步更新对应模块文档和验收矩阵。
- 最终说明必须列出未运行的测试和剩余风险，不能使用含糊的“应该可用”。

## 10. 测试最低要求

| 变更类型 | 最低验证 |
| --- | --- |
| React/UI | TypeScript、lint、unit、目标尺寸截图、键盘/触控主路径 |
| Rust domain/API | fmt、clippy、unit/contract、错误/超时/脱敏 fixture |
| Bootstrap/crypto | AEAD 篡改/错 key/过期、内存清零、strings/log scan |
| sing-box 配置 | schema、sanitizer、启动 smoke、坏配置回滚 |
| 平台 VPN/helper | 目标系统 build、真实 start/stop、crash/recovery、权限/IPC |
| 系统代理/TUN | 出口验证、端口、DNS/route、停止/重启/卸载恢复 |
| 资源/依赖 | manifest、hash/signature、SBOM、license、包内容差异 |
| 安全隐私 | 权限、诱饵文件、端口扫描、控制面抓包、crash/log scan |

## 11. 不允许的完成声明

以下情况不能标记 done：

- 只完成 UI，没有真实 Rust/平台能力。
- 只在浏览器或 mock 测试，没有目标平台验证。
- 只测试成功路径，没有权限拒绝、超时、crash 或回滚。
- 需要 Apple entitlement、签名、真机、境外 API 等外部条件但尚未取得。
- 端口、出网、权限、SBOM、系统恢复等 G0 证据缺失。
- 验收规则本身被实现临时修改得更宽松但没有架构评审。

## 12. AI 交接模板

```markdown
切片：BOOT-G0-003
状态：in_progress / blocked / review
已完成：
- ...
修改文件：
- ...
验证证据：
- 命令/报告/截图路径：...
未通过或未执行：
- ...
安全影响：
- 权限：无变化
- 出网：...
- secret：...
下一步：
- ...
```

## 13. 评审优先级

1. 数据泄露、相册/OCR/助记词、secret 和任意代码执行。
2. Control Plane 裸连、监听端口、SSRF 和签名/回滚。
3. 特权 helper/IPC、TUN 路由环、代理/DNS 恢复。
4. 订单重复、认证错误、配置损坏和状态机竞态。
5. 跨平台兼容、性能、可访问性和视觉差异。
