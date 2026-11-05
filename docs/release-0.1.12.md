# dytes 0.1.12 版本更新

## 新增

- `dytes check [DIR]`：校验 `dytes.json`；`design_plugins` 执行完整清单规则，
  其它类型检查当前平台导出文件是否存在。
- `dytes publish` 默认对 `design_plugins` 执行清单校验，失败即中止；
  紧急情况可用 `--skip-check` 跳过。
- `dytes init --type design_plugins` 自动写入 `plugins_type` / `targets` /
  `use_ai` / `mcp` 四个必填字段。

## 修复

- GitHub Action 在 `args` 为空时，macOS 的 bash 3.2 会因 `extra[@]` 触发
  `unbound variable`；改为按数组长度选择调用方式。
- Action 下载 dytes 二进制时增加 curl 重试，避免 GitHub Releases 的
  504 直接导致发布失败。
- `current_platform` 统一使用 `shared::platform`，同时支持
  `DYTES_PLATFORM` 与 `PIKSEL_PLATFORM` 覆盖。
- S3 下载遇到 404 时明确报告“S3 对象不存在”，并输出底层错误链，
  不再只显示 `S3 HEAD 请求失败`。

## 验证

```bash
cargo test
```

覆盖清单校验、模板、依赖排序、self-update 选择与下载路径等 44 项测试。
