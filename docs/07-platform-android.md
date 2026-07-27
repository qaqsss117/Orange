# 模块 07：Android 平台

## 模块目标

通过 Tauri Mobile Plugin、官方 sing-box/libbox 和 Android `VpnService` 实现 Control Plane direct-dial 与 Data Plane TUN，满足后台、通知、网络切换和隐私权限要求。

**安全前置基线**：`SEC-G0-003` 已在 `native/android` 建立 Android Keystore AES-256-GCM token 存储原语、受控生成注入、lint/测试 APK CI 和 API 36 模拟器生命周期/篡改测试。该原语尚未接入类型化登录命令，也不代表 `AND-G0-001` libbox/Tauri 插件切片已开工。

## AND-G0-001：libbox 与 Tauri Kotlin 插件 PoC

**目标**：证明固定 sing-box 版本可在 Tauri Android 中构建并调用。

**依赖**：`ARC-G0-001`、`BOOT-G0-003`、`SEC-G0-004`。

**交付物**：可复现 AAR/so、Kotlin plugin、版本握手、ABI 构建。

**验收规则**：

1. arm64-v8a 真机构建、安装并调用 core version；armeabi-v7a 是否支持有明确结论和冒烟结果。
2. AAR/SO 来源、commit、Go/NDK 版本、SHA-256 和许可证进入资源 manifest。
3. Kotlin plugin 只暴露类型化命令，不提供任意 Go method、文件路径或 JSON 执行入口。
4. Control Plane GET/POST direct-dial PoC 无 TCP/UDP listener。
5. release 构建不含原工程 `libclash.so`、包名或模块。
6. 全新环境按脚本能重复生成相同版本产物。

**非目标**：不在此切片完成系统 VPN。

## AND-P0-002：VpnService、权限与前台生命周期

**目标**：完成 Android 系统 VPN 授权和可靠后台运行。

**依赖**：`AND-G0-001`、`VPN-P0-002`、`SEC-G0-002`。

**交付物**：VpnService、foreground notification、prepare/start/stop、状态恢复。

**验收规则**：

1. 首次启动通过 `VpnService.prepare()` 请求授权；拒绝、取消和稍后重试行为明确。
2. Android 13+ 通知权限拒绝时按平台限制处理，不假报后台可用。
3. Android 14+ FGS 类型、声明、启动时机和商店合规说明完整。
4. UI/WebView 被系统回收后 VPN 按用户期望保持，重新打开可回读真实状态。
5. 通知提供状态和安全停止入口，不泄漏节点 secret/订阅 URL。
6. API 24、29、33、35 完成 start/stop/锁屏/后台冒烟。

**非目标**：不申请照片、媒体库或旧式全存储权限。

## AND-P0-003：TUN、Socket Protect 与网络切换

**目标**：Data Plane 路由系统流量且 Control Plane 不形成环路。

**依赖**：`AND-P0-002`、`VPN-P1-006`。

**交付物**：TUN adapter、protect callback、DNS/route、network monitor。

**验收规则**：

1. 连接后出口 IP 改变，断开后出口、DNS 和路由恢复。
2. sing-box 数据面 outbound sockets 被正确 protect，不回流自身 TUN。
3. Control Plane 在线请求在 Data Plane 启动后继续成功且抓包无代理套娃。
4. Wi-Fi/移动网络切换、断网恢复、飞行模式、私网 DNS 场景有测试。
5. IPv4 必测；IPv6 按产品设置允许/禁止时行为与配置一致，无泄漏。
6. 移动构建无 mixed/HTTP/SOCKS 本地监听端口。

**非目标**：不默认允许 LAN 分享。

## AND-P1-004：分应用、快捷设置与开机恢复

**目标**：完成 Android 特有生产功能。

**依赖**：`AND-P0-003`、`UI-P1-007`。

**交付物**：app list adapter、allow/deny mode、Quick Settings Tile、boot/update receiver。

**验收规则**：

1. 应用列表只读取包管理器必要元数据，不读取应用数据；系统应用过滤可控。
2. allow/deny 模式、全选/反选、卸载应用清理和重启保持有测试。
3. Tile 状态来自 VpnService，点击幂等，未授权时打开正常授权入口。
4. 开机/应用更新恢复只在用户显式开启时生效，并遵守新系统后台限制。
5. 自身 package 的路由/protect 规则不会破坏 BootstrapTransport。
6. `QUERY_ALL_PACKAGES` 若保留必须有产品必要性和商店合规说明；能用 query intent 替代时移除。

**非目标**：不扫描应用安装目录或用户文件。

## AND-P1-005：Android 打包、升级与隐私验收

**目标**：交付可签名发布的 APK/AAB。

**依赖**：Android 全部 `P0/P1`、`SEC-P1-005`、`REL-P1-005`。

**交付物**：signed APK/AAB、manifest/ABI/size report、升级与卸载报告。

**验收规则**：

1. release manifest 权限与白名单完全一致，无照片、相机、麦克风、位置权限。
2. AAB split 包含声明支持的 ABI，缺失或多余 ABI 有明确处理。
3. 新装、覆盖升级、数据库迁移、清数据、卸载重装均通过。
4. 卸载/清数据后无活动 VpnService、通知、路由和明文配置。
5. Play policy 所需隐私、安全和 VPN 声明完成；签名密钥不在仓库。
6. 产物哈希、mapping/symbol、SBOM 和真机结果登记到进度台账。

**非目标**：没有原签名时不承诺覆盖旧 APK。
