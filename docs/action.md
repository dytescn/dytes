# GitHub Action

`dytes` 提供 composite Action，供各包仓库在自己的 workflow 里完成发布 / 更新，
不需要在本地编译 `dytes`。

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

      # 构建本包自己的产物（示例为 Rust cdylib）
      - uses: dtolnay/rust-toolchain@stable
      - run: cargo build --release

      # 调用 dytes 发布当前目录
      - uses: dytescn/dytes@v0.1.11
        with:
          command: publish
```

Action 会自动下载对应平台的 `dytes` 发布二进制并执行，无需在仓库里提交 exe。

## 2. 输入

| 输入 | 默认值 | 说明 |
| --- | --- | --- |
| `command` | `publish` | 子命令：`publish` / `update` / `self-update` / `install` / `init` / `config` |
| `args` | 空 | 追加参数，例如 `--force`、`--dry-run`、`--platform windows` |
| `dir` | `.` | 包目录（相对 workspace） |
| `version` | `0.1.12` | 下载的 dytes 版本，与 Action 引用版本保持一致 |

## 3. 常见示例

### 3.1 只预览，不上传

```yaml
      - uses: dytescn/dytes@v0.1.11
        with:
          command: publish
          args: --dry-run
```

### 3.2 Web 包（Deno）

```yaml
      - uses: denoland/setup-deno@v1
      - run: deno task pack

      - uses: dytescn/dytes@v0.1.11
        with:
          command: publish
          args: --force
```

### 3.3 多平台发布

`publish` 默认使用当前 runner 的平台，因此多平台包应使用矩阵：

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
      - uses: dytescn/dytes@v0.1.11
        with:
          command: publish
          args: --platform ${{ matrix.platform }}
```

各 runner 分别构建并发布自己平台的产物。包的 `dytes.json` 必须为每个平台声明
`platform-exports`，dytes 才会把文件上传到 `<type>/<name>/<version>/<platform>/`。

```json
{
  "platform-exports": {
    "windows": { "./target/release/database.dll": "database.dll" },
    "linux": { "./target/release/libdatabase.so": "database.so" },
    "macos": { "./target/release/libdatabase.dylib": "database.dylib" }
  }
}
```

## 4. S3 凭证

发布版 `dytes` 二进制已内置 CI 发布凭证，通常**不需要**额外配置。

如果要改用独立的只读 / 读写操作员，或覆盖存储地址，可通过环境变量传入：

```yaml
      - uses: dytescn/dytes@v0.1.11
        with:
          command: publish
        env:
          DYTES_S3_ENDPOINT: ${{ secrets.UPYUN_ENDPOINT }}
          DYTES_S3_BUCKET: ${{ secrets.UPYUN_BUCKET }}
          DYTES_S3_ACCESS_KEY: ${{ secrets.UPYUN_API_KEY }}
          DYTES_S3_SECRET_KEY: ${{ secrets.UPYUN_API_SECRET }}
```

变量优先级：环境变量 > `dytes config` 用户配置 > 构建内置值。

## 5. 发布到 GitHub Marketplace

`action.yml` 已在仓库根目录。要把 Action 上架 Marketplace，需要在网页端操作一次：

1. 确认仓库为 **Public**；
2. 打开 `Releases`，编辑目标 release（例如 `v0.1.11`）；
3. 勾选 **Publish this Action to the GitHub Marketplace**；
4. 选择分类（例如 `Utilities`），保存。

之后其他仓库即可通过 `uses: dytescn/dytes@v0.1.11` 引用。
