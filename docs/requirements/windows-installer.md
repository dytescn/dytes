# 需求：`dytes-windows.exe` 双击自安装（免 irm / PowerShell）

**状态**：关键决策已确认（2026-11-28）：D1 复用 `dytes-windows.exe`、D2 最低 Windows 10、D3 双击默认在线检查；**M1 已实现**（`dytes setup` / 双击安装，见 [../commands/setup.md](../commands/setup.md)），M2 / M3 待评审
**提出日期**：2026-11-28
**关联**：[issues.md](../issues.md) I-03（Windows 安装脚本重试与校验）、I-22（旧版 Windows 启动失败）、[packaging.md](../packaging.md)（系统依赖）

## 1. 背景

当前 Windows 用户获取 dytes 的方式只有两种：

1. 在 PowerShell 里执行 `irm https://.../install.ps1 | iex`（官网 / 文档提供）；
2. 手动下载产物，自己放到某个目录，再手工改 `PATH`。

两处现实问题：

- **不能执行 irm 的环境越来越多**：企业组策略禁用 `Invoke-WebRequest` / 脚本执行、
  只允许 cmd、不允许下载 `.ps1`、内网无法访问脚本地址；
- 即便下载成功，用户也不知道“该放哪里、怎么让命令可用”，最终仍要人工配置 `PATH`。

同时，GitHub Release 与分发源上的资产 `dytes-windows.exe` **现在就是 dytes 主程序**
（带 GUI 的 CLI），双击只会弹出一个控制台窗口后关闭（无子命令时 clap 输出帮助），
不会安装、不会写 `PATH`。因此“下载 → 双击 → 能用”的链路是缺失的。

本需求要补齐这条链路：**一个不需要 PowerShell、不需要管理员权限的单文件安装程序**。

## 2. 目标与非目标

### 2.1 目标

| 编号 | 目标 |
| --- | --- |
| G1 | 用户只下载一个 `.exe`、双击，即可完成安装并在新开的 cmd / PowerShell 里使用 `dytes` |
| G2 | 全程不需要 PowerShell、不需要 `.ps1`、不需要管理员权限、不需要预装 VC++ 运行库之外的任何运行时（后者见 [packaging.md](../packaging.md) W1） |
| G3 | 安装后 `dytes` 命令进入用户 `PATH`，且不破坏用户已有 `PATH` |
| G4 | 重复安装 / 覆盖升级是幂等的；与 `dytes self-update` 共用同一安装路径与文件命名 |
| G5 | 安装过程可诊断：失败有清晰提示与日志，不留下半成品文件 |
| G6 | 支持卸载，卸载后不残留 `PATH` 项 |

### 2.2 非目标

- 不做系统级安装（`Program Files` / `HKLM` 的 `PATH`）——需要管理员权限，和 G2 冲突；
- 不做后台自动更新服务 / 开机自启；
- 不支持 Windows 10 以下的系统——见 I-22；已确认不做 legacy 产物、也不 patch Slint
  （Rust std 与底层依赖同样依赖 Win10-only API，去掉 GUI 也无法解决）；
  安装器本身也受同一限制，旧系统提示只能放在下载页 / 安装脚本；
- 不覆盖 Linux / macOS 的安装器（后续可复用同一套 CLI 设计）；
- 不把 Windows Lite（无 GUI 构建）当作 Win7 兼容版：Lite 只是去掉 Slint / 图形依赖，
  最低系统仍是 Windows 10（Rust std 的基线，见 [../issues.md](../issues.md) I-22）。

## 3. 用户场景

| 编号 | 场景 | 期望结果 |
| --- | --- | --- |
| S1 | 企业电脑，PowerShell 与 `.ps1` 被执行策略禁用，只能双击运行 exe | 双击后安装成功，打开新 cmd 即可 `dytes --version` |
| S2 | 用户在网吧 / 共享电脑，没有管理员权限 | 安装到 `%LOCALAPPDATA%`，不触发 UAC |
| S3 | 内网 / 离线机器，无法访问 S3 | 双击安装当前 exe 自带版本，不报网络错误；可显式选择在线检查 |
| S4 | 已用旧脚本装过 dytes，`PATH` 里已有旧路径 | 识别已有安装，覆盖为新版本，`PATH` 不重复、不失效 |
| S5 | 想卸载 | 通过 Windows“设置 → 应用”或 `dytes setup --uninstall` 移除程序与 `PATH` 项 |
| S6 | 安装后想升级 | `dytes self-update` 原地替换，`PATH` 不变 |
| S7 | 下载的是旧版本安装包 | 可选择升级到线上最新版；默认行为按 5.4 的版本策略执行 |

## 4. 功能需求

### 4.1 启动模式识别

| 编号 | 需求 | 优先级 |
| --- | --- | --- |
| F1.1 | 双击（无参数、从资源管理器启动）进入**安装模式**；带子命令时保持现有 CLI 行为不变 | P0 |
| F1.2 | 在终端中无参数执行 `dytes` 时仍输出帮助（不能因为安装模式改变脚本 / 用户的既有预期） | P0 |
| F1.3 | 识别方式需覆盖：无参数、父进程为 `explorer.exe`、标准输出非重定向；三者同时满足才进入安装模式 | P0 |
| F1.4 | 提供显式入口 `dytes setup`，语义与双击一致，便于脚本与企业分发 | P0 |

### 4.2 安装位置与文件布局

| 编号 | 需求 | 优先级 |
| --- | --- | --- |
| F2.1 | 默认安装到 `%LOCALAPPDATA%\Programs\dytes\dytes.exe` | P0 |
| F2.2 | 支持 `--dir <path>` 覆盖安装目录；路径含空格 / 非 ASCII 时正常工作 | P0 |
| F2.3 | 安装目录只放程序文件；配置仍在 `%APPDATA%\dytes\config.toml`（`dirs::config_dir()`），升级 / 卸载不误删配置 | P0 |
| F2.4 | 安装包自身不额外解压、不写临时 DLL；安装完成后目录中只有 `dytes.exe`（及失败备份） | P1 |

### 4.3 `PATH` 写入

| 编号 | 需求 | 优先级 |
| --- | --- | --- |
| F3.1 | 把安装目录追加到**用户** `PATH`（`HKCU\Environment` 的 `Path`，类型 `REG_EXPAND_SZ`），不写 `HKLM` | P0 |
| F3.2 | 写入前按大小写不敏感去重；保留原有值、原顺序、`%VAR%` 展开语法 | P0 |
| F3.3 | 写入后广播 `WM_SETTINGCHANGE`（`Environment`），让新开的终端生效；必须提示“已打开的终端需重开” | P0 |
| F3.4 | `PATH` 写入失败（权限 / 超长）时安装仍算成功，但必须明确提示手动添加的命令与目录 | P0 |
| F3.5 | `--no-path` 跳过 `PATH` 修改，供有自管 `PATH` 的环境使用 | P1 |

### 4.4 安装源与版本

| 编号 | 需求 | 优先级 |
| --- | --- | --- |
| F4.1 | 双击安装时**默认在线**：先读取线上清单（`core/dytes/dytes.json`），下载并安装线上最新版 | P0 |
| F4.2 | 提供 `--offline`：只安装 exe 内置版本；在线检查 / 下载失败时必须提示失败原因，并提供“改为安装当前 exe 版本”的可选操作（不改变默认在线策略） | P0 |
| F4.3 | 下载走 `https://packages.funxdata.com/core/dytes/...`，失败重试 3 次并给出可读错误 | P0 |
| F4.4 | 版本选择必须与 `self-update` 的清单 / 产物命名一致（`windows/dytes.exe`、同名 `.sha256`） | P0 |
| F4.5 | 安装完成后打印 / 展示安装版本与“运行 `dytes --version` 验证”的指引 | P1 |

### 4.5 校验与原子性

| 编号 | 需求 | 优先级 |
| --- | --- | --- |
| F5.1 | 下载完成后校验 `dytes.exe.sha256`；校验失败终止安装并删除临时文件 | P0 |
| F5.2 | 安装采用“临时文件 → 校验 → 原子 rename”：同目录写 `.dytes-download.<pid>`，成功后替换 | P0 |
| F5.3 | 替换前备份旧 `dytes.exe` 为 `dytes.exe.old`；任一步失败回滚并保留旧版本可用 | P0 |
| F5.4 | 目标文件被占用（旧进程未退出）时提示关闭后重试，不静默失败 | P0 |
| F5.5 | 若提供代码签名，安装程序校验 Authenticode 签名（P1，依赖签名证书） | P2 |

### 4.6 幂等、升级与卸载

| 编号 | 需求 | 优先级 |
| --- | --- | --- |
| F6.1 | 重复安装同一版本成功且无副作用（不重复写 `PATH`、不丢配置） | P0 |
| F6.2 | 检测到旧位置（历史脚本安装的目录）时提示并迁移，旧路径从 `PATH` 移除 | P1 |
| F6.3 | `dytes setup --uninstall`：删除安装目录、移除 `PATH` 项；配置需 `--purge` 才删除 | P1 |
| F6.4 | 写入卸载注册项 `HKCU\Software\Microsoft\Windows\CurrentVersion\Uninstall\dytes`（显示名、版本、发布者、图标、卸载命令），出现在“设置 → 应用” | P1 |
| F6.5 | `dytes self-update` 与安装器使用同一路径与替换逻辑，升级后 `PATH` 不变 | P0 |

### 4.7 界面与反馈

| 编号 | 需求 | 优先级 |
| --- | --- | --- |
| F7.1 | 双击安装时不残留控制台窗口：使用 GUI 对话框（推荐复用 Slint）或安装完成后自动关闭 | P0 |
| F7.2 | 显示进度（检查更新 / 下载 / 校验 / 写入 `PATH`）与最终结果 | P1 |
| F7.3 | 失败时展示可复制的错误文本与日志路径；退出码非 0 | P0 |
| F7.4 | 支持中文界面；文案与现有 CLI 一致（术语：安装目录、命令入口、用户 `PATH`） | P1 |
| F7.5 | 无桌面会话（如服务 / 远程脚本）下退化为纯文本输出，不崩溃；旧系统只走版本提示 | P1 |

### 4.8 命令行参数（供企业分发 / 自动化）

| 参数 | 语义 | 优先级 |
| --- | --- | --- |
| `dytes setup` | 显式安装（默认在线检查版本） | P0 |
| `--offline` | 只安装当前 exe 内置版本，不访问网络 | P0 |
| `--dir <path>` | 指定安装目录 | P0 |
| `--no-path` | 不修改 `PATH` | P1 |
| `--version <v>` | 安装指定版本 | P2 |
| `--yes` / `-y` | 非交互，全部使用默认值 | P1 |
| `--uninstall` / `--purge` | 卸载（可选同时删除配置） | P1 |
| `--json` | 机器可读结果（与其他命令一致） | P1 |
| `/S`、`/D=<dir>` | 兼容 Inno Setup / NSIS 风格的静默参数 | P2 |

### 4.9 安全与合规

| 编号 | 需求 | 优先级 |
| --- | --- | --- |
| F8.1 | 不写 `HKLM`、不要求 UAC、不修改系统目录 | P0 |
| F8.2 | 不调用 PowerShell / cmd 执行远程脚本；不使用 `iex` | P0 |
| F8.3 | 所有下载走 HTTPS，并校验 SHA-256 | P0 |
| F8.4 | 不把 S3 凭据写入日志或界面（沿用内置凭据的现有做法） | P0 |
| F8.5 | 卸载只删除自己创建的注册表项与 `PATH` 片段，按精确匹配 | P1 |

### 4.10 日志与诊断

| 编号 | 需求 | 优先级 |
| --- | --- | --- |
| F9.1 | 安装日志写到 `%LOCALAPPDATA%\dytes\logs\install.log`（滚动 / 追加），失败时在界面中给出路径 | P0 |
| F9.2 | 日志包含：版本、安装目录、`PATH` 变更前后、下载 URL、校验结果 | P1 |
| F9.3 | 错误使用统一退出码（沿用 [cmd.md](../cmd.md)：1 一般失败 / 2 参数 / 3 配置 / 4 有更新） | P1 |

## 5. 关键设计决策

### 5.1 产物命名（已确认：A）

**已确认（2026-11-28）：采用 A。**

| 方案 | 说明 | 优点 | 缺点 |
| --- | --- | --- | --- |
| **A（已选）** | 保持资产名 `dytes-windows.exe`，让它在“双击无参数”时进入安装模式；同一文件也是安装包 | 符合用户提出的“增加 dytes-windows.exe、双击安装”；只发一个文件 | `dytes-windows.exe` 语义变成“安装器 + 主程序”，需要 F1 的父进程判断避免误触 |
| B | 新增独立的 `dytes-setup.exe`，`dytes-windows.exe` 仍是纯 CLI | 语义清晰、CI 产物直观 | 多一个资产，用户仍可能下错、下到旧的那个 |

由此带来的约束：

- 安装模式识别必须严格（F1.3），避免脚本里无参数调用 `dytes` 时误触发安装；
- `dytes-windows.exe` 的 README / 官网文案需说明“双击安装、命令行使用同一文件”；
- `self-update` 下载的产物就是安装器本身，升级后双击行为保持一致。

### 5.2 最低 Windows 版本（已确认：Windows 10）

**已确认（2026-11-28）：最低支持 Windows 10，不产出 legacy 版本、不 patch Slint。**

- **程序自身无法做版本检测**：失败发生在 PE 加载器解析导入表阶段，早于 `main`
  与任何 Rust 代码；安装器与主程序是同一个二进制，同样起不来。因此不实现
  “启动时检测并弹提示”，改为在 README / 官网下载页 / 安装脚本（`install.ps1`
  用 `[Environment]::OSVersion`）提示；
- 提示文案需说明：需要 Windows 10 或更高版本、当前版本、可升级系统或用其它机器；
- Windows 7 / 8.1 的支持问题只通过 I-22 方案 A 处理，不再排期 legacy 产物——
  #37 评论的导入表分析显示 `ProcessPrng`、`WaitOnAddress`、`combase` / WinRT 等
  Win10-only 符号来自 Rust std 与底层依赖，与是否启用 `gui` 无关；
- 具体 build 基线：评论记录 `SystemParametersInfoForDpi` 最低 Windows 10 1703
  （build 15063），与 Rust 1.78 起的“最低 Windows 10”口径略有差异；文档统一写
  “Windows 10 及以上”，不写 build 号，避免在 1607 / 1703 的实机回归完成前过度承诺。

### 5.3 安装范围

默认用户级（`%LOCALAPPDATA%`）是唯一不需要管理员权限的选择，符合 G2。
若确有“全机器安装”需求，另加 `--system`（需要 UAC），不在本期范围。

### 5.4 双击时的版本策略（已确认：默认在线）

**已确认（2026-11-28）：默认在线检查并安装线上最新版。**

| 方案 | 行为 | 取舍 |
| --- | --- | --- |
| 默认离线 | 先装 exe 自带版本，安装完成后提示可运行 `dytes self-update` 升级 | 断网 / 内网可用；行为可预测 |
| **默认在线（已选）** | 先查线上清单，下载最新版并安装；`--offline` 改为安装 exe 内置版本 | 用户拿到最新版；离线 / 企业网络拦截时需要可读的失败提示与手动回退 |

配套要求（保证默认在线不会把离线用户卡死）：

1. 在线检查 / 下载失败时，界面必须给出失败原因（网络 / 5xx / 校验），并提供
   “改为安装当前 exe 版本”的操作，同时保留 `--offline`；
2. 安装成功后展示安装版本，与 `dytes self-update --check` 的结果一致；
3. 企业内网可继续通过 `DYTES_DOWNLOAD_BASE` / 代理配置指向镜像（沿用 I-03 的约定）。

### 5.5 是否做代码签名

未签名时，SmartScreen 会提示“Windows 已保护你的电脑”，且企业环境可能直接拦截。
签名证书（OV / EV）有持续成本，建议单独立项；本期需求把签名后的校验（F5.5）列为 P2，
但安装流程必须能在未签名时正常提示，不把 SmartScreen 当成安装失败。

## 6. 安装后的目录与注册表约定

| 内容 | 路径 / 键 |
| --- | --- |
| 程序 | `%LOCALAPPDATA%\Programs\dytes\dytes.exe` |
| 旧版本备份 | `%LOCALAPPDATA%\Programs\dytes\dytes.exe.old`（成功后可删） |
| 配置 | `%APPDATA%\dytes\config.toml`（既有行为，安装器不改） |
| 日志 | `%LOCALAPPDATA%\dytes\logs\install.log` |
| 用户 `PATH` | `HKCU\Environment` → `Path`（`REG_EXPAND_SZ`） |
| 卸载信息 | `HKCU\Software\Microsoft\Windows\CurrentVersion\Uninstall\dytes` |

## 7. 验收标准

| 编号 | 验收条件 |
| --- | --- |
| AC1 | 干净的 Windows 10 x64 虚拟机（**未安装 PowerShell 7、未装 VC++ 运行库**）中双击 exe，无需管理员权限完成安装 |
| AC2 | 安装后**新开** cmd 执行 `dytes --version` 成功；旧终端重开后同样成功 |
| AC3 | 用户原有 `PATH`（含 `%JAVA_HOME%\bin` 等变量）保持完整，无重复项 |
| AC4 | 断网 / 内网环境双击：在线检查失败后给出明确提示，可在界面上选择“安装当前 exe 版本”并成功完成；`dytes setup --offline` 直接成功 |
| AC5 | 同版本重复安装、跨版本覆盖安装均成功，配置不丢 |
| AC6 | 安装过程中强杀进程，旧版本仍可运行，不残留半成品 `.exe` |
| AC7 | 卸载后 `PATH` 无残留、安装目录删除、配置行为符合 `--purge` 约定 |
| AC8 | `dytes self-update` 后 `PATH` 与安装目录不变，版本更新 |
| AC9 | 在禁用 PowerShell 与脚本执行策略的机器上全流程可用（不调用 PowerShell） |
| AC10 | Windows 7 / 8.1：下载页与安装脚本给出“需要 Windows 10”的明确提示并停止；已下载的 exe 仍会以入口点缺失退出（加载器限制，已在 I-22 记录） |

## 8. 测试矩阵

| 维度 | 取值 |
| --- | --- |
| 系统 | Windows 10 22H2 / Windows 11；Windows 8.1（仅验证提示）；Windows Server 2019 |
| 权限 | 普通用户 / 管理员（验证不强制 UAC） |
| 网络 | 在线 / 断网 / 代理 / S3 返回 5xx（验证重试） |
| 既有安装 | 无 / 旧脚本安装的旧路径 / 同版本 / 更低版本 / 更高版本 |
| `PATH` | 空 / 含变量 / 接近长度上限 / 已含目标目录（大小写不同） |
| 安全策略 | PowerShell 禁用、脚本执行限制、SmartScreen 开启 |
| 参数 | 双击 / `setup` / `--offline` / `--dir` / `--no-path` / `--uninstall` / `--json` |

## 9. 里程碑

| 里程碑 | 范围 |
| --- | --- |
| M1（最小可用） | 双击进入安装模式（方案 A）、静态 CRT / 无外部依赖、在线检查并下载最新版（失败可回退内置版本）、安装到 `%LOCALAPPDATA%`、写用户 `PATH`、SHA-256 校验、原子替换、日志、失败提示。**不含系统版本检测**（加载器阶段失败，见 5.2） |
| M2 | `setup` / `--offline` / `--dir` / `--no-path` / `--json`、幂等与旧安装迁移、`.old` 回滚 |
| M3 | 卸载 + 卸载注册项 + `--purge`、GUI 进度、在线检查更新、代码签名校验 |

> M1 的“无需预装 VC++ 运行库”（AC1）依赖 [packaging.md](../packaging.md) 的
> P0 项：Windows 构建启用 `+crt-static` 并用 `dumpbin /dependents` 做门禁。

## 10. 待决策清单

| 编号 | 决策 | 建议 |
| --- | --- | --- |
| D1 | 产物命名：复用 `dytes-windows.exe`（A）还是新增 `dytes-setup.exe`（B） | **已确认 A** |
| D2 | 最低 Windows 版本：仅 Win10，还是额外产出无 GUI legacy 版 | **已确认最低 Windows 10**；#37 评论证明 legacy 产物不可行（std 层 Win10 API），不做 legacy / Slint patch；程序自身无法提示，改为下载页 / 脚本提示 |
| D3 | 双击默认在线检查还是离线安装 | **已确认默认在线**，失败时可回退内置版本 |
| D4 | 是否申请 Windows 代码签名证书 | 单独立项，不阻塞 M1 |
| D5 | 安装范围是否提供系统级 `--system` | 本期不做 |
| D6 | 静默参数规范：`--yes` 为准，是否额外兼容 `/S` | M2 评估 |
