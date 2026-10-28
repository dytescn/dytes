# dytes

`dytes` 是一个 AI 设计插件包管理工具，用于 Piksel 体系的插件安装、更新、发布与清单初始化。

它把原先分散的安装器、更新器、发布器合并为一个 Rust 单二进制，统一处理：

- 应用与依赖更新
- Web 包、系统能力库、模型、设计插件、设计软件
- `dytes.json` / `dytes.lock` / `packages.json`
- S3 凭证与发布配置

## 当前能力

| 命令 | 状态 | 说明 |
| --- | --- | --- |
| `dytes update` | 可用 | 原生更新引擎，支持 `--check` / `--offline` / `--discover` / `--init` |
| `dytes config` | 可用 | 管理 S3 endpoint、bucket、AK/SK 与平台 |
| `dytes init` | 可用 | 生成七种包类型的 `dytes.json` 模板 |
| `dytes publish --dry-run` | 可用 | 输出发布文件与远程路径，不上传 |
| `dytes publish` | 迁移中 | 实际上传仍待接入 |
| `dytes install` | 迁移中 | 安装引擎仍待接入 |

## 包类型

| 类型 | 安装位置 | 远程目录 |
| --- | --- | --- |
| `app` | 安装根目录 | `app/` |
| `core` | 安装根目录 / `libary/` | `core/` |
| `sys` | `libary/` | `system/` |
| `web` | `web/<name>/` | `webs/` |
| `design_plugins` | `plugins/<name>/` | `design_plugins/` |
| `design_soft` | `apps/<name>/` | `design_soft/` |
| `model` | `models/` | `models/` |

## 快速开始

```bash
# 配置 S3（敏感值可省略，使用交互式输入）
dytes config set s3.endpoint https://s3.api.upyun.com
dytes config set s3.bucket packages
dytes config set s3.access_key
dytes config set s3.secret_key

# 初始化一个能力库
dytes init --type sys --name info_lib --version 0.1.0

# 检查 / 执行更新
dytes update --check
dytes update

# 预览发布
dytes publish --dir <包目录> --dry-run
```

## 全局命令

`dytes` 设计为全局命令，发布产物直接加入 `PATH` 即可：

```text
Windows:  %USERPROFILE%\bin\dytes.exe
Linux:    ~/.local/bin/dytes
macOS:    /usr/local/bin/dytes
```

验证：

```bash
dytes --version
dytes --help
```

之后所有文档示例都直接使用 `dytes <command>`，不需要 `./dytes` 或绝对路径。

## 内置 S3 配置

发布构建会把 `UPYUN_API_KEY`、`UPYUN_API_SECRET`、`UPYUN_BUCKET`、`UPYUN_ENDPOINT`
注入 `build.rs`，生成的二进制默认携带这组 S3 配置。

- 本地构建未注入时，仍可通过 `dytes config` 或 `DYTES_S3_*` 环境变量配置；
- 运行时配置优先级：环境变量 > 用户配置 > 构建内置值 > 默认值；
- 凭证会被编译进二进制，仅用于内部发布，不应公开分发带凭证的构建产物。

## 构建与测试

```bash
cargo build --release --locked
cargo test --lib --locked
cargo clippy --all-targets -- -D warnings
cargo fmt --check
```

产物：

```text
target/release/dytes.exe   # Windows
target/release/dytes       # Linux / macOS
```

## 目录结构

```text
.
├── Cargo.toml
├── Cargo.lock
├── dytes.json
├── src/
│   ├── main.rs       # CLI
│   ├── lib.rs
│   ├── shared/       # 配置、凭证、锁文件、平台、模板
│   ├── update/       # 更新引擎
│   ├── install/      # 安装逻辑（待接入 CLI）
│   └── publish/      # 发布逻辑
├── docs/
│   ├── dytes.md      # 需求与设计
│   ├── cmd.md        # 命令清单
│   ├── todo.md       # 改造任务
│   └── archive/      # 历史说明
└── .github/workflows/release.yml
```

## 文档

- [需求与设计](docs/dytes.md)
- [命令清单](docs/cmd.md)
- [改造 TODO](docs/todo.md)
- [历史说明](docs/archive/piksel_install-notes.md)

## 发布

GitHub Actions 在推送 `v*` tag 或手动触发时：

1. 在 Windows / Linux / macOS 构建
2. 运行库测试
3. 上传二进制到 GitHub Release
4. 上传到 S3：
   - `core/dytes/<version>/<platform>/dytes`
   - `core/dytes/dytes.json`

需要配置 Environment `product` 下的 Secrets：

```text
UPYUN_API_KEY
UPYUN_API_SECRET
UPYUN_BUCKET
UPYUN_ENDPOINT
```

发布命令：

```bash
git tag v0.1.0
git push origin v0.1.0
```

## License

MIT
