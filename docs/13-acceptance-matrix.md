# 全局验收矩阵

## 1. 使用方式

模块切片验收证明局部完成，本矩阵证明系统整体可发布。每个目标平台必须分别记录 OS/版本、设备/架构、构建号、执行人、日期、结果和证据路径。

严重级别：

- `Blocker`：安全红线、隐私、裸连、路由/DNS/代理残留、无法连接/断开、签名问题；禁止发布。
- `Critical`：主流程不可完成、订单重复、状态严重错误；禁止公开测试。
- `Major`：非主流程或特定设备显著问题；需明确修复计划。
- `Minor`：不影响任务完成的视觉/文案问题。

## 2. 主流程矩阵

| 场景 ID | 场景 | 适用平台 | 通过规则 | 关联切片 |
| --- | --- | --- | --- | --- |
| E2E-001 | 未登录冷启动 | 全部 | 解密 bootstrap，Control Plane ready，无网络 listener，登录页可用 | `BOOT-G0-002`、`BOOT-G0-003`、`UI-P0-003` |
| E2E-002 | 境外 API 登录 | 全部 | 登录成功且抓包只经 bootstrap；token 不进入 WebView/日志 | `BOOT-P0-004`、`API-P0-002` |
| E2E-003 | Bootstrap 全失效 | 全部 | 显示可重试错误，无 API 裸连，无请求风暴 | `BOOT-P0-005` |
| E2E-004 | 拉取订阅并预启动 | 全部 | 净化、校验、健康检查后原子激活，失败保留旧版 | `VPN-G0-001`、`VPN-P0-003` |
| E2E-005 | 首次 VPN 授权/连接 | 移动/桌面 TUN | 权限明确、出口改变、状态真实 | `VPN-P0-002`、平台 P0 |
| E2E-006 | Mixed 系统代理 | 桌面 | 仅 loopback，设置成功，浏览器出站改变 | `VPN-P1-005`、桌面平台 P0 |
| E2E-007 | 节点切换与测速 | 全部 | core 回读选择、测速可超时/取消、API 不受影响 | `VPN-P0-004`、`VPN-P1-006` |
| E2E-008 | 停止与退出登录 | 全部 | Data Plane 停止、系统设置恢复、用户 secret 清除、可重新登录 | `API-P0-003`、`REL-P1-006` |
| E2E-009 | 套餐与订单 | 全部 | 金额正确、不重复下单、支付 host 校验、状态回读 | `API-P1-004`、`UI-P1-006` |
| E2E-010 | 工单生命周期 | 全部 | 创建/回复/关闭正确，无 HTML 执行/图片附件 | `API-P1-005` |

## 3. 双平面与网络异常

| 场景 ID | 注入故障 | 通过规则 |
| --- | --- | --- |
| NET-001 | Data Plane 启动中 kill | Control Plane 保持 ready；无系统代理/路由半设置 |
| NET-002 | Data Plane 在线 kill | 2 秒内状态失败；执行恢复或清理；API 仍可用 |
| NET-003 | Control Plane kill | Data Plane 按策略保持；API 明确失败且不 direct |
| NET-004 | Wi-Fi/移动/网卡切换 | 隧道恢复或明确失败，无 DNS 泄漏/环路 |
| NET-005 | DNS 不可用 | bootstrap 使用无环解析策略；失败不无限重试 |
| NET-006 | 新订阅坏配置 | 旧 Data Plane 保持，不修改 active version |
| NET-007 | 新规则签名/hash 错 | 拒绝候选，旧规则继续可用 |
| NET-008 | mixed 端口冲突 | 重新分配或安全失败，不覆盖其他进程 |
| NET-009 | 系统睡眠/锁屏 | 平台行为符合文档，恢复后状态真实 |
| NET-010 | 系统重启 | 自动连接仅按用户设置；代理/DNS/路由无陈旧残留 |

## 4. 系统代理与卸载

| 场景 ID | Windows | macOS | Linux | 通过规则 |
| --- | --- | --- | --- | --- |
| SYS-001 正常停止 | WinINET 恢复 | SystemConfiguration 恢复 | GNOME/KDE 恢复 | 与启动前快照一致 |
| SYS-002 UI crash | service/recovery 接管 | helper/extension 状态真实 | helper 状态真实 | 不留下 UI 假状态 |
| SYS-003 core/helper crash | 修复或恢复 | 修复或恢复 | 修复或恢复 | 用户不会长期断网 |
| SYS-004 用户运行期手改代理 | 不覆盖新值 | 不覆盖新值 | 不覆盖新值 | 所有权比较生效 |
| SYS-005 升级失败 | 回滚 EXE/service | 回滚 app/extension | 回滚 binary/helper | 不混用版本 |
| SYS-006 卸载 | service/代理/TUN 清理 | extension/代理清理 | systemd/polkit/路由清理 | 无 Orange 活跃组件 |

## 5. 隐私与安全

| 场景 ID | 方法 | 通过规则 | 级别 |
| --- | --- | --- | --- |
| SEC-E2E-001 | 检查权限清单 | 无照片、OCR、相机、麦克风等禁用权限 | Blocker |
| SEC-E2E-002 | 图片诱饵 + 文件审计 | 全流程无打开/读取/上传 | Blocker |
| SEC-E2E-003 | 控制面抓包 | host 全在 allowlist，无图片/助记词/secret | Blocker |
| SEC-E2E-004 | 阻断 bootstrap | API 不裸连 | Blocker |
| SEC-E2E-005 | 端口扫描 | Control Plane 无网络 listener | Blocker |
| SEC-E2E-006 | strings/SBOM | 无 Clash/mihomo/OCR/未登记二进制 | Blocker |
| SEC-E2E-007 | 日志/crash dump | 无 token、bootstrap、订阅、支付敏感数据 | Blocker |
| SEC-E2E-008 | IPC fuzz/权限 | 未授权调用、任意路径/URL/shell 被拒绝 | Blocker |

## 6. 平台最低矩阵

| 平台 | 最低系统/设备 |
| --- | --- |
| Android | API 24、29、33、35；arm64 真机；声明支持时增加 armeabi-v7a |
| iOS/iPadOS | 当前主版本及前一主版本；至少一台 iPhone、一台 iPad |
| macOS | 当前及前一主版本；Apple Silicon；声明支持时增加 Intel |
| Windows | Windows 10 22H2、Windows 11 当前版 |
| Linux | Ubuntu LTS、Fedora 当前版；GNOME/Wayland，KDE 至少一套 |

## 7. 发布证据包

每个平台发布候选必须生成：

1. 构建版本、commit、工具链、sing-box version。
2. 签名验证和 SHA-256。
3. 权限/entitlement/capabilities 快照。
4. SBOM、资源 manifest、许可证/Notices。
5. 主流程、异常恢复、隐私专项结果。
6. 端口扫描与控制面抓包摘要。
7. 安装、升级、卸载与系统设置恢复报告。
8. 已知问题及严重级别；不得含未关闭 Blocker/Critical。
