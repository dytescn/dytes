# dytes 设计说明

## 1. 定位

`dytes` 是 Piksel 的包管理工具，单一二进制提供安装、更新、发布、初始化和配置能力。
命令与参数见 [cmd.md](cmd.md)。

## 2. 代码结构

```text
src/
├── main.rs        CLI 定义与分派
├── install/       安装引擎（installer / package / s3 / dirs / cli）
├── update/        更新引擎（dytes / update_* / self_update / loader / script …）
├── publish/       发布（dytes / loader / cli）
├── test.rs        包自检与离线安装演练
├── init.rs        dytes init
└── utils/         共用层：清单、平台、版本、校验、凭据、配置、模板、S3、GUI
ui/                Slint 页面（gui feature）
tests/             集成测试
```

共用逻辑只放在 `src/utils`；`install` / `update` / `publish` 内部不再有子目录。
约定见 [../skills/code.md](../skills/code.md)。

## 3. 包模型

### 3.1 类型与远程布局

| type | 安装位置 | 远程前缀 | 索引 |
| --- | --- | --- | --- |
| `app` | 安装根目录 | `app/` | `app/packages.json` |
| `core` | 安装根目录 / `libary/` | `core/` | `core/packages.json` |
| `sys` | `libary/` | `system/` | `system/packages.json` |
| `web` | `web/<name>/` | `webs/` | `webs/packages.json` |
| `design_plugins` | `plugins/<name>/` | `design_plugins/` | `design_plugins/packages.json` |
| `design_soft` | `apps/<name>/` | `design_soft/` | `design_soft/packages.json` |
| `model` | `models/` | `models/` | `models/packages.json` |

产物路径：`<prefix>/<name>/<version>/[<platform>/]<file>`。

### 3.2 清单 `dytes.json`

```json
{
  "name": "info_lib",
  "uuid": "8b2cd615-eacf-43fb-a2c9-d6f0a8195946",
  "version": "0.1.0",
  "type": "sys",
  "exports": {},
  "platform-exports": { "windows": { "./target/release/info_lib.dll": "libary/info_lib.dll" } },
  "init": [],
  "dependencies-core": {},
  "dependencies-sys": {},
  "dependencies-web": {},
  "dependencies-model": {},
  "test-dependency": {
    "sys": { "info_lib": "0.1.0" },
    "web": { "setting": "0.2.0" }
  }
}
```

- `name`、`uuid`、`version`、`type` 必填；
- `exports` 与 `platform-exports` 至少一个非空（`--manifest-only` 包除外）；
- 依赖版本使用 semver，支持 `^` / `~` 与 `?` / `optional:` 可选标记；
- `design_plugins` / `design_soft` 不写入应用根清单，只记录在 `dytes.lock`；
- `test-dependency` 按包类型分组，仅用于 `dytes test`；它不会参与普通
  `update`，测试时只把本地已安装的对应依赖复制到 `.dytes/env/`。

### 3.3 锁文件 `dytes.lock`

记录已安装包的 `uuid`、`name`、`type`、`version`、`loader` 与声明版本；
新记录以 `uuid` 为主键，旧锁文件按 `name` 兼容读取。安装成功后才更新。

## 4. 配置与凭证

优先级：环境变量 > 用户配置 > 内置默认值。环境变量与配置键见
[cmd.md §3](cmd.md#3-dytes-config)。

- 日志、错误与 `--json` 默认不输出密钥；
- `install` / `update` 只需要读权限，`publish` 需要写权限；
- 配置文件 Unix 权限 `0600`，Windows 使用用户配置目录默认 ACL。

## 5. 关键行为

- **install**：应用安装走更新引擎；插件下载完成后通过 FFI 调用 `plugin_lib` 的
  `install_plugin` / `update_plugin` 登记插件数据库，设计软件只写包目录；
  依赖包先写入应用清单，再只安装目标包。
- **update**：支持全量、按类型、离线、检查、启动更新；单项失败不阻断其它项。
- **publish**：先上传全部产物，再更新 `packages.json`；`--dry-run` 不联网。
- **check**：离线校验清单与导出文件，设计插件执行完整规则。
- **test**：复用 check 规则；自动创建 `.dytes/` 并写入 `.gitignore`，把
  `test-dependency` 声明的本地依赖复制到 `.dytes/env/`，再在 `.dytes/` 内
  演练安装并生成锁文件。
- **self-update**：读取 `core/dytes` 清单，下载并用 `.sha256` 校验后替换自身。

## 6. 平台

当前平台由 `DYTES_PLATFORM` / `PIKSEL_PLATFORM` 或系统决定；
`Win32`、`darwin`、`osx` 等别名归一化为 `windows` / `macos`。
发布与安装按 `platform-exports` 选择文件，未配置时回退 `exports`。

## 7. 约束

| 类别 | 要求 |
| --- | --- |
| 幂等 | 重复 install / update / init 不产生重复记录或文件 |
| 原子性 | 校验或下载失败不更新锁文件与索引 |
| 网络 | 超时、重试、失败清理；`--offline` 行为明确 |
| 安全 | 凭证只从配置 / 环境读取；日志脱敏；路径拒绝 `..` |
| 跨平台 | Windows / Linux / macOS，路径、扩展名、平台导出一致 |
| 可测试性 | 业务逻辑与 CLI / GUI 解耦；集成测试见 `tests/` |
| 测试隔离 | 测试产物只写 `.dytes/`；依赖从本地安装目录复制到 `.dytes/env/` |

## 8. 测试

集成测试位于 `tests/`，按命令分文件，通过真实二进制验证退出码与输出。
测试工作目录统一使用 `.dytes/`，规范见 [../skills/testenv.md](../skills/testenv.md)。
