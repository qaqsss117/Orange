# Orange Bootstrap 加密包

本目录定义 Orange Bootstrap 的版本 2 明文和非敏感 manifest。

`bootstrap-remote-manifest.schema.json` 定义 OSS 上的 Ed25519 签名清单，
`txt-locator.schema.json` 定义 Cloudflare TXT 发现文档。DNS 可将编码后的
TXT 记录拆成多个字符串，客户端会在验签前按顺序合并。

## 明文边界

明文只允许包含：

- schema/configuration 版本和 Unix 到期时间；
- 1 至 8 个候选 outbound；
- 固定上限的故障切换参数；
- 启动 DNS；
- 业务 API host 白名单。

候选 outbound 当前可表达 Trojan、Hysteria2、Shadowsocks 和 VLESS Reality。所有对象拒绝未知字段；`userToken`、完整 URL、任意 sing-box JSON 和额外业务数据均无法通过反序列化。

## 运行时选择

客户端启动后按固定顺序尝试：包内 OSS manifest 地址、签名 TXT 发现的
OSS manifest 地址、设备加密的 last-known-good 缓存、安装包内置资源。远程
发现共用 4 秒预算；所有候选都失败时业务层返回“服务连接不可用”，不会直连
API。远程资源只有在 Ed25519 验签、版本/渠道/到期/兼容性校验、解密以及代理
API 健康检查全部成功后才会写入缓存。

远程 manifest 和 TXT locator 仅接受 HTTPS 443。客户端禁用重定向，通过内置
DoH resolver 获取 A/AAAA/TXT，并将 HTTPS 连接绑定到解析出的公网地址。TXT
locator 最多携带 4 个 manifest 地址。信任集合必须包含 2 至 4 把互不重复的
Ed25519 公钥，以支持当前/下一密钥轮换；发布签名私钥必须与其中指定 key ID
对应，且只能由 CI Secret 注入。

缓存保存签名后的 manifest 与密文，不保存解密后的代理或 API 配置。桌面使用
系统凭据存储生成缓存密钥，Android 使用 Keystore；两端均保留 current 和
previous 槽位，并严格拒绝过期、回滚或同版本不同内容的缓存。

## 信封格式

| 偏移 | 长度 | 内容 |
| ---: | ---: | --- |
| 0 | 8 | ASCII magic `ORNGBTP1` |
| 8 | 2 | 大端 envelope version，当前为 `1` |
| 10 | 1 | algorithm ID，XChaCha20-Poly1305 为 `1` |
| 11 | 24 | 每次构建由系统安全随机源生成的 nonce |
| 35 | 可变 | ciphertext，末尾包含 16 字节 Poly1305 tag |

认证附加数据依次绑定 envelope version、bootstrap schema、算法、渠道、产品版本、配置版本、到期时间和 key ID。修改这些 manifest 字段会导致认证失败。

## 构建密钥

`orange-bootstrap-crypto` 只从 `ORANGE_BOOTSTRAP_BUILD_KEY_HEX` 读取 64 位十六进制字符串，对应 32 字节 key。CLI 不提供 key 参数，不打印 key、明文、节点或 credential；key、stdin 明文和序列化中间缓冲使用 zeroize 清理。

生产 CI 还需注入：

- `ORANGE_BOOTSTRAP_CONFIG_JSON`：符合 schema 的生产明文；
- `ORANGE_BOOTSTRAP_CHANNEL`：渠道标识；
- `ORANGE_BOOTSTRAP_PRODUCT_VERSION`：目标应用版本；
- `ORANGE_BOOTSTRAP_KEY_ID`：非敏感轮换标识。

## CI 入口

```powershell
python scripts/ci/build_bootstrap_resource.py
```

该命令生成安装包内置的 `bootstrap.enc`/`bootstrap.manifest.json`、与包内 OSS
地址对应的 `bootstrap.remote.manifest.hardcoded.N.json`、与 TXT rescue OSS
地址对应的 `bootstrap.remote.manifest.rescue.N.json`，以及供 Cloudflare DNS 发布的
`bootstrap.txt`。Android 自更新使用独立的
`android-update-manifest.schema.json`，由 `build_android_update_manifest.py`
对实际签名 APK 的包名、versionCode、证书摘要、大小和 SHA-256 校验后生成；
同一脚本还会生成供 Android 更新发现使用的 `android-update.txt`。
APK 镜像同样只接受 HTTPS 443，并通过内置 DoH 解析后绑定公网 IP 流式下载；
下载完成后 Rust 与 Android 安装插件分别校验 SHA-256，安装插件还会复核包名、
递增 versionCode 和签名证书摘要，再交给系统安装器。
