# dytes 已知问题

> 记录实际使用中遇到的阻塞问题、复现方式与修复方向。
> 最近更新：2026-11-03

---

## I-01 `dytes publish` 无法更新 packages.json（S3 只可写、不可读/列举）

**发现日期**：2026-11-01
**影响版本**：`dytes` 当前构建（含 `872d4de fix: handle missing packages index on object storage`）
**影响包**：`sys/chatllm` 1.0.9、`ui/chat_ui` 1.0.12
**严重程度**：P0 —— 发布无法完成，新版本不会出现在索引中
**状态**：0.1.8 已提供 `--force` 临时通道；根因是 S3 凭证缺少读取 / 列举权限，仍需运维处理。代码侧错误分类（B）、ListObjects v1 降级（D）、本地索引兜底（E）尚未实现。

### 1. 现象

`dytes publish` 能成功上传所有导出文件与 `dytes.json`，但在最后更新 `packages.json` 时中止：

```text
上传完成: 成功 2 个，失败 0 个
上传 dytes.json...
  文件上传成功: system/chatllm/dytes.json
  dytes.json 上传成功

更新 packages.json...
  从 S3 获取 packages.json: packages/system/packages.json
读取远程 system/packages.json 失败，且无法列出 system/: ListObjectsV2 system/ 失败: service error（原读取错误: 下载失败: service error）。发布已中止。
```

`chat_ui` 的发布结果一致（换成 `webs/` 前缀）。

`dytes update --check` 同样无法读取任何对象，包括刚刚上传成功的 `system/chatllm/dytes.json`：

```text
读取失败 (尝试 1/3): S3 对象读取失败: system/packages.json，稍后重试...
检查 chatllm 线上版本失败: 读取 system/chatllm/dytes.json 失败: S3 对象读取失败
```

### 2. 已确认的事实

| 操作 | 结果 |
| --- | --- |
| `PutObject` / multipart 上传（导出文件、`dytes.json`） | ✅ 成功 |
| `GetObject`（`packages.json`、刚上传的 `dytes.json`） | ❌ `service error` |
| `ListObjectsV2`（`system/`、`webs/`） | ❌ `service error` |
| `dytes publish --dry-run` | ✅ 正常（不访问网络） |

结论：**不是网络问题，也不是文件缺失**——PUT 可用，而 GET / ListObjectsV2 被服务端拒绝。
`--force` 不改变该行为（仍先读索引再决定是否创建）。

### 3. 根因假设

1. **凭证权限不足**：`effective_s3()` 在用户配置缺失时回退到 `builtin_s3()`，
   而内置凭证来自构建期环境变量 `UPYUN_API_KEY` / `UPYUN_API_SECRET`（见 `build.rs`）。
   该操作员很可能只配置了「可写」，缺少「可读 / 可列举」权限。
   `dytes config list` 显示 `s3.access_key` / `s3.secret_key = (未设置)`，说明实际使用的是内置凭证。
2. **又拍云对不存在对象的 GET 返回 403**：`is_not_found_error()` 只能识别
   `NoSuchKey` / `404` / `not found` 等文本，403 会被当成通用 `service error`，
   与真正的权限错误无法区分。
3. **ListObjectsV2 兼容性**：又拍云 S3 兼容层对 `ListObjectsV2` 的支持可能不完整，
   即使凭证正确也可能失败。旧版 `publish.exe` 在读不到索引时会直接“创建新的”，
   而新版为避免覆盖已有索引改为中止（`872d4de`），因此暴露出该问题。

### 4. 相关代码

| 位置 | 作用 |
| --- | --- |
| `src/publish/run/dytes.rs::update_packages_json` | 读取索引；读取失败 + 列举失败 → 中止 |
| `src/publish/utils/loader.rs::download_from_s3` | `GetObject`，错误统一为 `service error` |
| `src/publish/utils/loader.rs::list_keys` | `ListObjectsV2`，用于判断索引是否真实存在 |
| `src/shared/credentials.rs::effective_s3` / `builtin_s3` | 凭证来源与回退顺序 |

### 5. 修复方向

**A. 运维（无需改代码，最快）**

配置具备读取 + 列举权限的操作员：

```bash
dytes config set s3.access_key <AK>
dytes config set s3.secret_key <SK>
# 或使用环境变量
export DYTES_S3_ACCESS_KEY=<AK>
export DYTES_S3_SECRET_KEY=<SK>
```

随后重新执行 `dytes publish`。

**B. 区分 S3 错误类型（代码）**

在 `download_from_s3` / `list_keys` 中保留 aws-sdk 的错误码（`403`、`AccessDenied`、`NoSuchKey`），
不要统一压成 `service error`，让上层能判断「无权限」与「对象不存在」。

**C. `--force` 语义扩展（代码）**

`--force` 时若索引读取失败，允许创建新索引并在输出中明确提示风险；
非 `--force` 保持当前的安全中止行为。

**D. `list_keys` 降级（代码）**

`ListObjectsV2` 失败时尝试 `ListObjects`（v1），以兼容又拍云。

**E. 本地索引兜底（代码）**

`dytes publish --index <path>`：读取本地 `packages.json` 合并后上传，绕开远程读取。

### 6. 当前状态（0.1.8 修复）

- `--force` 现在真正生效：读取索引失败且无法列举时，`dytes publish --force` 会创建新的
  `packages.json`（输出明确提示“可能覆盖其它包条目”）；不加 `--force` 仍安全中止。
- 若列举成功并确认索引存在，即使 `--force` 也不会覆盖。
- 错误信息会提示配置 `s3.access_key` / `s3.secret_key`（具备读 + 列举权限）。
- 新增 6 个单元测试覆盖上述决策分支（`src/publish/run/dytes.rs::tests`）。
- 两个包的文件与 `dytes.json` **已上传成功**；
- `packages.json` **未更新**，`dytes update` / `install` 暂时看不到 1.0.9 / 1.0.12；
- 建议：配置读取权限后重跑 `dytes publish`；若接受风险，可使用 `dytes publish --force`。

---

## I-02 内存内容上传返回 403 `ErrInvalidAccessKeyID`

**发现日期**：2026-11-01
**影响**：`dytes.json`、`packages.json` 等通过 `upload_bytes` 上传的对象
**状态**：0.1.8 已修复

### 现象

导出文件（`upload_file` / `upload_path_to_s3`）上传正常，但 `dytes.json` 连续重试均失败：

```text
上传: core/dytes/dytes.json (尝试 1/3)
S3上传失败: ServiceError(... code: Some("ErrInvalidAccessKeyID") ... status: 403 ...)
```

### 原因

`upload_bytes` 走 `upload_to_s3`，使用 `ByteStream::from(Vec)` 作为请求体；
又拍云对这类内存 body 的签名（aws-chunked / trailer）不兼容，返回
`ErrInvalidAccessKeyID`。文件路径 `ByteStream::from_path` 使用普通签名，可正常上传。

### 修复

`upload_to_s3` 先写入临时文件，再复用 `upload_path_to_s3`，与导出文件走同一条上传路径：

```rust
let tmp = std::env::temp_dir().join(format!("dytes-upload-{}-{}.tmp", std::process::id(), nonce));
fs::write(&tmp, file_content)?;
let ok = upload_path_to_s3(tmp.to_string_lossy().as_ref(), bucket_name, object_name);
let _ = fs::remove_file(&tmp);
```

修复后 `core/dytes/dytes.json`、`system/chatllm/dytes.json` 均可正常上传。

### 备注

- `packages.json` 同样走 `upload_bytes`，因此该修复对索引上传同样必要；
- 索引读取仍受 I-01 的权限问题限制，本次未执行 `--force` 重建。

---

## I-03 GitHub #2：Windows 安装脚本缺少重试与校验

**发现日期**：2026-11-03
**来源**：<https://github.com/dytescn/dytes/issues/2>
**状态**：0.1.11 修复（官网安装脚本与下载源）

### 现象

- `install.ps1` 直连 GitHub Release，国内网络慢且易断；
- 下载中途失败会在目标路径留下截断的 `dytes.exe`；
- 没有重试、进度与完整性校验。

### 修复

- 官网下载源改为 `https://packages.funxdata.com/core/dytes/latest/<platform>/`；
- `install.sh` / `install.ps1` 先写临时文件，校验后原子替换；
- 增加 3 次重试与进度提示；
- 发布 workflow 为每个平台的二进制同时上传 `<file>.sha256`，安装脚本校验 SHA-256；
- 支持 `DYTES_DOWNLOAD_BASE` 覆盖下载源，便于镜像 / 内网。

### 配套

- `dytes` release workflow 增加 `core/dytes/latest/<platform>/` 别名目录；
- 官网 `index.html` 的下载按钮与版本号改为读取 packages 域名。

---

## I-04 GitHub #3：增加 CLI 自更新

**发现日期**：2026-11-03
**来源**：<https://github.com/dytescn/dytes/issues/3>
**状态**：0.1.11 已实现 `dytes self-update`

### 现象

`dytes update` 只更新当前应用与依赖，不检查 CLI 自身版本；升级只能重跑安装脚本。

### 修复

新增命令：

```bash
dytes self-update --check   # 只检查，不下载
dytes self-update           # 下载并替换自身
dytes self-update --json    # 机器可读结果
```

- 从 `https://packages.funxdata.com/core/dytes/dytes.json` 读取最新版本；
- 按当前平台下载 `core/dytes/latest/<platform>/<file>`；
- 校验 `<file>.sha256`；
- Unix 直接原子替换；Windows 先尝试重命名，失败时由退出后的 helper 完成替换；
- `--check` 不写文件；同版本默认跳过，`--force` 可强制重装。
