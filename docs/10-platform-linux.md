# 模块 10：Linux 平台

## 模块目标

在主流 systemd 桌面发行版通过用户 UI、受限 helper、loopback mixed 和可选 TUN 提供 VPN；分别适配 GNOME/KDE 系统代理并可靠恢复 DNS/路由。

## LNX-G0-001：Helper 权限与 IPC PoC

**目标**：确定最小权限 helper、systemd/polkit 和非 TCP IPC 边界。

**依赖**：`ARC-G0-001`、`BOOT-G0-003`、`SEC-G0-002`。

**交付物**：helper PoC、UDS/stdio IPC、polkit policy、威胁模型。

**验收规则**：

1. Control Plane direct-dial 通过 stdio 或 Unix Domain Socket 调用，不监听 TCP。
2. socket 文件位于受控 runtime 目录，权限仅当前用户/helper，其他用户连接失败。
3. helper 不接受 shell、任意路径、任意 URL、任意 capability 或完整 root 命令。
4. 只有 TUN/route/DNS 操作经过 polkit/root；纯用户 mixed 模式不要求 root。
5. helper 无 Home 目录遍历权限或代码路径，SELinux/AppArmor 差异有记录。
6. Ubuntu LTS 和 Fedora 当前版完成 smoke PoC。

**非目标**：不支持无 systemd 的所有发行版。

## LNX-P0-002：Mixed Inbound 与桌面系统代理

**目标**：在 GNOME/KDE 提供用户级系统代理。

**依赖**：`LNX-G0-001`、`VPN-P1-005`。

**交付物**：GNOME/KDE adapter、desktop detection、snapshot/restore、tray 状态。

**验收规则**：

1. mixed 只监听 loopback，启动成功后才修改桌面代理。
2. GNOME 使用受支持设置接口，KDE 使用已记录接口；禁止拼接 shell 执行用户输入。
3. 原代理快照、所有权比较和恢复覆盖正常停止、crash、logout/login、升级、卸载。
4. 未识别桌面环境时不声称设置成功，提供手动代理地址和明确限制。
5. 环境变量代理只影响 Orange 启动的受控子进程，不全局修改 shell profile。
6. Wayland/X11 下托盘或替代状态入口有测试。

**非目标**：P0 不保证所有轻量桌面环境自动配置。

## LNX-P1-003：TUN、DNS 与路由恢复

**目标**：通过最小权限 helper 提供 Linux TUN 模式。

**依赖**：`LNX-G0-001`、`VPN-P1-006`。

**交付物**：TUN adapter、systemd unit、DNS backend adapter、route cleanup。

**验收规则**：

1. 使用 `/dev/net/tun` 和最小 `CAP_NET_ADMIN`/polkit，UI 进程不以 root 运行。
2. 支持的 NetworkManager/systemd-resolved 组合有明确检测和配置路径。
3. 连接、停止、helper kill、系统重启、网络切换和卸载后 DNS/route 恢复。
4. Control Plane socket/connection 不被 Data Plane 捕获形成环路。
5. IPv4 必测；IPv6 enable/disable 与路由泄漏测试通过。
6. 无支持 DNS backend 时拒绝 TUN 或明确降级，不覆盖 `/etc/resolv.conf` 未受控内容。

**非目标**：不直接支持路由器/无桌面服务器发行版。

## LNX-P1-004：发行版打包与生命周期

**目标**：交付可安装、升级和卸载的 Linux 包。

**依赖**：Linux `P0/P1`、`REL-P1-005`。

**交付物**：deb/rpm，AppImage 是否支持的决策，systemd/polkit 安装脚本。

**验收规则**：

1. Ubuntu LTS deb 和 Fedora rpm 可安装、启动、升级、卸载；目标架构明确。
2. pre/post install/uninstall 脚本幂等，不执行远程代码，不删除包管理范围外文件。
3. 升级正确停止/迁移/restart helper；失败不混用新旧 binary。
4. 卸载停止服务并恢复代理/DNS/route，systemd unit/polkit policy 无残留。
5. helper/binary/SRS/MMDB 的权限、owner、hash、许可证与 manifest 一致。
6. AppImage 若无法安全安装 helper/TUN，则只支持明确的用户代理模式或不发布。

**非目标**：P1 不覆盖所有发行版包管理器。

## LNX-P2-005：更多桌面与无特权模式

**目标**：扩展非 GNOME/KDE 和不安装 helper 的使用方式。

**依赖**：`LNX-P1-004`。

**交付物**：手动代理指南、额外桌面 adapter、portable mode 决策。

**验收规则**：

1. 每个新增桌面环境有设置/读取/恢复自动测试或可重复人工脚本。
2. 无特权模式明确只提供 loopback 代理，不暗示 TUN 可用。
3. portable mode 不把 secret 明文写在可移动目录。
4. 未支持环境显示能力差异，不执行猜测性系统修改。

**非目标**：不牺牲 G0 权限边界追求覆盖率。
