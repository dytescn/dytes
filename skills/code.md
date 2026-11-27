# 代码规范

本文件记录 dytes 项目的目录与代码组织规范。后续按需逐步增加条目。

## 3. 目录与文件组织

### 3.1 不在 `mod.rs` 中编写代码

`mod.rs` 只允许声明子模块（`pub mod`）和必要的重导出，不允许写逻辑代码。

正确：

```rust
pub mod cli;
pub mod dytes;
pub mod loader;
```

错误：

```rust
pub mod cli;

pub fn run() { /* 逻辑代码 */ }
```

命令实现放在独立文件中，例如 `src/install/cli.rs`、`src/utils/config.rs`；
模块入口 `mod.rs` 只负责组装。

### 3.2 测试不写在 `src/` 的源码文件中

`src/` 下的源码文件不得包含 `#[cfg(test)] mod tests` 等测试代码；
测试统一放在仓库根目录的 `tests/` 目录中，按模块拆分为独立测试文件。

```text
tests/
├── version.rs
├── dytes_manifest.rs
└── ...
```

测试应通过公共 API（`dytes::...`）验证行为；需要测试内部实现时，
优先重构为可测试的公共接口，而不是把测试塞回 `src/`。

> 存量说明：仓库中已有的 `src/` 内测试尚未迁移，后续按模块逐步移入 `tests/`；
> 新增测试必须遵守本条。

### 3.3 发布与 CI 相关文件统一放在 `.github/workflows/`

所有发布 / CI / workflow 文件统一放在 `.github/workflows/`，**禁止**放在仓库
根目录，例如：

- `release.yml`
- `purge-cache.yml`

具体要求：

- 新增同类文件时直接建在 `.github/workflows/`；不要先放根目录“方便验证/
  引用”再移动。
- 仓库根目录只保留构建与包清单相关文件（`Cargo.toml`、`Cargo.lock`、
  `dytes.json`、`build.rs` 等），不出现发布或 CI 配置文件。唯一例外是
  GitHub 硬性要求放在根目录的 composite Action 定义 `action.yml`。
- 文档、示例与脚本提到这些文件时，路径一律写完整路径（如
  `.github/workflows/release.yml`）。
- `.github/workflows/` 只放 workflow 定义：该目录下所有 `*.yml` 都会被
  GitHub 当作 workflow 解析，composite Action 定义放在这里会被报为无效
  workflow。本仓库的 Action 定义固定在根目录 `action.yml`，调用方通过
  `uses: dytescn/dytes@vX.Y.Z` 引用（见 [../docs/action.md](../docs/action.md)）。

正确：

```text
.github/workflows/release.yml
.github/workflows/purge-cache.yml
```

错误：

```text
release.yml
purge-cache.yml
.github/workflows/dytes-action.yml  # Action 定义会被当作 workflow 解析
```
