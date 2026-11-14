# 0.2.2

## 新增 `dytes test`

- 新增本地包自检命令：复用 `dytes check` 的规则校验清单、平台导出与设计插件。
- 所有测试产物统一写入包目录的 `.dytes/`，不存在时自动创建。
- 首次执行时自动创建或追加 `.gitignore` 的 `.dytes/` 规则，避免误提交测试产物。
- `dytes.json` 新增 `test-dependency`，按包类型声明测试依赖：

  ```json
  "test-dependency": {
    "sys": { "info_lib": "0.1.0" },
    "web": { "setting": "0.2.0" }
  }
  ```

- 执行 `test` 时把本地已安装的对应依赖复制到 `.dytes/env/`，不访问网络。
- 在 `.dytes/` 内按安装布局演练导出文件、init 脚本与 `dytes.lock` 生成。
- 支持 `--manifest-only`、`--platform`、`--type`、`--json`；`--keep` 保留演练目录。
- `--work-dir` 只能位于 `.dytes/` 内，并增加源路径、目标路径、包名与依赖路径安全检查。

## 发布与 GitHub Action

- 发布构建调整为 4 个环境：
  - Windows / Linux / macOS 三个带 GUI 的发布构建；
  - 1 个 Linux 无 GUI 的 Action 构建。
- 发布资产统一平台后缀：
  - `dytes-windows.exe`
  - `dytes-linux.bin`
  - `dytes-macos.app`
  - `dytes-action.bin`
- Action 定义移动到 `.github/workflows/action.yml`，不再作为仓库根目录的
  composite Action 发布。

## 升级

```bash
dytes self-update
```
