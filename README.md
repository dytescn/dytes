# dytes

Design software package management tool: a single Rust binary that installs, updates, publishes, and configures applications, plugins, and dependencies.

## 命令

| 命令 | 说明 |
| --- | --- |
| `dytes install` | 安装应用、插件或依赖包；`--gui` 启动图形安装器 |
| `dytes update` | 检查并安装更新（`--check` / `--offline` / `--discover` / `--init`） |
| `dytes self-update` | 检查并升级 dytes 自身 |
| `dytes publish` | 发布当前包；`--dry-run` 只预览 |
| `dytes check` | 校验包清单 |
| `dytes test` | 本地包自检、在线更新依赖并演练安装 |
| `dytes init` | 初始化 `dytes.json` |
| `dytes config` | 管理 S3 凭证与平台配置 |
| `dytes setup` | 安装 dytes 自身（Windows：用户目录 + 用户 PATH；双击 exe 等同） |

完整参数与退出码见 [docs/cmd.md](docs/cmd.md)，每个命令的详细规划见 [docs/commands/](docs/commands/README.md)。

## 系统要求

| 平台 | 最低要求 |
| --- | --- |
| Windows | Windows 10 及以上（产物含 Win10-only API；Windows 7 / 8.1 会在加载器阶段报 `0xC0000139`，详见 [docs/issues.md](docs/issues.md) I-22）。低配机器可用 **Windows Lite**（无 GUI） |
| Linux | glibc ≥ 2.35（Ubuntu 22.04 / Debian 12 及更新）；GUI 需要 X11 或 Wayland、OpenGL、fontconfig |
| macOS | macOS 11 及以上，universal2（Apple Silicon 与 Intel 均可运行） |

## 快速开始

```bash
dytes config set s3.access_key
dytes config set s3.secret_key

dytes init --type sys --name info_lib
dytes check
dytes publish --dry-run
dytes publish
```

安装 Piksel 应用（图形界面可选择安装位置）：

```bash
dytes install --gui --dir D:\Piksel
```

## 构建与测试

```bash
cargo build --release --locked
cargo test --locked
cargo clippy --all-targets -- -D warnings
```

默认构建不包含 GUI，用于 GitHub Action：

```bash
cargo build --release --locked
```

对外发布需要启用 `gui` feature：

```bash
cargo build --release --features gui
```

发布流程共 5 个构建：

- Windows / Linux / macOS 三个带 GUI 的发布构建；
- 1 个 Windows Lite（无 GUI）构建：`dytes-windows-lite.exe`，适合低端电脑，
  不链接 Slint / OpenGL，仍要求 Windows 10+；
- 1 个 Linux 无 GUI 的 Action 构建。

Lite 版本只发布到版本化目录与 GitHub Release（`core/dytes/<version>/windows/dytes-lite.exe`），
不进入 `latest` 清单，`self-update` 仍指向完整版。`dytes setup` 与双击安装在 Lite
版本中同样可用，只是 `install --gui` / `update --gui` 不可用。

各平台产物的系统依赖（glibc / VC++ 运行库 / 最低 Windows 版本 / macOS 架构）与
降低依赖的改造方案见 [docs/packaging.md](docs/packaging.md)。

## 文档

- [设计与包模型](docs/dytes.md)
- [命令清单](docs/cmd.md)
- [设计插件 MCP 规范](docs/mcp.md)
- [官方 MCP 规范对照](docs/mcp-official.md)
- [GitHub Action](docs/action.md)
- [已知问题](docs/issues.md)
- [代码规范](skills/code.md)
- [测试环境规范](skills/testenv.md)

## 发布

同步 `dytes.json`、`Cargo.toml`、`Cargo.lock` 三处版本号，提交后打 tag 并推送：

```bash
git tag vX.Y.Z
git push origin main --tags
```

GitHub Actions 会构建三平台产物并更新 `latest` 清单；之后可用 `dytes self-update` 升级自身。

## License

MIT
