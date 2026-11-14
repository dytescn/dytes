# `dytes config`

## 1. 职责

管理用户配置与 S3 凭证。配置项包括 S3 端点、桶、区域、访问密钥、平台；
敏感值默认脱敏显示，只有 `get --reveal` 才输出明文。

## 2. 用法

```bash
dytes config list [--json]
dytes config get <key> [--reveal] [--json]
dytes config set <key> <value> [--json]
dytes config unset <key>
dytes config path [--json]
```

| 子命令 | 说明 | 当前状态 |
| --- | --- | --- |
| `list` | 列出解析后的全部配置 | 已实现 |
| `get` | 读取单个配置项 | 已实现 |
| `set` | 设置配置项；敏感值可交互输入 | 已实现 |
| `unset` | 删除配置项 | 已实现 |
| `path` | 输出配置文件路径 | 已实现 |

## 3. 配置键

| 键 | 说明 | 敏感 |
| --- | --- | --- |
| `s3.endpoint` | S3 端点 | 否 |
| `s3.bucket` | 桶名 | 否 |
| `s3.region` | 区域 | 否 |
| `s3.access_key` | 访问密钥 | 是 |
| `s3.secret_key` | 私密密钥 | 是 |
| `platform` | 覆盖当前平台 | 否 |

优先级：环境变量 > 用户配置 > 内置默认值（endpoint / bucket / region）。

| 环境变量 | 配置键 |
| --- | --- |
| `DYTES_S3_ENDPOINT` | `s3.endpoint` |
| `DYTES_S3_BUCKET` | `s3.bucket` |
| `DYTES_S3_REGION` | `s3.region` |
| `DYTES_S3_ACCESS_KEY` | `s3.access_key` |
| `DYTES_S3_SECRET_KEY` | `s3.secret_key` |
| `DYTES_PLATFORM` | `platform` |

## 4. 执行流程

### `list`

1. 加载用户配置；
2. 合并环境变量与内置默认值；
3. 输出解析后的配置，密钥显示为 `****` 或 `(未设置)`。

### `get`

1. 校验键名，未知键返回 `2`；
2. 敏感键默认脱敏，`--reveal` 输出明文。

### `set`

1. 未提供 `value` 且键为 `s3.access_key` / `s3.secret_key` 时交互输入；
2. 其它键缺少值返回 `2`；
3. 校验键名并写回配置文件；
4. 敏感值输出 `****`。

### `unset` / `path`

- `unset` 删除键并保存；
- `path` 只输出配置文件路径。

## 5. 配置文件位置

| 平台 | 路径 |
| --- | --- |
| Windows | `%APPDATA%\dytes\config.toml` |
| Linux / macOS | `~/.config/dytes/config.toml` |
| 覆盖 | `DYTES_CONFIG` |

Unix 下配置文件权限为 `0600`；Windows 使用用户配置目录默认 ACL。

## 6. 输出

`list` 文本：

```text
配置文件: C:\Users\...\AppData\Roaming\dytes\config.toml
s3.endpoint   = https://s3.api.upyun.com
s3.bucket     = packages
s3.region     = us-east-1
s3.access_key = ****
s3.secret_key = ****
platform      = windows
```

`get --json`：

```json
{ "key": "s3.bucket", "value": "packages" }
```

`set --json`：

```json
{ "key": "s3.bucket", "saved": true }
```

## 7. 退出码

| 退出码 | 场景 |
| --- | --- |
| `0` | 成功 |
| `1` | 读取/保存配置、交互输入失败 |
| `2` | 未知键、缺少值 |
| `3` | 配置文件格式错误 |

## 8. 边界与安全

- 日志与普通输出不打印密钥；
- `--reveal` 只影响当前命令输出；
- 不使用命令行参数回显密钥以外的凭证来源；
- 配置目录与仓库测试目录隔离，测试使用 `DYTES_CONFIG` 指向 `.dytes/`。

## 9. 测试点

- `list` 脱敏；
- `get` 未知键返回 `2`；
- `set` 写入后可读回；
- `unset` 删除；
- `path` 与 `DYTES_CONFIG` 一致；
- 环境变量优先级高于配置文件。

对应测试文件：`tests/cli_config.rs`。

## 10. 后续规划

1. 支持 `config edit` 打开编辑器；
2. 增加配置校验（URL、区域格式）；
3. 支持多套命名配置（`--profile`）；
4. 密钥迁移到系统钥匙串；
5. `list --json` 增加“来源”字段（env / file / default）。
