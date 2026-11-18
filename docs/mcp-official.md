# 官方 MCP 规范与 dytes 现状对照

> 官方规范版本：**2026-07-28**（Current）
> 核对日期：2026-11-17
> 来源：<https://modelcontextprotocol.io/specification/2026-07-28>
> 对照对象：[mcp.md](mcp.md)（dytes 设计插件 MCP 规范 v1）与当前代码实现
> （`src/utils/plugin_manifest.rs`、`src/update/update_design.rs`、
> `src/utils/templates.rs`、`src/utils/ffi.rs`）。
>
> 本文只整理规范与差异、给出改造建议，不改代码。文中 **MUST / MUST NOT /
> SHOULD / MAY** 为官方原文语义；「我们」指 dytes 当前实现。

## 1. 一句话结论

官方 MCP 是**基于 JSON-RPC 2.0 的无状态协议**：一个 MCP **server** 通过
`tools/list` 动态发布工具，通过 `tools/call` 执行，结果以「内容块 +
`structuredContent` + `isError`」返回，传输层是 stdio 或 Streamable HTTP。

dytes 当前的 `mcp.json` **不是** MCP 协议实现，而是一份**静态工具清单**：宿主读文件、
自己加载 `libary` 下的动态库、用 C ABI + `{code, data, msg}` 直接调用。

两者在「工具长什么样」上可以对齐，但在「怎么发现、怎么调用、怎么返回、怎么协商」
上属于完全不同的层次。改造方向必须在下面两条路线中明确选择（见 §5）：

- **路线 A**：保留静态清单 + 宿主适配层，把字段、命名、schema、结果形态对齐官方，
  在宿主侧合成 MCP server（成本低，兼容现有安装链路）。
- **路线 B**：插件附带真正的 MCP server 进程（stdio 或 HTTP），dytes 只负责安装、
  注册与启动，官方 SDK 可直接对接（改造成本高，长期最标准）。

## 2. 官方规范要点

### 2.1 角色与架构

| 角色 | 职责 |
| --- | --- |
| Host | LLM 应用，发起连接，实施用户同意与安全策略 |
| Client | Host 内的连接器，与一个 server 通信，处理 JSON-RPC |
| Server | 提供 tools / resources / prompts 等能力 |

- 所有消息 **MUST** 是 JSON-RPC 2.0；请求 `id` 不得为 `null`，且同一发送方
  未收到响应的请求之间 `id` 不得重复。
- 通知 **MUST NOT** 带 `id`，接收方 **MUST NOT** 回复。
- 结果响应 **MUST** 含 `resultType`：`"complete"` 或 `"input_required"`；
  客户端对缺失 `resultType` 的旧版响应 **MUST** 视为 `"complete"`。
- 错误码：通用 JSON-RPC 码之外，`-32020` HeaderMismatch、`-32021`
  MissingRequiredClientCapability、`-32022` UnsupportedProtocolVersion；
  `-32020..-32099` 为规范保留，新实现 **MUST NOT** 自造该区间错误码。

### 2.2 无状态与请求元数据

2026-07-28 移除了 `initialize` / `notifications/initialized` 握手。每个请求自带元数据：

| `_meta` 键 | 类型 | 必需 | 说明 |
| --- | --- | --- | --- |
| `io.modelcontextprotocol/protocolVersion` | string | **是** | 本请求使用的协议版本，如 `"2026-07-28"` |
| `io.modelcontextprotocol/clientCapabilities` | object | **是** | 与本请求相关的客户端能力 |
| `io.modelcontextprotocol/clientInfo` | Implementation | 否（**SHOULD**） | 客户端名称与版本 |
| `io.modelcontextprotocol/logLevel` | LoggingLevel | 否 | 本请求的最低日志级别 |

- 缺必需字段 = 非法请求，服务端 **MUST** 返回 `-32602`；HTTP 上状态码
  **MUST** 为 `400`。
- 响应 `_meta` 中服务端 **SHOULD** 带 `io.modelcontextprotocol/serverInfo`。
- 服务端不得依赖连接上先前的请求来推断版本、能力或身份；跨请求状态必须用
  显式 handle 由客户端在每次请求中携带。
- `_meta` 键名有两段式规则，`io.modelcontextprotocol/`、`*.mcp/` 等前缀保留；
  自定义元数据应使用反向域名前缀。

### 2.3 版本协商与发现

- 协议版本形如 `YYYY-MM-DD`，只在出现**不向后兼容**变更时递增。
- 客户端可在任意请求中直接声明版本；服务端不支持时返回
  `UnsupportedProtocolVersionError`（`-32022`），`data.supported` 列出可接受版本。
- 服务端 **MUST** 实现 `server/discover`：一次返回 `supportedVersions`、
  `capabilities`、`serverInfo`、`instructions`，并带 `ttlMs` / `cacheScope` 缓存提示。
- stdio 上与旧版（`2025-11-25` 及更早）互操作时，客户端 **SHOULD** 先发
  `server/discover` 探测：返回 `DiscoverResult` 即新版；返回现代错误码即新版但不支持
  该版本；其它错误或无响应则回退 `initialize` 握手。
- HTTP 上每个 POST **MUST** 带 `MCP-Protocol-Version` 头，且必须与 body `_meta`
  中的版本一致，否则 `400` + `-32020`。

### 2.4 tools 能力

- 声明 `capabilities.tools`（可带 `listChanged`）的服务端 **MUST** 响应
  `tools/list`；工具集合**不得**随连接变化，**可以**因授权范围变化。
- `tools/list` 支持分页（`cursor` / `nextCursor`），结果 **MUST** 带 `ttlMs` 与
  `cacheScope`（`public` / `private`），**SHOULD** 返回确定顺序（利于缓存与
  prompt cache）。
- `tools/call` 请求：`params.name` + `params.arguments`；不支持工具时用协议错误
  `-32602`。
- 工具列表变化：服务端通过 `notifications/tools/list_changed` 通知已订阅
  `subscriptions/listen`（`toolsListChanged: true`）的客户端；2026-07-28 不再有
  独立的长连接 GET 流。
- 需要用户输入时，服务端返回 `InputRequiredResult`（`resultType:
  "input_required"` + `inputRequests` + `requestState`），客户端带
  `inputResponses` 重试原请求，重试 **MUST** 使用新的 `id`（MRTR 模式）。

### 2.5 Tool 定义（2026-07-28）

| 字段 | 类型 | 必需 | 说明 |
| --- | --- | --- | --- |
| `name` | string | 是 | 工具唯一标识，server 内唯一 |
| `title` | string | 否 | 面向用户的显示名 |
| `description` | string | 否（强烈建议提供） | 面向模型的功能说明 |
| `icons` | Icon[] | 否 | 界面图标；客户端须按不可信输入处理 |
| `inputSchema` | object | **是** | 必须是**有效** JSON Schema 对象，不能为 `null` |
| `outputSchema` | object | 否 | 结构化输出 schema |
| `annotations` | ToolAnnotations | 否 | 行为提示，**客户端不得**据此做安全决策 |

`annotations` 字段（全部为**提示**）：

| 字段 | 默认 | 含义 |
| --- | --- | --- |
| `title` | — | 人类可读标题 |
| `readOnlyHint` | `false` | 不修改环境 |
| `destructiveHint` | `true` | 可能执行破坏性更新（仅在 `readOnlyHint=false` 时有意义） |
| `idempotentHint` | `false` | 重复调用无额外副作用 |
| `openWorldHint` | `true` | 与开放世界交互（如联网搜索） |

> 2025-11-25 中的 `execution.taskSupport` 与 `tasks` 相关能力在 2026-07-28 已移出
> 核心协议，归入 `io.modelcontextprotocol/tasks` 扩展。

**工具命名规则（SHOULD 级）：**

- 长度 1–128（含界）；
- 大小写敏感；
- 仅允许 `A-Z a-z 0-9 _ - .`；
- 不含空格、逗号等特殊字符；
- 同一 server 内唯一；跨 server 重名由客户端/代理自行消歧（如前缀），
  不得依赖 `serverInfo.name` 的唯一性。

**inputSchema 规则：**

- 无 `$schema` 时默认 **JSON Schema 2020-12**；实现 **MUST** 支持 2020-12，
  并 **SHOULD** 声明额外支持的方言；schema **MUST** 按声明方言合法。
- 无参工具推荐 `{"type":"object","additionalProperties":false}`；
  或 `{"type":"object"}`（接受任意对象）。
- `$ref` 指向网络 URI 时实现 **MUST NOT** 自动解引用；未解析的外部 `$ref`
  应当**拒绝**，不能静默放行。
- 组合关键字与 `$defs` 应当设置深度/数量/时间上限，防 DoS。
- HTTP 传输下属性可加 `x-mcp-header`，把原始类型参数镜像为 `Mcp-Param-*` 头；
  违规的工具定义客户端 **MUST** 从 `tools/list` 结果中剔除。

### 2.6 调用结果

结果结构：

```json
{
  "resultType": "complete",
  "content": [ { "type": "text", "text": "..." } ],
  "structuredContent": { "any": "JSON value" },
  "isError": false
}
```

- `content[]` 支持 `text` / `image` / `audio` / `resource_link` / `resource`
  （嵌入资源），各内容块可带 `annotations`（audience、priority、lastModified）。
- `structuredContent` 在 2026-07-28 放宽为**任意 JSON 值**（对象、数组、标量），
  有 `outputSchema` 时**必须**符合它。
- 返回结构化内容时 **SHOULD** 同时提供序列化 JSON 的 `text` 块，兼容旧客户端。
- 工具执行错误用 `isError: true` + 可读 `content` 返回，让模型自我纠正；
  请求结构错误才用 JSON-RPC 协议错误。

### 2.7 传输

**stdio**（MCP server 作为子进程）：

- 每行一条 JSON-RPC 消息，UTF-8，**MUST NOT** 含内嵌换行；
- stdout 只能写 MCP 消息；日志写 stderr；
- 客户端关闭 stdin 即优雅关闭；服务端读到 EOF **SHOULD** 尽快退出；
- 客户端 **MUST NOT** 向 stdin 写非 MCP 内容；
- 服务端进程崩溃后，因协议无状态，客户端 **SHOULD** 重启并重试在途请求。

**Streamable HTTP**：

- 单一 MCP endpoint，只接受 POST（2026-07-28 移除了 GET 流与会话）；
- 每个请求：`Accept: application/json, text/event-stream`，并带
  `MCP-Protocol-Version`、`Mcp-Method`，`tools/call` / `resources/read` /
  `prompts/get` 还要带 `Mcp-Name`；不匹配则 `400` + `-32020`；
- 通知返回 `202`；请求返回单 JSON 或请求级 SSE 流；
- 服务端 **MUST** 校验 `Origin`（防 DNS rebinding），本地 **SHOULD** 只绑
  `127.0.0.1`；
- 取消 = 关闭 SSE 流；不支持 `Last-Event-ID` 续传；SSE 中不得再推服务端请求
  （需要输入走 MRTR）；
- 客户端不支持某方法时服务端返回 `404` + `-32601`，与旧版 HTTP+SSE 的 404 区分。

### 2.8 安全与信任

- 服务端 **MUST**：校验全部工具输入、实现访问控制、限流、清洗输出。
- 客户端 **SHOULD**：敏感操作请求用户确认、调用前展示输入、校验结果、设置超时、
  记录审计日志；`annotations` 与图标一律按不可信处理。
- 必须保留「人类在环」，可拒绝调用。
- HTTP 使用 OAuth 授权框架；stdio **SHOULD NOT** 使用 OAuth，凭据从环境读取。
- 工具若需跨调用状态，使用不透明 handle，并在创建工具的 `description` 中说明
  有效期与过期错误。

### 2.9 2026-07-28 相对 2025-11-25 的破坏性变更

1. 移除协议级会话与 `Mcp-Session-Id`；
2. 移除 `initialize` 握手，改为每请求 `_meta`；
3. 新增 `server/discover`（**MUST** 实现）；
4. 用 `subscriptions/listen` 取代 GET 流与 `resources/subscribe`；
5. 移除 `ping`、`logging/setLevel`、`notifications/roots/list_changed`；
6. tasks 移出核心，改成 `io.modelcontextprotocol/tasks` 扩展；
7. 引入 MRTR（`InputRequiredResult`）取代服务端发起的请求；
8. 所有结果新增必需 `resultType`；
9. 移除 SSE `Last-Event-ID` 续传。

同时弃用：Roots、Sampling、Logging、HTTP+SSE 传输。

## 3. 我们当前的实现（摘要）

| 方面 | 我们的做法 |
| --- | --- |
| 协议 | 无。`mcp.json` 是静态 JSON 清单，不经 JSON-RPC |
| 发现 | 宿主读包目录的 `mcp.json`，按 `tools` 数组遍历 |
| 工具枚举 | 字符串（引用 `*.json` 工具描述文件）或内联对象 |
| 工具字段 | `name`（非空）、`ffi_symbol`、`inputSchema`、`outputSchema`（后两者只验对象） |
| 调用 | 宿主加载 `libary/<library>.<ext>`，调用 C ABI 符号，输入 JSON 字符串，输出 64 KiB 缓冲 |
| 绑定 | `transport.library` + `transport.library_uuid`，安装时校验库存在且 UUID 一致 |
| 返回 | `{code, data, msg}`，`code == 200` 为成功；符号可补 `post_` 前缀 |
| 生命周期 | 无会话、无版本协商；安装后用 FFI 调 `plugin_lib` 登记能力 |
| 安全 | 离线校验、文件名禁止路径分隔符、UUID 绑定、依赖声明、SHA-256 |
| 校验 | `dytes check` / `test` / `publish` 共用 `check_design_plugin` |

## 4. 差异对照表

等级：**A** = 可直接对齐且低风险；**B** = 需要协议/架构适配；**C** = 目标不同，
不应对齐或需重新定位。

| # | 维度 | 官方 MCP 2026-07-28 | dytes 现状 | 差距 | 等级 |
| --- | --- | --- | --- | --- | --- |
| 1 | 协议 | JSON-RPC 2.0，逐请求元数据 | 静态 JSON 文件 + C ABI | 不是 MCP server | C / B |
| 2 | 发现 | `server/discover` + `tools/list` 动态 | 读 `mcp.json` 静态清单 | 无 RPC、无分页、无缓存字段 | B |
| 3 | 会话 | 无状态，每请求带版本/能力 | 无会话概念 | 概念可比（都不依赖连接） | A |
| 4 | 版本 | `YYYY-MM-DD` + 协商 + `_meta` | 无协议版本字段 | 无法协商 | B |
| 5 | 工具字段 | `name/title/description/icons/inputSchema/outputSchema/annotations` | `name/ffi_symbol/inputSchema/outputSchema` | 缺 `title`、`description`、`annotations`、`icons` | A |
| 6 | 名称规则 | 1–128、`[A-Za-z0-9_.-]`、server 内唯一 | 仅非空；唯一性/字符集/长度未校验 | 校验缺口 | A |
| 7 | `inputSchema` | 必须有效 JSON Schema（默认 2020-12） | 只校验是 JSON 对象 | 无有效性/方言/根类型校验 | A |
| 8 | 无参工具 | 推荐 `additionalProperties: false` | 无约定 | 缺约定与检查 | A |
| 9 | `outputSchema` | 可选，有则结果必须符合 | 只校验是对象 | 无一致性校验 | A |
| 10 | 结果形态 | `content[]` + `structuredContent` + `isError` | `{code, data, msg}` + 64 KiB 缓冲 | 模型无法看到多模态与结构化结果语义 | B |
| 11 | 错误模型 | 协议错误 vs 执行错误（`isError`） | 所有失败都是 `code != 200` | 错误不可自纠正 | B |
| 12 | 状态 | 无协议级状态，用显式 handle | 无状态约定 | 需补设计约定 | A |
| 13 | 传输 | stdio / Streamable HTTP | 进程内 DLL 调用 | 无传输层 | B / C |
| 14 | 能力协商 | `capabilities` + `clientCapabilities` | 无 | 无 | B |
| 15 | 列表变更 | `subscriptions/listen` + `list_changed` | 安装时 FFI 登记，重启生效 | 无运行时通知 | B |
| 16 | 人类在环 | **SHOULD** 展示与确认 | 未定义（宿主责任） | 需在规范中明确 | A |
| 17 | 服务端安全义务 | 输入校验、访问控制、限流、输出清洗 | 仅安装期校验 | 运行时约束缺失 | B |
| 18 | 客户端安全义务 | 确认、展示输入、超时、审计 | 仅 UUID 绑定与路径约束 | 需明确宿主契约 | A |
| 19 | `annotations` 语义 | 提示，不可信 | 无 | 需新增字段并声明不可信 | A |
| 20 | 图标 | `icons` + 严格安全处理 | 无 | 可选 | A |
| 21 | `_meta` / 扩展 | 命名空间与保留键 | 无 | 可用命名空间承载私有字段 | A |
| 22 | 依赖/安装 | 不属于 MCP | dytes 的核心能力 | 官方无对应物 | C |
| 23 | 供应链 | 不属于 MCP | SHA-256、索引、锁文件 | dytes 优势，保留 | C |
| 24 | 弃用策略 | 12 个月弃用窗口 + 注册表 | 无 | 可借鉴到 `mcp.json` 演进 | A |

> 注：第 4、7、10、11 行是「若要让第三方 MCP 客户端直接使用 dytes 插件」的硬门槛；
> 第 5、6、7、8、9、19 行是「即使不接官方协议，也应立即对齐」的字段与校验问题。

## 5. 两条改造路线

### 路线 A：静态清单对齐 + 宿主适配层（推荐先做）

保持 `mcp.json` 与 DLL 调用不变，做三件事：

1. **字段与校验对齐官方**（纯 dytes 侧改动，收益最大）：
   - 工具对象增加 `title`、`description`、`annotations`；
   - `name` 按 1–128、字符集、唯一性校验；
   - `inputSchema` / `outputSchema` 必须是合法 JSON Schema，根为 `object`，
     默认方言 2020-12；
   - 把 dytes 私有字段（`ffi_symbol`、`transport`）迁到命名空间下
     （如 `x-dytes`），或保留但明确标注为非标准扩展。
2. **结果适配**：宿主把 `{code, data, msg}` 映射为
   `content[{type:"text"}]` + `structuredContent` + `isError`，让模型能自纠正。
3. **宿主侧合成 tools/list**：宿主把静态清单变成内存中的 tool 列表，
   对模型暴露标准 MCP 语义；dytes 不参与运行时协议。

优点：改动集中在 `plugin_manifest.rs` / `templates.rs` / `ffi.rs`，不动安装链路；
现有包可以平滑迁移（旧字段继续识别，新增字段可选）。
缺点：仍需宿主实现 MCP；第三方 MCP 客户端不能直接连 dytes 安装的插件。

### 路线 B：插件自带 MCP server（长期标准）

插件包额外产出一个 stdio MCP server（可由插件 DLL 或独立 exe 实现），
`mcp.json` 从「工具清单」改为「server 启动配置」：

```json
{
  "server": {
    "transport": "stdio",
    "command": "./<name>-mcp",
    "args": [],
    "env": {}
  }
}
```

dytes 负责安装二进制、校验 checksum、写入注册信息；宿主负责启动子进程并复用官方 SDK。

优点：完全合规，能直接用官方客户端与生态；不再需要自定义 transport 绑定。
缺点：需要实现 `server/discover`、`tools/list`、`tools/call`、`_meta`、`resultType`、
分页/缓存字段、`subscriptions/listen` 等；`library_uuid` 绑定机制需重新设计为
server 内部校验；发布与锁文件模型要扩展。

### 建议

分阶段：**先 A 后 B**。

- 阶段 1（A1）：字段、命名、schema、`annotations`、唯一性校验——纯校验增强，
  不破坏现有包；
- 阶段 2（A2）：结果适配与宿主契约文档化，补 `isError` / `structuredContent`；
- 阶段 3（B）：新增 `mcp.json` v2（`server` 形态），与 v1 共存；
  dytes 只做安装/注册，协议交给官方 SDK。

### 落地进度

- 阶段 1（A1）已完成：`check_mcp` 已对齐工具名字符集/长度/唯一性、
  `inputSchema` 存在与根类型、`$schema` 方言、无参约定、`title` /
  `description` / `annotations` 类型、`transport` 必填与依赖声明、
  引用文件解析、`mcp.json` 未知字段；这些规则当前输出 `WARN`，
  不影响退出码与发布。
- 阶段 2（A2，结果映射）与阶段 3（B，stdio MCP server）尚未开始。

## 6. 代码修改清单

### 6.1 `src/utils/plugin_manifest.rs`

0.2.4 起 `check_mcp` 已从「文件存在 + 对象形状」扩展到下表的大部分规则，
但当前统一输出 `WARN`、不阻断发布；下表「验收」列是最终目标，待后续版本
升级为 `FAIL`。建议按优先级继续补齐：

| 优先级 | 改动 | 验收 |
| --- | --- | --- |
| P0 | 新增常量：工具名最大 128、字符集 `[A-Za-z0-9_.-]` | 超长/非法字符产生 FAIL |
| P0 | 三种形态（引用文件名、内联对象、描述文件）统一解析出 `name`，校验**全包唯一** | 重名 FAIL（标签含 `mcp 工具 name 唯一`） |
| P0 | `inputSchema` 必须存在且为对象、`type == "object"`；`outputSchema` 同 | 缺失或根非 object FAIL |
| P0 | `inputSchema` / `outputSchema` 带 `$schema` 时只接受受支持方言（2020-12、draft-07） | 未知方言 FAIL |
| P0 | 无参工具（`properties` 为空且无 `required`）时提示补 `additionalProperties: false` | 警告级 |
| P1 | 引用形态工具描述文件**必须可解析**，字段校验与内联对象一致 | 坏 JSON FAIL |
| P1 | `transport` 校验：`library` / `library_uuid` 非空字符串；`library` 必须在 `dependencies-sys` | 缺失 FAIL，未声明 FAIL |
| P1 | `title` / `description` 类型为字符串并限制长度（建议 120 / 500） | 类型错 FAIL |
| P1 | `annotations` 五个字段类型校验；未知字段忽略 | 类型错 FAIL |
| P2 | `icons` 结构校验（`src` 为 `https:` 或 `data:`；不允许 `file:` 等） | 非法 scheme FAIL |
| P2 | 顶层未知字段给警告，不静默忽略 | 报告含 WARN |
| P2 | 校验项分级：`CheckItem` 增加 severity（error / warn），`--json` 输出 | 警告不改变退出码 |

配套：`CheckReport` 目前只有 `pass` / `fail`；引入 `warn` 需要同步更新
`print`、`failure_count`、`passed` 与 `publish` 的阻断条件。

### 6.2 `src/utils/templates.rs`

`init_manifest` 的 `design_plugins` 分支应生成与官方一致的骨架：

```json
{
  "tools": [
    {
      "name": "demo",
      "title": "Demo Tool",
      "description": "Describe what this tool does for the model",
      "inputSchema": { "type": "object", "properties": {}, "additionalProperties": false },
      "annotations": { "readOnlyHint": true, "destructiveHint": false },
      "transport": { "library": "demo_lib", "library_uuid": "<uuid>" }
    }
  ]
}
```

同时生成 `mcp.json` 与占位工具文件，避免 `init` 后立即 `check` 失败。

### 6.3 `src/update/update_design.rs`

- `verify_mcp_libraries` 已支持**内联对象**：与引用形态一样读取 `transport`
  并校验 `library` / `library_uuid`。文件读取或解析失败、字段缺失时输出
  带工具名与索引的警告，而不是静默跳过。
  剩余严格化建议：
  - `mcp.json` 或工具文件解析失败直接报错，不再跳过；
  - 缺少 `transport` 的关键字段时升级为中止安装，并给出字段路径。
- 若采用路线 B：新增安装后启动/注册 MCP server 的逻辑，校验可执行文件存在、
  可执行权限、参数与 `env` 合法（禁止把密钥写进 `mcp.json`）。
- `mcp_root` 同时支持包根与 `mcp/` 子目录，建议文档化并在 `check` 中保持一致
  （目前 `check` 只认导出后的包根布局）。

### 6.4 `src/utils/ffi.rs` 与 `src/utils/plugin_lib.rs`

- 64 KiB 输出缓冲对 `structuredContent` 偏小。建议：
  - 调用层返回原始 `Value`，由适配层构造 MCP 结果；
  - 支持分块/截断标记，或在超过缓冲时返回明确错误而不是解析失败；
  - `code != 200` 时映射为 `isError: true` + `content[{type:"text",text:msg}]`，
    而不是抛出宿主异常；
- 现有 `symbol_candidates` 的 `post_` 前缀兼容应当保留，但需在文档中固定为约定，
  避免官方 MCP 客户端场景下出现隐式行为。
- `plugin_lib` 登记时传入的工具数据应已经是「对齐后的 tool 列表」，
  避免宿主自行猜测字段。

### 6.5 文档与测试

- `docs/mcp.md` 增补：字段来源（官方 vs dytes 扩展）、迁移说明、`annotations`
  不可信声明、结果映射规则；
- `docs/commands/check.md` 更新校验项与警告级别；
- 测试（`plugin_manifest.rs` 单元测试）至少覆盖：
  - 名称超长/非法字符/重复；
  - `inputSchema` 缺失、根非 object、未知 `$schema`；
  - 引用文件坏 JSON；
  - `transport.library` 未声明依赖；
  - `annotations` 字段类型错误；
  - 无参工具 `additionalProperties` 警告；
  - 旧版清单（无 `title`/`annotations`）仍然通过（向后兼容）。

## 7. 迁移与兼容策略

| 场景 | 策略 |
| --- | --- |
| 已发布旧包（无 `title` / `annotations`） | 保持通过；新规则以警告提示 |
| 工具名非法（空格、中文、超长） | 先警告一个版本，再升为失败 |
| `inputSchema` 不合法 | 立即失败（官方为 MUST） |
| `ffi_symbol` / `transport` | 保留，但在文档中标注为 dytes 扩展；路线 B 时迁入 `x-dytes` 或 server 配置 |
| 结果格式 | 宿主适配层先兼容 `{code,data,msg}`，新工具可返回 `content`/`structuredContent` |
| 协议版本 | `mcp.json` 顶层新增 `schemaVersion`（先默认 1），为路线 B 的 v2 留位 |

## 8. 需要你决定的事项

1. 目标是「让宿主/模型更好地使用插件工具」，还是「让任何 MCP 客户端都能直连」？
   前者选路线 A，后者必须走路线 B；
2. 是否接受在 `mcp.json` 中新增 `title` / `description` / `annotations` /
   `schemaVersion`（对旧包只警告）；
3. `ffi_symbol`、`transport.library_uuid` 是否作为 dytes 扩展长期保留，
   还是迁入 `x-dytes` 命名空间；
4. 结果协议是否统一为 `content[]` + `structuredContent` + `isError`，
   还是先由宿主适配、dytes 保持 `{code,data,msg}`。

## 9. 参考

- 规范总览：<https://modelcontextprotocol.io/specification/2026-07-28.md>
- Tools：<https://modelcontextprotocol.io/specification/2026-07-28/server/tools.md>
- stdio：<https://modelcontextprotocol.io/specification/2026-07-28/basic/transports/stdio.md>
- Streamable HTTP：<https://modelcontextprotocol.io/specification/2026-07-28/basic/transports/streamable-http.md>
- Discovery：<https://modelcontextprotocol.io/specification/2026-07-28/server/discover.md>
- 版本与兼容：<https://modelcontextprotocol.io/specification/versioning.md>
- 2026-07-28 变更：<https://modelcontextprotocol.io/specification/2026-07-28/changelog.md>
- 上一版（2025-11-25）：<https://modelcontextprotocol.io/specification/2025-11-25/server/tools.md>
