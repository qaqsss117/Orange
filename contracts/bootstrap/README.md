# Orange Bootstrap 加密包

本目录定义 Orange Bootstrap 的版本 1 明文和非敏感 manifest。

## 明文边界

明文只允许包含：

- schema/configuration 版本和 Unix 到期时间；
- 1 至 8 个候选 outbound；
- 固定上限的故障切换参数；
- 启动 DNS；
- 业务 API host 白名单。

候选 outbound 当前可表达 Trojan、Hysteria2 和 Shadowsocks。所有对象拒绝未知字段；`userToken`、完整 URL、任意 sing-box JSON 和额外业务数据均无法通过反序列化。

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
