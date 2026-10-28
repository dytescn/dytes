# dytes 统一工具需求

> 状态：需求草案
> 仓库：`dytescn/dytes`
> 项目定位：`dytes` 是一个 AI 设计插件包管理工具
> 目标：把 `install.exe`、`update.exe`、`publish.exe` 合并为一个 `dytes` 可执行文件
> 更新日期：2026-10-28

> 详细改造任务、文件清单与验收项见 [todo.md](todo.md)；完整命令清单见 [cmd.md](cmd.md)。

---

## 1. 背景

`piksel_install` 合并前由三个独立程序组成：

| 程序 | 目录 | 现状 |
| --- | --- | --- |
| `install.exe` | `install/` | Slint 图形安装器，只负责下载 `update.exe`、应用 `dytes.json`，再调用 `update.exe --auto` |
| `update.exe` | `update/` | 读取 `dytes.json` / `dytes.lock`，更新 app / core / sys / web / design_plugins / design_soft / model，执行 `setup.ts` |
| `publish.exe` | `publish/` | 读取当前目录 `dytes.json`，上传产物并更新对应 `packages.json` |

三者共享同一套包模型（`dytes.json`、`dytes.lock`、`packages.json`、S3 路径），但入口分散、
参数风格不一致、安装器只能 GUI 操作，插件与能力库的安装也缺少统一命令。

**最直接的痛点是 S3 凭证管理**：`install/src/utils/s3.rs`、`update/src/utils/loader.rs`、
`publish/src/utils/loader.rs` 各自硬编码同一组 endpoint / bucket / access key / secret key。
轮换凭证需要修改三处源码并重新构建三个程序，且凭证随源码分发，无法按读写权限分离。

本需求把三者合并为一个 CLI：**`dytes`**，提供单一 S3 凭证入口，
并新增 `dytes init` 生成包清单。

---

## 2. 目标

1. **一个二进制**：`dytes` 提供 `install` / `update` / `publish` / `init` / `config` 子命令。
2. **安装**：`dytes install` 可安装应用、插件与依赖包，不再依赖独立安装器。
3. **更新**：`dytes update` 覆盖现有 `update.exe` 的全部模式，并保留 `--auto` / `--startup` 等内部调用。
4. **发布**：`dytes publish` 覆盖现有发布流程，并支持 `--dry-run`。
5. **初始化**：`dytes init` 生成合法 `dytes.json`，减少手写清单错误。
6. **凭证统一**：S3 endpoint / bucket / key / secret 只在一处配置，支持环境变量与配置文件。
7. **兼容**：旧 `install.exe` / `update.exe` / `publish.exe` 保留为薄封装或别名，已有脚本不失效。

---

## 3. 产品定义

### 3.1 命令形态

```bash
dytes <command> [options]
```

| 命令 | 作用 |
| --- | --- |
| `dytes install` | 安装应用、插件或指定包 |
| `dytes update` | 检查并安装依赖更新 |
| `dytes publish` | 发布当前包到对象存储 |
| `dytes init` | 初始化 `dytes.json` |
| `dytes config` | 查看 / 设置 S3 凭证与平台 |
| `dytes help` / `dytes --help` | 帮助 |

无子命令时输出帮助，不启动 GUI；需要图形安装时使用 `dytes install --gui`。

### 3.2 全局选项

| 选项 | 说明 |
| --- | --- |
| `--dir <path>` | 工作目录；默认当前目录 |
| `--config <path>` | 指定 `dytes.json`；默认 `<dir>/dytes.json` |
| `--offline` | 不访问网络（与 `--no-online` 等价） |
| `--json` | 机器可读输出，便于 CI / 脚本 |
| `--verbose` / `-v` | 输出详细日志 |
| `--yes` / `-y` | 跳过交互确认 |
| `--dry-run` | 只展示计划，不写文件 / 不上传 |
| `--help` | 子命令帮助 |

退出码：

| code | 含义 |
| --- | --- |
| `0` | 成功 |
| `1` | 一般失败（网络、校验、下载、安装） |
| `2` | 参数错误 |
| `3` | 配置 / 清单错误 |
| `4` | 有可用更新（仅 `update --check`） |

---

## 4. 包与远程布局

### 4.1 包类型

沿用 `dytes.json` 的 `type`：

| type | 安装 / 读取位置 | 索引 |
| --- | --- | --- |
| `app` | 安装根目录 | `app/packages.json` |
| `core` | 安装根目录 / `libary/` | `core/packages.json` |
| `sys` | `libary/` | `system/packages.json` |
| `web` | `web/<name>/` | `webs/packages.json` |
| `design_plugins` | `plugins/<name>/`（兼容 `libary/`） | `design_plugins/packages.json` |
| `design_soft` | `apps/<name>/` 或设计软件自己的下载目录 | `design_soft/packages.json` |
| `model` | `models/` | `models/packages.json` |

### 4.2 清单字段

`dytes.json` 至少包含：

```json
{
  "name": "piksel",
  "version": "1.1.13",
  "type": "app",
  "exports": {},
  "platform-exports": {},
  "init": [],
  "dependencies-core": {},
  "dependencies-sys": {},
  "dependencies-web": {},
  "dependencies-design-plugins": {},
  "dependencies-design-soft": {},
  "dependencies-model": {}
}
```

约束：

- `name`、`version`、`type` 必填；
- `exports` 与 `platform-exports` 至少一个非空；
- `init` 脚本必须存在于包内；
- 依赖版本使用 semver；支持 `^` / `~`；
- 凭证和存储地址通过环境变量 / 配置注入，不写入清单。

### 4.3 锁文件

`dytes.lock` 记录已安装包与版本；更新、安装、发布前都必须与 `dytes.json` 校验。
缺失时按首次安装处理。

### 4.4 S3 凭证与配置

**现状问题**：三个程序各自硬编码：

```text
ENDPOINT_URL = "https://s3.api.upyun.com"
ACCESS_KEY   = "..."
SECRET_KEY   = "..."
BUCKET_NAME  = "packages"
```

对应文件：

- `install/src/utils/s3.rs`
- `update/src/utils/loader.rs`
- `publish/src/utils/loader.rs`

轮换凭证需要同时修改三处并重新构建，且无法区分只读（install / update）与读写（publish）权限。

**目标**：

- 凭证与 endpoint / bucket 统一由 `dytes config` 管理；
- 支持环境变量覆盖，便于 CI；
- 源码中不出现真实凭证；
- 日志、`--json`、错误信息默认脱敏；
- `install` / `update` 可使用只读凭证，`publish` 需要写权限；
- 配置文件优先级：环境变量 > 用户配置 > 非敏感默认值。

详细命令、配置键与文件位置见 [cmd.md](cmd.md) §3。

---

## 5. `dytes install`

### 5.1 用途

安装应用、插件或指定包；默认安装 Piksel 应用。

### 5.2 语法

```bash
# 安装应用（默认 piksel）
dytes install [--dir <path>] [--version <ver>] [--gui]

# 安装插件（design_plugins）
dytes install plugin <name>[@<version>] [--dir <path>]

# 安装其他包类型
dytes install sys <name>[@<version>]
dytes install web <name>[@<version>]
dytes install core <name>[@<version>]
dytes install design-soft <name>[@<version>]
dytes install model <name>[@<version>]
```

类型别名：

| 别名 | 标准 type |
| --- | --- |
| `app` | `app` |
| `core` | `core` |
| `sys` | `sys` |
| `web` | `web` |
| `plugin` / `plugins` | `design_plugins` |
| `design-soft` | `design_soft` |
| `model` | `model` |

### 5.3 应用安装流程

1. 创建安装目录；
2. 用内置更新引擎准备 core 包；过渡期若目标目录没有 `dytes`，可下载
   `core/update` 的 `update.exe` 作为引导，再执行 `dytes update --auto`；
3. 获取 `app/<app>/dytes.json`，写入安装目录；
4. 调用同一二进制的更新引擎（目标形态），等价于 `dytes update --auto`；
5. 生成 / 更新 `dytes.lock`；
6. 保留安装日志 `install.log`；
7. 返回应用可执行文件路径，不自动启动（`--launch` 可选）。

> 目标形态不依赖外部 `update.exe`；引导下载仅为兼容旧安装目录。

### 5.4 插件 / 其他包安装流程

1. 校验类型、名称、版本；
2. 读取对应 `packages.json`；
3. 下载 `exports` / `platform-exports`；
4. 写入目标目录：
   - `design_plugins` → `plugins/<name>/`；
   - `sys` → `libary/`；
   - `web` → `web/<name>/`；
   - `model` → `models/`；
   - `design_soft` → `apps/<name>/`；
5. 写入 / 更新 `dytes.lock`；
6. `web` 与需要初始化的包执行 `init` 脚本；
7. 可选调用 `plugin_lib.post_scan` 重新登记插件能力。

### 5.5 选项

| 选项 | 说明 |
| --- | --- |
| `--version <ver>` | 指定版本；缺省取索引最新版本 |
| `--dir <path>` | 安装目录 |
| `--gui` | 启动现有 Slint 安装界面 |
| `--force` | 已安装时重新下载覆盖 |
| `--launch` | 应用安装完成后启动 |
| `--no-init` | 不执行 `setup.ts` |
| `--offline` | 仅使用本地缓存 / 已下载包 |

### 5.6 要求

- 安装目录必须可写；
- 下载失败不留下半成品；已写入文件需回滚或记录；
- 重复安装同一版本应幂等；
- 安装插件后，插件目录与 `dytes.lock` 必须一致；
- 无网络时给出明确错误，不回退到旧版本。

---

## 6. `dytes update`

### 6.1 用途

检查并安装 `dytes.json` 声明的依赖更新。

### 6.2 语法

```bash
dytes update [--all] [--type <type,...>] [--offline] [--check] [--discover] [--init] [--startup] [--auto]
```

### 6.3 行为

```text
读取 dytes.json + dytes.lock
  → 先更新应用自身（app）
  → 按 core / sys / web / design_plugins / design_soft / model 顺序检查
  → 线上模式：以 packages.json 最新版本为准
  → 离线模式：按本地声明版本检查
  → 下载并解包
  → 执行 web / 插件 init 脚本
  → 写回 dytes.lock
  → 汇总失败项
```

### 6.4 选项

| 选项 | 说明 |
| --- | --- |
| `--all` | 更新全部（默认） |
| `--type <list>` | 只更新指定类型，逗号分隔 |
| `--offline` / `--no-online` | 不访问网络 |
| `--check` | 只检查，不下载；有更新返回 `4` |
| `--discover` | 扫描线上索引，列出未声明包 |
| `--init` | 只重跑已安装 Web 包的 `setup.ts` |
| `--startup` | 启动时更新；遵守 `auto_update` 与 1 天节流 |
| `--auto` | 静默自动更新（安装器内部调用） |
| `--force` | 忽略版本比较，强制重装 |

### 6.5 要求

- 更新应用自身后重新读取 `dytes.json`，依赖列表以新配置为准；
- 单个依赖失败不阻断其它依赖，但最终返回非 0；
- `dytes.lock` 只在安装成功后更新；
- 更新 `sys` 包时执行 `setup`；
- 更新 `web` 包时执行 `setup.ts`；
- `--startup` 与 `--auto` 行为兼容现有 `update.exe`；
- 失败时保留旧版本，不删除可回退文件。

---

## 7. `dytes publish`

### 7.1 用途

把当前目录的包发布到对象存储，并更新对应 `packages.json`。

### 7.2 语法

```bash
dytes publish [--dir <path>] [--dry-run] [--platform <platform>] [--force]
```

### 7.3 流程

1. 读取并校验 `dytes.json`；
2. 选择 `exports` 或当前平台 `platform-exports`；
3. 上传每个导出文件到：
   `./<type>/<name>/<version>/[<platform>/]<file>`；
4. 上传 `dytes.json`；
5. 所有文件成功后再更新 `<type>/packages.json`；
6. 输出上传清单与最终版本。

### 7.4 要求

- `--dry-run` 只打印将上传的文件与远程路径；
- 任一文件上传失败不更新 `packages.json`；
- `packages.json` 不存在时允许创建；
- `type` 与远程前缀映射：
  - `app` → `app/`
  - `core` → `core/`
  - `sys` → `system/`
  - `web` → `webs/`
  - `design_plugins` → `design_plugins/`
  - `design_soft` → `design_soft/`
  - `model` → `models/`
- 平台导出使用 `platform-exports`；没有平台配置时回退 `exports`；
- 导出目标文件名不允许包含路径分隔符（当前 `publish` 的限制保留）；
- 凭证错误必须给出明确提示，不打印密钥。

---

## 8. `dytes init`

### 8.1 用途

在当前目录创建 / 补全 `dytes.json`。

### 8.2 语法

```bash
dytes init [--type <type>] [--name <name>] [--version <ver>] [--dir <path>] [--force]
```

### 8.3 模板

| type | 默认字段 |
| --- | --- |
| `app` | `exports` 指向 `target/release/<name>.exe`；`dependencies-*` 空 |
| `core` | `exports` 指向可执行文件；`dependencies` 空 |
| `sys` | 默认 `platform-exports.windows` → `libary/<name>.dll` |
| `web` | `exports` 包含 `setup.ts`；`init: ["./setup.ts"]` |
| `design_plugins` | `targets: []`、`use_ai: false`、`exports` 指向 DLL + `mcp.json` |
| `design_soft` | `exports` 指向单文件 exe；`type: design_soft` |
| `model` | `exports` 指向模型文件 |

### 8.4 要求

- 默认 `name` 取目录名，默认 `version` 为 `0.1.0`；
- `dytes.json` 已存在时默认拒绝覆盖，`--force` 才重写；
- 生成前校验 `name` / `type` / `version`；
- 输出生成路径与后续命令建议：
  ```text
  已生成 dytes.json
  下一步：
    dytes publish --dry-run
    dytes publish
  ```
- 不自动创建构建产物；
- 不写入凭证。

---

## 9. 兼容与迁移

### 9.1 旧入口

| 旧命令 | 新等价 |
| --- | --- |
| `install.exe` | `dytes install --gui` |
| `update.exe` | `dytes update` |
| `update.exe --auto` | `dytes update --auto` |
| `update.exe --startup` | `dytes update --startup` |
| `update.exe --discover` | `dytes update --discover` |
| `update.exe --init` | `dytes update --init` |
| `publish.exe` | `dytes publish` |

要求：

- 旧二进制至少保留一个版本，作为 `dytes` 的薄封装；
- 旧参数继续可用，并打印一次迁移提示；
- 安装器内部调用改为 `dytes update --auto`，但需兼容尚未更新的旧安装目录。

### 9.2 迁移阶段

1. 新增 `dytes` 主程序与四个子命令；
2. 把 `install` / `update` / `publish` 的核心逻辑抽成库模块；
3. 旧二进制改为调用库；
4. 安装器与 `update.exe` 的互相调用切换到 `dytes`；
5. 文档、测试与发布流程更新；
6. 下一版本移除旧入口（或保留别名）。

---

## 10. 非功能需求

| 类别 | 要求 |
| --- | --- |
| 幂等 | 重复 install / update / init 不产生重复记录或重复文件 |
| 原子性 | 下载失败、校验失败不更新锁文件；publish 不提前更新索引 |
| 网络 | 超时、重试、断点 / 失败清理；`--offline` 明确行为 |
| 安全 | 凭证只从配置 / 环境变量读取；日志脱敏；包校验可选 |
| 跨平台 | Windows 为主；路径、扩展名、平台导出按现有规则 |
| 日志 | 控制台 + 文件；`--json` 提供结构化结果 |
| 性能 | 更新只下载变化文件；大包显示进度 |
| 可测试性 | 业务逻辑与 CLI / GUI 解耦，核心模块可单测 |
| 兼容性 | 旧 `dytes.json` / `dytes.lock` / `packages.json` 不需要迁移 |

---

## 11. 验收标准

### `dytes install`

- 空目录执行 `dytes install` 能安装 Piksel 并生成 `dytes.lock`；
- `dytes install plugin cdr_check_txt` 安装到 `plugins/cdr_check_txt/`；
- 重复安装同版本不重复下载 / 不重复登记；
- 无网络时返回明确错误。

### `dytes update`

- `dytes update --check` 有更新返回 `4`，无更新返回 `0`；
- `dytes update --offline` 不访问网络；
- `dytes update --init` 只执行已安装 Web 包的 `setup.ts`；
- 单个包失败时其它包继续，最终返回非 0；
- 成功后 `dytes.lock` 与磁盘版本一致。

### `dytes publish`

- `--dry-run` 不上传、不更新索引；
- 上传失败时 `packages.json` 保持旧版本；
- `platform-exports` 选择正确；
- 敏感信息不出现在日志。

### `dytes init`

- 七种 type 均能生成合法 `dytes.json`；
- 已存在文件默认拒绝，`--force` 可覆盖；
- 生成后 `dytes publish --dry-run` 能通过清单校验。

---

## 12. 不在本期范围

- 新的对象存储后端；
- 包签名 / 哈希校验（可列为后续增强）；
- GUI 安装器重写；
- 插件市场 UI；
- 依赖冲突的自动降级 / 自动回滚（先保证失败可重试）；
- 跨平台打包矩阵扩展。

---

## 13. 待确认

1. `dytes install` 默认安装 Piksel，还是必须显式 `dytes install app`？
2. 插件安装是否统一为 `dytes install plugin <name>`，还是 `dytes install design_plugins <name>`？
3. `dytes publish` 是否需要发布前自动执行构建 / 打包？
4. 旧 `install.exe` / `update.exe` / `publish.exe` 保留多久？
5. 是否需要 `dytes uninstall`（本期不实现，后续评估）。

---

## 14. 现有实现映射

| 现有代码 | 迁移到 |
| --- | --- |
| `install/src/service/installer.rs` | `dytes install` 应用安装流程 |
| `install/src/main.rs` | `dytes install --gui` |
| `update/src/run/dytes.rs` | `dytes update` |
| `update/src/run/update_*.rs` | `dytes update` 各类型更新器 |
| `update/src/utils/dytes.rs` | 共享包模型 / 锁文件 |
| `publish/src/run/dytes.rs` | `dytes publish` |
| `publish/src/utils/platform.rs` | 平台导出选择 |

> 共享逻辑已经合并到根 crate 的 `src/shared`，避免三个子命令重复实现。
