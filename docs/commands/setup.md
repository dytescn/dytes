# `dytes setup`

## 1. 职责

把 dytes 安装到当前用户的目录，并把安装目录追加到**用户** `PATH`。
Windows 上双击 `dytes-windows.exe`（无参数、由资源管理器启动）时进入同一流程。

不修改系统级设置：不写 `HKLM`、不要求管理员权限、不调用 PowerShell。

> 旧版 Windows（7 / 8 / 8.1）上的提示不在本命令内实现——产物依赖 Win10-only
> API，加载器阶段就会失败；提示放在下载页 / 安装脚本，见
> [../issues.md](../issues.md) I-22。

## 2. 用法

```bash
dytes setup [--offline] [--dir <DIR>] [--no-path] [--json]
```

| 参数 | 说明 | 当前状态 |
| --- | --- | --- |
| `--offline` | 只安装当前 exe 内置版本，不访问网络 | 已实现 |
| `--dir <DIR>` | 安装目录，默认 `%LOCALAPPDATA%\Programs\dytes` | 已实现 |
| `--no-path` | 不修改用户 `PATH` | 已实现 |
| `--json` | JSON 输出 | 已实现 |

双击启动时等价于 `dytes setup`（默认在线，失败时可弹窗选择安装当前版本）。

Windows Lite（无 GUI）产物同样支持 `dytes setup` 与双击安装：安装流程与完整版一致，
但 Lite 不包含 Slint 界面，`install --gui` / `update --gui` 会提示当前构建未启用图形界面。

## 3. 执行流程

1. 解析安装目录并创建；目标文件为 `<DIR>\dytes.exe`。
2. 选择安装来源：
   - 默认在线：读取 `core/dytes/dytes.json`，按 `windows` 平台选择产物，
     下载并用同名 `.sha256` 校验；
   - `--offline`：复制当前 exe；
   - 当前 exe 已经是目标文件时跳过替换。
3. 原子替换：先写同目录临时文件 `.dytes-download.<pid>`，校验通过后把旧文件
   改名为 `dytes.exe.old`，再 rename 新文件；失败时回滚旧版本。
4. 写入用户 `PATH`：
   - 读 `HKCU\Environment` 的 `Path`（`REG_EXPAND_SZ`）；
   - 按大小写不敏感去重后追加，保留原有 `%VAR%`；
   - 写回并广播 `WM_SETTINGCHANGE`；
   - 失败只告警，不影响已完成的文件安装。
5. 记录日志并输出结果。

## 4. 文件与注册表约定

| 内容 | 路径 / 键 |
| --- | --- |
| 程序 | `%LOCALAPPDATA%\Programs\dytes\dytes.exe` |
| 旧版本备份 | 同目录 `dytes.exe.old`（成功后被删除） |
| 日志 | `%LOCALAPPDATA%\dytes\logs\install.log` |
| 用户 `PATH` | `HKCU\Environment` → `Path`（`REG_EXPAND_SZ`） |

## 5. 输出

文本：

```text
已安装 dytes v0.2.28 到 C:\Users\me\AppData\Local\Programs\dytes\dytes.exe
已加入用户 PATH；请打开新的终端后再运行 dytes
安装日志: C:\Users\me\AppData\Local\dytes\logs\install.log
验证: 新开 cmd 执行 dytes --version
```

`--json`：

```json
{
  "version": "0.2.28",
  "source": "online",
  "path": "C:\\Users\\me\\AppData\\Local\\Programs\\dytes\\dytes.exe",
  "path_updated": true,
  "log": "C:\\Users\\me\\AppData\\Local\\dytes\\logs\\install.log"
}
```

`source` 取值：`online` / `bundled` / `inplace`。

## 6. 退出码

| code | 含义 |
| --- | --- |
| `0` | 安装完成（`PATH` 写入失败仍返回 0，但会提示手动配置） |
| `1` | 下载、校验、写入文件失败 |
| `2` | 参数错误 |

双击模式下失败会弹出错误框，并在其中给出日志路径。

## 7. 测试点

- 空 `PATH` / 含 `%VAR%` / 已包含目标目录（大小写、尾部斜杠不同）时不重复追加；
- 安装目录含空格与非 ASCII 字符；
- 在线下载 5xx 重试、校验失败、旧文件被占用时的回滚与提示；
- `--offline` 在断网环境可用；双击在线失败后选择“安装当前版本”可用；
- 已安装同版本重复执行不改变 `PATH`、不丢配置。

## 8. 后续

M2 / M3 见 [../requirements/windows-installer.md](../requirements/windows-installer.md)：
`--yes`、卸载与卸载注册项、旧安装位置迁移、GUI 进度、代码签名校验。
