# 测试环境规范

## 1. 测试目录

- 所有本地测试的临时文件统一放在仓库根目录的 `.dytes/` 下。
- `.dytes/` 已在 `.gitignore` 中屏蔽，不得提交任何测试产物。
- 每个测试使用独立子目录：`.dytes/run-<pid>/<test-name>/`，避免并发冲突。
- 测试结束保留现场便于排查；整个 `.dytes/` 可随时手动删除。

## 2. 目录结构

```text
.dytes/
└── run-<pid>/
    └── <test-name>/
        ├── work/        # 命令工作目录
        ├── cache/       # 下载缓存
        ├── logs/        # 输出与日志
        └── config.toml  # DYTES_CONFIG
```

## 3. 环境隔离

- `DYTES_CONFIG` 指向 `.dytes/.../config.toml`，禁止读写用户真实配置目录
  （`%APPDATA%\dytes`、`~/.config/dytes`）。
- 清除 `DYTES_S3_*`、`PIKSEL_PLATFORM` 等外部环境变量，避免本机环境影响结果。
- 平台相关用例固定 `DYTES_PLATFORM=windows`，或按用例显式指定。
- 默认不访问网络；需要真实 S3 / 网络的分支必须显式标记并跳过。

## 4. 测试代码

- 测试统一放在 `tests/`，不写入 `src/`（见 [code.md](code.md) 3.2）。
- 共享辅助代码放在 `tests/common/mod.rs`；`TestEnv` 负责创建并隔离 `.dytes` 目录。
- 测试通过 `CARGO_BIN_EXE_dytes` 调用真实二进制，验证退出码、stdout/stderr 与文件结果。

## 5. 没有 S3 配置时

本地 `cargo build` 默认**不包含**内置 S3 凭据（只有构建时提供 `UPYUN_API_KEY` /
`UPYUN_API_SECRET` 才会注入）。因此 `dytes config list` 显示
`s3.access_key` / `s3.secret_key = (未设置)` 时：

- `install` / `update` / `publish` / `self-update` 访问对象存储会失败；
- `packages.funxdata.com` 上的清单由发布流程更新，并可能受 CDN 缓存影响，
  不能保证立即拿到最新包。

处理方式：

1. 配置凭据后再测试：

   ```bash
   dytes config set s3.access_key
   dytes config set s3.secret_key
   ```

   或使用环境变量 `DYTES_S3_ACCESS_KEY` / `DYTES_S3_SECRET_KEY`。

2. 先确认线上版本，再判断问题：

   ```bash
   curl -fsSL "https://packages.funxdata.com/core/dytes/dytes.json?t=$(date +%s)"
   ```

   版本落后时先完成发布，不要用旧包验证新功能。

3. 只测本地逻辑时，使用 `.dytes/` 隔离目录与 `DYTES_CONFIG`，不依赖线上包。

## 6. 发布 → Action → 自更新 → 测试

需要基于最新发布版本测试时，按以下顺序执行：

1. 同步三处版本号（`dytes.json`、`Cargo.toml`、`Cargo.lock`），提交并打 tag：

   ```bash
   git tag vX.Y.Z
   git push origin main --tags
   ```

2. 等待 GitHub Actions 的 release workflow 完成，确认三平台产物和 `latest`
   清单已更新，`preflight` / `verify-release` 通过（`version.js` 仅供官网展示，
   `dytes self-update` 不读取它）。

3. 用当前 `dytes` 自更新到最新版并确认版本：

   ```bash
   dytes self-update --check
   dytes self-update --force
   dytes --version
   ```

4. 版本确认后再执行测试：

   ```bash
   dytes install --gui --dir .dytes/gui-test
   dytes update --check
   ```

5. 如果自更新仍拿到旧版本，优先检查 CDN 缓存（清单 URL 加时间戳）或发布是否
   成功，不要反复重装。

## 7. 后续补充

- 测试数据（fixtures）与清理策略；
- CI 中的 `.dytes` 缓存与保留期限；
- 需要凭据的集成测试标记方式。
