# dytes Lite（Windows 7 兼容方向）

本目录是从主项目分叉出的 **无 GUI** 版本，用于验证 Windows 7 兼容构建。
独立 `Cargo.toml` / `Cargo.lock`，通过 `lite-build.yml` 在 Windows runner 上
使用 [`Fenex/setup-rust-win7@v1`](https://github.com/Fenex/setup-rust-win7)
安装 `1.99.0-win7.1` 自定义工具链（std 打过 Win7 兼容补丁）构建。

## 与主版本差异

| 项 | 主版本 | Lite |
| --- | --- | --- |
| GUI | `gui` feature（Slint / rfd） | 移除；`--gui` 参数已删除 |
| `build.rs` | 编译 `ui/*.slint` | 只注入 `UPYUN_*` 环境变量 |
| Windows API | `windows` 0.62 | `windows-sys` 0.52（不再链接 combase/WinRT） |
| 版本 / 发布 | `dytes-windows.exe` | `dytes-windows-lite.exe`（构建中） |

## 当前状态（2026-11-28）

- `cargo build --release` 在 `windows-latest` + `1.99.0-win7.1` 上成功；
- 产物 **8,841,216 字节（约 8.4 MB）**（完整版约 32 MB）；
- `dumpbin /imports`：`ProcessPrng` / `WaitOnAddress` / `WakeByAddressSingle` /
  `GetSystemTimePreciseAsFileTime` / `RoOriginateErrorW` /
  `SystemParametersInfoForDpi` **全部消失**；
- `combase.dll` **已消失**（latest run：`combase=False`）。

达成方式：

| 问题 | 处理 |
| --- | --- |
| Rust std 的 Win10 API | `Fenex/setup-rust-win7` 自定义工具链（1.99.0-win7.1） |
| Slint 的 `SystemParametersInfoForDpi` | 删除 GUI 与 slint/rfd/slint-build |
| `windows` crate 的 combase/WinRT | 改为 `windows-sys 0.52` |
| `dirs-sys` 的 `CoTaskMemFree` | 用 `utils/platform_dirs.rs`（环境变量）替换 `dirs` |
| `windows-core` 的 combase 导出 | `lite/vendor/windows-core` 补丁：`CoTaskMemAlloc` / `CoTaskMemFree` / `CoCreateFreeThreadedMarshaler` → `ole32.dll` |
| AWS SDK 的 hyper-util/windows-registry | 移除 `aws-sdk-s3`，改用 `utils/s3.rs` 轻量只读客户端（SigV4） |

## 命令范围

Lite 只保留：`install` / `update` / `self-update` / `config` / `setup` / `init` / `help`。
不支持 `publish` / `check` / `test`（在线部分）与 `--gui`。

## 下一步

1. **Windows 7 SP1 实机验证**：`--version` / `setup --offline` / `install` / `update --check`；
2. 若实机仍缺符号，用 `dumpbin /imports` 对照 Win7 导出表继续收敛；
3. 决定 Lite 的发布路径（GitHub Release + S3 版本化目录，不进 `latest` 清单）。

## 构建

```bash
cd lite
cargo build --release --locked
```

CI：`.github/workflows/lite-build.yml`（push `spike/win7-lite` 或手动触发）。

> 分叉目录的代价：主版本 `src/` 变更需要手动同步到 `lite/src/`；合并前请对照
> `git log --oneline main -- src`。
