# dytes 命令文档索引

每个子命令一份独立文档，包含职责、参数、执行流程、文件约定、退出码、
测试点与后续规划。总览与跨命令约定仍以 [../cmd.md](../cmd.md) 为准。

| 命令 | 文档 | 一句话职责 |
| --- | --- | --- |
| `install` | [install.md](install.md) | 安装插件、设计软件或依赖包 |
| `update` | [update.md](update.md) | 检查并安装应用与依赖更新 |
| `self-update` | [self-update.md](self-update.md) | 升级 dytes 自身 |
| `publish` | [publish.md](publish.md) | 发布当前包及索引 |
| `check` | [check.md](check.md) | 校验清单与导出文件 |
| `test` | [test.md](test.md) | 本地自检、构建 `.dytes` 测试环境并演练安装 |
| `init` | [init.md](init.md) | 初始化 `dytes.json` |
| `config` | [config.md](config.md) | 管理 S3 凭证与平台配置 |
| `help` | [help.md](help.md) | 帮助与版本输出 |

## 通用约定

### 退出码

| code | 含义 |
| --- | --- |
| `0` | 成功 |
| `1` | 一般失败（网络 / 下载 / 校验 / 安装 / IO） |
| `2` | 参数错误 |
| `3` | 配置 / 清单错误 |
| `4` | 有可用更新（仅 `update --check`） |

### 公共参数

| 参数 | 适用命令 | 说明 |
| --- | --- | --- |
| `--dir <path>` | install / update / publish / init | 工作目录，默认当前目录 |
| `[DIR]` | check / test | 包目录，默认当前目录 |
| `--platform <platform>` | publish / check / test | `windows` / `linux` / `macos`，别名会归一化 |
| `--json` | 除 `config unset` 外的主要命令 | 机器可读输出 |
| `--offline` | install / update | 不访问网络；范围见各命令文档 |
| `--force` | install / publish / self-update / init | 覆盖或重装 |
| `--manifest-only` | publish / check / test | 允许只有 `dytes.json`、没有产物的包 |

### 环境变量

| 环境变量 | 配置键 / 用途 |
| --- | --- |
| `DYTES_CONFIG` | 覆盖配置文件路径 |
| `DYTES_S3_ENDPOINT` | `s3.endpoint` |
| `DYTES_S3_BUCKET` | `s3.bucket` |
| `DYTES_S3_REGION` | `s3.region` |
| `DYTES_S3_ACCESS_KEY` | `s3.access_key` |
| `DYTES_S3_SECRET_KEY` | `s3.secret_key` |
| `DYTES_PLATFORM` / `PIKSEL_PLATFORM` | 当前平台 |

### 本地测试目录

所有本地测试与演练产物统一放在包目录的 `.dytes/` 下，
不得写入用户真实配置目录。`dytes test` 会自动创建目录并把 `.dytes/`
追加到 `.gitignore`。规范见 [../../skills/testenv.md](../../skills/testenv.md)。
