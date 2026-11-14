# `dytes init`

## 1. 职责

在当前目录（或 `--dir`）生成一份新的 `dytes.json`。根据包类型写入相应的
`exports` / `platform-exports`、`init`、设计插件字段，并自动生成 `uuid`。

## 2. 用法

```bash
dytes init [--type <type>] [--name <name>] [--version <ver>]
           [--dir <path>] [--no-exports] [--force] [--json]
```

| 参数 | 默认值 | 说明 | 当前状态 |
| --- | --- | --- | --- |
| `--type <type>` | `app` | `app` / `core` / `sys` / `web` / `design_plugins` / `design_soft` / `model` | 已实现 |
| `--name <name>` | 目录名 | 包名；目录名无法推断时必须显式指定 | 已实现 |
| `--version <ver>` | `0.1.0` | 初始版本 | 已实现 |
| `--dir <path>` | 当前目录 | 生成位置 | 已实现 |
| `--no-exports` | 否 | 生成不含导出的最小清单 | 已实现 |
| `--force` | 否 | 覆盖已存在清单 | 已实现 |
| `--json` | 否 | JSON 输出 | 已实现 |

## 3. 执行流程

1. 解析包类型；未知类型返回 `2`。
2. 确定目录与包名；目录名无法推断且未提供 `--name` 时返回 `2`。
3. 目标文件 `<dir>/dytes.json` 已存在且未 `--force` 时返回 `3`。
4. 创建目录，按类型生成模板：
   - 写入 `name`、`uuid`、`version`、`description`、`author`、`type`；
   - 设计插件额外写入 `plugins_type`、`targets`、`use_ai`、`mcp`；
   - `--no-exports` 时不写 `exports` / `platform-exports`。
5. 写入格式化 JSON，输出路径与 UUID。

## 4. 类型模板

| type | 模板要点 |
| --- | --- |
| `app` / `core` | `exports` 指向 `target/release/<name>.exe` |
| `sys` | `platform-exports.windows` → `libary/<name>.dll` |
| `web` | `exports` 包含资源与 `setup.json`，`init: ["./setup.json"]` |
| `design_plugins` | `plugins_type=plugins`、`targets=["common"]`、`use_ai=false`、`mcp=mcp.json`，导出 DLL 与 `mcp.json` |
| `design_soft` | 单文件 exe |
| `model` | `exports` 指向 `<name>.gguf` |

## 5. 输出

```text
已生成 D:\pkg\dytes.json
包 UUID: 8b2cd615-eacf-43fb-a2c9-d6f0a8195946
下一步:dytes publish --dry-run
```

`--json`：

```json
{
  "path": "D:\\pkg\\dytes.json",
  "type": "sys",
  "name": "info_lib",
  "uuid": "8b2cd615-eacf-43fb-a2c9-d6f0a8195946",
  "version": "0.1.0"
}
```

## 6. 退出码

| 退出码 | 场景 |
| --- | --- |
| `0` | 生成成功 |
| `1` | 文件写入失败 |
| `2` | 未知类型、无法推断包名 |
| `3` | 清单已存在且未指定 `--force` |

## 7. 边界与约束

- 不访问网络；
- `uuid` 每次生成新值，重复 `init --force` 会更换 UUID；
- 模板只提供占位路径，不创建被导出的文件；
- 生成后建议依次执行 `dytes check`、`dytes test`、`dytes publish --dry-run`。

## 8. 测试点

- 各类型模板均可被 `DytesConfig::validate` 接受；
- 默认包名取目录名；
- 已存在清单拒绝覆盖，`--force` 覆盖；
- `--no-exports` 生成最小清单；
- `--json` 字段完整。

对应测试文件：`tests/cli_init.rs`；模板测试在 `src/utils/templates.rs`。

## 9. 后续规划

1. 支持从已有包复制模板（`--from <path>`）；
2. 根据平台自动生成多平台 `platform-exports`；
3. 生成时提示缺失的源文件；
4. 与 `dytes test` 串联为 `init && test` 的脚手架命令。
