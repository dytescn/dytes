# `dytes test`

## 1. 职责

统一管理包的本地测试环境，并在发布前完成自检与安装演练：

1. 在包目录创建 `.dytes/`，自动确保它被 `.gitignore` 忽略；
2. 复用 `dytes check` 的规则校验清单、平台导出与设计插件；
3. 把 `test-dependency` 声明的、本地已安装的依赖复制到 `.dytes/env/`；
4. 在 `.dytes/` 内按安装布局演练当前包落盘与 `dytes.lock` 生成。

`test` 不访问网络、不执行 `setup.json`、不修改真实安装目录，适合本地开发与
CI 的发布前门禁。

## 2. 用法

```bash
dytes test [<dir>] [--type <type>] [--platform <platform>]
           [--manifest-only] [--work-dir <path>] [--keep] [--json]
```

| 参数 | 说明 | 当前状态 |
| --- | --- | --- |
| `[DIR]` | 包目录，默认当前目录 | 已实现 |
| `--type <type>` | 只接受指定类型；不一致返回 `3` | 已实现 |
| `--platform <platform>` | 选择 `platform-exports`，默认当前平台 | 已实现 |
| `--manifest-only` | 允许没有 exports 的仅清单包 | 已实现 |
| `--work-dir <path>` | 演练目录，相对 `.dytes/`；必须不存在 | 已实现 |
| `--keep` | 保留演练目录，便于排查 | 已实现 |
| `--json` | JSON 输出 | 已实现 |

## 3. `.dytes` 目录

所有测试产物都放在包目录的 `.dytes/` 下：

```text
.dytes/
├── env/                         # 测试依赖环境（每次 test 重建）
│   ├── libary/                  # sys / core 依赖
│   ├── web/<name>/              # web 依赖
│   ├── plugins/<name>/          # design_plugins 依赖
│   ├── apps/<name>/             # design_soft 依赖
│   └── models/<name>/           # model 依赖
└── run-<pid>/
    └── dytes-test-<时间戳>/     # 当前包的安装演练目录
```

- `.dytes/` 不存在时自动创建；
- 首次执行时自动创建或追加 `.gitignore`，写入 `.dytes/`，避免误提交；
- `.dytes/env/` 每次执行前清空重建，防止旧依赖残留；
- 演练目录默认在结束后删除，`--keep` 保留；
- `--work-dir` 只能是 `.dytes/` 内的相对路径，不能包含 `..`。

## 4. `test-dependency`

在 `dytes.json` 中按包类型声明测试依赖，结构与现有 `dependencies-*` 风格一致：

```json
{
  "test-dependency": {
    "sys": {
      "info_lib": "0.1.0"
    },
    "web": {
      "setting": "0.2.0"
    },
    "design_plugins": {
      "cdr_check_txt": "1.1.0"
    }
  }
}
```

- 键为包类型：`core` / `sys` / `web` / `design_plugins` / `design_soft` / `model`；
- 值为 `包名 → 声明版本`；
- 未知类型会让清单校验失败（退出码 `3`）；
- `test` 不会下载依赖，只复制本地已安装的文件。

依赖来源目录默认为当前包目录；如果测试包不是应用根目录，可用
`DYTES_APP_DIR` 指向真实应用目录。

## 5. 执行流程

### 5.1 准备测试目录

1. 创建 `<包目录>/.dytes/`；
2. 清空并创建 `.dytes/env/`；
3. 检查 `.gitignore`：
   - 不存在 → 创建并写入 `.dytes/`；
   - 存在但没有该规则 → 追加；
   - 已有该规则 → 不修改。

### 5.2 清单自检

直接调用 `check` 的校验实现：

- 清单可解析、类型合法、`test-dependency` 类型合法；
- 当前平台导出存在且源文件存在；
- 设计插件执行完整规则。

任何一项失败都不会继续复制依赖或演练。

### 5.3 复制测试依赖

1. 读取 `test-dependency`；
2. 从 `DYTES_APP_DIR`（默认包目录）读取 `dytes.lock`；
3. 按类型复制到 `.dytes/env/`：
   - `sys`：复制 `libary/<name>/`，并按锁文件 `loader` 复制导出文件；
   - `core`：按锁文件 `loader` 复制，缺失时回退到 `<name>` / `libary/<name>`；
   - `web` / `design_plugins` / `design_soft` / `model`：复制对应包目录；
4. 任一声明依赖找不到时报失败，测试环境不完整。

### 5.4 安装演练

1. 在 `.dytes/` 内创建演练目录；
2. 按安装布局写入 `dytes.json`；
3. 对每个导出：
   - 拒绝绝对路径、盘符与 `..`；
   - 检查目标名不重复、源文件存在；
   - 清单含 `checksums` 时校验 SHA-256 / 大小 / 采样模式；
   - 复制到演练目录；
4. 对每个 `init` 脚本：校验路径、源文件并复制；
5. 写入并回读 `dytes.lock`；
6. 未指定 `--keep` 时删除演练目录。

## 6. 安装布局

| type | 导出目录（相对演练根） | 清单目录 |
| --- | --- | --- |
| `app` / `core` | `.` | `.` |
| `sys` | `libary/`（`dest` 已带 `libary/` 前缀时不重复添加） | `libary/<name>/` |
| `web` | `web/<name>/` | `web/<name>/` |
| `design_plugins` | `plugins/<name>/` | `plugins/<name>/` |
| `design_soft` | `apps/<name>/` | `apps/<name>/` |
| `model` | `models/<name>/` | `models/<name>/` |

## 7. 输出

文本：

```text
PASS dytes.json 可解析  D:\pkg\dytes.json
PASS exports 非空  1 个
PASS exports 文件存在: ./bin/demo.exe  D:\pkg\bin\demo.exe
PASS 测试目录已就绪  D:\pkg\.dytes
PASS .gitignore 已追加 .dytes/  D:\pkg\.gitignore
PASS 测试依赖 sys/info_lib  D:\pkg\.dytes\env
PASS 演练目录已创建  D:\pkg\.dytes\run-1234\dytes-test-...
PASS 清单已写入演练目录  ...\dytes.json
PASS 导出文件可落盘: demo.exe  ...\demo.exe
PASS 锁文件可序列化  ...\dytes.lock

测试目录: D:\pkg\.dytes
依赖目录: D:\pkg\.dytes\env
演练目录: D:\pkg\.dytes\run-1234\dytes-test-...
```

`--json`：

```json
{
  "passed": true,
  "dir": "D:\\pkg",
  "name": "demo",
  "type": "app",
  "version": "0.1.0",
  "uuid": "11111111-1111-1111-1111-111111111111",
  "platform": "windows",
  "manifest_only": false,
  "dytes_dir": "D:\\pkg\\.dytes",
  "env_dir": "D:\\pkg\\.dytes\\env",
  "gitignore": "updated",
  "dependencies": [
    {
      "type": "sys",
      "name": "info_lib",
      "source": "D:\\pkg",
      "target": "D:\\pkg\\.dytes\\env",
      "files": ["libary/info_lib.dll"]
    }
  ],
  "checks": [
    { "label": "dytes.json 可解析", "ok": true, "detail": "D:\\pkg\\dytes.json" }
  ],
  "work_dir": "D:\\pkg\\.dytes\\run-1234\\dytes-test-...",
  "kept": false,
  "files": [
    {
      "source": "./bin/demo.exe",
      "target": "D:\\pkg\\.dytes\\run-1234\\dytes-test-...\\demo.exe"
    }
  ]
}
```

未使用 `--keep` 时，`work_dir` 在命令结束后已被删除；`.dytes/env` 会保留。

## 8. 退出码

| 退出码 | 场景 |
| --- | --- |
| `0` | 自检、依赖复制与演练全部通过 |
| `1` | 创建 `.dytes` / `.gitignore` / 演练目录或复制文件等 IO 失败 |
| `2` | 参数错误；`--work-dir` 越界或已存在；未知类型 |
| `3` | 清单、`test-dependency`、导出、init、checksum、路径安全或锁文件校验失败 |

## 9. 边界与约束

- 全程离线，不读取 S3，不调用 FFI，不修改真实安装目录；
- 只复制依赖文件，不执行 `setup.json`，不启动应用；
- `.dytes/` 是生成目录，可随时删除；`test` 会自动重建；
- 自动维护 `.gitignore`，但不会执行 `git add` 或提交；
- 路径安全：导出目标、源路径、init 路径、依赖路径都拒绝绝对路径、盘符、`..`；
- 包名必须是不含路径分隔符的单段名称；
- `test-dependency` 的版本是声明值，复制时不重新下载或校验远端版本。

## 10. 测试点

- 正常包通过，演练目录生成 `dytes.json`、导出文件与 `dytes.lock`；
- 自动创建 `.dytes/`、`.dytes/env/` 与 `.gitignore` 规则，重复执行不重复追加；
- `test-dependency` 的 sys / web 依赖复制到 `.dytes/env/`；
- 声明的依赖未安装时返回 `3`；
- 非法 `test-dependency` 类型返回 `3`；
- 导出源文件缺失、目标为 `../evil.exe` 时返回 `3`；
- `init` 脚本缺失返回 `3`；
- `--manifest-only` 无导出也能通过；
- `--work-dir ../outside` 返回 `2`；
- `--json` 输出 `dytes_dir` / `env_dir` / `dependencies` / `files` / `checks`。

对应测试文件：`tests/cli_test.rs`。测试目录规范见
[../../skills/testenv.md](../../skills/testenv.md)。

## 11. 后续规划

1. 增加 `--run-init`：在 `.dytes/env` 中执行 `setup.json`（需要宿主环境，默认关闭）；
2. 为 `test-dependency` 增加版本匹配检查与缺失时的在线安装选项；
3. 输出失败项的可修复建议；
4. 与 CI 集成：`dytes test --json` 作为发布前门禁；
5. 为设计插件增加 DLL 依赖与 MCP 运行时检查；
6. 支持 `.dytes/env` 缓存与增量复制，减少大依赖的重复复制。
