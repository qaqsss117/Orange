# 模块 08：iOS 与 macOS

## 模块目标

在 Apple 平台通过 Network Extension 和 libbox 提供系统 VPN，在 macOS 可选提供 loopback mixed + 系统代理；完成 entitlement、App Group、Keychain、签名和 notarization。

## APL-G0-001：Entitlement、构建机与签名 PoC

**目标**：最早确认 Apple 平台不是外部不可行路线。

**依赖**：`ARC-G0-001`、`SEC-G0-002`。

**交付物**：Mac 构建机、Developer Team 配置、Network Extension PoC、签名记录。

**验收规则**：

1. Apple Developer Team 已获可用 Network Extension entitlement，真机可安装主 App + extension。
2. iOS 和 macOS bundle ID、App Group、extension ID 有稳定命名且写入非敏感配置。
3. Xcode/SDK 最低版本与 CI runner 固定，证书/profile 从密钥系统注入。
4. 最小 PacketTunnelProvider/Network Extension 可启动、停止并把状态返回 Tauri。
5. entitlements/Info.plist 不含 Photos、Camera、Microphone、Contacts、Location。
6. 无 entitlement 时切片标 blocked 并记录解除条件，不能用普通 WebView 伪装 VPN 完成。

**非目标**：不要求 PoC 有完整代理协议。

## APL-G0-002：libbox XCFramework 与版本握手

**目标**：生成 iOS/macOS 可复现的 sing-box 绑定。

**依赖**：`APL-G0-001`、`SEC-G0-004`。

**交付物**：XCFramework、构建脚本、架构矩阵、hash/license manifest。

**验收规则**：

1. 支持目标 iOS device/simulator 和 macOS Apple Silicon；Intel 是否支持有明确决策。
2. debug/release slice 正确，release 不含非必要符号或原工程二进制。
3. 主 App/extension 启动时核对 core version，不匹配则拒绝使用。
4. XCFramework commit、Go/Xcode 版本、架构、SHA-256 和许可证登记。
5. 重复构建步骤可复现功能等价产物并通过 smoke test。
6. 依赖和符号扫描不含 Clash/mihomo。

**非目标**：不把 bootstrap 明文编译进 Swift 源码。

## IOS-P0-003：Packet Tunnel Data Plane

**目标**：在 iPhone/iPad 上建立真实系统隧道。

**依赖**：`APL-G0-002`、`VPN-P0-002`、`VPN-G0-001`。

**交付物**：PacketTunnelProvider、配置桥、状态/流量、路由和 DNS。

**验收规则**：

1. 系统设置可授权、启动、停止 VPN；主 App 状态与系统状态一致。
2. 连接后出口改变，停止/extension 崩溃后路由和 DNS 恢复。
3. iPhone/iPad 锁屏、切后台、网络切换和 extension 重启有真实设备测试。
4. 配置经 App Group 传递时加密/最小化，主 App 不写完整明文到共享日志。
5. IPv4/IPv6 和 DNS 行为符合配置，无明显泄漏。
6. iOS 不开放 mixed/HTTP/SOCKS listener。

**非目标**：不提供 iOS 全局“系统代理端口”。

## IOS-P0-004：Control Plane 与 Packet Tunnel 防环

**目标**：VPN 开启后业务 API 仍稳定经过 bootstrap outbound。

**依赖**：`BOOT-P0-005`、`IOS-P0-003`、`VPN-P1-006`。

**交付物**：Control Plane 执行位置决策、App Group/IPC、route exclusion、测试报告。

**验收规则**：

1. 明确 Control Plane 在主 App 或 extension 执行，生命周期和唤醒限制有文档。
2. Data Plane 在线时登录态刷新、账户和订阅请求可用，无路由环和 DNS 自举死锁。
3. 主 App 被挂起时不要求后台执行不被 iOS 允许的任务；恢复后状态正确。
4. IPC 只传固定请求/响应，不传任意 URL、文件路径或 bootstrap secret 给 WebView。
5. extension 故障不破坏落盘 bootstrap 更新和下次登录恢复。
6. 抓包/系统日志证明业务 API 不裸连。

**非目标**：不绕过 iOS 后台执行政策。

## MAC-P0-005：macOS Network Extension 与系统代理

**目标**：在 macOS 支持 Network Extension，并按产品模式提供系统代理。

**依赖**：`APL-G0-002`、`VPN-P1-005`。

**交付物**：macOS extension/helper、SystemConfiguration adapter、菜单栏状态。

**验收规则**：

1. Network Extension/TUN 模式连接、断开、睡眠唤醒和网络切换正常。
2. mixed 模式只监听 loopback，监听成功后才调用 SystemConfiguration 设置代理。
3. 保存原代理快照并在停止、崩溃恢复、升级和卸载后比较恢复。
4. Control Plane 使用 XPC/受限 IPC direct-dial，无 TCP 控制端口。
5. 普通代理设置尽量不请求管理员权限；需要 privileged helper 的操作最小化且签名验证。
6. Apple Silicon 必测，Intel 按发布范围；系统设置中不遗留失效 VPN 配置。

**非目标**：不默认修改全机器所有用户代理。

## APL-P1-006：Keychain、更新、签名与发布

**目标**：交付可 TestFlight、签名和公证的 Apple 产物。

**依赖**：Apple 全部 `P0`、`SEC-P1-005`、`REL-P1-005`。

**交付物**：Keychain adapter、IPA/TestFlight、DMG/PKG、notarization、隐私清单。

**验收规则**：

1. token/device key 使用正确 access group 和 accessibility，extension 只获必要项。
2. iOS 新装/升级/卸载、macOS 新装/升级/卸载和配置迁移通过。
3. macOS app/helper/extension 均签名一致并通过 codesign 验证、notarization 和 Gatekeeper。
4. App Store Privacy Manifest/隐私说明与实际权限和 SDK 一致。
5. release 包无 debug endpoint、测试账号、开发 profile、bootstrap 明文和未登记 framework。
6. 产物哈希、dSYM、SBOM、真机版本和审核前检查登记。

**非目标**：不承诺在没有商店/证书权限时绕过 Apple 分发规则。
