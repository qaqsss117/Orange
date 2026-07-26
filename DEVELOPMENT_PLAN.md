# Orange 跨平台 VPN 开发计划

> 文档版本：v1.0  
> 更新日期：2026-07-26  
> 状态：文档基线完成，产品代码尚未开工  
> 技术路线：Tauri 2 + React/TypeScript + Rust + 纯 sing-box  
> 目标平台：Windows、Android、macOS、iOS、Linux

## 1. 硬性目标

- 使用 clean-room 方式重建 UUVPN 的合法界面和业务能力，原 Android 工程仅作为不可信只读参考。
- 所有平台只使用 sing-box 内核，不引入 Clash、mihomo、Clash.Meta 代码、库、配置运行时或二进制。
- 应用启动即建立无网络 inbound 的 Control Plane sing-box outbound；登录、注册、订阅等控制面流量禁止裸连境外 API。
- 获取有效订阅后独立启动 Data Plane；移动端使用系统 TUN，桌面端支持 TUN 和 loopback mixed inbound + 系统代理。
- 严禁相册读取/上传、OCR、助记词识别、后台文件扫描和隐蔽数据采集。
- 五个平台都必须经过权限、出网、签名、路由恢复和真实设备/系统验收后才能发布。

## 2. 切片等级

| 等级 | 含义 | 开工/发布规则 |
| --- | --- | --- |
| `G0` | 架构、安全、供应链或平台可行性门禁 | 未完成时，依赖它的功能不得开工；任一失败阻断发布 |
| `P0` | 最小可用产品主链路 | 对应平台公开测试前必须完成 |
| `P1` | 完整业务、系统集成和生产质量 | 正式发布前必须完成 |
| `P2` | 增强、渠道或非核心兼容能力 | 可在正式版后迭代，但不能破坏 G0/P0/P1 |

切片 ID 使用 `模块-等级-序号`，例如 `BOOT-G0-003`。切片只有满足所属模块文档中的全部验收规则，并在 [PROGRESS.md](PROGRESS.md) 登记证据后才能标记完成。

## 3. 文档地图

| 模块 | 文档 | 主要责任 |
| --- | --- | --- |
| 文档规范与切片索引 | [docs/README.md](docs/README.md) | 切片格式、依赖关系、证据规范 |
| 安全与隐私 | [docs/01-security-privacy.md](docs/01-security-privacy.md) | 不可信源隔离、权限、出网、隐私门禁 |
| 共享架构 | [docs/02-shared-architecture.md](docs/02-shared-architecture.md) | Workspace、DTO、状态机、平台 adapter |
| Bootstrap Control Plane | [docs/03-bootstrap-control-plane.md](docs/03-bootstrap-control-plane.md) | 包内加密节点、Rust 解密、无端口 direct-dial |
| sing-box Data Plane | [docs/04-singbox-data-plane.md](docs/04-singbox-data-plane.md) | 订阅、TUN/mixed、节点、流量、双平面切换 |
| 业务 API | [docs/05-business-api.md](docs/05-business-api.md) | 登录、账户、套餐、订单、邀请、工单 |
| UI 与资产 | [docs/06-ui-assets.md](docs/06-ui-assets.md) | 高保真页面、响应式、资产白名单 |
| Android | [docs/07-platform-android.md](docs/07-platform-android.md) | VpnService、libbox、权限、后台能力 |
| Apple 平台 | [docs/08-platform-apple.md](docs/08-platform-apple.md) | iOS Packet Tunnel、macOS Extension、签名 |
| Windows | [docs/09-platform-windows.md](docs/09-platform-windows.md) | EXE/Service、Named Pipe、系统代理、TUN |
| Linux | [docs/10-platform-linux.md](docs/10-platform-linux.md) | helper、polkit/systemd、代理、TUN、打包 |
| 规则与地理数据 | [docs/11-rules-geo-data.md](docs/11-rules-geo-data.md) | `.srs`、可选 MMDB、签名更新、许可证 |
| 测试与发布 | [docs/12-testing-release.md](docs/12-testing-release.md) | CI、E2E、隐私专项、安装包和发布门禁 |
| 全局验收矩阵 | [docs/13-acceptance-matrix.md](docs/13-acceptance-matrix.md) | 主流程、平台、异常与安全验收总表 |
| 进度台账 | [PROGRESS.md](PROGRESS.md) | 所有切片状态、证据、阻塞和下一步 |
| AI 开发规范 | [AI_DEVELOPMENT_RULES.md](AI_DEVELOPMENT_RULES.md) | AI/自动化代理必须遵守的实施规则 |

## 4. 总体架构

```text
React/Tauri UI
    │
Rust shared application layer
    ├─ Business API commands
    ├─ BootstrapTransport
    ├─ ControlPlaneState / DataPlaneState
    ├─ sing-box config validation
    └─ PlatformVpnAdapter
          ├─ Android VpnService + libbox
          ├─ iOS PacketTunnelProvider + libbox
          ├─ macOS Network Extension/helper
          ├─ Windows Service/helper
          └─ Linux systemd/polkit helper

Control Plane: bootstrap outbound only, no network listener
Data Plane: subscription outbounds + mobile TUN / desktop TUN or loopback mixed
```

详细状态机、IPC 和平台边界见对应模块文档，不在总计划重复维护。

## 5. 建议实施顺序

1. `SEC-G0-*`：完成安全隔离、权限/出网白名单和供应链门禁。
2. `ARC-G0-*`：初始化五平台 workspace、统一 DTO/状态机和 adapter 契约。
3. `BOOT-G0-*`：证明 Rust 内存解密与 sing-box 无端口 direct-dial 可行。
4. `WIN-G0-*`、`AND-G0-*`、`APL-G0-*`、`LNX-G0-*`：完成平台可行性 PoC。
5. `VPN-G0-*`：完成纯 sing-box 配置规范与双平面隔离。
6. `UI-P0-*`、`API-P0-*`、`VPN-P0-*`：打通登录、订阅、连接、节点和账户主链路。
7. 完成各平台 `P0`，交付内部可用构建。
8. 完成全部 `P1`、全局验收矩阵和签名发布流程。
9. 按渠道和用户反馈安排 `P2`。

## 6. 预估

五平台完整交付预计约 95～140 个工程日，另加 Apple entitlement、代码签名、商店审核和设备等待时间。建议 2～3 名具备 Tauri、Rust/Go 和平台 VPN 经验的工程师并行，目标周期 14～20 周；完成 G0 PoC 后重新估算。

## 7. 全局完成定义

- 对应切片验收规则全部通过，并在进度台账链接到测试、截图、日志或产物。
- 依赖树和产物中没有 Clash/mihomo，也没有照片/OCR/助记词相关权限或依赖。
- Control Plane 无网络监听且业务 API 不裸连；Data Plane 失败不破坏 Control Plane。
- 敏感配置不落盘明文、不进入日志、WebView、崩溃报告或构建输出。
- 平台代理、DNS、路由能在停止、崩溃、升级和卸载后正确恢复。
- 所有 EXE/DLL/helper/appex/SRS/MMDB 都有版本、哈希、来源、许可证和签名记录。
- 五平台 UI 无阻断级布局、输入、返回、权限或可访问性问题。

## 8. 当前下一步

按 [PROGRESS.md](PROGRESS.md) 从 `SEC-G0-001` 开始；在 `BOOT-G0-003` 无端口 direct-dial PoC 通过前，不批量开发业务页面。
