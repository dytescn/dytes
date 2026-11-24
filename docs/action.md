# GitHub Action

> **规范**：发布与 CI 相关文件统一放在 `.github/workflows/`，不得放在仓库
> 根目录（见 [skills/code.md](../skills/code.md) 3.3）。本文件描述的 Action
> 定义位于 `.github/workflows/dytes-action.yml`，该位置不会被识别为仓库
> 根目录的 composite Action，因此 `uses: dytescn/dytes@vX.Y.Z` 的引用方式
> 不可用；不要为了 `uses:` 引用把文件移回根目录。

发布流程会构建 Linux 无 GUI 的 `dytes-action.bin`。需要在 workflow 中执行
dytes 时，直接下载该资产并调用子命令，无需在包仓库里编译或提交 `dytes`。

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

      - name: Download dytes
        env:
          DYTES_VERSION: v0.2.20
        run: |
          set -euo pipefail
          mkdir -p "${RUNNER_TEMP}/dytes-bin"
          curl -fsSL --retry 5 --retry-all-errors --retry-delay 2 \
            "https://github.com/dytescn/dytes/releases/download/${DYTES_VERSION}/dytes-action.bin" \
            -o "${RUNNER_TEMP}/dytes-bin/dytes"
          chmod +x "${RUNNER_TEMP}/dytes-bin/dytes"
          echo "${RUNNER_TEMP}/dytes-bin" >> "${GITHUB_PATH}"

      - name: Publish
        run: dytes publish
```

## 2. Action 定义与输入

`.github/workflows/dytes-action.yml` 只作为仓库内维护的 composite Action 定义，
不会被发布为可 `uses: dytescn/dytes@vX.Y.Z` 的 Action。需要复用时，把它复制
到调用方仓库的本地 Action 目录（不能放在 `.github/workflows`）：

```bash
mkdir -p .github/actions/dytes
curl -fsSL https://raw.githubusercontent.com/dytescn/dytes/main/.github/workflows/dytes-action.yml \
  -o .github/actions/dytes/action.yml
```

```yaml
      - uses: ./.github/actions/dytes
        with:
          command: publish
          version: v0.2.20
```

本地 Action 的 `github.action_ref` 指向调用方仓库的 ref，无法推断 dytes 版本，
必须显式传入 `version`。输入如下：

| 输入 | 默认值 | 说明 |
| --- | --- | --- |
| `command` | `publish` | 子命令：`publish` / `update` / `install` / `init` / `test` / `config` |
| `args` | 空 | 追加参数，如 `--force`、`--dry-run`、`--platform linux` |
| `dir` | `.` | 包目录（相对 workspace） |
| `version` | 空（从 `github.action_ref` 推断） | 下载的 dytes 版本；本地 Action 无法推断，必须显式传入 |

发布资产统一使用平台后缀：Windows 为 `.exe`，Linux 为 `.bin`，macOS 为 `.app`。
Action 只提供 **Linux 无 GUI** 构建，必须使用 `runs-on: ubuntu-latest`
（或兼容的 Linux runner）；不支持 `install --gui` / `update --gui`。

## 3. 示例

只预览、不上传：

```yaml
      - uses: ./.github/actions/dytes
        with:
          command: publish
          args: --dry-run
          version: v0.2.20
```

**多平台发布**：Action 本身只跑 Linux。需要发布 Windows / macOS 产物时，
应在对应平台的打包流程中直接使用发布版 `dytes`（或自建 workflow），
不要用同一个 Action job 跨平台发布。

```yaml
      - uses: ./.github/actions/dytes
        with:
          command: publish
          args: --platform linux
          version: v0.2.20
```

包的 `dytes.json` 需为每个平台声明 `platform-exports`，产物会上传到
`<type>/<name>/<version>/<platform>/`。

## 4. 凭证

优先级：环境变量 > `dytes config` 用户配置 > 构建内置值。需要独立凭证时：

```yaml
      - uses: ./.github/actions/dytes
        with:
          command: publish
          version: v0.2.20
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

`.github/workflows/dytes-action.yml` 不发布为 Marketplace Action，也不再提供
`uses: dytescn/dytes@vX.Y.Z` 的引用方式。不要为了上架把发布或 CI 文件移回
仓库根目录；需要在 workflow 中执行 dytes 时，按第 1 节直接下载发布二进制。
