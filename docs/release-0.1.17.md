# dytes 0.1.17 版本更新

## init 脚本

- 有 `deno` / `bun` / `node` 时直接执行 `setup.ts`，覆盖函数封装、循环与
  try/catch；静态提取只作为无运行时环境的回退；
- 静态提取遇到未解析变量时直接报错，不再把 `symbol` / `body` 当字面量发送；
- HTTP 非 2xx 或响应 `code != 200/0` 时 init 失败，不再打印“执行成功”。

## 插件安装

- `dytes install plugin <name>` 下载完成后调用 `plugin_lib` 的 `post_scan`，
  登记 `plugins.db` 并触发 kv 资源导入；
- `plugin_lib` 自己调用 dytes 时设置 `DYTES_SKIP_PLUGIN_SCAN=1`，避免嵌套回调。
