# `dytes check`

## 1. 职责

离线校验一个包的 `dytes.json` 与导出文件，不访问网络。根据包类型执行不同规则：

- 普通包：清单可解析、类型匹配、`test-dependency` 类型合法、当前平台导出非空、
  每个源文件存在；
- 设计插件：额外执行完整规则（身份、分类、目标、导出、MCP、skills 等）。

`check` 与 `test` 共用同一套校验实现（`publish::cli::evaluate_check`），
避免两个命令行为漂移。

## 2. 用法

```bash
dytes check [<dir>] [--type <type>] [--platform <platform>] [--manifest-only] [--json]
```

| 参数 | 说明 | 当前状态 |
| --- | --- | --- |
| `[DIR]` | 包目录，默认当前目录 | 已实现 |
| `--type <type>` | 只接受指定类型；不一致返回 `3` | 已实现 |
| `--platform <platform>` | 选择 `platform-exports`，默认当前平台 | 已实现 |
| `--manifest-only` | 允许没有 exports 的仅清单包 | 已实现 |
| `--json` | JSON 输出 | 已实现 |

## 3. 执行流程

1. 读取 `<dir>/dytes.json`；缺失或 JSON 非法返回 `3`。
2. 解析为 `DytesConfig`；`--manifest-only` 时允许没有导出。
3. `--type` 与清单 `type` 比较，不一致返回 `3`。
4. 逐项校验：
   - 普通包：
     - 当前平台导出表存在且非空；
     - 每个导出源文件存在；
   - 设计插件：
     - `name` / `version` / `type` 合法；
     - `plugins_type` 属于允许集合；
     - `targets` 非空、均为小写、无重复、取值合法、`common` 只能单独出现；
     - `use_ai` 为布尔值；
     - `exports` 目标只能是文件名，源文件存在；
     - `mcp.json` 存在、`tools` 为非空数组、工具已导出；
     - `use_ai=true` 时校验 `skills.json` 与技能导出。
5. 汇总输出；有失败项返回 `3`。

## 4. 输出

文本：

```text
PASS dytes.json 可解析  D:\pkg\dytes.json
PASS exports 非空  1 个
PASS exports 文件存在: ./bin/demo.exe  D:\pkg\bin\demo.exe

OK: 包结构检查通过
```

`--json`：

```json
{
  "passed": false,
  "checks": [
    { "label": "dytes.json 可解析", "ok": true, "detail": "D:\\pkg\\dytes.json" },
    { "label": "exports 文件存在: ./bin/demo.exe", "ok": false, "detail": "D:\\pkg\\bin\\demo.exe" }
  ]
}
```

## 5. 退出码

| 退出码 | 场景 |
| --- | --- |
| `0` | 全部校验通过 |
| `2` | `--type` 不是已知类型 |
| `3` | 清单缺失/非法、类型不一致、导出缺失、设计插件规则失败 |

## 6. 边界与约束

- 全程离线，不读取 S3；
- 只校验文件是否存在，不校验 checksum（由 `test` / 安装流程负责）；
- `--manifest-only` 只放宽“必须有 exports”，其它规则仍生效；
- 路径安全由发布与安装流程负责，`check` 只报告缺失；
- 设计插件规则与文档仓库 `sysinfo/design_plugins/dytes-manifest.md` §3 对齐。

## 7. 测试点

- 导出文件存在时通过；
- 导出缺失返回 `3` 且 stderr 含“清单校验失败”；
- `--type` 不一致返回 `3`；
- `--manifest-only` 允许无导出；
- `--json` 输出 `passed` 与 `checks`；
- `platform-exports` 缺少平台时失败；
- 清单缺失返回 `3`。

对应测试文件：`tests/cli_check.rs`。

## 8. 后续规划

1. 将设计插件规则进一步拆分为可配置规则集；
2. 增加路径安全检查（绝对路径、`..`）；
3. 支持 `--checksum` 校验本地文件与清单 `checksums`；
4. 输出机器可读的错误码，便于 CI 精确判断；
5. 与 `test` 统一报告结构，避免两套标签。
