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
