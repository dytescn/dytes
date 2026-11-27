# GitHub Action

本仓库在根目录提供 composite Action（`action.yml`），可在任意 workflow 中
通过 `uses: dytescn/dytes@vX.Y.Z` 调用 `dytes` 子命令。Action 会按自身 tag
下载对应 release 资产：

| runner | 资产 |
| --- | --- |
| Linux | `dytes-action.bin`（无 GUI） |
| Windows | `dytes-windows.exe` |
| macOS | `dytes-macos.bin` |

> 根目录 `action.yml` 是 GitHub 对 composite Action 的硬性要求，属于
> [skills/code.md](../skills/code.md) 3.3 的例外；其余发布与 CI 文件仍必须
> 放在 `.github/workflows/`。

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

      - name: Check package
        uses: dytescn/dytes@v0.2.28
        with:
          command: check
          dir: .

      - name: Publish
        uses: dytescn/dytes@v0.2.28
        with:
          command: publish
          dir: .
```

输入：

| 输入 | 默认值 | 说明 |
| --- | --- | --- |
| `command` | `publish` | `check` / `publish` / `test` / `install` / `update` / `init` / `config` |
| `args` | 空 | 附加参数，如 `--dry-run`、`--force` |
| `dir` | `.` | 包目录（相对 `GITHUB_WORKSPACE`） |
| `version` | Action 的 ref | 显式指定 dytes 版本（`vX.Y.Z`）；使用 `@main` 等非版本 ref 时必须显式传入 |

## 2. 示例

只预览、不上传：

```yaml
      - name: Preview
        uses: dytescn/dytes@v0.2.28
        with:
          command: publish
          args: --dry-run
          dir: workspace_ui
```

**多平台发布**：一个 job 只跑一个平台。需要发布 Windows / macOS 产物时，
应在对应平台的打包流程中分别发布，不要用同一个 job 跨平台发布。

```yaml
      - name: Publish Linux
        uses: dytescn/dytes@v0.2.28
        with:
          command: publish
          args: --platform linux
```

包的 `dytes.json` 需为每个平台声明 `platform-exports`，产物会上传到
`<type>/<name>/<version>/<platform>/`。

## 3. 凭证

发布产物在构建时已内置官方 S3 凭据（端点 `https://s3.api.upyun.com`、桶
`packages`），下载后可直接 `dytes publish`，发布官方仓库的其它库无需额外配置。

优先级：环境变量 > `dytes config` 用户配置 > 构建内置值。需要独立凭证时，
在 `uses` 步骤上设置环境变量：

```yaml
      - name: Publish
        uses: dytescn/dytes@v0.2.28
        with:
          command: publish
        env:
          DYTES_S3_ENDPOINT: ${{ secrets.UPYUN_ENDPOINT }}
          DYTES_S3_BUCKET: ${{ secrets.UPYUN_BUCKET }}
          DYTES_S3_ACCESS_KEY: ${{ secrets.UPYUN_API_KEY }}
          DYTES_S3_SECRET_KEY: ${{ secrets.UPYUN_API_SECRET }}
```

## 4. 直接下载（不使用 Action）

不能使用 composite Action 的环境（例如需要固定二进制的脚本）可以直接下载
发布资产：

```bash
mkdir -p "${RUNNER_TEMP}/dytes-bin"
curl -fsSL --retry 5 --retry-all-errors --retry-delay 2 \
  "https://github.com/dytescn/dytes/releases/download/vX.Y.Z/dytes-action.bin" \
  -o "${RUNNER_TEMP}/dytes-bin/dytes"
chmod +x "${RUNNER_TEMP}/dytes-bin/dytes"
export PATH="${RUNNER_TEMP}/dytes-bin:${PATH}"
dytes publish
```

`dytes-action.bin` 是无 GUI 的 Linux 构建，只支持 `ubuntu-latest`（或兼容的
Linux runner）；不支持 `install --gui` / `update --gui`。它只上传到 GitHub
Release，不写入 S3 自更新清单。
