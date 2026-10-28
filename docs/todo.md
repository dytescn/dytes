# dytes 改造 TODO

> 对应需求：[dytes.md](dytes.md)
> 命令清单：[cmd.md](cmd.md)
> 目标：把 `install.exe`、`update.exe`、`publish.exe` 合并为单一 `dytes` 可执行文件，并统一 S3 凭证
> 状态：需求已细化，待实施
> 更新日期：2026-10-28

---

## 0. 总览

### 0.1 目标产物

```text
piksel_install/
├── Cargo.toml              # 可选 workspace（见 D-01）
├── dytes/                  # 新统一入口
│   └── src/main.rs         # dytes install/update/publish/init/config
├── core/                   # 新共享库（dytes-core）
│   └── src/                # 配置、锁、包模型、S3、平台、版本
├── install/                # 保留 GUI，改为薄壳
├── update/                 # 保留库与兼容入口
└── publish/                # 保留库与兼容入口
```

最终发布包：

| 包 | 产物 | 类型 |
| --- | --- | --- |
| `dytes` | `dytes.exe` | `core`（或独立 `app` 引导包） |
| `install` | `install.exe` | `core`（兼容） |
| `update` | `update.exe` | `core`（兼容） |
| `publish` | `publish.exe` | `core`（兼容） |

### 0.2 任务优先级

- **P0**：不完成则 `dytes` 无法使用或会破坏现有安装
- **P1**：影响体验、可测试性或发布流程
- **P2**：优化项，可后续迭代

### 0.3 实施进度（本轮）

| 任务 | 状态 | 说明 |
| --- | --- | --- |
| D-01 工程结构 | 部分完成 | 已新增 `core/`（`dytes-core`）与 `dytes/`；未建根 workspace，避免影响旧子项目 |
| D-02 配置 / 清单 / 锁 | 部分完成 | `core` 已实现 `DytesConfig`、`DytesLock`；旧 `update` / `publish` 尚未切换 |
| D-03 存储 / 平台 / 版本 | 部分完成 | 平台、版本、包路径已统一；S3 客户端与下载上传尚未迁移 |
| D-03.1 S3 凭证 | 部分完成 | 已实现 `core::credentials` 与 `dytes config`；旧三处硬编码尚未删除 |
| D-04 install 引擎 | 未开始 | `dytes install` 当前返回未实现提示 |
| D-05 GUI | 未开始 | 旧 `install.exe` 保持原状 |
| D-06 update | 基本完成 | `dytes update` 已接入原生 update 引擎；`--type` / `--force` / startup 节流待补 |
| D-07 publish | 部分完成 | `publish --dry-run` 可用；上传与索引更新未迁移 |
| D-08 init | 完成 | 七种模板均可生成并通过校验 |
| D-09 CLI | 部分完成 | clap 子命令、`config` / `init` / `publish --dry-run` 已接通 |
| D-10 兼容入口 | 部分完成 | `dytes update` 已通过 `update.exe` 兼容运行 |

> 本轮已完成 `core` / `dytes` 骨架、`dytes config`、`dytes init`、`dytes publish --dry-run`；
> `dytes install` 与 `dytes update` 仍待迁移。

本轮验证：

```text
cargo test --manifest-path core/Cargo.toml      # 12 passed
cargo clippy --manifest-path core/Cargo.toml --all-targets -- -D warnings
cargo clippy --manifest-path dytes/Cargo.toml --all-targets -- -D warnings
cargo fmt --manifest-path core/Cargo.toml -- --check
cargo fmt --manifest-path dytes/Cargo.toml -- --check
```

### 0.4 总体依赖

```text
D-01 工程结构
  ├─► D-02 共享模型
  │     ├─► D-03 共享存储 / 下载
  │     ├─► D-04 install 引擎
  │     ├─► D-05 update 引擎
  │     ├─► D-06 publish 引擎
  │     └─► D-07 init
  ├─► D-08 CLI
  │     └─► D-09 install/update/publish/init 子命令接线
  └─► D-10 兼容入口
        └─► D-11 安装器 / Piksel 调用切换
D-12 测试
D-13 文档
D-14 发布与回滚
```

---

## 1. 现状盘点

### 1.1 现有入口

| 文件 | 当前职责 | 改造方向 |
| --- | --- | --- |
| `install/src/main.rs` | Slint GUI，绑定安装按钮、进度、启动应用 | 保留为 `dytes install --gui` 的 UI 层 |
| `install/src/service/installer.rs` | 下载 update、写 `dytes.json`、调用 `update.exe --auto` | 抽成 `dytes install` 引擎，去掉 UI 依赖 |
| `install/src/utils/s3.rs` | S3 读取与下载 | 与 update / publish 的 S3 合并 |
| `update/src/main.rs` | 手工解析 `--auto/--startup/--discover/--init/--offline` | 改为 `dytes update` 子命令 |
| `update/src/run/dytes.rs` | 应用自身更新、依赖遍历、锁文件、init 脚本 | 移入共享 update 引擎 |
| `update/src/run/update_*.rs` | 各类型更新器 | 保留，改为库 API |
| `update/src/utils/dytes.rs` | `DytesConfig`、依赖类型、清单校验 | 移入 `core` |
| `update/src/utils/lock.rs` | `dytes.lock` 读写 | 移入 `core` |
| `update/src/utils/platform.rs` | 平台导出选择 | 移入 `core` |
| `update/src/utils/loader.rs` | HTTP / S3 下载 | 移入 `core` |
| `publish/src/main.rs` | 读取当前目录 `dytes.json`，执行发布 | 改为 `dytes publish` |
| `publish/src/utils/*` | 包类型、S3、平台、版本、packages.json | 与 `core` 合并去重 |

### 1.2 已知重复与耦合

- `install`、`update`、`publish` 各自实现 S3 / env / loader / platform；
- `update/src/utils/dytes.rs` 与 `publish/src/utils/dytes.rs` 各有一份 `DytesConfig`；
- `update` 与 `publish` 的 `platform.rs`、`version.rs`、`lock.rs` 高度重复；
- `install` 硬编码应用名 `piksel` 与 `update` 包名；
- `install` 直接调用外部 `update.exe --auto`；
- `update` 的 CLI 是手写字符串匹配，没有帮助、版本号、子命令；
- `publish` 没有 `--dry-run`，也不能指定 `--dir`；
- 三个包没有统一 workspace，依赖版本与构建目标各自维护。

---

## 2. 详细任务

### D-01 建立共享工程结构（P0）

**目标**：为 `dytes` 和一个共享库建立可编译的骨架，避免复制代码。

- [ ] 创建 `core/`（建议 crate 名 `dytes-core`），包含：
  - `config.rs`：`DytesConfig`、依赖类型、清单校验；
  - `lock.rs`：`dytes.lock` 读写；
  - `package.rs`：包类型、远程路径、导出选择；
  - `platform.rs`：平台识别与 `platform-exports`；
  - `version.rs`：semver 比较；
  - `storage.rs`：S3 读写与下载抽象；
  - `env.rs`：安装目录、临时目录、网络检查；
  - `error.rs`：统一错误与退出码映射。
- [ ] 创建 `dytes/` 二进制 crate，依赖 `core`。
- [ ] 决定是否增加根 `Cargo.toml` workspace：
  - 方案 A：workspace members = `core`、`dytes`、`install`、`update`、`publish`；
  - 方案 B：无 workspace，`dytes` 通过 path 依赖引用 `core` / `update` / `publish`。
- [ ] 保证三个旧子项目仍可单独 `cargo build --release`。
- [ ] 更新 `.gitignore`（保持 `target/` 忽略）。

**涉及文件**

```text
Cargo.toml（可选）
core/Cargo.toml
core/src/lib.rs
dytes/Cargo.toml
dytes/src/main.rs
```

**验收**

- `cargo check` 在 `core`、`dytes`、`install`、`update`、`publish` 均通过；
- 旧 `install` / `update` / `publish` 的构建命令不变；
- 根目录不产生重复 `target` 之外的构建产物。

---

### D-02 抽取配置、清单与锁文件（P0）

**目标**：三处共用同一套 `dytes.json` / `dytes.lock` 模型。

- [ ] 把 `update/src/utils/dytes.rs` 的 `DytesConfig`、`DependencyType`、
  `Dependency`、`read_dytes_config`、`get_all_dependencies`、`validate_config`
  移入 `core`。
- [ ] 把 `publish/src/utils/dytes.rs` 中发布需要的字段（`exports`、`platform-exports`、
  `type`、`init`）合并到同一 `DytesConfig`。
- [ ] 把 `update/src/utils/lock.rs` 移入 `core`，并让 `publish` 也能读取。
- [ ] 统一 `dytes.lock` 的写入路径与格式，保持向后兼容。
- [ ] 统一错误：配置缺失、字段非法、版本不合法、锁文件损坏。
- [ ] 增加单元测试：
  - 缺少 `name` / `version` / `type`；
  - 依赖版本非法；
  - `exports` 与 `platform-exports` 同时为空；
  - 旧版 `dytes.lock` 可读取。

**涉及文件**

```text
update/src/utils/dytes.rs
update/src/utils/lock.rs
publish/src/utils/dytes.rs
publish/src/utils/lock.rs
core/src/config.rs
core/src/lock.rs
```

**验收**

- 同一份 `dytes.json` 在 install / update / publish 中得到相同解析结果；
- 旧 `dytes.lock` 无需迁移即可读取；
- 配置错误统一返回退出码 `3`。

---

### D-03 合并 S3 / 下载 / 平台 / 版本工具（P0）

**目标**：消除 install / update / publish 的三份实现。

- [ ] 对比 `install/src/utils/s3.rs`、`update/src/utils/loader.rs`、
  `publish/src/utils/loader.rs`，确定统一 API：
  - `fetch_text(key)`
  - `fetch_bytes(key)`
  - `download_to(key, path, progress)`
  - `upload_file(local, key)`
  - `object_exists(key)`
- [ ] 把平台选择（`exports` / `platform-exports`）统一到 `core::platform`。
- [ ] 把版本比较统一到 `core::version`。
- [ ] 保留环境变量 / 配置读取（endpoint、bucket、key、secret）。
- [ ] 增加可注入的存储 trait，测试时使用内存实现：
  ```rust
  trait Storage {
      fn get(&self, key: &str) -> Result<Vec<u8>>;
      fn put(&self, key: &str, bytes: &[u8]) -> Result<()>;
      fn list(&self, prefix: &str) -> Result<Vec<String>>;
  }
  ```
- [ ] 日志中不输出凭证。

**涉及文件**

```text
install/src/utils/s3.rs
update/src/utils/loader.rs
publish/src/utils/loader.rs
update/src/utils/platform.rs
publish/src/utils/platform.rs
update/src/utils/version.rs
publish/src/utils/version.rs
core/src/storage.rs
core/src/platform.rs
core/src/version.rs
```

**验收**

- 三处不再各自依赖 AWS SDK 的重复逻辑；
- 下载失败返回统一错误；
- 平台选择与原 `publish` 行为一致；
- 单元测试可离线运行。

---

### D-03.1 统一 S3 凭证管理（P0）

**目标**：解决三个程序各自硬编码 S3 凭证的问题。

- [ ] 新增 `core/src/credentials.rs`：
  - [ ] `S3Config { endpoint, bucket, region, access_key, secret_key }`；
  - [ ] 从环境变量读取：
    `DYTES_S3_ENDPOINT`、`DYTES_S3_BUCKET`、`DYTES_S3_REGION`、
    `DYTES_S3_ACCESS_KEY`、`DYTES_S3_SECRET_KEY`；
  - [ ] 从配置文件读取（Windows `%APPDATA%\dytes\config.toml`，Unix `~/.config/dytes/config.toml`）；
  - [ ] 优先级：环境变量 > 用户配置 > endpoint / bucket / region 默认值；
  - [ ] 凭证缺失时返回可读错误，不尝试匿名访问。
- [ ] 删除硬编码常量：
  - [ ] `install/src/utils/s3.rs`；
  - [ ] `update/src/utils/loader.rs`；
  - [ ] `publish/src/utils/loader.rs`。
- [ ] 三个调用方统一使用 `core::storage` 创建的 S3 客户端。
- [ ] 日志 / 错误 / `--json` 默认脱敏 `secret_key`。
- [ ] Unix 配置文件权限 `0600`。
- [ ] 支持只读与读写凭证分离（install / update 只读，publish 读写）。
- [ ] 实现 `dytes config`（list / get / set / unset / path）。
- [ ] 增加测试：
  - [ ] env 覆盖配置文件；
  - [ ] 缺失凭证错误；
  - [ ] 脱敏输出；
  - [ ] 配置文件不存在时使用默认 endpoint / bucket。

**涉及文件**

```text
install/src/utils/s3.rs
update/src/utils/loader.rs
publish/src/utils/loader.rs
core/src/credentials.rs
core/src/storage.rs
dytes/src/config.rs
```

**验收**

- 源码中不再出现 Access Key / Secret Key；
- 旋转凭证只需更新一处配置；
- CI 可通过环境变量注入；
- `dytes config` 可查看 / 设置 / 删除配置；
- 敏感值默认显示为 `****`。

---

### D-04 实现 `dytes install` 安装引擎（P0）

**目标**：无 GUI 也能完成应用 / 插件 / 依赖安装。

- [ ] 抽取 `install/src/service/installer.rs` 的纯逻辑到 `core::install` 或
  `update` 库，去掉 Slint 依赖。
- [ ] 应用安装流程：
  - [ ] 创建目录；
  - [ ] 解析应用名（默认 `piksel`）；
  - [ ] 引导 core `dytes` / 兼容 `update.exe`；
  - [ ] 下载应用 `dytes.json`；
  - [ ] 执行内置更新引擎（等价 `update --auto`）；
  - [ ] 写 `dytes.lock`；
  - [ ] 写 `install.log`。
- [ ] 插件 / 包安装流程：
  - [ ] `design_plugins` → `plugins/<name>/`；
  - [ ] `sys` → `libary/`；
  - [ ] `web` → `web/<name>/`；
  - [ ] `model` → `models/`；
  - [ ] `design_soft` → `apps/<name>/`；
  - [ ] 更新 `dytes.lock`；
  - [ ] 执行 `init` 脚本（可 `--no-init`）。
- [ ] 失败处理：
  - [ ] 下载失败清理临时文件；
  - [ ] 不更新锁文件；
  - [ ] 返回非 0。
- [ ] 进度回调接口：GUI / CLI 共用。
- [ ] 单元测试：临时目录 + 内存存储。

**涉及文件**

```text
install/src/service/installer.rs
install/src/utils/s3.rs
core/src/install.rs
dytes/src/install.rs
```

**验收**

- 空目录 `dytes install` 能生成 `dytes.lock`；
- `dytes install plugin <name>` 安装到 `plugins/<name>/`；
- 重复安装同版本不重复下载；
- 单测不需要真实 S3。

---

### D-05 保留 GUI 安装器（P1）

**目标**：`dytes install --gui` 继续提供现有 Slint 界面。

- [ ] 保留 `install/src/main.rs` 的 Slint 页面与事件。
- [ ] 把安装动作改为调用 `dytes` 的安装引擎，而不是启动 `update.exe`。
- [ ] 保留目录选择、进度、完成页、启动应用。
- [ ] 失败时展示错误并允许返回。
- [ ] 允许 `install.exe` 作为薄封装调用 `dytes install --gui`。
- [ ] 不把 Slint 依赖引入 `dytes` 核心二进制（条件编译 / 独立 crate）。

**验收**

- `install.exe` 行为与当前一致；
- `dytes install` 不依赖 GUI；
- GUI 与 CLI 使用同一安装引擎。

---

### D-06 实现 `dytes update`（P0）

**目标**：覆盖现有 `update.exe` 的全部模式。

- [ ] 用 `clap`（或手写解析）实现：
  - `--all`（默认）
  - `--type <core,sys,web,design_plugins,design_soft,model>`
  - `--offline` / `--no-online`
  - `--check`
  - `--discover`
  - `--init`
  - `--startup`
  - `--auto`
  - `--force`
  - `--dir <path>`
  - `--json`
- [ ] 行为对齐：
  - [ ] 先更新应用自身；
  - [ ] 更新后重新读取 `dytes.json`；
  - [ ] 按 core → sys → web → design_plugins → design_soft → model 顺序；
  - [ ] 线上 / 离线两种版本判断；
  - [ ] 更新 `sys` 执行 `setup`；
  - [ ] 更新 `web` 执行 `setup.ts`；
  - [ ] 汇总失败项，最终返回非 0；
  - [ ] 成功后更新 `dytes.lock`；
  - [ ] `--startup` 遵守 `auto_update` 与 1 天节流；
  - [ ] `--check` 有更新返回 `4`。
- [ ] `--discover` 扫描所有 `packages.json`，列出未声明包。
- [ ] `--init` 只重跑已安装 Web 包 `setup.ts`。
- [ ] 保留旧 `update.exe` 参数映射。

**涉及文件**

```text
update/src/main.rs
update/src/run/dytes.rs
update/src/run/update_*.rs
update/src/utils/*.rs
dytes/src/update.rs
core/src/*.rs
```

**验收**

- `dytes update --check` 退出码正确；
- `dytes update --offline` 不访问网络；
- `dytes update --init` 不下载包，只跑脚本；
- 单个依赖失败不阻断后续依赖；
- 旧 `update.exe --auto` 等参数仍可用。

---

### D-07 实现 `dytes publish`（P0）

**目标**：覆盖现有发布流程，增加安全预览。

- [ ] 抽取 `publish/src/run/dytes.rs` 到 `core::publish` 或 `publish` 库。
- [ ] 支持 `--dir`、`--dry-run`、`--platform`、`--force`。
- [ ] 校验：
  - [ ] `dytes.json` 存在且合法；
  - [ ] `type` 合法；
  - [ ] `exports` / `platform-exports` 非空；
  - [ ] 导出文件存在；
  - [ ] 导出目标为单层文件名。
- [ ] 上传顺序：文件 → `dytes.json` → `packages.json`。
- [ ] `packages.json` 不存在时允许创建。
- [ ] `--dry-run` 输出：
  - 每个本地文件；
  - 每个远程 key；
  - 目标 `packages.json`；
  - 不写任何文件、不发起上传。
- [ ] 失败时保持 `packages.json` 旧版本。
- [ ] 日志脱敏。

**涉及文件**

```text
publish/src/main.rs
publish/src/run/dytes.rs
publish/src/utils/*.rs
core/src/publish.rs
dytes/src/publish.rs
```

**验收**

- `dytes publish --dry-run` 不产生网络请求；
- 上传失败不更新索引；
- `design_plugins` / `design_soft` / `web` 的远程路径与当前一致。

---

### D-08 实现 `dytes init`（P1）

**目标**：生成合法的 `dytes.json`。

- [ ] 支持：
  - `--type app|core|sys|web|design_plugins|design_soft|model`
  - `--name`
  - `--version`
  - `--dir`
  - `--force`
  - `--json`
- [ ] 模板：
  - [ ] `app`：`exports` 指向 `target/release/<name>.exe`；
  - [ ] `core`：`exports` 指向可执行文件；
  - [ ] `sys`：`platform-exports.windows` 指向 `libary/<name>.dll`；
  - [ ] `web`：导出 `setup.ts`，`init: ["./setup.ts"]`；
  - [ ] `design_plugins`：`targets`、`use_ai`、`exports`（DLL + `mcp.json`）；
  - [ ] `design_soft`：单文件 exe；
  - [ ] `model`：模型文件。
- [ ] 默认值：
  - `name` = 目录名；
  - `version` = `0.1.0`；
  - `type` = `app`。
- [ ] 已存在时拒绝，`--force` 才覆盖。
- [ ] 输出下一步命令。
- [ ] 单元测试：七种模板均可被 `core` 解析并通过校验。

**涉及文件**

```text
dytes/src/init.rs
dytes/src/config.rs
core/src/templates.rs
```

**验收**

- `dytes init` 生成的清单可被 `dytes publish --dry-run` 读取；
- 不覆盖已有文件（除非 `--force`）；
- 不写入任何凭证。

---

### D-09 CLI 与退出码（P0）

**目标**：统一参数、帮助、错误处理和机器可读输出。

- [ ] 建立 `dytes` 入口：
  ```text
  dytes install ...
  dytes update ...
  dytes publish ...
  dytes init ...
  ```
- [ ] `--help` / `dytes <cmd> --help` 输出用法与示例。
- [ ] `--version` 输出版本。
- [ ] 未识别子命令 / 参数返回 `2`。
- [ ] 配置错误返回 `3`。
- [ ] `--json` 输出：
  ```json
  { "ok": true, "command": "update", "data": {}, "error": null }
  ```
- [ ] 日志级别：`-v` 详细，默认简洁。
- [ ] 交互确认：危险操作（覆盖、强制更新）需 `--yes` 或交互确认。

**验收**

- 所有子命令 `--help` 可用；
- 退出码符合 `dytes.md` §3.2；
- `--json` 下 stdout 只有 JSON。

---

### D-10 兼容旧入口（P0）

**目标**：现有脚本、安装目录、快捷方式不失效。

- [ ] `install.exe` 作为 `dytes install --gui` 的薄封装。
- [ ] `update.exe` 支持旧参数：
  - `--auto`
  - `--install`
  - `--startup`
  - `--discover`
  - `--init`
  - `--offline` / `--no-online`
- [ ] `publish.exe` 作为 `dytes publish` 的薄封装。
- [ ] 旧参数不报错，并打印一次迁移提示。
- [ ] 不改变旧 `dytes.json` / `dytes.lock` 格式。
- [ ] 旧安装目录缺少 `dytes` 时，安装器仍能引导。

**涉及文件**

```text
install/src/main.rs
update/src/main.rs
publish/src/main.rs
```

**验收**

- 旧命令逐项对照测试通过；
- 新安装目录统一使用 `dytes`；
- 旧目录无回归。

---

### D-11 调用链切换（P1）

**目标**：安装器、Piksel 与更新器内部不再依赖旧二进制名。

- [ ] `install` 引擎不再启动 `update.exe`，改为调用共享更新引擎或 `dytes update --auto`。
- [ ] `update --startup` / `--auto` 的调用方（Piksel / 安装器）切换。
- [ ] 检查 `dytes_sys_operations` 文档中对 `update.exe` / `install.exe` 的引用，补充 `dytes` 命令。
- [ ] 保留旧命令至少一个版本。

**验收**

- 新安装链路不依赖 `update.exe`；
- 旧链路可回退；
- 文档与命令一致。

---

### D-12 测试（P0/P1）

**目标**：核心逻辑可离线测试，端到端流程可在测试环境验证。

- [ ] 单元测试：
  - [ ] `DytesConfig` 解析 / 校验；
  - [ ] `dytes.lock` 旧格式；
  - [ ] 版本比较（`^` / `~` / 精确）；
  - [ ] 平台导出选择；
  - [ ] CLI 参数解析；
  - [ ] `init` 七种模板；
  - [ ] `publish --dry-run` 路径生成。
- [ ] 集成测试：
  - [ ] 临时目录 + 内存存储：`install`、`update`、`publish`；
  - [ ] 失败注入：网络错误、缺失文件、版本冲突；
  - [ ] `packages.json` 更新顺序。
- [ ] 端到端（需要测试 S3 / 本地模拟）：
  - [ ] `dytes init` → `dytes publish --dry-run`；
  - [ ] `dytes publish` → `dytes update`；
  - [ ] `dytes install plugin <name>`；
  - [ ] 旧 `update.exe --auto`。
- [ ] CI：`cargo fmt --check`、`cargo clippy`、`cargo test`。

**验收**

- 核心逻辑测试不需要真实 S3；
- 端到端测试可在隔离环境重复执行；
- 旧入口参数测试通过。

---

### D-13 文档与发布（P1）

- [ ] 更新 `dytes.md` 的实施状态与待确认项。
- [ ] 新增 `README.md`（或 `docs/README.md`）：
  - 构建；
  - 安装；
  - 四个子命令示例；
  - 退出码；
  - 迁移说明。
- [ ] 更新 `dytes_sys_operations` 中 `app/install.md`、`app/publish.md`、`app/update.md` 的命令示例。
- [ ] 发布 `dytes` 包：
  - `dytes.json`（`type: core`）；
  - `exports` 指向 `target/release/dytes.exe`；
  - 版本号与旧包解耦策略。
- [ ] 记录灰度 / 回滚步骤。

**验收**

- 新用户只读 `README.md` 即可完成安装、更新、发布、init；
- 文档命令与实际 `--help` 一致。

---

### D-14 迁移与回滚（P1）

- [ ] 阶段 1：新增 `dytes`，旧入口不变。
- [ ] 阶段 2：安装器内部切换 `dytes update --auto`。
- [ ] 阶段 3：Piksel 启动更新切换 `dytes update --startup`。
- [ ] 阶段 4：旧入口改为薄封装。
- [ ] 阶段 5：评估移除旧入口。
- [ ] 每个阶段保留回滚方案：
  - 旧二进制仍在；
  - 旧 `dytes.lock` 可恢复；
  - `packages.json` 不提前更新。

**验收**

- 任一阶段失败可使用旧 `update.exe` 恢复；
- 不需要手工迁移数据。

---

## 3. 需要修改的文件清单

### 新增

```text
dytes/Cargo.toml
dytes/src/main.rs
dytes/src/install.rs
dytes/src/update.rs
dytes/src/publish.rs
dytes/src/init.rs
dytes/src/config.rs
core/Cargo.toml
core/src/lib.rs
core/src/config.rs
core/src/lock.rs
core/src/package.rs
core/src/platform.rs
core/src/version.rs
core/src/storage.rs
core/src/credentials.rs
core/src/env.rs
core/src/error.rs
core/src/install.rs
core/src/publish.rs
core/src/templates.rs
```

### 修改

```text
install/Cargo.toml              # 依赖 core；拆分 GUI / 引擎
install/src/main.rs             # 改为 GUI 入口 / 薄壳
install/src/service/installer.rs# 去 UI 化，复用 core
install/src/utils/s3.rs         # 迁移到 core 或删除
update/Cargo.toml               # 依赖 core
update/src/main.rs              # 参数映射 / 薄壳
update/src/run/dytes.rs         # 迁入 core 或改为调用
update/src/utils/*.rs           # 迁入 core 或 re-export
publish/Cargo.toml              # 依赖 core
publish/src/main.rs             # 薄壳
publish/src/run/dytes.rs        # 迁入 core 或改为调用
publish/src/utils/*.rs          # 迁入 core 或 re-export
```

### 文档

```text
dytes.md
cmd.md
README.md（新增）
todo.md（本文件）
../dytes_sys_operations/sysinfo/app/install.md
../dytes_sys_operations/sysinfo/app/publish.md
../dytes_sys_operations/sysinfo/app/update.md
```

---

## 4. 命令与选项对照

| 旧命令 / 参数 | 新命令 |
| --- | --- |
| `install.exe` | `dytes install --gui` |
| `update.exe` | `dytes update` |
| `update.exe --auto` | `dytes update --auto` |
| `update.exe --startup` | `dytes update --startup` |
| `update.exe --discover` | `dytes update --discover` |
| `update.exe --init` | `dytes update --init` |
| `update.exe --no-online` | `dytes update --offline` |
| `publish.exe` | `dytes publish` |
| — | `dytes install plugin <name>` |
| — | `dytes init --type <type>` |
| — | `dytes publish --dry-run` |
| — | `dytes config set s3.access_key` |
| — | `dytes config list` |

---

## 5. 风险与待确认

| 风险 | 影响 | 处理 |
| --- | --- | --- |
| 旧安装目录没有 `dytes` | 新安装器无法直接调用 | 保留 `update.exe` 引导与兼容参数 |
| `install` / `update` / `publish` 依赖版本不一致 | workspace 编译冲突 | 先统一 `core`，再逐 crate 迁移 |
| S3 凭证 / endpoint 分散 | 发布失败或泄露 | 统一到 `core::storage` + 环境变量 |
| 版本号语义不统一 | 更新判断错误 | 统一 semver 比较，补测试 |
| GUI 与 CLI 双入口 | 行为分叉 | 共享安装引擎，GUI 只做展示 |
| `packages.json` 提前更新 | 发布半成品 | 保持“文件全部成功后再更新索引” |

待确认（同步 `dytes.md` §13）：

1. `dytes install` 默认安装 Piksel，还是必须显式 `app`？
2. 插件命令是 `dytes install plugin <name>` 还是 `dytes install design_plugins <name>`？
3. `dytes publish` 是否自动构建 / 打包？
4. 旧入口保留多久？
5. 是否在下一期加入 `dytes uninstall`？

---

## 6. Definition of Done

- [ ] `dytes` 单二进制可完成 install / update / publish / init / config；
- [ ] S3 凭证只在一处配置，源码中不再出现真实 AK/SK；
- [ ] 旧 `install.exe` / `update.exe` / `publish.exe` 兼容；
- [ ] 三处重复的配置 / S3 / 平台 / 版本逻辑收敛到 `core`；
- [ ] `cargo fmt --check`、`cargo clippy`、`cargo test` 全绿；
- [ ] 端到端流程在隔离环境通过；
- [ ] 文档与 `--help` 一致；
- [ ] 发布与回滚步骤可执行。
