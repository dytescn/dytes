# dytes 命令清单

> 包模型与设计见 [dytes.md](dytes.md)。测试环境见 [../skills/testenv.md](../skills/testenv.md)。

## 1. 命令总览

```text
dytes install      安装应用 / 插件 / 依赖包
dytes update       检查并安装更新
dytes self-update  检查并升级 dytes 自身
dytes publish      发布当前包
dytes check        校验包清单
dytes init         初始化 dytes.json
dytes config       S3 凭证与工具配置
dytes help         帮助
dytes --version    版本
```

无子命令时输出帮助。

## 2. 通用约定

退出码：

| code | 含义 |
| --- | --- |
| `0` | 成功 |
| `1` | 一般失败（网络 / 下载 / 校验 / 安装） |
| `2` | 参数错误 |
| `3` | 配置 / 清单错误 |
| `4` | 有可用更新（仅 `update --check`） |

常用选项（按命令支持）：

| 选项 | 说明 |
| --- | --- |
| `--dir <path>` | 工作目录，默认当前目录 |
| `--json` | 机器可读输出 |
| `--offline` / `--no-online` | 不访问网络（install / update） |
| `--dry-run` | 只展示计划，不上传（publish） |
| `--force` | 覆盖已存在文件 / 索引 |
| `--platform <platform>` | 指定平台（check / publish） |

> `update --all`、`update --auto`、`install --no-init` 等兼容参数已保留，
> 部分尚未接线；以 `dytes <command> --help` 为准。

## 3. `dytes config`

```bash
dytes config list [--json]
dytes config get <key> [--reveal] [--json]
dytes config set <key> <value> [--json]
dytes config unset <key>
dytes config path [--json]
```

配置键：`s3.endpoint`、`s3.bucket`、`s3.region`、`s3.access_key`、
`s3.secret_key`、`platform`。凭证默认显示 `****`，`--reveal` 才输出明文。

优先级：环境变量 > 用户配置 > 内置默认值（endpoint / bucket / region）。

| 环境变量 | 配置键 |
| --- | --- |
| `DYTES_S3_ENDPOINT` | `s3.endpoint` |
| `DYTES_S3_BUCKET` | `s3.bucket` |
| `DYTES_S3_REGION` | `s3.region` |
| `DYTES_S3_ACCESS_KEY` | `s3.access_key` |
| `DYTES_S3_SECRET_KEY` | `s3.secret_key` |
| `DYTES_PLATFORM` | `platform` |

配置文件：Windows `%APPDATA%\dytes\config.toml`，Linux/macOS
`~/.config/dytes/config.toml`；可用 `DYTES_CONFIG` 覆盖路径。

```bash
dytes config set s3.endpoint https://s3.api.upyun.com
dytes config set s3.bucket packages
dytes config set s3.access_key
dytes config set s3.secret_key
```

## 4. `dytes install`

```bash
dytes install <type> <name>[@<version>] [--version <ver>] [--dir <path>]
dytes install --gui [--dir <path>] [--launch]
```

| type | 说明 | 安装位置 |
| --- | --- | --- |
| `plugin` / `design_plugins` | 设计插件 | `plugins/<name>/` |
| `design-soft` / `design_soft` | 设计软件 | `apps/<name>/` |
| `sys` / `system` | 系统能力库 | `libary/` |
| `core` | 核心包 | 安装根目录 |
| `web` | Web 资源 | `web/<name>/` |
| `model` | 模型 | `models/` |
| `app` | 应用 | 请使用 `dytes update` |

行为：

- `plugin` / `design-soft` 复用更新引擎，只写 `dytes.lock` 与包目录清单，
  不修改应用根 `dytes.json`；插件下载完成后通过 FFI 调用 `plugin_lib` 的
  `install_plugin` / `update_plugin` 登记插件数据库；
- `sys` / `core` / `web` / `model` 先把依赖写入应用 `dytes.json`，再只安装目标包；
- 安装失败不更新锁文件；同版本默认跳过，`--force` 重新下载；
- `--gui` 需要 `--features gui` 构建，未启用时返回参数错误。

## 5. `dytes update` 与 `dytes self-update`

```bash
dytes update [--type <type[,type...]>] [--check] [--offline]
              [--discover] [--init] [--startup] [--dir <path>] [--json]
```

| 选项 | 说明 |
| --- | --- |
| `--type <list>` | 只更新指定类型：`core,sys,web,design_plugins,design_soft,model` |
| `--check` | 只检查不下载；有更新返回 `4` |
| `--offline` | 不访问网络 |
| `--discover` | 扫描线上索引，列出未声明包 |
| `--init` | 只重跑已安装 Web 包的 `setup.json` |
| `--startup` | 启动时更新，遵守 `auto_update` 与 1 天节流 |

行为：先更新应用自身，再按 core → sys → web → design_plugins → design_soft → model
顺序处理；单个依赖失败不阻断其它依赖，最终返回非 0；成功后写 `dytes.lock`。

`--check --json` 输出：

```json
{
  "check": true,
  "types": ["design_plugins"],
  "count": 1,
  "updates": [
    {
      "type": "design_plugins",
      "name": "cdr_check_txt",
      "local_version": "1.0.0",
      "remote_version": "1.1.0",
      "needs_update": true
    }
  ]
}
```

```bash
dytes self-update [--check] [--force] [--json] [--source auto|cdn|s3]
```

`self-update` 从 `core/dytes` 清单读取最新版本，下载对应平台二进制并用同名
`.sha256` 校验；`--check` 只比较版本。

## 6. `dytes publish`

```bash
dytes publish [--dir <path>] [--config <path>] [--dry-run]
              [--platform <platform>] [--manifest-only] [--skip-check] [--force] [--json]
```

| 选项 | 说明 |
| --- | --- |
| `--dry-run` | 只打印上传计划，不联网 |
| `--platform <platform>` | 指定平台，默认当前系统 |
| `--manifest-only` | 只发布 `dytes.json`，要求 / 训练类包使用 |
| `--skip-check` | 跳过设计插件完整校验（不建议） |
| `--force` | 允许覆盖已有对象；索引不可读时允许新建 |

流程：校验清单 → 选择 `exports` / 当前平台 `platform-exports` → 上传产物 →
上传 `dytes.json` → 更新 `<type>/packages.json`。任一文件失败不更新索引；
缺少 `uuid` 时自动生成并写回。导出目标名不允许包含路径分隔符或 `..`。

## 7. `dytes check`

```bash
dytes check [<dir>] [--type <type>] [--platform <platform>] [--manifest-only] [--json]
```

校验 `dytes.json` 可解析、类型匹配、导出文件存在；设计插件执行完整规则
（`plugins_type`、`targets`、`use_ai`、`mcp`、skills 等）。失败退出码为 `3`。

## 8. `dytes init`

```bash
dytes init [--type <type>] [--name <name>] [--version <ver>]
           [--dir <path>] [--no-exports] [--force] [--json]
```

默认 `name` 取目录名，`version` 为 `0.1.0`，自动生成 `uuid`；已存在清单默认拒绝，
`--force` 才覆盖。

| type | 模板要点 |
| --- | --- |
| `app` / `core` | `exports` 指向 `target/release/<name>.exe` |
| `sys` | `platform-exports.windows` → `libary/<name>.dll` |
| `web` | `exports` + `init: ["./setup.json"]` |
| `design_plugins` | `plugins_type`、`targets`、`use_ai`、`mcp`、DLL |
| `design_soft` | 单文件 exe |
| `model` | 模型文件 |

## 9. 推荐工作流

```bash
dytes config set s3.bucket packages
dytes init --type sys --name info_lib
dytes check
dytes publish --dry-run
dytes publish

dytes install sys info_lib
dytes update --check
dytes update
```
