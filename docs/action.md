# GitHub Action

> **规范**：发布与 CI 文件统一放在 `.github/workflows/`，不得放在仓库根目录
> （见 [skills/code.md](../skills/code.md) 3.3）。该目录只放 workflow 定义；
> 本仓库不维护 composite Action 定义，因此 `uses: dytescn/dytes@vX.Y.Z`
> 的引用方式不可用。

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
          DYTES_VERSION: v0.2.22
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

发布资产统一使用平台后缀：Windows 为 `.exe`，Linux 为 `.bin`，macOS 为 `.app`。
`dytes-action.bin` 是无 GUI 的 Linux 构建，只支持 `ubuntu-latest`（或兼容的
Linux runner）；不支持 `install --gui` / `update --gui`。

## 2. 示例

只预览、不上传：

```yaml
      - run: dytes publish --dry-run
```

**多平台发布**：一个 job 只跑 Linux。需要发布 Windows / macOS 产物时，
应在对应平台的打包流程中直接使用发布版 `dytes`（或自建 workflow），
不要用同一个 job 跨平台发布。

```yaml
      - run: dytes publish --platform linux
```

包的 `dytes.json` 需为每个平台声明 `platform-exports`，产物会上传到
`<type>/<name>/<version>/<platform>/`。

## 3. 凭证

优先级：环境变量 > `dytes config` 用户配置 > 构建内置值。需要独立凭证时：

```yaml
      - name: Publish
        run: dytes publish
        env:
          DYTES_S3_ENDPOINT: ${{ secrets.UPYUN_ENDPOINT }}
          DYTES_S3_BUCKET: ${{ secrets.UPYUN_BUCKET }}
          DYTES_S3_ACCESS_KEY: ${{ secrets.UPYUN_API_KEY }}
          DYTES_S3_SECRET_KEY: ${{ secrets.UPYUN_API_SECRET }}
```

## 4. 发布资产

发布流程专门构建的无 GUI 资产：

```text
https://github.com/dytescn/dytes/releases/download/vX.Y.Z/dytes-action.bin
```

该资产与三个带 GUI 的发布资产（`dytes-windows.exe`、`dytes-linux.bin`、
`dytes-macos.app`）一起由 release workflow 构建，但只上传到 GitHub Release，
不写入 S3 自更新清单。引用旧版本 tag 时如果该资产不存在，请升级到包含
`dytes-action.bin` 的版本。

## 5. 不提供 Marketplace Action

本仓库不发布 composite Action，也不提供 `uses: dytescn/dytes@vX.Y.Z` 的引用
方式。不要为了 `uses:` 引用把发布或 CI 文件移回仓库根目录。
