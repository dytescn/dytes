# 设计插件 MCP 规范

> 适用范围：`type=design_plugins` 的 dytes 包。
> 关联文档：[dytes.md](dytes.md)（包模型）、[cmd.md](cmd.md)（命令）、
> [commands/check.md](commands/check.md)（校验流程）、
> [commands/init.md](commands/init.md)（模板）、
> [mcp-official.md](mcp-official.md)（官方 MCP 规范对照与改造建议）。
>
> 字段来源说明：`name` / `title` / `description` / `inputSchema` /
> `outputSchema` / `annotations` 与官方 MCP 对齐；`ffi_symbol` 与
> `transport` 是 dytes 扩展，不属于官方协议。
>
> 文档状态：v1.1（基线对应 dytes 0.2.4）。文中用「已实现」标注当前代码行为，
> 用「已实现（警告）」标注当前只输出 `WARN`、尚不阻断发布的规则，
> 用「规划」标注尚未落地、但作者与 CI 应当遵守的目标规范。
>
> 关键词：**必须**（MUST）、**不得**（MUST NOT）、**应当**（SHOULD）、
> **可以**（MAY）按 RFC 2119 语义使用。

## 1. 目的

设计插件（如 CorelDRAW / Photoshop / AutoCAD / VectorCraft 插件）在提供人机界面的
同时，可以把自身能力以 **MCP 工具**（Model Context Protocol tool）的形式暴露给
AI 宿主（Piksel / cyctron 等）。本规范定义：

- 插件包内 MCP 文件的布局与命名；
- `dytes.json`、`mcp.json` 与工具描述文件的字段与约束；
- `dytes check` / `dytes test` / `dytes publish` 的离线校验规则；
- 安装、更新时的运行时校验与 FFI 登记行为；
- 工具参数 schema、命名、版本演进与安全约束；
- 作者自检清单与错误排查表。

不在本规范范围内：MCP 协议本身的传输层（stdio / HTTP / SSE）实现细节、
AI 宿主的提示词与调度策略、非 `design_plugins` 类型包的 AI 能力。

## 2. 术语

| 术语 | 含义 |
| --- | --- |
| 插件包 | `type=design_plugins` 的包，安装到 `<应用根>/plugins/<name>/` |
| 插件 DLL | 插件包导出的动态库，安装后同时注册到 `<应用根>/libary/` |
| MCP 工具（tool） | 插件暴露给 AI 宿主的一个可调用能力 |
| 工具描述文件 | 独立 JSON 文件，描述单个 MCP 工具的元数据与执行入口 |
| transport | 工具的运行载体，当前指「通过 `library` + `library_uuid` 绑定能力库」 |
| 能力库 | `type=sys` 的包，安装到 `<应用根>/libary/<library>.<ext>` |
| 宿主 | 加载插件与能力库、向模型暴露 MCP 工具的应用（Piksel / cyctron） |
| 导出目标名 | `exports` 映射中的远程文件名（只能是不带目录的文件名） |

## 3. 总体模型

```text
dytes.json ──mcp──▶ mcp.json ──tools[]──┬─▶ "text_list.json" ──▶ 工具描述文件 ──▶ transport.library ──▶ libary/<library>.<ext>
                                        └─▶ { 内联工具对象 } ──▶ name / ffi_symbol / schema
```

要点：

1. `dytes.json` 只声明一个固定文件名 `mcp.json`，不内联工具信息。
2. `mcp.json` 的 `tools` 支持两种形态：
   - **引用形态**：字符串，指向同目录下的工具描述文件；
   - **内联形态**：对象，直接携带 `name` / `ffi_symbol` / schema。
3. 引用形态的工具描述文件通过 `transport.library` + `transport.library_uuid`
   绑定真实能力库；安装时 dytes 校验库文件存在且 UUID 匹配，避免宿主
   `dlopen` 失败或调用到错误版本的库。
4. 安装完成后，dytes 通过 FFI 调用 `plugin_lib` 的 `install_plugin` /
   `update_plugin`，由插件管理库登记 `plugins.db`、skills、prompts、taskflow
   与 MCP 能力。

## 4. 包内文件布局

### 4.1 推荐布局

```text
<package>/
├── dytes.json
├── target/release/<name>.dll      # 插件 DLL（按平台扩展名）
├── mcp/
│   ├── mcp.json                   # 工具清单
│   ├── text_list.json             # 工具描述文件（引用形态）
│   └── image_export.json
└── skills.json                    # 仅在 use_ai=true 时存在
```

`dytes.json` 通过 `exports` 把 `./mcp/mcp.json` 映射为 `mcp.json`，
把 `./mcp/text_list.json` 映射为 `text_list.json`。发布与安装后，
这些文件都落在包目录根（导出目标名不得包含目录）。

### 4.2 安装后的布局

```text
<应用根>/
├── plugins/<name>/
│   ├── dytes.json                 # 云端清单原样保存
│   ├── <name>.<ext>               # 插件 DLL
│   ├── mcp.json
│   └── text_list.json             # 工具描述文件
├── libary/
│   ├── <name>.<ext>               # 插件 DLL 的副本
│   ├── info_lib.dll               # 能力库
│   └── info_lib/dytes.json        # 能力库清单（含 uuid）
└── dytes.lock
```

### 4.3 命名约束

| 对象 | 约束 | 状态 |
| --- | --- | --- |
| `mcp` 字段 | 必须恰为 `mcp.json`，不得改名 | 已实现 |
| 导出目标名 | 只能是文件名，禁止 `/`、`\`、`..`、绝对路径 | 已实现（导出名禁止 `/`、`\`） |
| `tools` 中的字符串 | 只能是不带目录的文件名，且必须已被导出 | 已实现 |
| 工具描述文件名 | 建议 `<snake_case 工具名>.json`，全包唯一 | 规划（未强制） |
| 工具 `name` | 非空；建议 `snake_case`，同一 `mcp.json` 内唯一 | 已实现（唯一性、长度、字符集为警告） |

## 5. `dytes.json` 字段规范

设计插件必须包含以下与 MCP 相关的字段：

```json
{
  "name": "cdr_check_txt",
  "uuid": "8b2cd615-eacf-43fb-a2c9-d6f0a8195946",
  "version": "0.2.0",
  "type": "design_plugins",
  "plugins_type": "plugins_coreldraw",
  "targets": ["coreldraw"],
  "use_ai": true,
  "mcp": "mcp.json",
  "skills": "skills.json",
  "exports": {
    "./target/release/cdr_check_txt.dll": "cdr_check_txt.dll",
    "./mcp/mcp.json": "mcp.json",
    "./mcp/text_list.json": "text_list.json",
    "./skills.json": "skills.json"
  },
  "dependencies-sys": {
    "info_lib": "0.1.0"
  }
}
```

| 字段 | 类型 | 必填 | 约束 |
| --- | --- | --- | --- |
| `mcp` | string | 是 | 必须为 `"mcp.json"`；`dytes check` 失败项为 `mcp == mcp.json` |
| `use_ai` | bool | 是 | 布尔值；只控制 skills 校验，**不控制** MCP 校验 |
| `exports` / `platform-exports` | object | 是 | 必须把 `mcp.json` 与每个引用形态的工具文件映射为同名目标 |
| `dependencies-sys` | object | 条件必填 | 工具 `transport.library` 指向的能力库必须在此声明，否则安装时提示补声明 |
| `skills` | string | `use_ai=true` 时必填 | 固定 `"skills.json"`，与 MCP 相互独立 |

规则细节：

1. **`mcp` 始终参与校验**：`check_design_plugin` 无条件执行 `check_mcp`；
   `use_ai=false` 时只要求「不得声明 / 导出 skills」，MCP 仍必须合法。
2. **平台导出优先**：清单存在非空 `platform-exports` 时，校验与发布只读取
   当前平台的映射，忽略顶层 `exports`。因此若使用 `platform-exports`，
   `mcp.json` 与全部工具文件必须在**每个目标平台**的映射中出现。
3. **导出目标名即安装文件名**：源路径可以带目录（如 `./mcp/mcp.json`），
   映射目标不得带目录；`tools` 中的字符串必须与某个映射目标完全一致。
4. **`uuid`**：发布时缺失会自动生成并写回；安装时用于能力库绑定校验，
   不得随意更换。
5. **`version`**：必须是合法 semver，发布前校验。

## 6. `mcp.json` 规范

### 6.1 顶层结构

```json
{
  "tools": [
    "text_list.json",
    {
      "name": "text_replace",
      "description": "批量替换选区文本",
      "ffi_symbol": "post_text_replace",
      "inputSchema": { "type": "object", "properties": {} },
      "outputSchema": { "type": "object" }
    }
  ]
}
```

| 字段 | 类型 | 必填 | 约束 |
| --- | --- | --- | --- |
| `tools` | array | 是 | 非空数组；元素为字符串或对象 |
| `schemaVersion` | number / string | 否 | 预留版本标记，当前仅识别不解释 |

约束：

- `mcp.json` 必须是可解析的 JSON 对象。
- `tools` 缺失、不是数组、或为空数组，`dytes check` 均失败
  （失败项标签 `mcp tools 为非空数组`）。
- 顶层只识别 `tools` 与保留字段 `schemaVersion`；其它字段会产生
  `mcp.json 字段已知: <field>` 警告并被忽略，作者**不得**依赖其生效。
  后续若引入新字段，应当先登记到 `MCP_TOP_LEVEL_FIELDS`，避免长期告警。
- 文件编码必须为 UTF-8（无 BOM）。

### 6.2 引用形态（字符串元素）

```json
{ "tools": ["text_list.json"] }
```

- 字符串必须是非空、不带 `/` 或 `\` 的文件名；含路径分隔符直接判失败
  （标签 `mcp 文件名合法: <tool>`）。
- 该文件名必须作为导出目标出现（标签 `mcp 工具已导出: <tool>`），
  且对应本地文件存在。
- 安装后工具描述文件与 `mcp.json` 同目录（包根）。
- 当前未禁止重复文件名，但作者**不得**重复声明；重复项会导致同一工具被加载多次。

### 6.3 内联形态（对象元素）

| 字段 | 类型 | 必填 | 约束 | 状态 |
| --- | --- | --- | --- | --- |
| `name` | string | 是 | 去除首尾空白后非空 | 已实现 |
| `ffi_symbol` | string | 否 | 出现时必须为非空字符串 | 已实现 |
| `title` | string | 否 | 出现时为字符串，建议 ≤ 120 字符 | 已实现（警告） |
| `inputSchema` | object | 否 | 出现时必须是 JSON 对象 | 已实现 |
| `outputSchema` | object | 否 | 出现时必须是 JSON 对象 | 已实现 |
| `description` | string | 应当 | 面向模型的能力说明，建议 ≤ 500 字符 | 已实现（警告） |
| `transport` | object | 否 | 结构见 §6.4；含 `transport` 时安装期也校验 | 已实现（警告） |
| `annotations` | object | 否 | 只读/幂等等提示，按 MCP 约定透传 | 已实现（警告） |
| `icons` | array | 否 | 官方字段，当前不校验 | 规划 |

约束：

- 元素既不是字符串、也不是对象（如数字、数组、`null`）时失败
  （标签 `mcp 工具为对象或文件名`）。
- 同一 `mcp.json` 内 `name` 不得重复；重复时产生
  `mcp 工具 name 唯一: <name>` 警告。
- 内联形态的 `transport` 同样参与安装期的能力库校验（见 §9.2）。

### 6.4 工具描述文件（引用形态指向的 JSON）

```json
{
  "name": "text_list",
  "description": "列出当前文档的全部文本块",
  "inputSchema": {
    "type": "object",
    "properties": {
      "page": { "type": "integer", "minimum": 1 }
    },
    "additionalProperties": false
  },
  "outputSchema": {
    "type": "object",
    "properties": {
      "items": { "type": "array", "items": { "type": "string" } }
    }
  },
  "transport": {
    "library": "info_lib",
    "library_uuid": "1f0c1a3e-2b7d-4f6e-9c5a-8d3f0b6e2a11",
    "symbol": "post_text_list"
  }
}
```

| 字段 | 类型 | 必填 | 约束 |
| --- | --- | --- | --- |
| `name` | string | 应当 | 建议与文件名一致（去掉 `.json`） |
| `description` | string | 应当 | 面向模型的说明，避免为空 |
| `inputSchema` | object | 是 | JSON Schema（对象根） |
| `outputSchema` | object | 可以 | 提供时必须是对象 |
| `transport` | object | 是 | 至少含 `library` 与 `library_uuid` 才参与安装期校验 |
| `transport.library` | string | 是 | 能力库名，对应 `dependencies-sys` 的键 |
| `transport.library_uuid` | string | 是 | 能力库 `dytes.json` 的 `uuid`，必须完全一致 |
| `transport.symbol` | string | 可以 | FFI 入口符号；缺省时宿主按工具名推断 |

文件必须：

- 是 UTF-8 编码的可解析 JSON 对象；
- 只描述**一个**工具；
- 未知字段允许存在，解析方忽略。

> `dytes check` 会读取并校验工具描述文件：文件必须已导出、可读并可解析为
> JSON 对象；新增字段规则（名称、schema、annotations、transport）当前以
> `WARN` 输出。安装期再读取 `transport` 做能力库绑定校验（见 §9.2）。

## 7. 工具命名与 schema 规范

### 7.1 命名

- 工具 `name` 必须非空，建议使用 `snake_case`（如 `text_replace`、
  `image_export`），不得包含空白与控制字符。
- `name` 应当是全局可辨识的：跨插件重名时由宿主按 `插件名/工具名` 消歧，
  因此**不应**依赖裸 `name` 唯一。
- `ffi_symbol` 与 `name` 可以不同；宿主解析符号时先按原样，再尝试
  `post_` 前缀（与 dytes 共用 FFI 约定，见 §10.2）。
- 工具描述文件名建议与 `name` 一致（`<name>.json`），便于人工核对。

### 7.2 参数 schema

- `inputSchema` 必须是 JSON Schema 对象，根类型为 `object`。
- 应当声明 `properties` 与 `required`；无参数工具使用
  `{ "type": "object", "properties": {}, "additionalProperties": false }`。
- 应当设置 `additionalProperties: false`，避免模型传入未定义参数。
- 每个属性的 `description` 应当说明业务含义、单位、取值范围。
- 枚举值使用 `enum`，并给出所有允许值。
- 数组必须给出 `items`；数值应当给出 `minimum` / `maximum`。
- schema 中不得包含本地绝对路径、密钥、许可证串等敏感信息。
- 规划：限制 schema 序列化后大小（建议 ≤ 64 KiB）与嵌套深度（建议 ≤ 16 层）。

### 7.3 输出

- `outputSchema` 可选；提供时根类型建议为 `object`，并返回稳定字段。
- 工具实际返回体建议为 `{ "code": 200, "data": ..., "msg": "" }`，
  失败时 `code != 200` 并给出可读 `msg`（与 dytes 的 FFI 约定一致）。

## 8. 静态校验规范（`dytes check`）

### 8.1 执行时机

| 命令 | 行为 |
| --- | --- |
| `dytes check` | 对 `design_plugins` 执行完整规则（含 MCP） |
| `dytes test` | 复用同一套规则，再做本地安装演练 |
| `dytes publish` | 默认执行完整规则；`--skip-check` 可跳过（不建议） |

校验实现位于 `src/utils/plugin_manifest.rs`，`check` 与 `publish` 共用
`check_design_plugin`，避免两套行为漂移。任一失败项使退出码为 `3`。

### 8.2 MCP 校验项（失败）

| 标签 | 条件 | 失败后果 |
| --- | --- | --- |
| `mcp == mcp.json` | `dytes.json` 的 `mcp` 恰为 `mcp.json` | 退出码 3 |
| `mcp.json 已导出` | 当前平台导出表存在目标 `mcp.json` | 退出码 3 |
| `mcp.json 存在` | 导出源文件在本地存在 | 退出码 3 |
| `mcp.json 可解析` | 文件可读且为合法 JSON | 退出码 3 |
| `mcp tools 为非空数组` | `tools` 是非空数组 | 退出码 3 |
| `mcp 文件名合法: <f>` | 字符串元素不含 `/`、`\` | 退出码 3 |
| `mcp 工具已导出: <f>` | 字符串元素与导出目标同名且文件存在 | 退出码 3 |
| `mcp 工具为对象或文件名` | 元素类型为字符串或对象 | 退出码 3 |
| `mcp 工具 name 非空: tools[<i>]` | 内联对象 `name` 去空白后非空 | 退出码 3 |
| `mcp 工具 ffi_symbol 非空: <name>` | 出现 `ffi_symbol` 时必须为非空字符串 | 退出码 3 |
| `mcp 工具 inputSchema 为对象: <name>` | 内联对象出现时必须为 JSON 对象 | 退出码 3 |
| `mcp 工具 outputSchema 为对象: <name>` | 内联对象出现时必须为 JSON 对象 | 退出码 3 |

说明：

- 校验全程离线，只读取本地文件，不访问 S3。
- 校验只检查文件存在性，不校验 checksum（由 `test` / 安装流程负责）。
- `--manifest-only` 不跳过设计插件的 MCP 规则；设计插件仍必须有可用的导出表。
- `--type` 与清单类型不一致时直接返回 `3`，不进入 MCP 校验。

### 8.3 与官方对齐的警告规则（当前不阻断）

新增规则先以 `WARN` 输出，不计入失败数、不影响退出码与 `publish`；
按「先警告后失败」策略，后续版本升级为 `FAIL`。

| 标签 | 条件 |
| --- | --- |
| `mcp 工具 name 长度合法: <name>` | 长度不超过 128（官方 1–128） |
| `mcp 工具 name 字符集合法: <name>` | 仅含 `A-Z a-z 0-9 _ - .` |
| `mcp 工具 name 唯一: <name>` | 同一 `mcp.json` 内工具名唯一 |
| `mcp 工具 title 为字符串 / title 长度` | `title` 为字符串且 ≤ 120 字符 |
| `mcp 工具 description 为字符串 / description 长度` | 为字符串且 ≤ 500 字符 |
| `mcp 工具 inputSchema 存在` | 工具必须提供 `inputSchema` |
| `mcp 工具 inputSchema 根为 object` | `type` 为 `object` |
| `mcp 工具 <field> 方言受支持` | `$schema` 为 2020-12 / 2019-09 / draft-07 / draft-04 |
| `mcp 工具 inputSchema 无参声明` | 无参数时建议 `additionalProperties: false` |
| `mcp 工具 annotations 为对象` | `annotations` 为对象 |
| `mcp 工具 annotations.<field> 为布尔值` | `readOnlyHint` / `destructiveHint` / `idempotentHint` / `openWorldHint` |
| `mcp 工具 annotations.title 为字符串` | `annotations.title` 类型 |
| `mcp 工具 transport 为对象` | `transport` 为对象 |
| `mcp 工具 transport.library 非空` | `library` 为非空字符串 |
| `mcp 工具 transport.library_uuid 非空` | `library_uuid` 为非空字符串 |
| `mcp 工具依赖已声明: <library>` | `library` 出现在 `dependencies-sys` |
| `mcp 工具描述可解析: <file>` | 引用形态的工具文件可读且为合法 JSON |
| `mcp 工具描述为对象: <file>` | 工具文件顶层为对象 |
| `mcp.json 字段已知: <field>` | 顶层仅 `tools` / `schemaVersion` |

引用形态与内联形态都会执行上述规则；内联对象的 schema 类型错误仍按
§8.2 直接失败，引用文件中的同类问题先给警告。

`--json` 输出在每条 `checks` 中增加 `severity`：`pass` / `warning` / `error`；
`passed` 只由 `error` 决定。

### 8.4 后续校验项

以下规则尚未实现，作为规范目标与评审依据：

1. 工具数量、schema 大小与嵌套深度上限；
2. `icons` 结构校验（`src` 只允许 `https:` / `data:`）；
3. 字符串工具文件扩展名必须为 `.json`；
4. 警告项在下一版本升级为失败（建立版本切换与迁移期）；
5. `transport.symbol` 与能力库导出符号的一致性检查。

## 9. 发布与安装规范

### 9.1 发布

`dytes publish` 的流程对 MCP 的影响：

1. 完整校验通过后，按当前平台选择导出表（`platform-exports` 优先）；
2. 逐个上传导出文件到 `<type>/<name>/<version>/[<platform>/]<目标名>`
   （设计插件前缀 `design_plugins/`）；
3. 为每个上传文件记录 SHA-256 到清单 `checksums`；
4. 上传带 `checksums` 的 `dytes.json`；
5. 更新 `design_plugins/packages.json`。

发布要求：

- `mcp.json` 与全部引用形态工具文件**必须**出现在导出表中，否则不会上传；
- 任一文件上传失败则不更新索引，需修复后重发；
- `--force` 可覆盖已有对象，但不得借此跳过 MCP 校验（仍受 `--skip-check` 控制）；
- `--dry-run` 只打印计划，可用于核对 MCP 文件是否齐全。

### 9.2 安装与更新

设计插件安装（`src/update/update_design.rs`）在 MCP 方面依次执行：

1. 下载导出文件到 `<应用根>/plugins/<name>/`，并按 `checksums` 校验；
2. 原样保存云端 `dytes.json`；
3. **MCP 能力库校验**（见下）；
4. 若为插件，把包内 `.dll` / `.so` / `.dylib` 复制到 `<应用根>/libary/`；
5. 通过 FFI 调用 `plugin_lib` 的 `install_plugin` / `update_plugin`，
   同步 `plugins.db`、skills、prompts、taskflow 与 MCP 能力；
6. 写 `dytes.lock`（失败则不写）。

能力库校验 `verify_mcp_libraries` 的规则：

- 定位 `mcp.json`：先找包根 `mcp.json`，再找 `mcp/mcp.json`（兼容手工布局），
  都没有则跳过；
- 同时处理 `tools` 中的**字符串**元素与**内联对象**；缺失 `transport` 的
  工具跳过。文件读取或解析失败、以及 `transport.library` /
  `library_uuid` 缺失时，输出带工具名的警告并跳过该校验，不中止安装；
- 对每个（去重后的）`(library, library_uuid)` 组合：
  - 必须存在 `<应用根>/libary/<library>.<ext>`，扩展名按平台：
    Windows `.dll`、macOS `.dylib`、其它 `.so`；
  - 从 `<应用根>/libary/<library>/dytes.json` 读取 `uuid`，
    缺失时回退到 `dytes.lock`；
  - 实际 `uuid` 必须等于 `library_uuid`；
  - 任一不满足即中止安装，错误信息提示在
    `dytes.json` 的 `dependencies-sys` 中声明该库。

注意：

- 校验在写锁文件之前执行，失败不会留下半安装状态；
- 只有同时提供 `transport.library` 与 `transport.library_uuid` 的工具才参与
  绑定校验；读取失败、解析失败或字段缺失会输出警告并跳过，属「先诊断后严格」
  的过渡策略，后续版本再升级为中止安装。

### 9.3 宿主侧加载约定

> 宿主（Piksel / cyctron）的实现不在本仓库。以下是与 dytes 安装结果对接时
> 应当遵守的接口约定，用于指导工具作者与宿主联调。

宿主从包目录读取 `mcp.json`，对引用形态再读取工具描述文件，随后：

1. 按 `plugins_type` / `targets` 判断是否在当前设计软件中启用；
2. 依赖能力库时加载 `<应用根>/libary/<library>.<ext>`；
3. 以工具 `name`（或 `ffi_symbol`）作为入口，输入 JSON 字符串，
   输出缓冲 64 KiB，返回 `{code, data, msg}`，`code == 200` 表示成功；
4. 符号解析优先原样，其次补 `post_` 前缀。

以上为 dytes 与 `libary` 共用的 FFI 约定；具体超时、并发与结果裁剪由宿主决定。

## 10. 安全与可靠性

| 风险 | 要求 | 状态 |
| --- | --- | --- |
| 路径穿越 | 导出目标名、`tools` 字符串不得含 `/`、`\`、`..` | 部分实现（`..` 未显式拦截） |
| 任意库加载 | `transport.library` 只允许库名，`uuid` 必须匹配 | 已实现 |
| 依赖缺失 | 能力库必须由 `dependencies-sys` 声明并安装 | 已实现（错误提示） |
| 内容超限 | 工具数量、描述长度、schema 大小设置上限 | 规划 |
| 敏感信息 | 工具描述不得包含密钥、绝对路径、内部地址 | 规划（人工评审） |
| 供应链 | 发布文件记录 SHA-256，安装时校验 | 已实现 |
| 原子性 | 校验失败不写 `dytes.lock`、不更新索引 | 已实现 |
| 日志 | 错误信息不输出凭证；仅输出库名与路径 | 已实现 |

附加约定：

- 工具实现（能力库）必须自行校验入参，不假设宿主已按 schema 校验；
- 工具不得在描述文件中声明「删除任意文件」类无边界能力；
- 插件升级若改变工具语义而保留同名工具，属于破坏性变更（见 §11）。

## 11. 版本与兼容

- `mcp.json` 当前为无版本号格式（schema v1）。顶层 `schemaVersion` 已作为
  预留字段被识别，但当前不解释其值；后续版本再赋予语义（缺省视为 v1，
  未知大版本拒绝加载并给出诊断）。
- 兼容规则：
  - 新增可选字段：向后兼容，可随 patch / minor 发布；
  - 新增工具：向后兼容，minor；
  - 删除工具、重命名工具、改变 `inputSchema` 的必填项或类型、
    更换 `library_uuid` 指向：破坏性变更，必须提升 minor 以上，
    并在发布说明中列出；
  - 修改 `description`、补充 `enum` 取值属于兼容变更。
- 插件版本使用 semver；`uuid` 与能力库 `uuid` 保持不变，变更即视为新绑定。

## 12. 完整示例

### 12.1 目录

```text
cdr_check_txt/
├── dytes.json
├── Cargo.toml
├── target/release/cdr_check_txt.dll
├── mcp/
│   ├── mcp.json
│   ├── text_list.json
│   └── text_replace.json
└── skills.json
```

### 12.2 `dytes.json`

```json
{
  "name": "cdr_check_txt",
  "uuid": "8b2cd615-eacf-43fb-a2c9-d6f0a8195946",
  "version": "0.2.0",
  "description": "文本检查与替换",
  "author": "funxdata",
  "type": "design_plugins",
  "plugins_type": "plugins_coreldraw",
  "targets": ["coreldraw"],
  "use_ai": true,
  "mcp": "mcp.json",
  "skills": "skills.json",
  "exports": {
    "./target/release/cdr_check_txt.dll": "cdr_check_txt.dll",
    "./mcp/mcp.json": "mcp.json",
    "./mcp/text_list.json": "text_list.json",
    "./mcp/text_replace.json": "text_replace.json",
    "./skills.json": "skills.json"
  },
  "dependencies-sys": {
    "info_lib": "0.1.0"
  }
}
```

### 12.3 `mcp/mcp.json`

```json
{
  "tools": [
    "text_list.json",
    "text_replace.json",
    {
      "name": "plugin_info",
      "description": "返回插件版本与能力列表",
      "ffi_symbol": "post_plugin_info",
      "inputSchema": { "type": "object", "properties": {} },
      "outputSchema": {
        "type": "object",
        "properties": {
          "version": { "type": "string" },
          "tools": { "type": "array", "items": { "type": "string" } }
        }
      }
    }
  ]
}
```

### 12.4 `mcp/text_list.json`

```json
{
  "name": "text_list",
  "description": "列出当前文档页面上的全部文本块",
  "inputSchema": {
    "type": "object",
    "properties": {
      "page": { "type": "integer", "minimum": 1, "description": "页码，缺省为当前页" }
    },
    "additionalProperties": false
  },
  "outputSchema": {
    "type": "object",
    "properties": {
      "items": { "type": "array", "items": { "type": "string" } }
    }
  },
  "transport": {
    "library": "info_lib",
    "library_uuid": "1f0c1a3e-2b7d-4f6e-9c5a-8d3f0b6e2a11",
    "symbol": "post_text_list"
  }
}
```

### 12.5 `mcp/text_replace.json`

```json
{
  "name": "text_replace",
  "description": "在选区内批量替换文本",
  "inputSchema": {
    "type": "object",
    "properties": {
      "from": { "type": "string", "minLength": 1 },
      "to": { "type": "string" },
      "case_sensitive": { "type": "boolean", "default": false }
    },
    "required": ["from", "to"],
    "additionalProperties": false
  },
  "transport": {
    "library": "info_lib",
    "library_uuid": "1f0c1a3e-2b7d-4f6e-9c5a-8d3f0b6e2a11",
    "symbol": "post_text_replace"
  }
}
```

### 12.6 依赖的能力库

`info_lib` 是 `type=sys` 包，安装后位于 `<应用根>/libary/info_lib.dll`，
且 `<应用根>/libary/info_lib/dytes.json` 的 `uuid` 必须为
`1f0c1a3e-2b7d-4f6e-9c5a-8d3f0b6e2a11`。

## 13. 错误与排查

| 现象 / 输出 | 原因 | 处理 |
| --- | --- | --- |
| `FAIL mcp == mcp.json` | 字段缺失或写成了其它文件名 | 改为 `"mcp": "mcp.json"` |
| `FAIL mcp.json 已导出` | 导出表没有目标 `mcp.json` | 在 `exports`（或当前平台 `platform-exports`）补映射 |
| `FAIL mcp.json 存在` | 源文件路径错误 | 核对 `./mcp/mcp.json` 是否存在 |
| `FAIL mcp tools 为非空数组` | 缺失、类型错误或空数组 | 至少声明一个工具 |
| `FAIL mcp 文件名合法: x/y.json` | 字符串含目录 | 只写文件名，把路径放进 `exports` 映射 |
| `FAIL mcp 工具已导出: x.json` | 文件未导出或目标名不一致 | 补导出映射，确保目标名与字符串一致 |
| `FAIL mcp 工具为对象或文件名` | 元素是数字/数组/null | 改为字符串或对象 |
| `FAIL mcp 工具 name 非空: tools[i]` | 内联对象缺 `name` | 补唯一工具名 |
| `库文件不存在: .../libary/info_lib.dll` | 能力库未安装 | 在 `dependencies-sys` 声明并由 `dytes update` 安装 |
| `库 info_lib 的 uuid 不匹配` | `library_uuid` 与已装库不一致 | 同步库版本与 `uuid`，或修正工具描述 |
| `无法读取库 info_lib 的 uuid` | 库目录缺少 `dytes.json` 且锁文件无记录 | 重新安装该 sys 包 |
| 宿主加载工具失败 | 符号缺失或 ABI 不一致 | 核对 `transport.symbol`，确认导出 `post_` 前缀约定 |

退出码：校验失败 `3`，参数错误 `2`，下载/安装失败 `1`，成功 `0`。

## 14. 作者自检清单

发布前逐项确认：

- [ ] `dytes.json` 的 `type=design_plugins`，`mcp` 为 `mcp.json`；
- [ ] `mcp.json` 已按当前（及每个目标）平台导出，目标名恰为 `mcp.json`；
- [ ] `tools` 非空，字符串元素均为不带目录的文件名且已导出、文件存在；
- [ ] 内联对象工具 `name` 非空且全包唯一（`dytes check` 会给出警告）；
- [ ] 每个引用形态工具描述文件是合法 JSON 对象，含 `name` 与 `description`；
- [ ] `inputSchema` 根为 `object`，参数有类型与说明，`required` 准确；
- [ ] `transport.library` 在 `dependencies-sys` 中声明；
- [ ] `transport.library_uuid` 与目标能力库版本的实际 `uuid` 一致；
- [ ] 工具名、描述、schema 中无密钥、绝对路径、内部地址；
- [ ] `dytes check --type design_plugins` 通过；
- [ ] `dytes test` 通过（覆盖 MCP 运行时与依赖下载）；
- [ ] `dytes publish --dry-run` 的文件清单包含全部 MCP 文件；
- [ ] 破坏性变更（删/改工具语义、换绑定库）已提升版本并写发布说明。

## 15. 实现映射与后续工作

| 关注点 | 位置 |
| --- | --- |
| 静态校验（含 MCP） | `src/utils/plugin_manifest.rs`（`check_mcp`） |
| 清单模板 | `src/utils/templates.rs`（`design_plugins` 分支） |
| 发布前校验与上传 | `src/publish/cli.rs`、`src/publish/dytes.rs` |
| 安装期库校验 | `src/update/update_design.rs`（`verify_mcp_libraries`） |
| FFI 同步 | `src/utils/plugin_lib.rs`、`src/utils/ffi.rs` |
| 命令文档 | `docs/cmd.md`、`docs/commands/check.md`、`docs/commands/test.md` |
| 测试 | `src/utils/plugin_manifest.rs` 单元测试、`tests/cli_check.rs`、`tests/cli_init.rs` |

已完成（0.2.4，均为警告级，不阻断发布）：

1. §8.3 的校验项已落地并统一到 `check` 报告；
2. `dytes check` 已解析引用形态的工具描述文件，并与 `dependencies-sys`
   交叉校验；
3. `verify_mcp_libraries` 已支持内联对象，读取/解析失败与字段缺失会输出
   带工具名的诊断。

后续按以下顺序补齐：

1. 把 §8.3 的警告项升级为失败，并给出迁移期；
2. 引入 `mcp.json` 的大小/数量上限与 `schemaVersion` 语义（期望版本、拒绝
   未知大版本）；
3. 在 `dytes test` 的 MCP 运行时检查中覆盖符号解析与入参/出参契约；
4. 输出稳定错误码（如 `MCP001` 起）供 CI 精确判断；
5. 在发布流程中汇总 MCP 校验结果到 `--json` 输出，便于 GitHub Action 展示。

## 16. 变更记录

| 版本 | 日期 | 说明 |
| --- | --- | --- |
| v1 | 2026-11-17 | 首版：整理现有 `mcp` 字段、`mcp.json`、校验与安装规则，标注规划项 |
| v1.1 | 2026-11-18 | 对齐警告级校验：`title`/`description`/`annotations`/`transport` 与工具名规则落地；`check` 解析工具描述文件；安装期支持内联对象并输出诊断；顶层字段限定 `tools`/`schemaVersion` |
