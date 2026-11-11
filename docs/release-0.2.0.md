# 0.2.0

## self-update 只走 S3

- `dytes self-update` 不再经过 `packages.funxdata.com` 的 CDN：清单、二进制与
  `.sha256` 全部直接读取 S3，避免 CDN 缓存导致更新到旧版本或下载到不存在的文件。
- 不再读取 `latest/version.js`，最新版本只以 `core/dytes/dytes.json` 为准。
- `--source` 默认 `s3`；`auto` 兼容为 S3，`cdn` 会明确报错。

## 安装后自更新

- 安装流程完成后，使用安装目录里的 dytes 执行一次 `self-update`，保证安装后的
  CLI 是最新版本；自更新失败只写入 `install.log`，不影响安装结果。

## 下载文件名与平台后缀

- app / design_soft 更新时校验平台导出文件后缀，平台专用清单缺少
  `.exe` / `.bin` / `.app` 时直接报错，避免拼出不存在的下载路径。
- 发布 workflow 的 `version.js` 与官网 dispatch 改为从 `dytes.json` 的
  `platform-exports` 生成下载链接，不再手写平台文件名。

## 其他

- `self-update` 下载地址统一为 `core/dytes/<version>/<platform>/<导出名>`。
