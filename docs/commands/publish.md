# `dytes publish`

## 1. 职责

把当前包的产物与 `dytes.json` 上传到 S3，并更新对应类型的 `packages.json` 索引。
发布遵循“先产物、后清单、最后索引”：任一文件上传失败都不更新索引，避免线上
出现不可下载的版本。

## 2. 用法

```bash
dytes publish [--dir <path>] [--config <path>] [--dry-run]
              [--platform <platform>] [--manifest-only] [--skip-check] [--force] [--json]
```

| 参数 | 说明 | 当前状态 |
| --- | --- | --- |
| `--dir <path>` | 包目录，默认当前目录 | 已实现 |
| `--config <path>` | 清单路径，默认 `<dir>/dytes.json` | 已实现 |
| `--dry-run` | 只打印上传计划，不联网；导出缺失返回 `3` | 已实现 |
| `--platform <platform>` | 预览时选择平台导出 | 仅影响计划与校验 |
| `--manifest-only` | 只发布 `dytes.json`，不上传产物 | 已实现 |
| `--skip-check` | 跳过设计插件完整校验（不建议） | 已实现 |
| `--force` | 允许覆盖已有对象；索引不可读时允许新建 | 已实现 |
| `--json` | JSON 输出发布计划 | 已实现（计划阶段） |

> 注意：实际发布流程读取当前平台 `DYTES_PLATFORM` / 系统平台，`--platform`
> 目前只影响 CLI 计划展示。跨平台发布应在对应平台执行。

## 3. 执行流程

1. 读取清单，解析类型；设计插件先执行完整清单校验（`--skip-check` 可跳过）。
2. 选择导出表：
   - `--manifest-only`：不上传产物；
   - 否则使用当前平台的 `platform-exports`，未配置时回退 `exports`。
3. 生成上传计划：
   - 源文件、远程路径、是否存在、SHA-256 摘要；
   - 拒绝绝对路径、盘符与包含 `..` 的导出目标。
4. `--dry-run`：打印计划；有缺失文件时返回 `3`。
5. 正式发布：
   - 确保 `uuid`：缺失时生成并写回 `dytes.json`；
   - 上传全部产物；
   - 上传版本化清单与 `dytes.json`；
   - 合并 `checksums`（按平台前缀）；
   - 最后更新 `<type>/packages.json`。

## 4. 远程布局

```text
<prefix>/<name>/dytes.json
<prefix>/<name>/<version>/[<platform>/]<file>
<prefix>/<name>/<version>/[<platform>/]dytes.json
<type>/packages.json
```

`<prefix>` 由包类型决定：`app/`、`core/`、`system/`、`webs/`、
`design_plugins/`、`design_soft/`、`models/`。

## 5. 输出

文本计划：

```text
包: info_lib 0.1.2 (sys)
UUID: 8b2cd615-...
平台: windows
索引: system/packages.json
  ./target/release/info_lib.dll -> system/info_lib/0.1.2/windows/info_lib.dll
    sha256=... sampled=false
```

`--json`：

```json
{
  "dry_run": true,
  "manifest_only": false,
  "type": "sys",
  "name": "info_lib",
  "uuid": "",
  "version": "0.1.2",
  "platform": "windows",
  "index": "system/packages.json",
  "files": [
    {
      "source": "./target/release/info_lib.dll",
      "remote": "system/info_lib/0.1.2/windows/info_lib.dll",
      "exists": true,
      "checksum": { "sha256": "...", "size": 1234, "sampled": false }
    }
  ]
}
```

## 6. 退出码

| 退出码 | 场景 |
| --- | --- |
| `0` | 发布成功或 `--dry-run` 通过 |
| `1` | 上传、网络、S3、索引更新失败 |
| `2` | 参数错误 |
| `3` | 清单、导出、设计插件校验失败；`--dry-run` 发现文件缺失 |

## 7. 边界与约束

- 索引更新必须是最后一步；
- 缺少 `uuid` 时写回清单，保证后续更新可识别；
- 导出目标名不允许目录分隔符或 `..`（设计插件要求纯文件名）；
- `--force` 才允许覆盖已有对象；
- 凭证只从配置 / 环境读取，日志脱敏；
- S3 只可写不可读时的索引处理见 [../issues.md](../issues.md) I-01。

## 8. 测试点

- `--dry-run` 不联网、输出计划、缺失文件返回 `3`；
- `--manifest-only` 不上传产物；
- 缺少 `uuid` 时生成并写回；
- 导出路径含 `..` / 绝对路径被拒绝；
- 设计插件校验失败阻止发布；
- 任一产物失败不更新索引；
- `checksums` 合并保留其它平台记录。

对应测试文件：`tests/cli_publish.rs`；单元测试在 `src/publish/dytes.rs`、
`src/publish/loader.rs`。

## 9. 后续规划

1. `--platform` 贯穿正式发布流程，支持一次发布多平台；
2. `--dry-run` 输出与正式发布使用同一计划对象，避免两套逻辑；
3. 上传重试与并发控制；
4. 发布完成后输出可回滚的版本摘要；
5. `--json` 覆盖正式发布结果，而不仅是计划。
