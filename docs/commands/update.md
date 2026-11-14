# `dytes update`

## 1. 职责

按当前目录的 `dytes.json` 检查并安装应用自身与全部依赖更新。
更新顺序固定为：应用 → `core` → `sys` → `web` → `design_plugins` →
`design_soft` → `model`。单个依赖失败不阻断其它依赖，最终返回非 0。

同时提供只检查、只执行 init 脚本、扫描线上未声明包、启动时更新与图形更新窗口。

## 2. 用法

```bash
dytes update [OPTIONS]
```

| 参数 | 说明 | 当前状态 |
| --- | --- | --- |
| `--type <list>` | 只更新指定类型，逗号分隔：`core,sys,web,design_plugins,design_soft,model` | 已实现（`--check`） |
| `--check` | 只检查不下载；有更新返回 `4` | 已实现 |
| `--offline` / `--no-online` | 不访问网络 | 已实现 |
| `--discover` | 扫描线上索引，列出未声明包 | 已实现 |
| `--init` | 只重跑已安装 Web / sys 包的 `setup.json` | 已实现 |
| `--startup` | 启动时更新：遵守 `auto_update` 与 1 天节流 | 已实现 |
| `--dir <path>` | 工作目录 | 已实现 |
| `--gui` | 图形更新窗口 | 需要 `--features gui` |
| `--json` | JSON 输出 | 已实现（`--check`） |
| `--all` | 兼容参数 | **尚未接线** |
| `--auto` / `--install` | 兼容参数 | **尚未接线** |
| `--force` | 兼容参数 | **尚未接线** |

`--startup` 使用独立线程（32 MiB 栈）并设置 8 秒总超时，超时后本次跳过并返回成功，
避免阻塞宿主启动。

## 3. 执行流程

1. `--gui`：进入图形更新窗口；未启用 feature 返回 `2`。
2. 处理 `--dir`，切换工作目录。
3. `--startup`：读取更新节流状态；不允许时直接成功返回；启用快速失败模式。
4. 创建异步运行时，按参数分派：
   - `--discover`：扫描线上设计包目录；
   - `--init`：只执行已安装包的 init / setup；
   - `--check`：生成更新报告，有更新返回 `4`；
   - 默认：执行完整更新。
5. 完整更新：
   - 先更新应用自身；
   - 展开依赖计划并排序；
   - 逐项更新，记录失败但继续；
   - 执行 init 脚本（sys 的 setup 先于 web init，`init` 包最优先）；
   - 结束后写 `dytes.lock`；
   - 有任何失败则整体返回非 0。

## 4. `--check --json` 输出

```json
{
  "check": true,
  "types": ["design_plugins"],
  "count": 1,
  "updates": [
    {
      "type": "design_plugins",
      "name": "cdr_check_txt",
      "local_version": "1.0.0",
      "remote_version": "1.1.0",
      "needs_update": true
    }
  ]
}
```

## 5. 文件与状态

| 文件 | 作用 |
| --- | --- |
| `dytes.json` | 应用清单与依赖声明 |
| `dytes.lock` | 实际安装版本、uuid、loader 映射 |
| `.dytes/` | 本地测试与演练目录 |
| 用户状态文件 | `--startup` 的最近检查时间与 `auto_update` 开关 |

## 6. 退出码

| 退出码 | 场景 |
| --- | --- |
| `0` | 全部成功 / 全部最新 / 启动更新被节流或超时 |
| `1` | 下载、校验、安装、网络失败 |
| `2` | 参数错误（未知类型等） |
| `3` | 清单或配置错误 |
| `4` | `--check` 发现可用更新 |

## 7. 测试点

- `--check` 无更新返回 `0`，有更新返回 `4`；
- `--check --json` 字段与计数；
- `--offline` 不发起网络请求；
- `--init` 只执行 setup，不下载；
- 单个依赖失败不阻断后续项；
- `--startup` 节流与超时；
- 更新失败不写锁文件。

对应测试文件：`tests/cli_update.rs`、`tests/cli_global.rs`。

## 8. 后续规划

1. 接线 `--all` / `--auto` / `--force`，或在帮助中移除避免误导；
2. `--type` 在完整更新链路生效，而不只影响 `--check`；
3. `--json` 覆盖完整更新过程（事件流而非最终结果）；
4. 依赖计划支持并行下载，写锁保持串行；
5. 失败项输出可重试命令；
6. 明确处理网络超时、重试与断点续传。
