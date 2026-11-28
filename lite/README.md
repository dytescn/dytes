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
- 产物约 **18.7 MB**（完整版约 32 MB）；
- `dumpbin /imports`：`ProcessPrng` / `WaitOnAddress` / `WakeByAddressSingle` /
  `GetSystemTimePreciseAsFileTime` / `RoOriginateErrorW` /
  `SystemParametersInfoForDpi` **全部消失**（rust-win7 工具链生效）；
- **仍导入 `combase.dll!CoTaskMemFree`**：来自 `aws-sdk-s3` →
  `aws-smithy-http-client` → `hyper-util` → `windows-registry` → `windows-strings`
  的 Windows 代理支持，Win7 上没有 `combase.dll`，因此**尚不能运行**。

## 下一步

1. 移除 `aws-sdk-s3` / `aws-config`（及 aws-smithy-*、hyper-util），用
   `reqwest 0.12` + 自实现 SigV4 只读客户端（`GET` / `HEAD`）替代；
   这是去掉 combase 与大幅缩小依赖的关键；
2. Windows 7 SP1 实机验证 `--version` / `setup --offline` / `install`；
3. 决定 Lite 的发布路径（GitHub Release + S3 版本化目录，不进 `latest` 清单）。

## 构建

```bash
cd lite
cargo build --release --locked
```

CI：`.github/workflows/lite-build.yml`（push `spike/win7-lite` 或手动触发）。

> 分叉目录的代价：主版本 `src/` 变更需要手动同步到 `lite/src/`；合并前请对照
> `git log --oneline main -- src`。
