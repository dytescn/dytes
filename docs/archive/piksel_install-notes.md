# 历史说明（已归档）

> 本文记录合并为单一 `dytes` crate 之前的三个独立 crate 结构，内容已过时，仅保留作历史参考。

本目录用于记录本项目的待办事项、计划与技术备忘。

## 项目速览

`piksel_install` 是一个基于 Rust + Slint 的 Piksel 应用安装 / 更新 / 发布工具链，包含三个独立 crate：

| 目录 | 产物 | 职责 |
| --- | --- | --- |
| `install/` | `install.exe` | Slint 图形安装器（welcome → install → installing → finish 四页流程）。目前 `src/main.rs` 中的“开始安装”仅延时 2 秒跳转到完成页，尚未接入真实解包/安装逻辑。另有 `install/publish.py`（boto3 上传又拍云）。 |
| `update/` | `update.exe` | 更新器兼库。读取本地 `dytes.json` 与 `dytes.lock`，按 `dependencies-core` / `dependencies-sys` / `dependencies-web` 对比远程版本（`core/`、`system/`、`webs/`、`app/` 下的 `packages.json` 与 `dytes.json`），从又拍云 S3（`https://s3.api.upyun.com`，bucket `packages`）下载并更新，最后批量执行 web 包的 `init` 脚本。 |
| `publish/` | `publish.exe` | 发布工具：读取当前目录 `dytes.json`，把构建产物及包元数据上传到 S3，并更新对应类型的 `packages.json`。 |

### 关键结构

- **配置**：`dytes.json`（name/version/type/exports/init，以及三类依赖）。
- **锁文件**：`dytes.lock`，记录已安装包名、版本、类型与 loader。
- **版本比较**：`utils/version.rs`（`semver`，兼容 `^` / `~` 前缀）。
- **下载/上传**：`utils/loader.rs`（S3 与 HTTP 双通道，含重试、目录创建）；凭证目前硬编码在源码中。
- **脚本引擎**：`update/src/utils/script.rs` 用 `oxc` 解析 JS AST，提取 `fetch` 调用并转为 Rust 侧的 HTTP 请求；`update_setup.rs` 负责 `run_all_init_scripts`、`cyctron` 进程生命周期（`./run.pid`）。
- **UI**：`install/ui/*.slint`（多页面安装向导）、`update/ui/app.slint`（进度条 + 重启按钮）。
- **测试**：`update/tests/`（`test_version.rs`、`test_script.rs`、`test_run_dytes.rs`；`test_setup.rs` 已整体注释）。`update/test_web/` 为脚本样例，`update/example/` 为示例包。

### 常用命令

```bash
cd install && cargo run        # 安装器（需要 Slint 运行时依赖）
cd update  && cargo run        # 更新器
cd update  && cargo test       # 更新器测试
cd publish && cargo run        # 在当前目录执行发布（需 dytes.json）
```

## 待办

- [ ] （在此登记具体任务）
