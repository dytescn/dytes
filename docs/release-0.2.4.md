# 0.2.4

## 设计插件 MCP 清单校验对齐官方字段

`dytes check` / `dytes test` 对 `design_plugins` 的 MCP 校验扩展为一套与官方
MCP Tool 定义对齐的规则。新规则先以 `WARN` 输出，不改变退出码，也不阻断
`publish`，便于存量包逐步迁移。

新增警告项：

- 工具名长度（≤ 128）、字符集（`A-Z a-z 0-9 _ - .`）与同一 `mcp.json` 内唯一；
- `title` / `description` 类型与长度（建议 120 / 500 字符）；
- `inputSchema` 存在、根类型为 `object`、`$schema` 方言受支持；
  无参工具建议 `additionalProperties: false`；
- `outputSchema` 为对象且方言受支持；
- `annotations` 为对象，`title` 与四个布尔 hint 的类型；
- `transport.library` / `library_uuid` 非空，`library` 已在
  `dependencies-sys` 声明；
- 引用形态的工具描述文件可读、可解析且顶层为对象；
- `mcp.json` 顶层仅认可 `tools` 与预留字段 `schemaVersion`，未知字段告警。

内联对象的 `name` 非空、`ffi_symbol` 非空、`inputSchema` /
`outputSchema` 类型错误仍按失败处理，退出码 `3` 不变。

## JSON 输出增加 `severity`

`dytes check --json` 与 `dytes test --json` 的每条 `checks` 新增 `severity`：

- `pass` / `warning` / `error`；
- `passed` 只由 `error` 决定，警告不影响退出码与发布。

文本输出新增 `WARN` 行与「（N 条警告）」汇总。

## 安装期能力库校验

`verify_mcp_libraries` 同时支持字符串引用与内联对象：

- 读取或解析失败、`transport.library` / `library_uuid` 缺失时输出带工具名
  与索引的警告，不再静默跳过；
- 内联形态含 `transport` 时同样校验库文件与 `uuid`；
- 无 MCP transport 的普通本地插件仍然跳过。

## 优化

- 内联工具的失败标签带上 `tools[<i>]`，多工具清单更易定位；
- `MAX_TOOL_TITLE` 注释澄清为本地展示建议，非官方硬限制。

## 文档

- 新增 [mcp.md](mcp.md)：设计插件 MCP 文件布局、字段与校验规范（v1.1）；
- 新增 [mcp-official.md](mcp-official.md)：官方 MCP 规范（2026-07-28）摘要、
  差异与改造路线；
- 同步 `docs/commands/check.md` 的校验项、`WARN` 输出与 `--json` 示例；
- README 与 `docs/README.md` 增加两份 MCP 文档入口。

## 测试

- 新增 `tests/mcp_manifest.rs`：名称规则、schema/方言、未声明依赖、
  对齐清单零警告、内联 schema 类型失败；
- `plugin_manifest.rs` 增加内联对象工具通过用例。

## 升级

```bash
dytes self-update
```
