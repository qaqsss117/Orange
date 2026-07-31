# GEO-G0-002 资源 Manifest 与路径沙箱验收（2026-07-31）

## 验收范围

本次验收规则资源的闭合 manifest、逻辑 ID 到应用管理目录的解析边界、文件完整性、
共享目录权限和包内容精确性。三项 SRS 继续是仅含 `.invalid`/文档地址的兼容性产物，
不作为正式 CN 规则分发；`Country.mmdb` 与 `ASN.mmdb` 在再分发条款确认前继续排除。

## Manifest 契约

`contracts/rules/rule-resource-manifest.schema.v1.json` 使用闭合 Draft 2020-12 schema；
`rules/resource-manifest.compat.json` 登记三个逻辑资源 ID。每项资源包含固定文件名、格式及
版本、sing-box 1.13.14、SHA-256、字节数、源仓库与源/输出 commit、许可证、生成/到期时间
和签名状态。兼容 manifest 只能声明 `unsigned-compatibility-fixture`，不能冒充发布签名。

schema 与 manifest 已登记到 `rules/source-registry.json`。静态门禁逐字段对照 registry，
拒绝未知/缺失字段、非法或路径式 ID、绝对路径、`file://`、`..`、重复 ID/文件名及大小写
歧义。包级校验要求实际目录和 manifest 文件集合完全相同，多文件、少文件、重复/大小写
冲突、子目录和未登记文件都失败。

## 运行时边界

`orange-platform::RuleResourceStore` 在打开时要求绝对的真实目录并 canonicalize；资源只能由
闭合的 `RuleResourceId` 查找。resolver 对直接子文件执行 `symlink_metadata`、canonical parent、
Windows reparse attribute、size、SHA-256、SRS v2 magic/MMDB metadata 和 Unix 执行位检查。
共享目录还必须通过平台权限 verifier，Unix 拒绝 group/world write。

候选 manifest 的全部资源验证成功后才替换活动状态。每次 resolve 都重新验证根目录和文件，
因此激活后的替换或篡改会 fail closed，同时保留上一活动 manifest。订阅净化 schema 没有
资源路径能力，并以回归测试显式拒绝顶层/规则内 `rule_set`、绝对路径、`file://` 和穿越值。

Windows installer 创建固定 `data-plane/rules`，复用受保护 runtime SDDL：只有 SYSTEM、
Administrators 和 Orange service SID 可写，普通用户没有替换共享规则的 ACE。

## 测试结果

以下定向检查通过：

```powershell
cargo test -p orange-platform rule_resources -- --nocapture
cargo test -p orange-platform subscription_cannot_supply_inbounds_dns_logs_services_or_paths -- --nocapture
python -m unittest scripts.security.tests.test_rule_resources -v
pnpm rules:check
pnpm run supply-chain:check
pnpm run sbom
```

固定 Go 1.25.5 smoke 从仓库源码生成三项 SRS 两次，字节完全一致，并由 sing-box 1.13.14
真实 load。随后同一 `validate_bundle` 校验实际临时包，报告 `manifest_exact: true`：

| 兼容性产物 | 大小 | SHA-256 | 格式 |
| --- | ---: | --- | --- |
| `geoip-cn.srs` | 45 bytes | `37b8d497215bc2d70b6e9c2f17b1105521a6364946d3a2416a8fcbbb3997b007` | SRS v2 |
| `geosite-cn.srs` | 55 bytes | `600162f955488b0c6233ce996211c02c6e7308358ccb49edc74a6e282377b9ce` | SRS v2 |
| `geosite-geolocation-not-cn.srs` | 58 bytes | `d0880437ccf781d74fe119eebadecd50315d9de4945a5dc2b6b3142ea5254f89` | SRS v2 |

Python 负例覆盖包多/少/重复、路径/URL/绝对路径、大小写冲突、size/hash/format、链接/执行位、
reparse 静态标记、Windows ACL 标记和状态不得重新打开。Unix 专项 Rust 测试覆盖真实符号链接、
执行位及共享目录写权限；Windows 运行覆盖统一 fail-closed reparse 分支与 installer SDDL 契约。

完整验收还通过 Python 全量 discovery、Rust workspace fmt/clippy/test/build、双 Go module、
供应链、SBOM、Windows Data Plane 及固定 Go 1.25.5 下的顶层 35/35 quality。生成的 JSON 与
SRS 均留在忽略的 `artifacts/`，不提交兼容性二进制，也不读取本地生产参数文件。

## 结论

六条验收规则均已闭环。Data Plane 配置入口不能接收订阅路径，规则资源只能经应用管理的
逻辑 ID 和完整性校验解析；候选失败与激活后篡改均保留上一活动状态；共享目录和包内容由
独立门禁锁定。`GEO-G0-002` 转为 `done`。
