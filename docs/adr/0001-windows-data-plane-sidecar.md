# ADR-0001：Windows Data Plane 使用受签名官方 sing-box sidecar

- 状态：已被 ADR-0002 取代
- 日期：2026-07-28
- 决策切片：`WIN-G0-001`

本 ADR 保留最初采用官方 CLI sidecar 的历史。`VPN-P0-004` 验证后确认官方 CLI 在不启用
网络控制 API 时无法提供 selector 权威回读、节点测速和流量统计，后续宿主模型由
`docs/adr/0002-windows-data-plane-managed-host.md` 取代。

## 背景

Windows Data Plane 需要同时支持 loopback mixed 和后续可选 TUN。候选方案是把
sing-box 编译进 `orange-service.exe`，或由 service 托管独立的 `sing-box.exe`。
Orange 已经固定 sing-box `1.13.14`，并由闭合配置模型生成 TUN、DNS、route 和
Shadowsocks/Trojan/Hysteria2/selector；不能再维护第二套核心实现或接受任意上游配置。

## 决策

Windows 只保留“受 Authenticode 签名的官方 `sing-box.exe` sidecar”这一条生产路径。
不把 sing-box 编进 `orange-service.exe`，也不维护自定义 sing-box 命令入口。制品从
`github.com/sagernet/sing-box/cmd/sing-box@v1.13.14` 构建，唯一启用标签是
`with_quic,with_utls`，分别用于已批准的 Hysteria2 与 VLESS Reality。官方默认标签中的 Clash API、Tailscale、
WireGuard、ACME、DHCP、gVisor、uTLS、CCM 和 OCM 均不启用。

上游官方命令仍包含通用命令和基础协议注册；Orange 不依赖裁剪这些入口作为安全边界。
安全边界是：service 只传入 Orange 自己重新生成的闭合配置，只执行固定 `run -c`
操作，不接收订阅提供的路径、命令、服务配置或原始 sing-box JSON。

`native/dataplane` 是独立的制品构建模块。它的依赖锁和实际 Windows 编译图进入
SBOM，但不扩张 `native/controlplane` 的运行依赖。运行时禁止下载或替换 EXE/DLL。

## 进程与权限边界

- `orange.exe` 始终以普通交互用户运行，不安装服务、不加载驱动，也不直接启动或向
  `sing-box.exe` 传递任意参数。
- 安装器是独立的显式管理员操作，负责安装受签名 `orange-service.exe`、固定同目录
  sidecar，以及后续获批的 Wintun 组件。日常 UI 不请求管理员权限。
- `orange-service.exe` 是唯一的特权协调边界。`WIN-P0-002` 必须以 service SID、最小
  令牌权限和 Named Pipe ACL 实现固定 DTO；不能暴露 shell、任意路径、URL、注册表
  路径或原始 sing-box 命令。
- service 只解析安装目录内固定兄弟文件 `sing-box.exe`，清空非必要环境，以固定服务
  所有且普通用户不可写的配置路径执行 `run -c`。sidecar 不拥有更新自身或安装组件的
  能力。
- mixed 模式只监听运行时登记的 loopback 端口；TUN、路由和 DNS 所需的最小权限及
  恢复责任由后续 Windows adapter 验证，不能通过提升 UI 权限解决。

## 启动握手与发布顺序

生产 service 启动 sidecar 前必须按顺序完成以下检查，任一步失败即拒绝启动：

1. 规范化固定兄弟路径，并确认其仍在受保护的安装目录内。
2. 计算 SHA-256，与签名后发布 manifest 中的摘要进行常量时间比较。
3. 使用 Windows `WinVerifyTrust` 验证 Authenticode 链和文件签名，不执行 PowerShell。
4. 将签名证书 SHA-1 指纹与发布策略白名单精确匹配。
5. 执行固定 `version` 握手，确认 `1.13.14`、`windows/amd64`、`with_quic,with_utls` 和
   `CGO: disabled`。
6. 只以固定参数启动，并由 Data Plane 生命周期监管器接管就绪、崩溃和回收。

发布流水线先进行锁定、可复现构建，再签名，最后对签名后的字节计算 SHA-256 并生成
manifest。开发构建允许 `NotSigned`，但强制记录为 `unsigned-debug` 和
`release_allowed: false`。只有 `Valid` 且指纹在白名单内的制品才能标记为可发布。

## 被拒绝的方案

- **编进 service**：会把 Rust service 与 Go 核心的构建、崩溃和升级耦合，并与现有
  sidecar 形成两条生产路径；拒绝。
- **自定义精简命令**：可以进一步删除官方 CLI 命令，但会形成需要长期跟随上游注册和
  生命周期变化的 Orange fork；当前以最小官方标签和闭合配置边界控制能力。
- **官方默认标签**：会编入当前产品未批准的 API、组网、驱动和证书能力；拒绝。
- **运行时下载官方二进制**：无法保证安装原子性、签名主体和回滚一致性；拒绝。

## 后果与未完成项

本决策固定了唯一宿主模型、构建来源、功能标签、manifest 和握手顺序。代价是安装包
包含一个独立 GPL 制品，service 必须实现可靠的子进程监管与固定路径校验。

`WIN-G0-001` 在取得正式代码签名证书、将允许指纹写入策略，并在 Windows 10 22H2
和 Windows 11 当前版完成签名后制品兼容测试前保持 `in_progress`。生产
`WinVerifyTrust`/固定 manifest 接线、service SID 与 Named Pipe 实现属于
`WIN-P0-002`，Wintun 清单和权限验证属于 `WIN-P1-004`。
