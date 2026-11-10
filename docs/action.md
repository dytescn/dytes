# GitHub Action

`dytes` 提供 composite Action：下载对应平台的发布二进制并执行子命令，
无需在包仓库里编译或提交 `dytes`。

## 1. 用法

```yaml
name: Publish package

on:
  push:
    tags: ["v*"]

jobs:
  publish:
    runs-on: windows-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - run: cargo build --release

      - uses: dytescn/dytes@v0.1.27
        with:
          command: publish
```

## 2. 输入

| 输入 | 默认值 | 说明 |
| --- | --- | --- |
| `command` | `publish` | 子命令：`publish` / `update` / `install` / `init` / `config` |
| `args` | 空 | 追加参数，如 `--force`、`--dry-run`、`--platform windows` |
| `dir` | `.` | 包目录（相对 workspace） |
| `version` | `0.1.27` | 下载的 dytes 版本，应与 Action 引用版本一致 |

## 3. 示例

只预览、不上传：

```yaml
      - uses: dytescn/dytes@v0.1.27
        with:
          command: publish
          args: --dry-run
```

多平台发布：为每个平台分别运行，并用 `--platform` 指定平台。

```yaml
    strategy:
      matrix:
        include:
          - os: windows-latest
            platform: windows
          - os: ubuntu-latest
            platform: linux
          - os: macos-latest
            platform: macos
    runs-on: ${{ matrix.os }}
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - run: cargo build --release
      - uses: dytescn/dytes@v0.1.27
        with:
          command: publish
          args: --platform ${{ matrix.platform }}
```

包的 `dytes.json` 需为每个平台声明 `platform-exports`，产物会上传到
`<type>/<name>/<version>/<platform>/`。

## 4. 凭证

优先级：环境变量 > `dytes config` 用户配置 > 构建内置值。需要独立凭证时：

```yaml
      - uses: dytescn/dytes@v0.1.27
        with:
          command: publish
        env:
          DYTES_S3_ENDPOINT: ${{ secrets.UPYUN_ENDPOINT }}
          DYTES_S3_BUCKET: ${{ secrets.UPYUN_BUCKET }}
          DYTES_S3_ACCESS_KEY: ${{ secrets.UPYUN_API_KEY }}
          DYTES_S3_SECRET_KEY: ${{ secrets.UPYUN_API_SECRET }}
```

## 5. 上架 Marketplace

`action.yml` 位于仓库根目录。上架需在网页端操作一次：确认仓库为 Public →
编辑目标 release → 勾选 **Publish this Action to the GitHub Marketplace** →
选择分类并保存。之后即可通过 `uses: dytescn/dytes@vX.Y.Z` 引用。
