# 构建产物与系统依赖

> 目的：记录当前打包命令、产物在各平台依赖的系统组件，以及“换一台电脑就跑不起来”
> 的已知原因与改造方案。
> 相关记录：Windows 旧系统启动失败见 [issues.md](issues.md) I-22；
> Windows 双击安装的需求见 [requirements/windows-installer.md](requirements/windows-installer.md)。

## 1. 当前打包命令

发布入口是 [`.github/workflows/release.yml`](../.github/workflows/release.yml)，由
`v*` tag 或手动触发，共 4 个构建：

| job | runner | 命令 | 产物 |
| --- | --- | --- | --- |
| `build`（Windows） | `windows-latest`（MSVC） | `cargo build --release --locked --features gui` | `dytes-windows.exe`（上传为 `dytes.exe`） |
| `build`（Linux） | `ubuntu-22.04` | 同上 | `dytes-linux.bin`（上传为 `dytes.bin`） |
| `build`（macOS） | `macos-latest` | 同上 | `dytes-macos.bin`（上传为 `dytes.app`） |
| `build-action`（Linux） | `ubuntu-22.04` | `cargo build --release --locked`（**无 GUI**） | `dytes-action.bin` |

三个 GUI 构建在编译前安装 Unix 依赖（Linux 为 `cmake ninja-build pkg-config`、
`libxkbcommon-dev`、X11 / Wayland、EGL / GL、`libfontconfig1-dev`、
`libfreetype6-dev`；macOS 为 `cmake ninja pkg-config`），仅用于**编译**。

发布前的校验：

- `preflight`：`dytes.json` / `Cargo.toml` / `Cargo.lock` 版本一致、tag 匹配、
  不低于线上版本；
- `Verify builtin S3 credentials`：确认构建期 `UPYUN_API_KEY` / `UPYUN_API_SECRET`
  真的写进了二进制（`build.rs` → `rustc-env`）；
- `Verify glibc baseline`：`readelf --version-info` 断言 Linux 产物不超过
  `GLIBC_2.35`；
- `Test library`：`cargo test --lib --locked`。

产物分发：`core/dytes/<version>/<platform>/`（版本化）、
`core/dytes/latest/<platform>/`（别名）、`core/dytes/dytes.json`（清单）、
`latest/version.js`（官网），随后刷新 CDN（见 `purge-cache.yml`）。

## 2. 各平台运行时依赖

“运行时依赖”指用户机器上必须预先存在、dytes 自己不携带的东西。

### 2.1 Linux

| 依赖 | 来源 | 说明 |
| --- | --- | --- |
| glibc ≥ 2.35 | 系统 | 在 Ubuntu 22.04 上编译；CI 已用 GLIBC 符号上限挡住构建机漂移 |
| X11 或 Wayland | 系统 | `winit` 通过 `x11-dl` / `wayland-client` **运行时加载**，无图形会话时 GUI 不可用 |
| `libxkbcommon` | 系统 | 键盘映射；缺失时 GUI 启动失败 |
| EGL / GL | 系统 | Slint FemtoVG 渲染后端（`glutin` / `glow`） |
| `libfontconfig.so.1` | 系统 | 字体枚举（`yeslogic-fontconfig-sys`，链接期绑定） |
| XDG Desktop Portal（DBus） | 系统 | `--gui` 的“选择目录”对话框（`rfd`），无 portal 时仅影响交互选择 |
| OpenSSL | 不需要 | HTTP / S3 走 `rustls`，产物不依赖 `libssl` |

把安装目录放在 exFAT / FAT32 移动硬盘时另有符号链接限制，已在 I-20 处理
（回退为 shell wrapper），不属于依赖问题。

### 2.2 Windows

| 依赖 | 来源 | 说明 |
| --- | --- | --- |
| VC++ 运行库（`vcruntime140.dll` 等） | 系统 / 需单独安装 | MSVC 目标默认**动态链接 CRT**（`rustc --print cfg --target x86_64-pc-windows-msvc` 无 `crt-static`），未装 VC++ 2015–2022 运行库的机器会在启动时报缺少 DLL。**待用 `dumpbin /dependents` 核实实际导入表** |
| Windows 10 及以上 | 系统 | 产物的 PE 导入表含 Win10-only API：Slint 的 `SystemParametersInfoForDpi`，以及 Rust std / 依赖的 `ProcessPrng`、`WaitOnAddress`、`combase` / WinRT 等（Rust 1.78 起官方最低支持 Windows 10）。旧系统在加载器阶段以 `0xC0000139` 退出，装运行库无效；无 GUI 构建同样无法绕过（见 I-22 / GitHub #37） |
| WebView2 | 不需要 | `wry` 不在依赖树中，init 脚本不依赖 WebView |
| 其它 DLL | 不需要 | `windows` crate 只链接系统 DLL |

Windows 是“换电脑不能运行”反馈最集中的平台，且两种原因（缺 VC++ 运行库、
系统过旧）报错都是系统弹窗，用户难以区分。

### 2.3 macOS

| 依赖 | 来源 | 说明 |
| --- | --- | --- |
| universal2（arm64 + x86_64） | — | 在 `macos-latest`（Apple Silicon）原生构建 arm64，再交叉构建 x86_64，`lipo` 合并；Intel Mac 与 Apple Silicon 均可运行。`macos-14` 已于 2026-11 退役，不要固定该标签 |
| macOS 11+ | 系统 | 两个架构统一 `MACOSX_DEPLOYMENT_TARGET=11.0`，CI 用 `otool -l` 的 `LC_BUILD_VERSION.minos` 校验 |
| 有效签名 / 公证 | — | 当前未签名、未公证，首次打开会被 Gatekeeper 拦截（“无法验证开发者”） |
| 动态库 | 不需要 | 仅链接系统框架 |

## 3. 风险清单

| 编号 | 风险 | 触发条件 | 用户看到的现象 | 参考 |
| --- | --- | --- | --- | --- |
| W1 | 缺 VC++ 运行库 | 干净安装的 Windows / 精简系统 | 缺少 `vcruntime140.dll` | 本页 2.2 |
| W2 | 系统版本过低 | Windows 7 / 8 / 8.1 / 10 早期版本 | 无法定位程序输入点 `SystemParametersInfoForDpi` / `ProcessPrng`，进程以 `0xC0000139` 退出；装运行库无效 | I-22 |
| W3 | 产物未签名 | 任意 Windows 下载 | SmartScreen“Windows 已保护你的电脑” | 本页 2.2 |
| M1 | ~~Intel Mac 不支持~~（已修复） | ~~x86_64 macOS~~ | 已改为 universal2，两个架构均可运行 | 本页 2.3 |
| M2 | 未签名 / 未公证 | 任意 macOS 下载 | “无法验证开发者”，需右键打开 | 本页 2.3 |
| L1 | glibc 过低 | CentOS 7 / 旧发行版（glibc < 2.35） | `GLIBC_2.35 not found` | 本页 2.1 |
| L2 | 缺少图形 / 字体库 | 无桌面会话、精简容器 | GUI 启动失败；CLI 可用 | 本页 2.1 |
| R1 | runner 漂移 | GitHub 更新 `*-latest` 镜像 | 新版本突然要求更高 glibc / 更新 Windows SDK | 第 5 节 |

历史教训：I-05（GitHub #20 / #22）记录过构建机升级把 glibc 基线抬到 2.43 导致产物不可运行，
现在用 `ubuntu-22.04` + `readelf` 门禁兜住；同类问题在 Windows SDK 与 macOS
最低版本上还没有门禁。

## 4. 降低系统依赖的改造方案

按“收益 / 成本”排序，均以 CI 可验证为前提。

### P0：Windows 静态 CRT（去掉 VC++ 运行库依赖）——已实施

- `.cargo/config.toml` 的 `[target.x86_64-pc-windows-msvc]` 设置
  `rustflags = ["-C", "target-feature=+crt-static"]`；
- 发布流程新增 `Verify static CRT`（`release.yml`，仅 Windows）：用 `vswhere`
  定位 `dumpbin`，断言 `upload/dytes.exe` 的依赖列表不含
  `VCRUNTIME` / `MSVCP` / `MSVCR` / `api-ms-win-crt`，并记录
  `SystemParametersInfoForDpi` 导入（不阻断，见 I-22）；
- 静态 CRT 要求所有 C 依赖一起用 `/MT`：`aws-lc-sys` 会随
  `target_feature=crt-static` 切换，仍需 CI 实测（`slint` / `glutin` 为纯 Rust）。

### P0：Windows 旧系统兼容（对应 I-22）

| 步骤 | 内容 |
| --- | --- |
| 1 | 明确最低支持版本：**Windows 10**（评论记录 `SystemParametersInfoForDpi` 最低 1703，先声明 Win10，实测后再决定是否写具体 build），写入 README / 官网下载页 / 安装脚本；脚本用 `[Environment]::OSVersion` 在下载前提示。**不要指望程序自身检测**：加载器阶段就会失败，Rust 代码不会执行 |
| 2 | ~~新增无 GUI 的 `dytes-windows-legacy.exe`~~：**已确认不可行**——Win10-only API 也在 Rust std / 底层依赖里，与 `gui` feature 无关；要支持 Win7 必须用 rustc ≤ 1.77 全量重建并规避 WinRT，成本高，当前不做 |
| 3 | 不采用 patch Slint 的方案（同样无法覆盖 std 层的 `ProcessPrng` 等） |

### P0：macOS 架构与签名——架构部分已实施

- 发布流程在 `macos-latest` 上原生构建 arm64、交叉构建 `x86_64-apple-darwin`，
  再用 `lipo -create` 合并为 universal2；`dytes.json` / 下载路径不变，
  仍是单个 `macos/dytes.app`；
- 新增 `Verify macOS universal binary` 门禁：`lipo -archs` 必须同时包含
  `arm64` 与 `x86_64`，`LC_BUILD_VERSION.minos` 不得高于 11.0；
- 签名 + 公证（`codesign` / `notarytool`）需要 Apple Developer 账号；
  在拿到之前，至少在官网给出 `xattr -dr com.apple.quarantine` 的说明。

### P1：Linux 基线

- 短期：保持 `ubuntu-22.04` + `GLIBC_2.35` 门禁；
- 中期：若需支持更旧发行版，改用 `manylinux_2_28`（glibc 2.28）容器构建，
  并把门禁同步下调；
- 增加 `ldd` 白名单检查，防止未声明的 `libssl` / `libgtk` 之类依赖混入。

### P1：统一产物依赖门禁

在发布流程里加一个 `verify-runtime-deps` job，逐平台断言：

```bash
# Linux
ldd dytes-linux.bin | grep -E 'libssl|libgtk|libwebkit' && exit 1
readelf --version-info dytes-linux.bin | grep -oE 'GLIBC_[0-9.]+' | sort -V | tail -1
# Windows（在 windows runner 上）
dumpbin /imports dytes-windows.exe | grep -iE 'VCRUNTIME|MSVCP|SystemParametersInfoForDpi'
# macOS
otool -L dytes-macos.bin && otool -l dytes-macos.bin | grep -A3 LC_BUILD_VERSION
```

### P2：固定 runner 版本

把 `windows-latest` / `macos-latest` 改为明确的 `windows-2022` 等受支持标签
（或团队同意的版本），升级时作为一次显式改动验证，避免今天能跑、明天基线变高。
**注意标签有生命周期**：`macos-14` 已于 2026-11 退役（`configuration not supported`），
固定前先确认 runner-images 仍提供该标签。

## 5. 本地验证

```bash
# Linux（当前环境即可）
cargo build --release --features gui
ldd target/release/dytes
readelf --version-info target/release/dytes | grep -oE 'GLIBC_[0-9.]+' | sort -V -u | tail -3

# Windows（PowerShell）
dumpbin /dependents target\release\dytes.exe
dumpbin /imports target\release\dytes.exe | findstr /i "SystemParametersInfoForDpi VCRUNTIME MSVCP"

# macOS
otool -L target/release/dytes
otool -l target/release/dytes | grep -A4 LC_BUILD_VERSION
```

## 6. 建议执行顺序

1. ~~先做 Windows 静态 CRT + `dumpbin` 门禁~~（已实施，待 CI 验证）；
2. ~~补 macOS x86_64 / universal2 与最低版本声明~~（已实施，待 CI 验证）；
3. 明确 Windows 最低系统版本：README / 下载页 / 安装脚本声明并提供提示（I-22 方案 A）；
4. 旧系统不做 legacy 产物，也不在程序内做版本检测（加载器阶段失败，见 I-22）；
5. 最后再评估更低 glibc 基线与签名 / 公证。
