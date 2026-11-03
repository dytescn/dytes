# dytes 0.1.11 版本更新

## 主题

跨平台安装与平台库下载：Windows / Linux / macOS 使用同一份 `dytes.json`，
由 dytes 按当前系统选择并下载对应的库文件。

## 变更

### 1. 平台识别与归一化

- 新增 `shared::platform::normalize_platform`，统一识别：
  - `win` / `win32` / `win64` / `windows` → `windows`
  - `mac` / `macos` / `darwin` / `osx` → `macos`
  - `linux` → `linux`
- 平台来源优先级：`DYTES_PLATFORM` > `PIKSEL_PLATFORM` > 编译目标 OS。
- `publish` / `update` / `install` 三个入口共用同一套平台规则。

### 2. 安装器支持 `platform-exports`

`dytes install` 的引导阶段原先只读取 `exports`，且固定下载
`core/<name>/<version>/<file>`。现在改为：

- 选择当前平台的 `platform-exports`，缺失时明确报错；
- 从 `core/<name>/<version>/<platform>/<file>` 下载 `dytes` 二进制；
- 无 `platform-exports` 的旧包继续回退 `exports` 与旧路径。

### 3. 平台库下载

`sys` / `core` / `web` / `design_plugins` / `design_soft` / `model` 更新流程
均已按平台选择导出文件，并下载到：

```text
<type>/<name>/<version>/<platform>/<file>
```

库文件的本地命名遵循平台约定：

| 平台 | 库文件示例 |
| --- | --- |
| Windows | `database.dll` |
| Linux | `database.so` |
| macOS | `database.dylib` |

### 4. CLI 自更新

新增：

```bash
dytes self-update --check   # 检查最新版本
dytes self-update           # 下载并替换自身
```

- 版本来源：`https://packages.funxdata.com/core/dytes/dytes.json`
- 下载地址：`https://packages.funxdata.com/core/dytes/latest/<platform>/<file>`
- 校验 `<file>.sha256`；Unix 原子替换，Windows 重命名 / 退出后 helper 替换
- 同时修复 GitHub #3（CLI 无升级入口）

### 5. 安装源与安装脚本

- 官网下载链接从 GitHub Release 改为 `packages.funxdata.com`；
- release workflow 额外发布 `core/dytes/latest/<platform>/` 别名；
- 同时上传 `<file>.sha256`；
- `install.sh` / `install.ps1` 增加重试、临时文件、SHA-256 校验与原子替换；
- 支持 `DYTES_DOWNLOAD_BASE` 覆盖下载源（镜像 / 内网）。
- 同时修复 GitHub #2（Windows 安装脚本直连 GitHub、无重试 / 校验）。

## 配套库变更

以下库增加 `platform-exports`，发布 workflow 将自动构建三个平台：

- `chatllm`
- `plugin_lib`
- `info_lib`
- `material_lib`
- `workspace_lib`
- `database`
- `designdb`
- `ostfil`
- `kv_lib`（已有）
- `markparse`（已有）
- `sysinfo`（已有）
- `cyctron`（核心二进制）

`designsoft_lib` 仍仅支持 Windows（依赖注册表 / COM / MSI API），不在本次范围。

## 使用

```bash
# 当前系统自动选择平台
dytes update --auto

# 模拟其它平台（测试缺包保护）
DYTES_PLATFORM=linux dytes update --auto

# 升级 dytes CLI 自身
dytes self-update --check
dytes self-update
```

## 发布后同步网站

从 0.1.11 起，release workflow 自动完成发布与官网同步：

1. **preflight** 拒绝重复版本 / 版本回退；
2. **verify-release** 检查三个平台的版本目录、`latest` 与 `<file>.sha256`；
   `dytes.json` / `version.js` 若仍被 CDN 缓存，只告警，不影响发布；
3. 官网 `www.dytes.cn` 运行时读取 `latest/version.js`，无需额外配置即可显示新版本；
4. 如需同时更新官网静态 `assets/data/release.json`，由 `notify-website` 发送
   `dytes-release`（配置 `WEBSITE_DISPATCH_TOKEN` 时启用）。

发布顺序：先发布 `dytes` `v0.1.11`，再推各库 `v*` tag。各库 workflow 引用
`dytescn/dytes@v0.1.11`。
