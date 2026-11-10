# dytes 0.1.15 版本更新

## Linux / macOS 可执行权限

- 安装 `type: core` 包（`cyctron` 等）后，对非文本导出设置 `0755`；
- 覆盖普通 core 下载与延迟替换路径；
- `setup.ts` / `mcp.md` / `*.so` / `*.dll` / `*.dylib` 不会被误设为可执行；
- 启动 cyctron 的错误文案按平台显示 `cyctron` / `cyctron.exe`；
- 对**已安装且版本最新**的 core 包也会补设置执行位，修复 0.1.14 之前安装的旧文件；
- 启动 cyctron 前再兜底设置一次执行位，无需重新下载。

## 背景

S3 对象不保留文件权限，`dytes` 下载落盘后默认是 `0644`；Linux 上
`Command::new("./cyctron")` 会因 `Permission denied` 失败。
`dytes self-update` 已有执行位处理，本次将同样的处理扩展到 core 包安装。
