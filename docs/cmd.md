# dytes 命令清单

> `dytes` 是一个 AI 设计插件包管理工具，以下命令按全局命令使用。
> 需求：[dytes.md](dytes.md)
> 实施：[todo.md](todo.md)
> 更新日期：2026-10-28

---

## 1. 命令总览

```text
dytes
├── install     安装应用 / 插件 / 依赖包
├── update      检查并安装更新
├── publish     发布当前包
├── init        初始化 dytes.json
├── config      S3 凭证与工具配置
├── help        帮助
└── --version   版本
```

无子命令时输出帮助。

---

## 2. 全局选项

| 选项 | 适用命令 | 说明 |
| --- | --- | --- |
| `--dir <path>` | install / update / publish / init | 工作目录，默认当前目录 |
| `--config <path>` | 全部 | 指定 `dytes.json`，默认 `<dir>/dytes.json` |
| `--offline` | install / update | 不访问网络，`--no-online` 等价 |
| `--json` | 全部 | 机器可读输出 |
| `--verbose` / `-v` | 全部 | 详细日志 |
| `--yes` / `-y` | 全部 | 跳过交互确认 |
| `--dry-run` | publish / init | 只展示计划，不写文件 / 不上传 |
| `--help` | 全部 | 帮助 |
| `--version` | 全部 | 版本 |

退出码：

| code | 含义 |
| --- | --- |
| `0` | 成功 |
| `1` | 一般失败（网络 / 下载 / 安装 / 发布） |
| `2` | 参数错误 |
| `3` | 配置 / 清单错误 |
| `4` | 有可用更新（仅 `update --check`） |

---

## 3. `dytes config`

用于集中管理 S3 凭证与工具配置，替代三个程序中硬编码的常量。

### 3.1 语法

```bash
dytes config list [--json]
dytes config get <key> [--reveal] [--json]
dytes config set <key> <value> [--json]
dytes config unset <key>
dytes config path [--json]
```

### 3.2 配置键

| key | 说明 | 是否敏感 |
| --- | --- | --- |
| `s3.endpoint` | S3 兼容端点，如 `https://s3.api.upyun.com` | 否 |
| `s3.bucket` | 存储桶，默认 `packages` | 否 |
| `s3.region` | 区域，默认 `us-east-1` | 否 |
| `s3.access_key` | Access Key | 是 |
| `s3.secret_key` | Secret Key | 是 |
| `platform` | 发布 / 更新目标平台，默认当前系统 | 否 |

### 3.3 示例

```bash
# 查看配置（敏感值默认脱敏）
dytes config list

# 设置端点与桶
dytes config set s3.endpoint https://s3.api.upyun.com
dytes config set s3.bucket packages

# 设置凭证（省略 value 时交互式隐藏输入）
dytes config set s3.access_key
dytes config set s3.secret_key

# 查看单个键（显式揭示敏感值）
dytes config get s3.access_key --reveal

# 删除键
dytes config unset s3.secret_key

# 配置文件路径
dytes config path
```

### 3.4 配置来源与优先级

从高到低：

1. 环境变量；
2. 用户配置文件；
3. 工具内置默认值（仅 endpoint / bucket / region，凭证无默认值）。

| 环境变量 | 对应键 |
| --- | --- |
| `DYTES_S3_ENDPOINT` | `s3.endpoint` |
| `DYTES_S3_BUCKET` | `s3.bucket` |
| `DYTES_S3_REGION` | `s3.region` |
| `DYTES_S3_ACCESS_KEY` | `s3.access_key` |
| `DYTES_S3_SECRET_KEY` | `s3.secret_key` |
| `DYTES_PLATFORM` | `platform` |

### 3.5 配置文件

| 平台 | 路径 |
| --- | --- |
| Windows | `%APPDATA%\dytes\config.toml` |
| Linux / macOS | `~/.config/dytes/config.toml` |

示例：

```toml
[s3]
endpoint = "https://s3.api.upyun.com"
bucket = "packages"
region = "us-east-1"
access_key = "..."
secret_key = "..."
```

要求：

- Unix 下文件权限 `0600`；
- 日志、`--json`、错误信息不得输出 `secret_key`；
- `config list` / `get` 默认显示 `****`，`--reveal` 才显示明文；
- `publish` 需要写权限；`install` / `update` 只需要读权限；
- 不再把凭证写入源码、`dytes.json` 或发布包。

---

## 4. `dytes install`

### 4.1 语法

```bash
# 安装应用（默认 piksel）
dytes install [--dir <path>] [--version <ver>] [--gui]

# 安装插件
dytes install plugin <name>[@<version>] [--dir <path>] [--force]

# 安装其他包
dytes install sys <name>[@<version>]
dytes install web <name>[@<version>]
dytes install core <name>[@<version>]
dytes install design-soft <name>[@<version>]
dytes install model <name>[@<version>]

# 从本地清单 / 目录安装
dytes install --manifest <path/to/dytes.json>
dytes install --file <package.zip|dir>
```

### 4.2 选项

| 选项 | 说明 |
| --- | --- |
| `--version <ver>` | 指定版本；缺省取索引最新版本 |
| `--dir <path>` | 安装目录 |
| `--gui` | 启动 Slint 图形安装器 |
| `--force` | 已安装时重新下载覆盖 |
| `--launch` | 应用安装完成后启动 |
| `--no-init` | 不执行 `setup.ts` |
| `--manifest <path>` | 从指定 `dytes.json` 安装 |
| `--file <path>` | 从本地包 / 目录安装 |
| `--offline` | 仅使用本地缓存或已下载包 |
| `--json` | 输出安装结果 JSON |

### 4.3 示例

```bash
# 安装 Piksel 到默认目录
dytes install

# 安装到指定目录并启动
dytes install --dir D:\Piksel --launch

# 安装 CorelDRAW 文本检查插件
dytes install plugin cdr_check_txt

# 安装指定版本的系统库
dytes install sys designsoft_lib@0.1.0

# 安装 Web 包
dytes install web setting

# 图形安装
dytes install --gui
```

### 4.4 行为

- 应用：准备 core → 下载应用清单 → 执行更新引擎 → 写 `dytes.lock`；
- 插件 / 包：下载对应 `packages.json` 中的版本 → 写入目标目录 → 更新锁；
- `web` / 需要初始化的包：执行 `init` 脚本（`--no-init` 跳过）；
- 安装失败不更新锁文件；
- 重复安装同版本默认跳过，`--force` 重新覆盖。

---

## 5. `dytes update`

### 5.1 语法

```bash
dytes update [--all]
dytes update --type <type[,type...]>
dytes update --check
dytes update --offline
dytes update --discover
dytes update --init
dytes update --startup
dytes update --auto
```

### 5.2 选项

| 选项 | 说明 |
| --- | --- |
| `--all` | 更新全部（默认） |
| `--type <list>` | 只更新指定类型：`core,sys,web,design_plugins,design_soft,model` |
| `--check` | 只检查，不下载；有更新返回 `4` |
| `--offline` / `--no-online` | 不访问网络 |
| `--discover` | 扫描线上索引，列出未声明包 |
| `--init` | 只重跑已安装 Web 包的 `setup.ts` |
| `--startup` | 启动时更新，遵守 `auto_update` 与 1 天节流 |
| `--auto` / `--install` | 静默自动更新（安装器内部调用） |
| `--force` | 忽略版本比较，强制重装 |
| `--dir <path>` | 安装目录 |
| `--json` | 输出更新结果 JSON |

### 5.3 示例

```bash
# 更新全部依赖
dytes update

# 只检查是否有更新
dytes update --check

# 只更新系统能力库
dytes update --type sys

# 离线更新
dytes update --offline

# 扫描未声明的线上包
dytes update --discover

# 只重跑 Web setup.ts
dytes update --init

# 安装器内部调用
dytes update --auto
```

### 5.4 行为

- 先更新应用自身，再重新读取 `dytes.json`；
- 按 core → sys → web → design_plugins → design_soft → model 顺序；
- 线上模式以 `packages.json` 最新版本为准；离线模式按本地声明版本；
- 单个依赖失败不阻断其它依赖，最终返回非 0；
- 成功后写 `dytes.lock`；
- `--startup` 检查 `auto_update` 与上次检查时间。

---

## 6. `dytes publish`

### 6.1 语法

```bash
dytes publish [--dir <path>] [--dry-run] [--platform <platform>] [--force]
```

### 6.2 选项

| 选项 | 说明 |
| --- | --- |
| `--dir <path>` | 包目录，默认当前目录 |
| `--dry-run` | 只打印上传计划，不上传、不更新索引 |
| `--platform <platform>` | 指定平台，默认当前系统 |
| `--force` | 允许覆盖已存在对象 |
| `--json` | 输出发布结果 JSON |

### 6.3 示例

```bash
# 预览发布
dytes publish --dry-run

# 发布当前包
dytes publish

# 发布指定平台
dytes publish --platform windows

# 发布指定目录
dytes publish --dir D:\codes\vectorcraft
```

### 6.4 行为

- 读取并校验 `dytes.json`；
- 选择 `exports` / `platform-exports`；
- 上传文件 → `dytes.json` → `packages.json`；
- 任一文件失败不更新 `packages.json`；
- `--dry-run` 不产生网络请求，只输出：
  - 本地文件；
  - 远程 key；
  - 目标 `packages.json`；
  - 将写入的版本。

---

## 7. `dytes init`

### 7.1 语法

```bash
dytes init [--type <type>] [--name <name>] [--version <ver>] [--dir <path>] [--force]
```

### 7.2 选项

| 选项 | 说明 |
| --- | --- |
| `--type` | `app` / `core` / `sys` / `web` / `design_plugins` / `design_soft` / `model` |
| `--name` | 包名，默认目录名 |
| `--version` | 版本，默认 `0.1.0` |
| `--dir` | 输出目录 |
| `--force` | 覆盖已存在的 `dytes.json` |
| `--json` | 输出生成结果 JSON |

### 7.3 示例

```bash
# 初始化应用
dytes init --type app --name piksel

# 初始化系统能力库
dytes init --type sys --name info_lib --version 0.1.0

# 初始化 Web 包
dytes init --type web --name setting

# 初始化设计插件
dytes init --type design_plugins --name cdr_check_txt

# 初始化设计软件
dytes init --type design_soft --name vectorcraft

# 初始化模型包
dytes init --type model --name qwen3-embed
```

### 7.4 输出模板

| type | 关键字段 |
| --- | --- |
| `app` | `exports` → `target/release/<name>.exe` |
| `core` | `exports` → 可执行文件 |
| `sys` | `platform-exports.windows` → `libary/<name>.dll` |
| `web` | `exports` + `init: ["./setup.ts"]` |
| `design_plugins` | `targets`、`use_ai`、DLL + `mcp.json` |
| `design_soft` | 单文件 exe |
| `model` | 模型文件 |

### 7.5 设计插件与设计软件

`design_plugins` 和 `design_soft` 已合并到 `dytes` 的包类型中，不再维护独立发布目录。

| 类型 | 安装目录 | S3 路径 | 索引 |
| --- | --- | --- | --- |
| `design_plugins` | `plugins/<name>/` | `design_plugins/<name>/<version>/` | `design_plugins/packages.json` |
| `design_soft` | `apps/<name>/` | `design_soft/<name>/<version>/` | `design_soft/packages.json` |

设计插件 `dytes.json` 示例：

```json
{
  "name": "cdrimex",
  "version": "0.1.0",
  "description": "CorelDRAW design plugin",
  "type": "design_plugins",
  "exports": {
    "./cdrimex.dll": "cdrimex.dll",
    "./mcp.json": "mcp.json"
  }
}
```

设计软件 `dytes.json` 示例：

```json
{
  "name": "vectorcraft",
  "version": "1.0.0",
  "description": "Vector design software",
  "type": "design_soft",
  "exports": {
    "./vectorcraft.exe": "vectorcraft.exe"
  }
}
```

发布与安装：

```bash
dytes publish --dir <包目录> --dry-run
dytes publish --dir <包目录>
dytes install plugin cdrimex
dytes install design-soft vectorcraft
```

---

## 8. 兼容命令

| 旧命令 | 新命令 | 说明 |
| --- | --- | --- |
| `install.exe` | `dytes install --gui` | 图形安装器 |
| `update.exe` | `dytes update` | 完整更新 |
| `update.exe --auto` | `dytes update --auto` | 静默更新 |
| `update.exe --startup` | `dytes update --startup` | 启动检查 |
| `update.exe --discover` | `dytes update --discover` | 扫描索引 |
| `update.exe --init` | `dytes update --init` | 重跑 setup |
| `update.exe --no-online` | `dytes update --offline` | 离线 |
| `publish.exe` | `dytes publish` | 发布 |

旧命令保留一个版本，作为 `dytes` 的薄封装；参数继续可用。

---

## 9. 完整命令树

```text
dytes
├── --version
├── --help
├── install
│   ├── [app] [name]         # 默认 piksel
│   ├── plugin <name>
│   ├── sys <name>
│   ├── web <name>
│   ├── core <name>
│   ├── design-soft <name>
│   ├── model <name>
│   ├── --gui
│   ├── --version <ver>
│   ├── --dir <path>
│   ├── --force
│   ├── --launch
│   ├── --no-init
│   ├── --manifest <dytes.json>
│   └── --file <path>
├── update
│   ├── --all
│   ├── --type <list>
│   ├── --check
│   ├── --offline
│   ├── --discover
│   ├── --init
│   ├── --startup
│   ├── --auto
│   ├── --force
│   └── --dir <path>
├── publish
│   ├── --dir <path>
│   ├── --dry-run
│   ├── --platform <platform>
│   └── --force
├── init
│   ├── --type <type>
│   ├── --name <name>
│   ├── --version <ver>
│   ├── --dir <path>
│   └── --force
└── config
    ├── list
    ├── get <key>
    ├── set <key> [value]
    ├── unset <key>
    └── path
```

---

## 10. 推荐工作流

```bash
# 1. 配置一次凭证
dytes config set s3.endpoint https://s3.api.upyun.com
dytes config set s3.bucket packages
dytes config set s3.access_key
dytes config set s3.secret_key

# 2. 初始化包
dytes init --type sys --name info_lib

# 3. 预览并发布
dytes publish --dry-run
dytes publish

# 4. 安装 / 更新
dytes install
dytes update --check
dytes update
```
