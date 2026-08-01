# Orange 跨平台 VPN 开发计划

> 更新日期：2026-08-01
> 当前阶段：优先迁移 UUVPN 产品内容
> 技术路线：Tauri 2 + React/TypeScript + Rust + sing-box
> 目标平台：Windows、Android、macOS、iOS、Linux

## 1. 产品目标

- 将 UUVPN 已有的用户界面和业务能力迁移到 Orange。
- 使用纯 sing-box 实现 Control Plane 与 Data Plane，不引入 Clash 或 mihomo。
- 桌面端提供系统代理和 TUN，移动端接入系统 VPN 能力。
- 五个平台共享业务模型、状态与界面，只在系统能力边界使用原生实现。
- 保留必要的隐私和安全边界，不迁移与 VPN 无关的数据采集能力。

## 2. 当前开发顺序

1. 盘点 UUVPN 与 Orange 的页面、业务 API、状态和平台能力差异。
2. 优先迁移登录、注册、首页连接、节点选择、订阅和账户主链路。
3. 补齐套餐、订单、支付、邀请、工单、公告和更新等业务页面。
4. 接通 Windows 产品能力，再依次完成 Android、macOS、iOS 和 Linux。
5. 完成设置、托盘、启动、升级与卸载等平台体验。
6. 由用户统一组织测试、验收和发布检查。

## 3. 架构

```text
React / Tauri UI
    |
Rust shared application layer
    |-- Business API commands
    |-- BootstrapTransport
    |-- Control Plane / Data Plane state
    |-- sing-box configuration
    `-- Platform adapters
         |-- Android VpnService
         |-- Apple Network Extension
         |-- Windows Service
         `-- Linux helper
```

Control Plane 只承担业务 API 出网，不开放本地网络入口。Data Plane 独立承担订阅节点、TUN、桌面 mixed inbound 和系统代理，两者生命周期互不破坏。

## 4. 实施原则

- 按用户可见功能迁移，每个提交形成一个独立可理解的产品增量。
- 优先复用 Orange 已有类型、组件和平台接口，再补缺失能力。
- UUVPN 来源代码先阅读再适配，不直接执行来源脚本或复制未知二进制。
- 不新增测试代码、证据文档或自动质量门禁；测试由用户负责。
- 每部分完成后使用中文提交信息提交并推送，然后继续下一部分。

## 5. 文档地图

- [共享架构](docs/02-shared-architecture.md)
- [Bootstrap Control Plane](docs/03-bootstrap-control-plane.md)
- [sing-box Data Plane](docs/04-singbox-data-plane.md)
- [业务 API](docs/05-business-api.md)
- [UI 与资产](docs/06-ui-assets.md)
- [Android](docs/07-platform-android.md)
- [Apple 平台](docs/08-platform-apple.md)
- [Windows](docs/09-platform-windows.md)
- [Linux](docs/10-platform-linux.md)
- [规则与地理数据](docs/11-rules-geo-data.md)
- [测试与发布](docs/12-testing-release.md)，由用户维护
- [验收矩阵](docs/13-acceptance-matrix.md)，由用户维护
