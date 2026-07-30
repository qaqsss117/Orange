# GEO-G0-001 规则源与生成链验收（2026-07-31）

## 验收范围

本次只验收 `.srs` 规则的可信来源、许可证、源码生成链和固定 sing-box 版本兼容性，
不把测试 fixture 或生成产物作为正式 CN 数据打包。`Country.mmdb` 和 `ASN.mmdb`
在上游与再分发条款获批前继续明确排除。

## 固定来源

| 资源 | 源仓库 commit | `rule-set` 输出 commit | 许可证 |
| --- | --- | --- | --- |
| `geoip-cn` | `SagerNet/sing-geoip@ecd02c178af5efbac38d427a8d178f940327de1f` | `5605651c12ed5b2fcf3b5de580c041eb9d8d938e` | `GPL-3.0-or-later` |
| `geosite-cn` | `SagerNet/sing-geosite@dd64ae0ebf2ee69166c0510042b3e96c085f27df` | `65e61fa36378107abe637fc2c5217d8e2c4dc994` | `GPL-3.0-or-later` |
| `geosite-geolocation-not-cn` | `SagerNet/sing-geosite@dd64ae0ebf2ee69166c0510042b3e96c085f27df` | `65e61fa36378107abe637fc2c5217d8e2c4dc994` | `GPL-3.0-or-later` |

两个上游仓库的 LICENSE blob SHA-256 均为
`2f02b7486bcfa90d115c71a20437f3906b6fd5bef81c5dc0efd341399e89d0fd`；
登记的发布 notice 为 `docs/licenses/rules/SagerNet-GPL-3.0-or-later.txt`，SHA-256 为
`8c7f15b324704ebc1e2b4f35eebeac5dba7516f549a27a67ac5562a584e28204`。
正式规则进入包体时必须继续携带该许可证/notice 义务，不能改标为专有数据。

官方 GitHub 在本次网络环境不可达，因此只读审计通过代理获取固定 commit，并在忽略的
`artifacts/upstream-audit/` 中核对仓库、分支、commit 与许可证；构建、测试和质量门禁
均不依赖该代理或在线上游，也不执行上游二进制。

## 源码生成与兼容性

`native/dataplane/cmd/orange-rule-set` 从当前锁定的
`github.com/sagernet/sing-box v1.13.14` 源码构建，并直接调用该版本的 `srs.Write` 和
`srs.Read`。命令面仅允许固定 `compile`/`inspect` 参数，输入必须是严格的 version 2
JSON，输出使用临时文件、同步、移除执行权限后原子提交，且拒绝覆盖既有文件。
命令只在成功时向 stdout 输出闭合元数据；失败仅返回非零退出码，不建立可能接触敏感
路径或内容的 stderr/日志 sink。

三个 `rules/fixtures/*.compat.json` 仅使用 RFC 2606 `.invalid` 域名和 RFC 5737/RFC 3849
文档地址，目的是验证编码兼容性与确定性；它们不是上游规则内容，也不会作为生产数据
进入包体。两次从相同输入生成的字节必须一致，损坏 SRS、未来版本、未知字段、空规则、
源文件覆盖和既有输出覆盖均由 Go 测试拒绝。

## 可重复结果

以下命令通过：

```powershell
cd native/dataplane
go test -tags with_quic,with_utls ./...
cd ../..
pnpm rules:check
python -m unittest scripts.security.tests.test_geo_sources -v
pnpm run supply-chain:check
pnpm run sbom
```

固定 smoke 结果：

| 兼容性产物 | 大小 | SHA-256 | load |
| --- | ---: | --- | --- |
| `geoip-cn.srs` | 45 bytes | `37b8d497215bc2d70b6e9c2f17b1105521a6364946d3a2416a8fcbbb3997b007` | sing-box SRS v2 / 1 rule |
| `geosite-cn.srs` | 55 bytes | `600162f955488b0c6233ce996211c02c6e7308358ccb49edc74a6e282377b9ce` | sing-box SRS v2 / 1 rule |
| `geosite-geolocation-not-cn.srs` | 58 bytes | `d0880437ccf781d74fe119eebadecd50315d9de4945a5dc2b6b3142ea5254f89` | sing-box SRS v2 / 1 rule |

去敏 smoke 报告为 `artifacts/rules/geo-g0-001-smoke.json`，1,355 bytes，SHA-256 为
`a7795c1baad4e831a2519a68376a389d3b78e1f3e1f2ef7fd78bf2705c96710f`。
报告和 `.srs` 兼容性产物均留在忽略目录，不提交二进制规则数据。

完整安全测试共 219 项通过；供应链检查覆盖 847 项依赖、7 个生态和 76 个配置 URL，
SBOM 检查覆盖 822 个组件与 59 个资源。固定 Go 1.25.5 工具链下的 Windows 顶层
`python scripts/ci/run.py quality` 35/35 通过。

## 结论

六条验收规则均已闭环：遗留数据被扫描拒绝，三项 `.srs` 的来源/commit/许可证/生成器/
哈希已固定，sing-box 1.13.14 完成真实 SRS load，许可证义务已登记，MMDB 保持排除，
生成过程只执行仓库源码且可离线复现。`GEO-G0-001` 转为 `done`。
