# `dytes self-update`

## 1. 职责

把当前 `dytes` 可执行文件升级到 S3 上的最新版本。只读取 `core/dytes` 清单，
下载对应平台二进制并用同名 `.sha256` 校验后替换自身。

CDN 来源已停用：`--source cdn` / `packages` 会直接返回参数错误，避免缓存导致
读到旧版本或下载到不存在的文件。`version.js` 不参与自更新。

## 2. 用法

```bash
dytes self-update [--check] [--force] [--source s3] [--json]
```

| 参数 | 说明 | 当前状态 |
| --- | --- | --- |
| `--check` | 只比较版本，不下载 | 已实现 |
| `--force` | 同版本也重新下载 | 已实现 |
| `--source s3` | 下载来源，默认 `s3`；`auto` 兼容为 `s3` | 已实现 |
| `--json` | JSON 输出 | 已实现 |

## 3. 执行流程

1. 解析 `--source`：只接受 `s3` / `auto`；`cdn` / `packages` 返回 `2`。
2. 读取当前版本与当前可执行文件路径。
3. 读取 S3 清单：
   - 先读未版本化 `core/dytes/dytes.json`；
   - 再尝试同版本的版本化清单，失败时回退。
4. 比较 semver：
   - `--check` 或“无更新且未 `--force`”：只输出结果；
   - 否则进入下载。
5. 下载到同目录临时文件 `.<name>.download.<pid>`。
6. 下载同名 `.sha256` 并校验。
7. 设置可执行位，替换当前二进制。
8. 输出结果（文本或 JSON）。

## 4. 输出

`--check` 文本：

```text
dytes 有新版本: v0.2.1 -> v0.2.2（windows）
运行 dytes self-update 升级
```

JSON：

```json
{
  "current": "0.2.1",
  "latest": "0.2.2",
  "platform": "windows",
  "updated": false,
  "has_update": true,
  "path": "C:\\...\\dytes.exe"
}
```

更新成功时追加 `source` / `download_path` 等字段。

## 5. 退出码

| 退出码 | 场景 |
| --- | --- |
| `0` | 已是最新、检查完成或更新成功 |
| `1` | 读取清单、下载、校验、替换失败 |
| `2` | `--source` 非法（如 `cdn`） |

## 6. 边界与安全

- 只替换当前可执行文件，不修改安装目录其它文件；
- 临时文件放在可执行文件同目录，确保替换是同一文件系统内的操作；
- 校验失败不得覆盖原文件；
- 不读取 CDN / `version.js`，避免缓存不一致；
- 平台二进制名从清单 `platform-exports` 推导。

## 7. 测试点

- 平台二进制名选择（windows / linux / macos）；
- S3 对象键拼接；
- `--source cdn` 返回 `2`；
- `--check` 不下载；
- 同版本 + `--force` 重新下载；
- checksum 不匹配时失败且保留原文件。

对应测试文件：`tests/cli_self_update.rs`；单元测试在
`src/update/self_update.rs`。

## 8. 后续规划

1. 下载失败重试与断点续传；
2. 替换前备份旧二进制，失败可回滚；
3. `--json` 输出补齐所有分支（下载路径、校验结果）；
4. 支持镜像源（需显式版本化清单，避免缓存）；
5. 自更新完成后提示重启宿主进程。
