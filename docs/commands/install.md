# `dytes install`

## 1. 职责

把一个已发布的包安装到当前应用目录。按类型分为三条链路：

- **默认（未指定类型）**：等价于 `dytes install core piksel`。
- **应用**（`app`）：检查安装目录是否已有 `dytes.json`：已有则报错（退出码
  `3`），提示改用 `dytes update`；没有则按 `app/<name>/dytes.json` 安装指定的
  应用（`--launch` 安装后启动）。
- **设计包**（`plugin` / `design-soft`）：复用更新引擎下载到 `plugins/<name>/`
  或 `apps/<name>/`，只写 `dytes.lock`，不修改应用根 `dytes.json`；插件安装完成后
  通过 FFI 同步插件数据库。
- **依赖包**（`sys` / `core` / `web` / `model`）：先检查应用根 `dytes.json`，
  缺失时自动初始化一个 `type = app` 的最小清单；再把依赖写入清单并只安装目标包。

安装入口同时也承担图形安装器的 `--gui` 分支。图形安装器会先写入应用清单，
再在安装目录运行更新引擎，等价于一次完整的 `dytes update`。窗口显示后置顶并在
Windows 上按主显示器居中，避免 kiosk / 全屏宿主遮挡或窗口越界。

## 2. 用法

```bash
dytes install [OPTIONS]                          # 等价 install core piksel
dytes install app <name> [OPTIONS]               # 安装指定应用
dytes install <type> <name>[@<version>] [OPTIONS]
dytes install --gui [--dir <path>] [--launch]
```

| 参数 | 必填 | 说明 | 当前状态 |
| --- | --- | --- | --- |
| `<type>` | 否 | 省略时按 `core` 安装 `piksel`；`app` / `plugin` / `design-soft` / `sys` / `core` / `web` / `model` | 已实现 |
| `<name>` | 是（省略类型时除外） | 包名；支持 `name@1.2.3`；安装 app 时为应用名 | 已实现 |
| `--version <ver>` | 否 | 显式版本，优先于 `name@version` | 已实现 |
| `--dir <path>` | 否 | 工作目录，默认当前目录 | 已实现 |
| `--gui` | 否 | 启动图形安装器 | 需要 `--features gui` |
| `--force` | 否 | 同版本也重新安装（设计包） | 已实现 |
| `--launch` | 否 | 图形安装完成后启动应用 | 已实现（GUI） |
| `--offline` | 否 | 不访问网络（依赖包需要显式版本） | 部分实现 |
| `--no-init` | 否 | 兼容参数 | **尚未接线** |
| `--json` | 否 | JSON 输出 | 已实现 |

类型别名：`plugins` / `design_plugins` 归一为设计插件；`system` 归一为 `sys`；
`design-soft` / `design_soft` 等价。

## 3. 执行流程

### 3.1 通用入口

1. `--gui` 分支：调用图形安装器；未启用 `gui` feature 时返回退出码 `2`。
2. 确定应用根：`--dir` > `DYTES_APP_DIR` > 当前目录，并切换工作目录。
3. 解析类型与名称：
   - 未指定类型 → 按 `core piksel` 处理；
   - `app` → `install_app`：目录已有 `dytes.json` 时返回 `3`；没有则按
     `app/<name>/dytes.json` 安装指定应用（`--launch` 安装后启动）；
   - 设计包 → `DesignKind::Plugin` / `DesignKind::Soft`；
   - 依赖包 → `install_dependency`（清单缺失时先初始化 `type = app`）；
   - 未知类型 → 退出码 `2`。

### 3.2 设计包

1. 拉取 `<remote>/<name>/dytes.json`；
2. 先安装清单声明的依赖；
3. 调用更新引擎安装目标包（版本比较、下载、checksum 校验）；
4. 设计软件只写包目录；设计插件额外把库注册到 `libary/` 并通过 FFI 同步插件数据；
5. 成功后写 `dytes.lock`。

### 3.3 依赖包

1. 读取应用根 `dytes.json`；不存在时按目录名初始化一个 `type = app` 的最小清单
   （`name` 取目录名，版本 `0.1.0`），然后继续安装；
2. 确定版本：
   - 显式指定 → 直接使用；
   - `--offline` 且未指定 → 参数错误；
   - 在线 → 读取远端 `dytes.json` 中的 `version`；
3. 把 `name = version` 写入对应 `dependencies-*` 分组并落盘；
4. 调用 `install_one_dependency` 只安装目标包；
5. `--json` 输出安装结果。

## 4. 目录与远程布局

| type | 本地位置 | 远程前缀 | 索引 |
| --- | --- | --- | --- |
| `app` | 安装根目录 | `app/` | `app/packages.json` |
| `core` | 安装根目录 / `libary/` | `core/` | `core/packages.json` |
| `sys` | `libary/` | `system/` | `system/packages.json` |
| `web` | `web/<name>/` | `webs/` | `webs/packages.json` |
| `design_plugins` | `plugins/<name>/` | `design_plugins/` | `design_plugins/packages.json` |
| `design_soft` | `apps/<name>/` | `design_soft/` | `design_soft/packages.json` |
| `model` | `models/<name>/` | `models/` | `models/packages.json` |

## 5. 输出与退出码

正常输出示例：

```text
已写入 dependencies-sys: info_lib@0.1.2；开始安装目标包
...
```

依赖包 `--json`：

```json
{
  "type": "sys",
  "name": "info_lib",
  "version": "0.1.2",
  "dependency_key": "dependencies-sys"
}
```

| 退出码 | 场景 |
| --- | --- |
| `0` | 安装成功或已是最新版本 |
| `1` | 网络、下载、checksum、安装失败 |
| `2` | 未知类型、缺少依赖包名、离线未指定版本、未启用 GUI |
| `3` | `dytes.json` 无法解析；安装 app 时目录里已有 `dytes.json` |

## 6. 边界与约束

- 安装失败不更新锁文件；
- 同版本默认跳过，`--force` 重新下载（设计包）；
- 依赖包会先修改 `dytes.json`，后续安装失败时清单里仍保留声明；
- `--offline` 目前只覆盖依赖包链路，设计包仍走在线安装；
- 导出目标路径拒绝绝对路径与 `..`；
- 凭证只从配置 / 环境读取，日志中脱敏。

## 7. 测试点

- 缺少依赖包名 / 未知类型返回 `2`；
- 省略类型时按 `core piksel` 安装；
- `app` 安装时目录已有 `dytes.json` 返回 `3`，提示改用 `dytes update`；
- `app` 缺少应用名返回 `2`；
- 依赖包在缺失 `dytes.json` 时自动初始化 `type = app` 清单并继续；
- `--offline` 未指定版本返回 `2`；
- 设计包安装写入 `plugins/<name>/` 与 `dytes.lock`；
- 同版本跳过、`--force` 重装；
- `--gui` 在未启用 feature 时返回 `2`。

对应测试文件：`tests/cli_install.rs`、`tests/cli_global.rs`。

## 8. 后续规划

1. 接线 `--no-init`：安装后跳过 init 脚本；
2. 设计包支持 `--offline`（只使用本地缓存 / 版本目录）；
3. 依赖包先校验目标版本可安装，再写 `dytes.json`，失败时回滚；
4. `--json` 覆盖设计包链路的完整进度事件；
5. 卸载 / 列出已安装包（独立命令，不在 install 内扩展）。
