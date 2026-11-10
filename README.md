# dytes

Piksel 包管理工具：单一 Rust 二进制，负责应用、插件与依赖的安装、更新、发布和配置。

## 命令

| 命令 | 说明 |
| --- | --- |
| `dytes install` | 安装应用、插件或依赖包；`--gui` 启动图形安装器 |
| `dytes update` | 检查并安装更新（`--check` / `--offline` / `--discover` / `--init`） |
| `dytes self-update` | 检查并升级 dytes 自身 |
| `dytes publish` | 发布当前包；`--dry-run` 只预览 |
| `dytes check` | 校验包清单 |
| `dytes init` | 初始化 `dytes.json` |
| `dytes config` | 管理 S3 凭证与平台配置 |

完整参数与退出码见 [docs/cmd.md](docs/cmd.md)。

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

图形安装器需要启用 `gui` feature：

```bash
cargo build --release --features gui
```

## 文档

- [设计与包模型](docs/dytes.md)
- [命令清单](docs/cmd.md)
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
