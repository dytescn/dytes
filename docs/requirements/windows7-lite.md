# 需求：Windows 7 兼容的 Lite 版本

**状态**：方案评估 + **S0 spike 已验证**（2026-11-28，见第 9 节），全量实现待评审
**关联**：[issues.md](../issues.md) I-22、[packaging.md](../packaging.md)、
[windows-installer.md](windows-installer.md)

## 1. 背景与结论

GitHub #37 实测：`dytes-windows.exe`（0.2.28 / 0.2.29）在 Windows 7 SP1 x64 上以
`0xC0000139`（`STATUS_ENTRYPOINT_NOT_FOUND`）退出，报
`SystemParametersInfoForDpi` 缺失。

0.2.29 的静态 CRT 只去掉了 `VCRUNTIME140.dll` / `api-ms-win-crt-*`，导入表里仍有：

| 符号 | 来源 | 去掉方式 |
| --- | --- | --- |
| `user32!SystemParametersInfoForDpi` | Slint winit 后端 | 不用 GUI 构建 |
| `combase!CoTaskMemFree` / `CoCreateFreeThreadedMarshaler` | `windows` crate（windows-core） | 改用 `windows-sys` 或旧版 windows |
| `api-ms-win-core-winrt-error-l1-1-0!RoOriginateErrorW` | windows-core 错误处理 | 同上 |
| `bcryptprimitives!ProcessPrng` | **Rust std** | rustc ≤ 1.77 |
| `api-ms-win-core-synch-l1-2-0!WaitOnAddress` / `WakeByAddressSingle` | **Rust std** | rustc ≤ 1.77 |
| `kernel32!GetSystemTimePreciseAsFileTime` | **Rust std** | rustc ≤ 1.77 |

**结论：Win7 兼容必须同时满足“rustc ≤ 1.77 + 无 GUI + 不依赖 Win8/10-only 的
combase/WinRT 调用”。** 只降工具链或只去 GUI 都不够。

补充事实：

- 当前依赖 MSRV：`slint 1.18` → 1.92、`aws-sdk-s3 1.156` → 1.94.1、
  `aws-config` → 1.94.1、`windows 0.62` → 1.82；
- `Cargo.lock` 为 version 4，rustc 1.77 的 Cargo 无法解析，需要重建为 v3；
- 项目自 2026-10 建立以来所有版本都用当前工具链，没有旧工具链产物可复用。

## 2. 目标与非目标

**目标**

- 新增 `dytes-windows-lite.exe`：无 GUI，可在 **Windows 7 SP1 x64** 上启动；
- 保留日常用户需要的命令：`install` / `update` / `self-update` / `config` /
  `setup`（双击安装）/ `init` / `help`；
- 安装、下载、校验、命令入口、用户 PATH 等行为与完整版一致；
- 体积与启动开销低于完整版，适配低端电脑。

**非目标**

- 不支持 `publish` / `test` 的在线部分（开发者功能，依赖 S3 写/列举）；
- 不支持 `install --gui` / `update --gui`（Lite 无 GUI）；
- 不要求 Lite 与完整版共享同一份依赖版本；
- 不把 Lite 当作 Win7 图形安装器（无 GUI）。

## 3. 方案对比

### 方案 A：降级 AWS SDK，保留现有代码

- 锁定 2024 年中版本：`aws-sdk-s3 1.46.0`（MSRV 1.76）、`aws-sdk-sso 1.39.0`、
  `aws-sdk-ssooidc 1.40.0`、`aws-sdk-sts 1.39.0`，以及 `aws-config 1.5.x`、
  `aws-smithy-*` 2024 年版本（多数未声明 `rust-version`，需实测）；
- 代码改动小（主要是 `Cargo.toml` 与少量 API 适配）；
- 风险：20+ 个 `aws-*` crate 需逐个 pin，旧版无安全更新；`aws-lc-rs` 的 C 代码
  在 Win7 上是否可用需实测。

### 方案 B：Lite 裁剪 + 轻量只读 S3 客户端（推荐）

- 新增 `lite` feature：不编译 `publish` / `test` 在线部分 / GUI；
- 用 `reqwest` + 自实现 SigV4（只读：`GET` / `HEAD`）替代 `aws-sdk-s3`，
  彻底移除 `aws-*` 依赖（约 20+ 个 crate）；
- 工具链锁 rustc 1.77.0，依赖统一降到 2024 年中的兼容版本；
- `windows` crate 改为 `windows-sys 0.52`（MSRV 1.56，不再导入 combase/WinRT）；
- 工作量更大，但依赖更小、Win7 行为更可控，也顺带降低了完整版的构建复杂度。

> 推荐 B。若只想“尽快出可用的 Win7 版本”，可先按 B 做只读能力（不实现 publish）。

## 4. 修改清单

### 4.1 工具链与构建

| 项 | 修改 |
| --- | --- |
| rustc | Lite 构建使用 `1.77.0`（最后官方支持 Win7 的版本）；完整版仍用 stable |
| Cargo.lock | 为 Lite 生成 v3 lock（或在 workspace 中维护独立的 `lite` 锁文件） |
| `Cargo.toml` | 新增 `lite` feature；`aws-*` / `slint` / `rfd` / `slint-build` 改为可选或 feature 依赖 |
| `build.rs` | 1.77 无法编译 `slint-build 1.16`（MSRV 1.92）。需要二选一：<br>① 把 GUI 移到独立 workspace crate（`dytes-gui`），主 crate 不再有 slint 构建依赖；<br>② 预生成 Slint Rust 代码并提交，`build.rs` 只处理 `UPYUN_*` |
| CI | 新增 `build-windows-lite-win7` job：`toolchain: 1.77.0`，`--no-default-features --features lite`，产物 `dytes-windows-lite-win7.exe`；增加导入表白名单检查 |

### 4.2 依赖版本矩阵（候选，需 spike 验证）

| crate | 当前 | Lite 候选 | 备注 |
| --- | --- | --- | --- |
| aws-sdk-s3 / aws-config / aws-credential-types / aws-smithy-* | 1.15x / 1.13 | **移除** | 方案 B；方案 A 需 pin 2024-06 版本集 |
| slint / slint-build / rfd | 1.18 / 1.16 / 0.17 | **移除** | Lite 无 GUI |
| reqwest | 0.13.4（MSRV 1.85） | 0.12.x（MSRV 1.63） | 启用 `rustls-tls` + ring |
| tokio | 1.52.3 | 1.38/1.39（MSRV 1.63） | `rt-multi-thread`/`macros`/`fs` |
| windows | 0.62.2（MSRV 1.82） | `windows-sys 0.52`（MSRV 1.56） | 只保留注册表 / Console / ToolHelp / MessageBox |
| clap | 4.5.x | 4.5.61（MSRV 1.74） | |
| chrono | 0.4.45（MSRV 1.62） | 0.4.38 | |
| uuid | 1.x（MSRV 1.89） | 1.8（MSRV 1.60） | |
| toml | 0.8 | 0.8.14（MSRV 1.70） | |
| libloading | 0.8 | 0.8.9（MSRV 1.71） | |
| rpassword | 7.x（MSRV 1.85） | 7.3 | |
| sha2 | 0.10 | 0.10.8（MSRV 1.41） | |
| futures-util | 0.3.33 | 0.3.31（MSRV 1.56） | |
| anyhow / serde / serde_json | 最新 | anyhow 1.0.86 / serde 1.0.203 / serde_json 1.0.117 | |
| rustls / TLS | 0.23 + aws-lc-rs | rustls 0.23 + **ring** | aws-lc-sys 的 C 代码 Win7 兼容性未知，ring 更稳 |

### 4.3 代码改动

| 文件 / 模块 | 修改 | 规模 |
| --- | --- | --- |
| `src/utils/s3.rs`、`src/update/loader.rs`、`src/publish/loader.rs` | 抽出存储接口：`get` / `head`（Lite）与 `put` / `list`（完整版）；新增 SigV4 实现 | 新增约 300–500 行，改 4 个文件 |
| `src/publish/*` | `#[cfg(not(feature = "lite"))]` 整体排除 | 3 个文件 |
| `src/test.rs` 在线部分 | Lite 下禁用（保留本地自检） | 1 个文件 |
| `src/main.rs` | `lite` 下不注册 `publish` / `test` | 少量 |
| `src/utils/desktop.rs`、`src/update/update_setup.rs`、`src/setup/mod.rs` | `windows` → `windows-sys` 重写（Registry / Console / ToolHelp / MessageBox / 快捷方式） | 3 个文件，约 150 行 |
| `src/utils/s3.rs:24`、`src/utils/plugin_manifest.rs:723/727/910` | `is_none_or`（Rust 1.82）改为 `map_or(true, ...)` | 4 处 |
| `build.rs` | 移除 `slint-build` 依赖或 GUI 拆分 | 见 4.1 |
| `Cargo.toml` | feature / optional 依赖 / target 依赖调整 | 约 35 个直接依赖中的一半 |

已扫描的、确认兼容 1.77 的用法：`let-else`（1.65）、`is_some_and`（1.70）、
`div_ceil`（1.73）。需要改的只有 `is_none_or` 4 处。

### 4.4 发布与文档

| 项 | 修改 |
| --- | --- |
| `release.yml` | Lite job 拆成两档：常规 Lite（stable，无 GUI）与 Win7 Lite（1.77，feature `lite`）；Win7 产物单独命名与路径 |
| 导入表门禁 | 用 `dumpbin /imports` 或 Python 解析 PE，断言不含 `ProcessPrng` / `WaitOnAddress` / `GetSystemTimePreciseAsFileTime` / `combase.dll` / `winrt-error` |
| 文档 | README 系统要求、packaging、setup、`docs/requirements` 增加 Win7 Lite 的适用范围与限制 |
| Release 说明 | 标注 Win7 Lite 不支持 publish / test 在线 / GUI |

## 5. 验证计划

1. `cargo +1.77.0 build --release --no-default-features --features lite` 在 Windows 上通过；
2. 解析产物导入表，确认上述 6 类符号全部消失；
3. Windows 7 SP1 x64 实机：
   - `dytes-lite.exe --version` / `--help`；
   - `dytes setup --offline --dir D:\dytes --no-path`（双击安装可另测）；
   - `dytes install core cyctron --offline`（本地包）；
   - `dytes update --check`（在线，需网络）；
4. Windows 10 上 Lite 与完整版行为一致（install/update/self-update）；
5. 回归完整版（stable + GUI）不退化。

## 6. 工作量估算

| 阶段 | 内容 | 估算 |
| --- | --- | --- |
| S0 spike | 用 rustc 1.77 + windows-sys + 最小命令集验证可编译、导入表干净 | 1 天 |
| S1 依赖与工具链 | 依赖降级、lock v3、feature 裁剪、`is_none_or` 等适配 | 1 天 |
| S2 GUI 解耦 | 拆分 `dytes-gui` 或预生成 Slint 代码，Lite 构建不再需要 `slint-build` | 0.5–1 天 |
| S3 存储层 | SigV4 只读客户端 + 现有 S3 调用适配 + 重试 / 错误分类 | 1.5–2 天 |
| S4 Windows 层 | `windows` → `windows-sys`、setup / 快捷方式 / 控制台 | 0.5–1 天 |
| S5 CI 与验证 | 新 job、导入表门禁、Win7 实机回归、文档 | 1 天 |
| **合计** | | **约 5–7 个工作日**（不含 Win7 实机环境准备） |

## 7. 风险与维护成本

- **旧工具链**：rustc 1.77 无后续安全修复，Lite 也拿不到新语言/标准库改进；
- **旧依赖**：`reqwest 0.12`、`tokio 1.38` 等需要单独维护安全更新；
- **TLS**：aws-lc-sys 的 Win7 兼容性未验证，建议 rustls + ring；ring 的 asm 在
  32 位 / 老 CPU 上的行为需实测；
- **功能分叉**：Lite 不支持 publish，文档与错误提示必须明确，避免用户误用；
- **测试成本**：Win7 只能靠虚拟机 / 实机回归，CI 无法覆盖运行时行为；
- **长期建议**：若 Win7 用户量小，可只提供“Win7 提示 stub + 完整版”的组合；
  Lite 的维护成本应在立项前确认。

## 8. 决策点

| 编号 | 决策 | 建议 |
| --- | --- | --- |
| W1 | 是否接受 Lite 不支持 `publish` / `test` 在线部分 | 接受（普通用户不需要） |
| W2 | GUI 构建依赖如何解耦：拆 `dytes-gui` crate 还是预生成 Slint 代码 | 预生成改动更小；拆 crate 更干净 |
| W3 | TLS 用 ring 还是继续 aws-lc-rs | ring（Win7 风险低） |
| W4 | Lite 是否用 rustc 1.77 独立构建，还是把 Win7 作为完整版的长期分支 | 独立构建，避免拖累主版本 |
| W5 | 是否先做 S0 spike 再决定 | 建议先做 spike |

## 9. S0 spike 结果（2026-11-28）

分支：`spike/win7-lite`（提交 `d1effa7`），workflow `Win7 Lite spike`
（[run 46115877820](https://github.com/dytescn/dytes/actions/runs/46115877820)）。
两个独立 crate 在 Windows runner 上构建，并用 `dumpbin /imports` 检查
`ProcessPrng` / `WaitOnAddress` / `WakeByAddressSingle` /
`GetSystemTimePreciseAsFileTime` / `RoOriginateErrorW` / `combase.dll`。

| crate | 依赖 | rustc 1.77.0 | 导入表 |
| --- | --- | --- | --- |
| `spike/win7-min` | `windows-sys 0.52` + clap / serde / chrono / uuid / toml / sha2 / anyhow / dirs / semver / libloading / futures-util / rpassword | 构建成功 | **WIN7_CLEAN** |
| `spike/win7-http` | `reqwest 0.12.5` + `tokio 1.38.1` + native-tls（Windows SChannel） | 构建成功 | **WIN7_CLEAN** |

`rustc 1.76.0` 的两个 job 失败，原因是 crate 声明了 `rust-version = "1.77"`，
不是依赖不兼容；1.77 是官方最后支持 Win7 的稳定版，已满足要求。

### 9.1 验证到的降级经验（实现时必须遵守）

1. `Cargo.lock` 必须是 **version 3**：1.77 的 Cargo 无法解析 v4；
2. 2026 年的传递依赖会引入 `edition2024`（例如 `indexmap 2.14.2` 直接报
   “feature `edition2024` is required”）；
3. 用 MSRV-aware resolver 生成 lock：
   `CARGO_RESOLVER_INCOMPATIBLE_RUST_VERSIONS=fallback cargo +nightly generate-lockfile`，
   可把大量依赖自动降到 1.77 兼容版本（生成的 lock 为 v3）；
4. `toml 0.8.14 / 0.8.19` 与新版 `toml_edit 0.22.27` 不兼容
   （`E0599: no method named serialize_field`），需
   `cargo update -p toml_edit --precise 0.22.20`；
5. `native-tls`（Windows SChannel）在 Win7 导入表中干净，可作为 Lite 的 TLS 后端；
   完整版继续使用现有 rustls/aws-lc 不受影响。

### 9.2 剩余阻塞

| 阻塞 | 说明 | 解决 |
| --- | --- | --- |
| `slint-build` 是无条件 build-dependency | 即使不启用 `gui`，1.77 构建 Lite 时仍会编译 `slint-build 1.16`（MSRV 1.92） | 预生成 Slint 代码并提交，或拆 `dytes-gui` crate（第 4.1 节） |
| 现有代码适配 | `is_none_or` 4 处；`windows` crate → `windows-sys` 3 个文件；`lite` feature 裁剪 | 第 4.3 节 |
| S3 只读客户端 | Lite 需要 `GET` / `HEAD` + SigV4，替代 `aws-sdk-s3` | 第 4.3 节，新增约 300–500 行 |
| Win7 实机验证 | CI 只验证导入表，不验证运行 | Windows 7 SP1 x64 虚拟机 / 实机 |

### 9.3 结论

**工具链与依赖层已证明可行**：1.77 + `windows-sys` + `reqwest/tokio/native-tls`
可以生成不导入 Win8/10-only API 的 PE。剩余工作是 GUI 构建解耦、现有代码适配、
S3 只读客户端与实机回归，估算仍需 **4–6 个工作日**（S0 已完成）。
