# ADR-0002：Windows Data Plane 使用 Orange 受管 sing-box 宿主

- 状态：已接受
- 日期：2026-07-28
- 决策切片：`WIN-G0-001`、`VPN-P0-004`
- 取代：ADR-0001

## 背景

ADR-0001 选择由 Windows service 托管官方 `sing-box` CLI。该路径可以固定配置检查和
`run -c` 生命周期，却不能在不启用额外控制面的前提下权威读取或修改 selector、执行
指定节点测速、读取流量总量。上游 Clash API 能提供这些能力，但需要 `with_clash_api`
并建立 HTTP listener；这会把通用 API、URL 和更大的对象模型带进特权 Data Plane，
与 Orange 的固定 DTO、无网络控制 listener 和最小能力策略冲突。

## 决策

Windows 仍只保留一个独立、受 Authenticode 签名并由 `orange-service.exe` 监管的 GPL
Data Plane 制品，但入口改为 Orange 自有 `orange-data-plane.exe`。该入口直接依赖锁定的
官方 `github.com/sagernet/sing-box@v1.13.14` 公共 Go API，不修改、复制或 fork sing-box
核心。唯一构建标签仍为 `with_quic`，用于已批准的 Hysteria2。

宿主只注册当前产品需要的 TUN/loopback mixed、direct、Shadowsocks、Trojan、Hysteria2、
selector 和 local DNS；不导入官方通用 CLI 注册表。`with_clash_api`、`with_v2ray_api`
及既有禁止标签继续失败关闭。固定 `check -c`、`run -c`、`version` 命令保持不变，service
仍只传入受保护 revision store 内的净化配置。

`run` 使用继承的 stdin/stdout 建立无 listener 的 v1 控制协议：

- 4 KiB 大端长度前缀 JSON 帧，请求 ID 必须正数且按读取顺序严格递增；
- 只接受 `select_node`、`read_selected_node`、`probe_delay`、`cancel_probe` 和 `traffic`；
- selector/node 只接受与 Rust 公共目录相同的 64 字节公开 ID，不接受 `orange-*`；
- selector 切换必须调用 sing-box 并立即以 `Now()` 回读确认；
- 测速只使用固定 HTTPS 204 地址，100～60000 ms，最多 8 个并发 probe，可关联取消；
- 流量通过 sing-box router tracker 统计 TCP/UDP 单调总字节，只返回安全整数；
- 不接受 URL、路径、凭据、原始配置、任意命令、日志对象或连接元数据。

任何未知字段、未知命令、超限帧、重复/倒退请求 ID 或非法公开 ID 都终止该进程协议。
进程仍由 Windows Job Object 回收，service 继续执行路径、SHA-256、`WinVerifyTrust`、签名
指纹、版本、配置哈希、TUN readiness 和 cleanup 检查。运行时下载仍被禁止。

## 被拒绝的方案

- **启用 Clash API**：需要网络 listener 和通用 HTTP 对象面，能力明显超过产品需求。
- **继续官方 CLI 并伪造 UI 状态**：无法满足回读、回滚和权威流量验收规则。
- **修改上游 sing-box 源码**：会形成长期 fork；本方案只组合公开 API。
- **把 Go 核心编进 Rust service**：破坏独立崩溃、签名和升级边界。

## 后果与迁移

宿主 main 包和控制协议由 Orange 维护，必须随每次 sing-box 升级重新运行协议、配置、
可复现构建、SBOM、签名和真实流量测试。制品整体继续按 GPL-3.0-or-later 记录。

当前实现已完成宿主协议、最小注册表、双构建一致性和离线 mixed HTTP/SOCKS5 实测。
service 以严格有界 Rust stdio client 校验 `ready`、按请求 ID 关联乱序响应与取消，协议失败
会关闭 stdin 并失败所有待处理请求；生产 `DataPlaneNodeBackend` 将 client 绑定到当前
configuration revision、supervisor instance、进程 PID 和 client 身份，并在操作前后复核。
真实 Rust/Go 进程互操作已完成 selector 切换/回读、流量读取和 EOF 回收。

外层受限 Named Pipe 已以 10 个固定命令接通节点 DTO。由于管道保持单实例且每次连接只
处理一个请求，测速采用 begin/poll/cancel，而不是阻塞 service 的同步调用；service 最多
运行 8 个探测、保留 32 条记录，完成结果 5 秒后失效，取消在与晚到结果竞争时优先。

平台共享 runtime owner 已建立候选恢复后原子发布、失败保留旧 revision 的边界；Windows
应用只从固定同目录 installer 身份文件建立一个原生 client，并让其同时进入生命周期
adapter 与节点 runtime host，非法或缺失身份保持未配置。真实 installer/ACL、活动净化
配置 handoff、生命周期流量事件、Tauri/UI 和真实签名 TUN 节点抓包尚未接线，因此
`WIN-G0-001`、`WIN-P0-002` 与 `VPN-P0-004` 均保持
`in_progress`。
