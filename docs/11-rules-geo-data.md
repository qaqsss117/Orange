# 模块 11：规则集与 IP 地理数据

## 模块目标

为纯 sing-box Data Plane 提供版本兼容、可追溯、可离线启动和可签名更新的 `.srs` 规则集；Country/ASN MMDB 仅作为可选 UI/诊断数据，不能混入核心路由或沿用原工程未知数据库。

## 资源分类

| 资源 | 用途 | 默认策略 |
| --- | --- | --- |
| `geoip-cn.srs` | 中国大陆 IP 分流 | 分流模式需要 |
| `geosite-cn.srs` | 中国大陆域名分流 | 分流模式需要 |
| `geosite-geolocation-!cn.srs` | 非中国大陆域名分流 | 视策略需要 |
| private/reserved 规则 | 局域网、回环、保留地址处理 | 必须，可由配置生成 |
| `Country.mmdb` | UI 显示出口 IP 国家/地区 | 可选，不参与路由 |
| `ASN.mmdb` | UI/诊断显示 ASN | 可选，不参与路由 |

## GEO-G0-001：可信上游、许可证与生成链

**目标**：固定每项地理资源的来源和法律/技术兼容性。

**依赖**：`SEC-G0-004`、固定 sing-box 版本。

**交付物**：source registry、许可证记录、生成命令、版本兼容矩阵。

**验收规则**：

1. 不复制原工程 `geoip.metadb`、`geosite.dat`、`ASN.mmdb`。
2. 每个 `.srs` 记录源仓库、commit/tag、许可证、生成工具版本和 SHA-256。
3. 生成工具与固定 sing-box 版本完成 load smoke test，不以文件扩展名猜兼容。
4. 上游许可证允许目标分发方式；署名/notice 要求进入发布清单。
5. MMDB 上游在打包前确认再分发条款；无法确认时不进入包体。
6. 生成过程可在 CI/受控环境复现，不执行上游未知二进制。

**非目标**：不要求首发包含 UI 国家/ASN 显示。

**验收结果（2026-07-31）**：`SagerNet/sing-geoip` 与 `SagerNet/sing-geosite` 的源码及
`rule-set` 输出 commit、GPL-3.0-or-later 许可证、LICENSE/notice 哈希和 sing-box 1.13.14
生成器均已固定。仓库源码构建的严格工具从三个仅含 `.invalid`/文档地址的兼容 fixture
确定性生成 SRS v2，并由同一固定 sing-box 库真实 load；未知字段、未来版本、损坏输入、
覆盖输出和遗留数据进入仓库均被门禁拒绝。fixture 与 smoke 产物不作为正式 CN 数据打包，
`Country.mmdb`/`ASN.mmdb` 在再分发条款未确认前继续排除。六条规则已闭环，本切片为
`done`；证据见 `docs/evidence/GEO-G0-001-source-chain-2026-07-31.md`。

## GEO-G0-002：资源 Manifest 与路径沙箱

**目标**：Data Plane 只能读取应用管理的已登记规则。

**依赖**：`GEO-G0-001`、`VPN-G0-001`。

**交付物**：manifest schema、path resolver、hash/size/format validator。

**验收规则**：

1. manifest 包含 name、format、schema/sing-box version、hash、size、source、license、generated/expires time 和签名。
2. Data Plane 仅接受逻辑资源 ID，Rust 映射到应用私有目录；订阅不能提交绝对路径、`..` 或 `file://`。
3. 文件 size/hash/format 任一不符时拒绝加载并保留上一可用配置。
4. 符号链接、junction、case normalization 和路径穿越有跨平台测试。
5. 可执行权限从 SRS/MMDB 移除；普通用户不能替换 service 使用的共享资源。
6. manifest 与实际包资源多/少/重复均使 CI 失败。

**非目标**：不让 WebView 直接读取数据库文件。

**验收结果（2026-07-31）**：闭合的 v1 manifest schema 固定三项兼容 SRS 的逻辑 ID、
文件名、sing-box/SRS 版本、大小、SHA-256、来源 commit、许可证、生成/到期时间和签名状态。
Rust resolver 只从 canonicalized 应用私有根解析逻辑 ID，拒绝路径式 ID、未知字段、大小写
歧义、穿越、符号链接/junction/reparse、可执行文件及 hash/size/格式漂移；候选全部验证后才
替换活动 manifest，激活后文件被修改时 resolve 再次拒绝且不改写活动状态。订阅 schema
显式拒绝 `rule_set`/路径字段，Windows installer 创建仅 SYSTEM、Administrators 和 service
SID 可写的共享规则目录。固定 smoke 实际生成三项 SRS 并以同一校验器证明包资源不多、
不少、不重复。兼容 fixture 仍不是正式 CN 数据，MMDB 继续排除。六条规则已闭环，本切片为
`done`；证据见 `docs/evidence/GEO-G0-002-resource-sandbox-2026-07-31.md`。

## GEO-P0-003：最小离线规则集打包

**目标**：首次启动不依赖境外下载即可建立基础路由。

**依赖**：`GEO-G0-002`、平台打包空壳。

**交付物**：最小 `.srs`、安装复制/只读访问、Data Plane fixture。

**验收规则**：

1. Windows、Android、macOS、iOS、Linux 构建都包含同逻辑版本规则集。
2. 安装后离线能完成规则加载和配置校验，不要求 API/上游可达。
3. 规则能正确匹配已知 CN/非 CN/private fixture，结果跨平台一致。
4. 包体压缩/解压后 hash 与 manifest 一致；首次复制原子完成。
5. 规则过期只显示更新警告，不在无可替代版本时破坏已有 Data Plane，除非安全策略明确阻断。
6. 包体大小记录到基线，异常增长有 CI 阈值。

**非目标**：最小集不追求覆盖所有广告/应用分类。

## GEO-P1-004：签名更新、原子替换与回滚

**目标**：通过 Control Plane 安全更新规则资源。

**依赖**：`BOOT-P1-006`、`GEO-P0-003`、`ARC-P1-004`。

**交付物**：update command、签名/版本校验、下载限额、双槽位回滚。

**验收规则**：

1. 更新 manifest 和文件经 BootstrapTransport 获取，host 在更新 allowlist。
2. 下载前后限制 Content-Length/实际大小，校验签名、hash、schema、sing-box version、到期时间。
3. 新规则在候选 Data Plane 验证加载后才激活；失败删除候选并保留旧版。
4. 断网、磁盘满、进程被杀、签名错误、hash 错、版本回滚和服务端 5xx 有测试。
5. 不允许远程更新 EXE/helper/core；资源更新目录无执行权限。
6. 更新历史保留有限版本和脱敏结果，不无限增长。

**非目标**：不从订阅任意 URL下载规则。

## GEO-P2-005：Country/ASN UI 数据

**目标**：在需要时为出口/节点提供国家和 ASN 展示。

**依赖**：`GEO-G0-001`、`UI-P1-007`。

**交付物**：可再分发 MMDB、Rust lookup、UI DTO、更新策略。

**验收规则**：

1. MMDB 许可证允许随五平台包分发，版本和 notice 登记。
2. lookup 只接收单个 IP，不扫描用户流量或域名历史。
3. 私网、保留、无匹配、IPv4、IPv6 有明确显示和测试。
4. MMDB 缺失/损坏只关闭地理展示，不阻止 Data Plane 路由。
5. UI 只接收 country code/name/ASN DTO，不获得数据库路径。
6. 更新沿用签名、hash、原子替换和回滚门禁。

**非目标**：MMDB 不参与 sing-box route 决策。
