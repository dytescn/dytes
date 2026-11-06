# dytes 0.1.18 版本更新

## init JSON

- `init` 支持 `setup.json`：`{ "requests": [...] }`；
- 每个请求可声明 `allow_error`：忽略幂等 DDL / 重复写入的 HTTP 或业务错误，
  传输失败仍会让 init 失败；
- JSON 请求直接发送到 cyctron，不再依赖 Deno / Node / WebView；
- 旧 `setup.ts` 仍保留兼容路径。
