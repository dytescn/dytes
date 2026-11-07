# 0.1.20

- `install plugin` 触发 `plugin_lib` 扫描时，`FFI-Symbol` 从 `post_scan` 改为 `scan`，
  避免 cyctron 拼成 `post_post_scan` 返回 404（#16）。
- `setup.json` 请求中的 `FFI-Symbol` 统一去掉 `post_` 前缀，兼容旧包误写
  `post_scan` / `post_post_scan` 的情况。
- 新增 `normalize_ffi_symbol` 单元测试。
