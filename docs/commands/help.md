# `dytes help` 与版本

## 1. 职责

帮助与版本信息由 clap 自动生成，不需要单独的 `run` 实现：

```bash
dytes help
dytes help <command>
dytes <command> --help
dytes --version
dytes -V
```

无子命令时输出总帮助。

## 2. 输出

总帮助包含全部子命令：

```text
Piksel 包安装、更新、发布与初始化工具

Usage: dytes <COMMAND>

Commands:
  install      安装应用、插件或依赖包
  update       检查并安装依赖更新
  self-update  更新 dytes CLI 自身
  publish      发布当前包
  check        校验包清单（设计插件会执行完整规则）
  test         本地包自检并离线演练安装
  init         初始化 dytes.json
  config       管理 S3 凭证与工具配置
  help         Print this message or the help of the given subcommand(s)
```

## 3. 边界

- 帮助文本来自 `src/main.rs` 的 `#[command]` / `#[arg]` 注释；
- `--version` 来自 `Cargo.toml` 的 `package.version`；
- 修改参数说明时同步更新 `docs/commands/` 对应文档；
- 帮助不触发网络、不读写配置文件。

## 4. 测试点

- `dytes --version` 退出码 `0` 且版本与 `Cargo.toml` 一致；
- `dytes <command> --help` 包含关键参数；
- 无子命令时输出帮助并返回 clap 约定的退出码。

对应测试文件：`tests/cli_global.rs`。

## 5. 后续规划

1. 增加 `dytes help --json` 输出命令树，便于自动化生成文档；
2. 文档中的参数表由帮助文本生成，减少手工同步。
