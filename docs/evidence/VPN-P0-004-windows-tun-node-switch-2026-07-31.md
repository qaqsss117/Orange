# VPN-P0-004 Windows 安装态 TUN 节点切换验收（2026-07-31）

## 结论

Windows 10 22H2 安装态生产链路通过。未签名候选包经安装后的 `orange-app.exe` 使用受限
Named Pipe 完成生产登录、订阅下载与净化、TUN revision 激活、18 节点有界测速、可用
非默认节点选择、sing-box core 回读、切换前后真实 HTTPS 流量，以及切换后的账户和订阅
Control Plane 请求。应用释放抓包检查点后正常停止 Data Plane、清空三项生产凭据并卸载；
service、安装目录、TUN、DNS、路由、代理、防火墙、进程和抓包会话均无残留。

本证据关闭 `VPN-P0-004` 的 Windows 安装态 TUN 节点切换抓包缺口，但不替代
Linux/macOS/iOS backend 和运行证据，因此切片仍为 `in_progress`。`WIN-P1-004` 也仍需
真实系统重启、睡眠/唤醒、网卡切换、VPN 冲突和 mixed 回退等规则内结果。

## 受测产物与环境

| 项目 | 结果 |
| --- | --- |
| OS | Windows 10 专业版 22H2，10.0.19045，64-bit |
| 源 revision | `a15e4766bc96b9293875dbe93359dc6b34247702` 加本批未提交验收改动 |
| 候选安装包 SHA-256 | `fe8c3cf51c8b172571408c1eeec81d8cbd83fbed8282f5b689119815e3a7f6c4` |
| Rust | `rustc 1.95.0` / `cargo 1.95.0` |
| Go | `go1.25.5 windows/amd64` |
| 签名/发布边界 | `unsigned-test`，`release_allowed=false` |

生产账号、bootstrap key/config、API host、节点 ID、节点地址、凭据和响应正文均只从忽略的
本地环境进入进程，没有写入报告或版本控制。

## 验收路径

1. `node-switch` 阶段要求管理员和显式 `-AllowSystemChanges`，先确认系统干净，再安装与
   `phase-build.json` SHA-256 一致的候选包。
2. 验收入口只在 `unsigned-test-runtime` 特性下编译，且必须同时满足精确命令行参数和
   `ORANGE_E2E_ACCEPTANCE_ENABLED=1`；正式构建不包含入口，也没有新增 Tauri command。
3. 入口位于已安装 `orange-app.exe` 内，因此服务端继续执行安装用户、medium integrity、
   PID 和固定映像复核；普通 `cargo test` 进程不能绕过 IPC 映像门禁。
4. 应用经嵌入生产 bootstrap 启动既有 Control Plane，完成登录、订阅元数据刷新、原生
   订阅正文下载、VLESS 净化和 TUN revision 激活。
5. 应用在 TUN online 后写去敏检查点并暂停；外层此时才启动 `pktmon`，避免 Windows 10
   遗漏动态创建的 Wintun 适配器。脚本用活动 `orange-tun` 的 InterfaceIndex 对齐
   `pktmon list --json` 中 `DriverName=wintun.sys` 的组件 ID，不依赖显示名称。
6. 抓包启动后应用执行 8 路有界测速，选择可用非默认节点并从 core 回读相同选择；切换
   前后分别发出不经系统代理的 HTTPS 请求并要求上下行计数都增长。
7. 节点切换后，账户与订阅请求继续经同一个 Control Plane 成功。应用保持 TUN 在线，
   直到外层读取 Wintun 组件计数后收到释放信号。
8. 成功和失败路径都会停止 Data Plane、清凭据、停止抓包并静默卸载；最终再次执行完整
   `Assert-Clean`。

## 去敏结果

| 断言 | 结果 |
| --- | --- |
| 生产节点总数 | 18 |
| 可用节点数 | 16 |
| 非默认节点选择 | 通过 |
| sing-box core 回读 | 通过 |
| 切换前 TUN HTTPS 与流量增长 | 通过 |
| 切换后 TUN HTTPS 与流量增长 | 通过 |
| 切换后账户请求 | 通过 |
| 切换后订阅请求 | 通过 |
| Wintun 组件包观测数 | 2,235 |
| Wintun 组件字节观测数 | 2,972,411 |
| Data Plane 停止 | 通过 |
| 三项凭据清空 | 通过 |
| 安装与系统网络状态清理 | 通过 |

## 抓包证据

原始抓包只保存在 git 忽略目录，可能包含网络地址，不进入仓库。版本控制内只登记以下
路径、大小和哈希：

| 产物 | 大小 | SHA-256 |
| --- | ---: | --- |
| `artifacts/acceptance/windows-development/node-switch-20260730T195803588Z-11064/node-switch.etl` | 184,188 bytes | `25c99cb836595e6b76559722fda15166600aa2a4d721101ee65a3add35b3053b` |
| `artifacts/acceptance/windows-development/node-switch-20260730T195803588Z-11064/node-switch.pcapng` | 50,872 bytes | `e90b7b8501252765b6e806d4ec7b4408420e2672b4b0b889faad852cbe2840fb` |
| 应用最终去敏结果 | 不登记原文件 | `eb8f7484c237afcb56294820fbdd1e71352e899e3493a2518132f8e65e6ab431` |

PCAPNG 由 ETL 按运行时映射得到的 Wintun 组件 ID 过滤生成。仓库内自动化要求组件包数和
字节数均大于零，并阻止删除双阶段握手、core 回读、切换后 Control Plane 请求、失败清理
或 `release_allowed=false`。

## 自动化结果

- `cargo check -p orange-app --features unsigned-test-runtime`：通过。
- `cargo test -p orange-app --features unsigned-test-runtime --lib`：29/29 通过。
- `python -m unittest scripts.security.tests.test_windows_development_acceptance`：15/15 通过。
- `python scripts/security/check_data_plane_nodes.py`：通过。
- `windows-development.ps1 -Phase node-switch ... -AllowSystemChanges`：通过。
- 固定 Go 1.25.5 工具链下 `python scripts/ci/run.py quality`：35/35 通过，其中 Python
  安全/变异测试 203/203 通过。
