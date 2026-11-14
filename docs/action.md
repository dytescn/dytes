# GitHub Action

> **注意**：`action.yml` 已移动到 `.github/workflows/action.yml`。
> 该位置不会作为仓库根目录的 composite Action 发布，因此
> `uses: dytescn/dytes@vX.Y.Z` 的引用方式不再可用。下面内容保留为移动前的
> 使用说明；如需继续提供 Marketplace Action，请将文件移回仓库根目录。

`dytes` 提供 composite Action：下载 Linux 无 GUI 的发布二进制并执行子命令，
无需在包仓库里编译或提交 `dytes`。

## 1. 用法

```yaml
name: Publish package

on:
  push:
    tags: ["v*"]

jobs:
  publish:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4

      - uses: dytescn/dytes@v0.2.1
        with:
          command: publish
```

## 2. 输入

| 输入 | 默认值 | 说明 |
| --- | --- | --- |
| `command` | `publish` | 子命令：`publish` / `update` / `install` / `init` / `test` / `config` |
| `args` | 空 | 追加参数，如 `--force`、`--dry-run`、`--platform linux` |
| `dir` | `.` | 包目录（相对 workspace） |
| `version` | Action 引用版本 | 下载的 dytes 版本；默认从 `github.action_ref`（如 `v0.2.1`）推断 |

发布资产统一使用平台后缀：Windows 为 `.exe`，Linux 为 `.bin`，macOS 为 `.app`。
Action 只提供 **Linux 无 GUI** 构建，必须使用 `runs-on: ubuntu-latest`
（或兼容的 Linux runner）；不支持 `install --gui` / `update --gui`。
如果 Action 不是通过版本 tag 引用（例如 `@main`），请显式传入 `version`。

## 3. 示例

只预览、不上传：

```yaml
      - uses: dytescn/dytes@v0.2.1
        with:
          command: publish
          args: --dry-run
```

**多平台发布**：Action 本身只跑 Linux。需要发布 Windows / macOS 产物时，
应在对应平台的打包流程中直接使用发布版 `dytes`（或自建 workflow），
不要用同一个 Action job 跨平台发布。

```yaml
      - uses: dytescn/dytes@v0.2.1
        with:
          command: publish
          args: --platform linux
```

包的 `dytes.json` 需为每个平台声明 `platform-exports`，产物会上传到
`<type>/<name>/<version>/<platform>/`。

## 4. 凭证

优先级：环境变量 > `dytes config` 用户配置 > 构建内置值。需要独立凭证时：

```yaml
      - uses: dytescn/dytes@v0.2.1
        with:
          command: publish
        env:
          DYTES_S3_ENDPOINT: ${{ secrets.UPYUN_ENDPOINT }}
          DYTES_S3_BUCKET: ${{ secrets.UPYUN_BUCKET }}
          DYTES_S3_ACCESS_KEY: ${{ secrets.UPYUN_API_KEY }}
          DYTES_S3_SECRET_KEY: ${{ secrets.UPYUN_API_SECRET }}
```

## 5. 发布资产

Action 下载的是发布流程专门构建的无 GUI 资产：

```text
https://github.com/dytescn/dytes/releases/download/vX.Y.Z/dytes-action.bin
```

该资产与三个带 GUI 的发布资产（`dytes-windows.exe`、`dytes-linux.bin`、
`dytes-macos.app`）一起由 release workflow 构建，但只上传到 GitHub Release，
不写入 S3 自更新清单。引用旧版本 tag 时如果该资产不存在，请升级到包含
`dytes-action.bin` 的版本。

## 6. 上架 Marketplace

当前 `action.yml` 位于 `.github/workflows/action.yml`，不再作为 Marketplace
Action 发布。上架需要把文件移回仓库根目录，然后确认仓库为 Public →
编辑目标 release → 勾选 **Publish this Action to the GitHub Marketplace** →
选择分类并保存。之后即可通过 `uses: dytescn/dytes@vX.Y.Z` 引用。
