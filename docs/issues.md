# dytes 已知问题

> 记录实际使用中遇到的阻塞问题、复现方式与修复方向。
> 最近更新：2026-11-28（同步 GitHub #37 并记录 I-22；核对线上状态：#37 open，#33–#36 均 closed。I-22 由 #37 评论确认为 Windows 7 SP1 x64（退出码 0xC0000139），导入表显示 Slint 的 `SystemParametersInfoForDpi` 之外还有 Rust std / 依赖的 `ProcessPrng`、`WaitOnAddress` 等 Win10 API，装运行库无效、无 GUI 构建也无法回到 Win7；修复方向定为“声明最低 Windows 10，并由下载页 / 安装脚本提前提示”（程序自身在加载器阶段崩溃，无法自检）。打包与系统依赖现状见 [packaging.md](packaging.md)）

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
   即使凭证正确也可能失败。旧版发布逻辑在读不到索引时会直接“创建新的”，
   而新版为避免覆盖已有索引改为中止（`872d4de`），因此暴露出该问题。

### 4. 相关代码

| 位置 | 作用 |
| --- | --- |
| `src/publish/dytes.rs::update_packages_json` | 读取索引；读取失败 + 列举失败 → 中止 |
| `src/publish/loader.rs::download_from_s3` | `GetObject`，错误统一为 `service error` |
| `src/publish/loader.rs::list_keys` | `ListObjectsV2`，用于判断索引是否真实存在 |
| `src/utils/credentials.rs::effective_s3` / `builtin_s3` | 凭证来源与回退顺序 |

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
- 新增 6 个单元测试覆盖上述决策分支（`src/publish/dytes.rs::tests`）。
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

---

## I-05 Linux 产物 glibc 版本过高（旧发行版无法启动）

**发现日期**：2026-11-11
**影响版本**：0.2.0 及更早版本的 Linux 产物（`dytes.bin`、core 包中的 `.bin`）
**严重程度**：P0 —— 旧发行版上 dytes 无法启动，安装 / 更新链路中断
**状态**：0.2.1 已将 Linux 构建固定到 Ubuntu 22.04（glibc 2.35），并增加产物 GLIBC 上限检查；更低基线（manylinux）待后续评估

### 现象

在 Linux 旧发行版上执行安装后的 `dytes.bin`，动态链接器直接拒绝启动：

```text
./dytes.bin: /lib64/libc.so.6: version `GLIBC_2.34' not found (required by ./dytes.bin)
```

实际缺失的符号版本以产物为准，可能是 `GLIBC_2.28` / `GLIBC_2.34` / `GLIBC_2.38` /
`GLIBC_2.39`。带来的连锁问题：

- 安装脚本下载完成后启动 dytes 失败；
- `dytes self-update` 无法运行，无法通过自更新修复；
- 依赖 core `dytes` 的应用安装流程无法完成。

### 影响范围

| 环境 | glibc |
| --- | --- |
| CentOS 7 | 2.17 |
| Debian 10 | 2.28 |
| Ubuntu 20.04 / Debian 11 | 2.31 |
| Ubuntu 22.04 | 2.35 |
| 当前构建机 `ubuntu-latest`（Ubuntu 24.04） | 2.39 |

### 原因

1. `release.yml` 的 Linux 构建使用 `ubuntu-latest`，当前对应 Ubuntu 24.04；
2. Rust 默认动态链接构建机的 glibc，产物会记录构建时的符号版本；
3. CI 没有限制产物依赖的 GLIBC 符号上限，也没有在旧 glibc 环境验证；
4. GUI 依赖（Slint / X11 / Wayland）使 musl 静态链接不能直接作为方案；
5. 没有在安装脚本或下载页标注最低 glibc 要求。

### 修复方向

1. **固定构建基线（0.2.1 已实施）**：Linux 构建机固定为 `ubuntu-22.04`
   （glibc 2.35），并在构建后检查产物的 GLIBC 符号上限，防止 `ubuntu-latest`
   再次升级后基线漂移；覆盖 Ubuntu 22.04+，CentOS 7 / Ubuntu 20.04 / Debian 10/11
   仍不满足；
2. **manylinux 容器构建（后续）**：在 `quay.io/pypa/manylinux_2_28_x86_64` 中构建
   （glibc 2.28）；若必须兼容 CentOS 7，则使用 `manylinux_2_17`，并确认
   Slint / X11 / Wayland / fontconfig 等依赖可在容器内安装；
3. **CI 增加产物检查（0.2.1 已实施）**：构建后执行
   `readelf --version-info upload/dytes.bin | grep GLIBC_`，超过 2.35 则发布失败；
4. **安装脚本预检**：执行前检查 `ldd --version`，低于基线时给出明确提示，
   而不是让动态链接器报错；
5. **文档标注**：在下载页与 README 写明最低 glibc / 发行版要求。

### 备注

- 0.2.1 的基线为 glibc 2.35（Ubuntu 22.04）；如需覆盖 Ubuntu 20.04 / Debian 10，
  后续需切到 manylinux 构建；
- 0.2.0 的 Linux 产物仍有该问题，需升级到 0.2.1；
- 具体报错发行版与缺失符号版本待补充。

---

## I-06 `dytes test` 依赖只复制本地，未实现在线更新下载

**发现日期**：2026-11-16
**影响版本**：0.2.2
**影响命令**：`dytes test`
**严重程度**：P2 —— 测试环境可能使用过期依赖，跨包联调需手工更新
**状态**：0.2.3 已实现

### 1. 现象

- 当前 `dytes test` 只从 `DYTES_APP_DIR`（默认包目录）与 `dytes.lock` 复制
  本地已安装的依赖；本地缺少时直接报“未找到已安装的依赖”。
- 跨包联调（例如 chatllm 依赖 database / kv_lib）时，开发者必须先手工
  `dytes update`，否则测试用的是旧文件，或直接失败。
- `test-dependency` 的声明版本只记录、不校验，也不触发任何下载。

### 2. 目标语义（已确认）

1. **当前调试包不下载**：始终使用包目录里的本地构建产物；
2. **依赖全部更新下载**：清单 `dependencies-core` / `dependencies-sys` /
   `dependencies-web` / `dependencies-design_plugins` / `dependencies-design_soft` /
   `dependencies-model` 声明的全部依赖（含传递依赖），加上 `test-dependency`
   补充声明的包；
3. **版本**：与 `dytes update` 一致，按声明 semver 下载线上最新版本；
   精确声明低于线上时装线上版本（锁文件记录声明值），高于线上时报错；
4. **失败**：下载失败即报错，不回退本地复制，不提供离线回退；
5. **隔离**：只写入 `.dytes/env/`，不修改真实安装目录。

示例（chatllm）：`chatllm.so` 是当前包产物，不下载；`database.so`、
`kv_lib.so` 是依赖包，需要下载更新到 `.dytes/env/libary/`。

### 3. 相关代码

| 位置 | 作用 |
| --- | --- |
| `src/test.rs::stage_test_dependencies` | 当前从本地复制依赖的实现 |
| `src/test.rs::stage_one` | 按包类型与锁文件 `loader` 定位本地文件 |
| `src/update/dytes.rs::plan_dependencies` | 依赖图展开、传递依赖与版本约束（可复用） |
| `src/update/dytes.rs::install_dependencies_for` | 按拓扑顺序安装依赖（可复用） |
| `src/update/env.rs::get_app_dir` | 通过 `DYTES_APP_DIR` 切换安装根 |

### 4. 修复方向

1. 把当前包清单写入 `.dytes/env/dytes.json`，并将 `DYTES_APP_DIR` 指向
   `.dytes/env/`；
2. 复用 `plan_dependencies` + 逐项 `update_*`，把依赖下载安装到 `.dytes/env/`；
3. 把 `test-dependency` 合并进依赖计划，作为清单之外仅测试需要的补充；
4. 当前包仍只从包目录复制导出文件到演练目录，不参与下载；
5. 下载失败立即中止并返回非 `0`，不再继续安装演练；
6. 补充集成测试：依赖下载成功、版本不满足、下载失败中止三条路径。

### 5. 备注

- 0.2.3 已实现：`src/test.rs` 改为把 `test-dependency` 合并进清单后，以
  `.dytes/env/` 为安装根调用更新引擎（新增 `install_dependencies_only`，
  不执行依赖包的 init 脚本）；依赖下载失败立即中止并返回 `1`，不再进入演练。
- 原实现的“依赖复制失败仍继续演练”差异已随本地复制逻辑移除。
- 目标语义已写入 [commands/test.md §4.1](commands/test.md)；
  [cmd.md §8](cmd.md)、[dytes.md](dytes.md)、
  [../skills/testenv.md](../skills/testenv.md) 已同步。
- 集成测试覆盖：合并覆盖规则、下载失败中止、非法类型；下载成功路径需要真实
  S3 环境，未纳入自动化测试。

---

## I-07 `dytes test` 在 Windows 上因 verbatim 路径失败（os error 123）

**发现日期**：2026-11-19
**影响版本**：`dytes` 0.2.4（Windows）
**影响范围**：所有在 Windows 上执行 `dytes test` 且带依赖的包
**严重程度**：P1 —— 依赖安装阶段直接失败，测试环境无法建立
**状态**：0.2.5 已修复并发布（提交 `6ddab92`）；对应 GitHub #23 的 Windows 部分

### 1. 现象

Windows 上执行 `dytes test`，清单自检与 `.dytes` 创建均通过，但依赖安装阶段失败：

```text
PASS 测试目录已就绪  ...\.dytes
PASS 测试依赖清单已就绪  ...\.dytes\env\dytes.json
FAIL 依赖安装  依赖下载失败: 文件名、目录名或卷标语法不正确。 (os error 123)
```

错误信息没有上下文，且与网络、凭据、包版本无关：即使 S3 清单能正常读取
（`从 S3 获取: system/database/dytes.json` 成功），仍然在同一位置失败。

### 2. 复现

1. 任一带 `dependencies-sys` 的包，在 Windows 上 `cargo build --release`；
2. 执行 `dytes test`；
3. 依赖安装阶段必然返回 `os error 123`。

HANDLE 层可直接复现同样的错误：

```python
import os
prefix = chr(92) * 2 + "?" + chr(92)          # \\?\
base = os.path.abspath("_probe")
os.makedirs(prefix + base + "/libary")        # WinError 123
os.makedirs(prefix + base + "\\libary")       # OK
```

### 3. 根因

`src/test.rs::run_dependency_install` 先对 `.dytes/env` 执行 `canonicalize()`。
Windows 的 `std::fs::canonicalize` 返回 `\\?\D:\...` 形式的 verbatim 路径；
该路径被写入 `DYTES_APP_DIR` 后，更新引擎用字符串拼接构造子目录：

```rust
let lib_dir = format!("{}/libary", app_dir.trim_end_matches('/'));
fs::create_dir_all(&lib_dir)?;
```

verbatim 路径只接受 `\` 作为分隔符，混入 `/` 后 `CreateDirectoryW` 返回
`ERROR_INVALID_NAME`（123）。因此失败发生在下载第一个依赖之前。

正常安装路径不受影响：那里 `app_dir` 来自 `current_dir()`，不是 verbatim 形式。

### 4. 修复

canonicalize 之后去掉 `\\?\` 前缀，再交给更新引擎：

```rust
let env_dir = strip_verbatim_prefix(env_dir);

fn strip_verbatim_prefix(path: PathBuf) -> PathBuf {
    let text = path.to_string_lossy().into_owned();
    if let Some(rest) = text.strip_prefix(r"\\?\") {
        PathBuf::from(rest)
    } else {
        path
    }
}
```

也可考虑根治：更新引擎内部统一改用 `Path::join`，避免用 `format!` 拼路径。

### 5. 验证

- 修复前：`依赖安装 ... os error 123`；
- 修复后：错误变为正常的依赖下载流程（本地无内置 S3 凭据时提示
  `S3 对象读取失败`，说明路径问题已消失）；
- 使用公开 CDN 手动补齐 `.dytes/env/libary` 后，用该环境完成了
  `cloud_lib` 的四条 FFI 用例，环境布局与 `dytes test` 一致。

---

## I-08 GitHub #23：`install --dir` 相对路径二次拼接；`dytes test` 依赖语义

**发现日期**：2026-11-16（GitHub #23）
**影响版本**：0.2.2 起；`--dir` 问题在 0.2.11 仍存在
**严重程度**：P1 —— 脚本中传相对 `--dir` 时无法读取清单
**状态**：`--dir` 问题 0.2.12 已修复；`dytes test` 只读 `test-dependency` 的部分已于 0.2.3 修复（见 I-06）。GitHub #23 已于 2026-11-23 关闭

### 1. 现象

`dytes.json` 确实存在，但用相对路径传给 `--dir` 时仍报不存在：

```bash
$ dytes install sys chatllm --dir .dytes --no-init
.dytes/dytes.json 不存在；请先执行 dytes init 或在应用目录运行
```

先 `cd .dytes` 再执行则正常。

0.2.11 复核（`target/release/dytes.exe`，相对 `--dir app`）：

```text
app\dytes.json 不存在；请先执行 dytes init 或在应用目录运行
```

同一目录改用绝对路径 `--dir <abs>/app` 可通过清单检查（继续报“离线安装需要显式版本”）。

### 2. 根因

`src/install/cli.rs::run` 先执行 `set_current_dir(&dir)`，随后仍用**原始的相对 `dir`**
拼接清单路径：

```rust
let dir = args.dir.clone().unwrap_or(std::env::current_dir()?);
std::env::set_current_dir(&dir)?;
...
let manifest_path = dir.join("dytes.json"); // 相对当前目录，被二次拼接
```

切到 `app` 后查找 `app/dytes.json`，实际变成 `app/app/dytes.json`。

### 3. 修复（0.2.12）

`set_current_dir` 之后重新获取绝对路径，后续统一使用：

```rust
std::env::set_current_dir(&dir)?;
// `--dir` 可能是相对路径；切换后再取一次绝对路径，避免后续拼接
// `dytes.json` 时把 `app` 又拼成 `app/app`。
let dir = std::env::current_dir()?;
```

集成测试：`tests/cli_install.rs::install_sys_relative_dir_resolves_manifest`。

### 4. 备注

- GitHub #23 提到的 `dytes test` 只读 `test-dependency` 已在 0.2.3 修复：
  `merge_test_dependencies` 把 `test-dependency` 合并进 `dependencies-*` 后下载（I-06）；
- `dytes init` 默认生成 `exports` 的疑问见 I-10。

---

## I-09 GitHub #24：S3 匿名读取 403 / 缺少凭据导致 update / install / self-update 失效

**发现日期**：2026-11-18（GitHub #24）
**影响版本**：0.2.3 及所有未内置 S3 凭据的构建（0.2.10 本地构建可复现）
**严重程度**：P0 —— 所有在线读写链路中断
**状态**：0.2.11 已改进错误提示；GitHub #24 已于 2026-11-23 关闭。bucket 匿名策略 / 凭证发放仍需运维确认，本地构建仍按 I-09 配置凭证

### 1. 现象

```text
读取失败 (尝试 1/3): S3 对象读取失败: app/screen/dytes.json，稍后重试...
应用自身尚未发布，跳过应用更新: 下载 app/screen/dytes.json 失败
```

直接请求 S3 返回 403，而同一对象的 CDN 可匿名读取：

```bash
curl -I https://s3.api.upyun.com/packages/app/screen/dytes.json   # 403 AccessDenied
curl -I https://packages.funxdata.com/app/screen/dytes.json       # 200
```

本仓库另有一种复现：本地 `cargo build` 未注入 `UPYUN_*`，`target/release/dytes.exe`
读取 `core/dytes/dytes.json` 同样失败；同版本的发布二进制（内置凭据）正常。

### 2. 原因

1. 运行时凭证为空：用户配置、`DYTES_S3_*` 环境变量、构建内置值都没有 AK/SK；
2. 又拍云 bucket 不再允许匿名读取，空凭证请求被拒绝；
3. 读取失败被统一压成 `S3 对象读取失败` / `service error`，无法区分无权限与对象不存在；
4. `src/update/loader.rs` 对这类错误仍会重试 3 次，日志容易误导。

### 3. 修复

- **0.2.11（已发布）**：`src/utils/s3.rs` 新增 `ensure_credentials()`；
  `fetch_text` / `download_file` / `list_keys` / `download_from_s3` 在发起请求前检查，
  缺凭证时立即返回可操作提示（配置命令 + 环境变量 + 构建期变量），不再盲目重试；
- **待运维**：确认又拍云 bucket 的匿名读取策略，或为客户端发放可读凭证；
- **本地开发**：设置 `DYTES_S3_ACCESS_KEY` / `DYTES_S3_SECRET_KEY`，或在构建时提供
  `UPYUN_API_KEY` / `UPYUN_API_SECRET`。

### 4. 验证

- 0.2.11 本地构建：错误信息变为明确的“S3 凭证未配置”提示；
- 带内置凭据的发布版执行 `dytes self-update --check` 可读到 `core/dytes/dytes.json`
  （发布 0.2.11 后返回 `v0.2.11`）；
- Linux 环境已实测 `dytes self-update` 从 0.2.10 升级到 0.2.11 成功（2026-11-22）。

---

## I-10 GitHub #27：`dytes init` 生成的清单出现空 `exports`

**发现日期**：2026-11-21（GitHub #27）
**影响版本**：issue 未注明（报告日期 2026-11-21）
**严重程度**：P2 —— 不影响运行，但自动生成的导出路径往往不可用
**状态**：已修复 —— `init` 默认生成空导出字段，由用户按实际产物填写；GitHub #27 已于 2026-11-23 关闭

### 1. 现象

issue 描述 `dytes init` 生成的 `dytes.json` 中出现 `"exports": {}`。
确认期望就是空对象：字段保留，内容留空由用户自由填写；有问题的是
自动填入的 `target/release/<name>.exe` 等猜测路径。

### 2. 原因（0.2.11 核查）

七种类型分别执行 `dytes init` 时，`exports` / `platform-exports` 都会被填入
模板猜测的路径（`.exe` / `target/release` / `windows` 平台），这些路径只适用于
Rust + Windows 项目；其他项目需要手工删除或改写，与 issue 的期望相反。

### 3. 修复

- `src/utils/templates.rs::init_manifest`：除元数据外，导出字段只生成空骨架：
  - `app` / `core` / `design_plugins` / `design_soft` / `model` / `web`：
    `"exports": {}`；
  - `sys`：`"platform-exports": {}`；
  - `web` 仍写入 `"init": ["./setup.json"]`；
  - `--no-exports` 保持原有语义：完全不写导出字段；
- 测试：`templates_default_to_empty_exports`、`tests/cli_init.rs` 各类型断言
  `exports` / `platform-exports` 为空对象且不含自动路径；
- 文档：`docs/cmd.md`、`docs/commands/init.md` 更新模板说明。

---

## I-11 GitHub #28：`dytes test` 环境需要有可用的 S3 访问

**发现日期**：2026-11-21（GitHub #28）
**影响版本**：0.2.3 起（依赖改为在线下载）
**严重程度**：P2 —— 无凭证 / 无网络时测试环境无法建立
**状态**：符合既有设计，待确认是否补充离线模式；GitHub #28 已于 2026-11-23 关闭

### 1. 说明

0.2.3 起 `dytes test` 会把 `test-dependency` 合并进 `dependencies-*`，
通过更新引擎把依赖在线下载到 `.dytes/env/`；下载失败立即中止，不回退本地复制（I-06）。

因此测试机必须满足其一：

- 使用带内置凭据的发布构建；
- 配置 `dytes config set s3.access_key` / `s3.secret_key`；
- 设置 `DYTES_S3_ACCESS_KEY` / `DYTES_S3_SECRET_KEY`。

本地 `cargo build` 默认不内置凭据，这与 I-09 是同一根因；0.2.11 已在读取/下载前
给出明确提示。

### 2. 待办

- `docs/commands/test.md` 与 `skills/testenv.md` 已写明在线下载，可再补充“需凭证”的显式提示；
- 评估是否提供离线模式（当前设计明确不支持）。

---

## I-12 GitHub #29 / #30：`dytes install` 不能执行 / 不能安装应用

**发现日期**：2026-11-21 / 2026-11-22（GitHub #29、#30）
**严重程度**：待定
**状态**：0.2.12 已修复相对 `--dir` 与 init 依赖 cyctron 两个已复现问题；GitHub #29 / #30 已于 2026-11-23 关闭，但两个 issue 仍缺复现信息

### 1. 现象

两个 issue 只有标题，没有命令、报错和日志：

- #29 `dytes install` 不能执行；
- #30 `dytes install` 不能安装应用。

### 2. 已知实现（0.2.11）

- `dytes install` 必须给出包类型：`dytes install sys chatllm`，否则报
  `请指定包类型`；
- **app 不由 `install` 安装**：代码明确返回
  `安装应用请使用 dytes update --auto（当前目录的 dytes.json 即应用清单）`；
- 只有 `--gui` 分支允许省略类型（`dytes install --gui --dir <dir>`）；
- README 的快速开始仍写 `dytes install --gui --dir D:\Piksel`，与命令行行为不一致，
  容易让用户误以为 `install` 可以直接安装应用。

### 3. 0.2.12 修复

- **相对 `--dir`**：切换目录后改用绝对路径，`dytes install sys <name> --dir app`
  不再查找 `app/app/dytes.json`（见 I-08）；
- **单包 install 的 init 脚本**：sys 包安装完成后若应用目录没有 cyctron，
  打印警告并跳过 init，不再让整个安装返回失败。包与锁文件已完成安装，
  执行 `dytes update` 安装 core 后可再次触发 init；
- 集成测试：`tests/cli_install.rs::install_sys_relative_dir_resolves_manifest`；
  单元测试：`update_setup::tests::detects_cyctron_candidates`。

### 4. 待办

- 请补充实际命令与完整错误输出，确认是否为 I-08 的相对 `--dir` 问题、I-09 的凭证问题，
  或仅仅是命令用法；
- 统一 README / `docs/commands/install.md` 中“安装应用”的入口说明（`update --auto`）。

---

## I-13 `dytes install` 未把新增依赖写入应用根 `dytes.json`

**发现日期**：2026-11-22
**影响版本**：0.2.12 及更早
**严重程度**：P1 —— 包已安装，但应用清单缺少声明，后续 `update` / 迁移无法复现
**状态**：0.2.13 已修复

### 1. 现象

执行 `dytes install core chatbot` 时，文件安装成功，但应用根 `dytes.json` 的
`dependencies-core` 没有新增 `chatbot`；已有 `dependencies-core` 时也没有更新版本。

### 2. 原因

`install` 只用 `--dir`（默认当前目录）定位清单，而更新引擎使用
`get_app_dir()`：它优先 `DYTES_APP_DIR`。当宿主 / 启动器设置了
`DYTES_APP_DIR`、或当前目录不是应用根时：

- 文件被安装到 `get_app_dir()`；
- 清单却写到当前目录（或直接报不存在），造成“安装了但清单没变”。

### 3. 修复（0.2.13）

1. 应用根解析改为：`--dir` 优先，否则使用 `get_app_dir()`（支持 `DYTES_APP_DIR`）；
2. `set_current_dir` 后固定 `DYTES_APP_DIR`，保证文件安装与清单写入使用同一个根；
3. 新增 `upsert_dependency`：
   - 已存在同名依赖 → 更新为本次解析出的版本（未显式指定时即线上最新）；
   - 不存在 → 新增到对应 `dependencies-*`；
4. 输出中打印实际写入的清单路径与分组。

### 4. 验证

- 单元测试：`upsert_dependency_adds_new_entry`、
  `upsert_dependency_updates_existing_entry`、`upsert_dependency_rejects_non_object_group`；
- 集成测试：`install_sys_uses_app_dir_manifest`（`DYTES_APP_DIR` 与当前目录不同）；
- 手工验证：当前目录为空、`DYTES_APP_DIR` 指向应用目录时，执行
  `dytes install core chatbot@0.1.2 --offline` 写入
  `app/dytes.json` → `dependencies-core.chatbot = "0.1.2"`，原有 `cyctron` 保留。



---

## I-14 发布后 `latest` 别名与未版本化清单仍返回旧版本（CDN 缓存）

**发现日期**：2026-11-22
**影响版本**：0.2.13 及更早的发布流程
**严重程度**：P0 —— 安装脚本 / 官网从 `latest` 下载到旧版，表现像“打包失败”
**状态**：0.2.14 修复发布 workflow

### 1. 现象

`0.2.13` 的 GitHub Release 与版本化对象均正常，但：

```bash
curl -I https://packages.funxdata.com/core/dytes/latest/windows/dytes.exe
# Last-Modified: 2026-11-14；Content-Length 与 0.2.13 产物不一致

curl https://packages.funxdata.com/core/dytes/latest/version.js
# {"version":"0.2.10"}
```

`Verify published files` job 失败，run 的 annotation 为
`Process completed with exit code 35`（curl SSL connect error）；官网与安装脚本
则继续拿到旧版 `latest`。

### 2. 原因

1. 未版本化路径（`latest/*`、`core/dytes/dytes.json`）的 CDN 缓存为
   `Cache-Control: max-age=691200`（8 天）；覆盖同名 S3 对象不会让又拍云 CDN
   立即回源；
2. Windows / Linux 的 `latest` 上传使用第三方 action，未统一设置短缓存；
   `.sha256` 也没有设置；
3. `verify-release` 只检查 HTTP 200，不比较 `latest` 与版本化产物的内容，
   旧缓存也能通过；
4. `curl` 没有 `--retry-all-errors`，CDN 偶发 TLS 35 会让 job 直接失败。

### 3. 修复（0.2.14 workflow）

- `latest` 别名三平台统一改用 `aws s3 cp`，二进制与 `.sha256` 均设置
  `Cache-Control: no-cache, max-age=60`；
- 未版本化 `dytes.json`、`version.js` 也用 `aws s3 cp` 上传并设置短缓存；
- 新增 `purge-cache` job，调用又拍云缓存刷新 API
  （`http://purge.upyun.com/purge/`，Operation 凭证复用
  `UPYUN_API_KEY` / `UPYUN_API_SECRET` / `UPYUN_BUCKET`），刷新
  `dytes.json`、`version.js` 与三平台 `latest` 产物；
- `verify-release` 增加 `latest` 与 `<version>` 的 `.sha256` 一致性校验，
  并给 `curl` 增加 `--retry 3 --retry-all-errors`。

### 4. 验证

- 发布 0.2.14 后检查：
  - `latest/<platform>/<file>.sha256` 与 `<version>/<platform>/<file>.sha256` 相同；
  - `latest/version.js` 返回 0.2.14；
  - `verify-release` 通过。
- 若 `purge-cache` 返回 401（操作员凭证不匹配），需在又拍云控制台手动刷新，
  或改用具备刷新权限的操作员配置 secrets。

- 0.2.14 发布时 macOS 的 `latest` 上传失败（`aws s3 cp --cache-control` 在
  macOS runner 上退出 1），已改为 macOS 保留原上传命令、缓存刷新交给
  `purge-cache`；0.2.15 重新发布。
- 0.2.15 发布时 `purge-cache` 返回 200，但 `latest/windows` 已更新、
  `latest/linux` 与 `latest/macos` 仍是旧内容，`verify-release` 按内容校验失败；
  0.2.16 改为同时提交 `http://` 与 `https://` 两批 URL，并把 purge 响应写入
  job notice，便于确认是否返回 `invalid_domain_of_url`。
- 0.2.16 的 `purge-cache` 实际未执行：该 job 没有声明
  `environment: product`，读不到定义在 environment 中的
  `UPYUN_API_KEY` / `UPYUN_API_SECRET`，只打印了“跳过 CDN 刷新”；
  0.2.17 补上 environment 后重新发布。
- 0.2.17 的 purge 使用 S3 凭证返回 `401 user not exists`：又拍云 CDN 刷新需要
  「操作员名 / 密码」，与 S3 的 `UPYUN_API_KEY` / `UPYUN_API_SECRET` 不同。
  0.2.18 改为读取 `UPYUN_CDN_OPERATOR` / `UPYUN_CDN_PASSWORD`；未配置时跳过且
  不阻断发布，`verify-release` 对 latest 旧缓存只告警。
- **待办**：在 GitHub environment `product` 配置 `UPYUN_CDN_OPERATOR` /
  `UPYUN_CDN_PASSWORD`，或在又拍云控制台手动刷新以下 URL：
  `core/dytes/dytes.json`、`core/dytes/latest/version.js`、
  `core/dytes/latest/{windows/dytes.exe,linux/dytes.bin,macos/dytes.app}`
  及其 `.sha256`。

---

## I-15 GitHub #25 / #26：更新窗口被全屏应用遮挡、初始位置越界

**发现日期**：2026-11-18（GitHub #25、#26）
**严重程度**：P1 —— kiosk / 全屏宿主下更新窗口无法操作
**状态**：#25 已在 0.2.4 修复；#26 在 Windows 上已显式居中；`install --gui` 已同样置顶并居中。GitHub #25 / #26 已于 2026-11-23 关闭；Linux/X11 的负坐标仍交由窗口管理器，未单独验证。

### 1. 现象

- **#25**：宿主应用全屏运行（`_NET_WM_STATE_FULLSCREEN, ABOVE, FOCUSED`）时，
  `dytes update --gui` 窗口被压在下面，点击「开始更新」事件被宿主窗口接收；
  dytes 窗口只有 `_NET_WM_STATE_DEMANDS_ATTENTION`，没有 `ABOVE`。
- **#26**：1920x1080 单屏上窗口初始位置为 `(-14, 20)`（窗口 428x326，
  内容区 400x260），左侧 14px 在屏幕外，标题栏被裁切。

### 2. 原因

1. `ui/update.slint` 原先没有声明 `always-on-top`；
2. Slint 1.18 的 `i-slint-core` 只有 `always_on_top` 字段，没有接线；后端
   `i-slint-backend-winit` 会在属性变化时读取并调用
   `set_window_level(AlwaysOnTop)`，但窗口映射前设置会丢失，之后属性不变不再重试；
3. 窗口位置完全交给 WM，无边框 + 置顶组合下被放到负坐标。

### 3. 修复

**0.2.4（#25）**

- `ui/update.slint`：新增 `in-out property <bool> pin_top: false;` 并绑定
  `always-on-top: pin_top;`（普通 `property` 不生成公开 setter）；
- `src/utils/gui.rs::run_update_gui`：窗口创建后起后台线程，延迟 400ms
  `invoke_from_event_loop` 将 `pin_top` 置 `true`，制造一次属性变化，
  使后端在窗口映射后应用 `WindowLevel::AlwaysOnTop`；
- 验证：窗口 `_NET_WM_STATE` 出现 `_NET_WM_STATE_ABOVE`，点击可正常触发更新。

**2026-11-23（Windows 支持）**

- `ui/install.slint` 的 `MainWindow` 同样增加 `pin_top` + `always-on-top`，
  安装窗口不再依赖更新窗口单独处理；
- `src/utils/gui.rs` 抽出 `schedule_window_placement`：两个窗口显示后延迟
  400ms 置顶并调用 `center_window`；
- Windows 上按主显示器与外框尺寸居中：`window.size()` 不含标题栏/边框，
  用 `GetSystemMetrics` 的 `SM_CXSIZEFRAME` / `SM_CXPADDEDBORDER` /
  `SM_CYCAPTION` 补齐后计算 `set_position`；
- Windows 依赖增加 `Win32_UI_WindowsAndMessaging` feature（`GetSystemMetrics`）；
- 其他平台不强制坐标，继续交给窗口管理器（Wayland 不支持强制定位）。

### 4. Windows 复核（2026-11-23）

构建 `cargo build --features gui`，用 Win32 `GetWindowLongPtr` /
`GetWindowRect` 检查窗口：

| 窗口 | visible | topmost (`WS_EX_TOPMOST`) | rect | 屏幕 | 在屏幕内 | 居中 |
| --- | --- | --- | --- | --- | --- | --- |
| `update --gui` | ✅ | ✅ | `752,390,1168,689` | 1920x1080 | ✅ | ✅ |
| `install --gui` | ✅ | ✅ | `592,270,1328,809` | 1920x1080 | ✅ | ✅ |

说明：#25 的 kiosk 遮挡问题在 Windows 上通过同一置顶机制生效；Linux X11 的
`_NET_WM_STATE` 行为未在本机复核。

### 5. 待办

- Linux/X11 下 #26 的负坐标问题仍需在目标发行版复核（当前非 Windows 平台
  不强制坐标）；
- GitHub #25 / #26 可在目标平台确认后关闭。

---

## I-16 GitHub #32：`dytes update` 后应用 `dytes.json` 中的版本未更新

**发现日期**：2026-11-23（GitHub #32）
**影响版本**：0.2.18 及更早
**严重程度**：待定 —— issue 只有标题，缺命令与日志
**状态**：0.2.19 已修复 —— `update` 成功后把锁文件中的实际版本写回已声明的依赖；GitHub #32 已于 2026-11-23 关闭

### 1. 现象

GitHub #32 只有标题「dytes update不会更新dytes.json中的版本问题」，没有正文、
命令或错误输出。

### 2. 已知实现与可能原因

- **应用自身**：`update_app::update_app_info` 会用远程
  `app/<name>/dytes.json` 合并后写回本地，`version` 与
  `dependencies-core/sys/web/model` 以远程为准，并保留本地额外依赖，
  理论上会更新；
- **依赖包**：`update_one_dependency` 只更新文件并写 `dytes.lock`，
  **不回写**应用根 `dytes.json` 的 `dependencies-*`。`install` 已在 0.2.13
  通过 `upsert_dependency` 写回（见 I-13），`update` 没有对应逻辑。
  因此升级 sys / web / model / core 包后，清单里声明的版本仍是旧值；
  离线模式（`--offline`）按清单声明版本检查，也会继续看到旧版本；
- **本地声明高于线上**：`check_version` 会判定无需更新，`dytes.json` 自然不变，
  这属于预期行为，需要与真实问题区分。

### 3. 相关代码

| 位置 | 作用 |
| --- | --- |
| `src/update/dytes.rs::update_one_dependency` | 按类型分发依赖更新，不写回应用清单 |
| `src/update/dytes.rs::init_dytes_with_options` | `dytes update` 主流程，最后只执行 init 脚本 |
| `src/update/update_app.rs::merge_local_dependencies` | 应用自身更新时合并写回清单 |
| `src/install/cli.rs::upsert_dependency` | install 写回依赖版本（0.2.13） |

### 4. 修复

- `src/update/dytes.rs`：`dytes update` 每次依赖更新成功后调用
  `sync_manifest_dependency`：
  - 版本以 `dytes.lock` 为准（各 `update_*` 函数写入的是实际安装版本）；
  - 只更新应用清单中**已声明**的依赖，不把锁文件里的其他包写成新依赖；
  - `design_plugins` / `design_soft` 仍按约定只记录在锁文件；
  - 写回失败只告警，不把已完成的更新判为失败；
- 单元测试：`set_dependency_version_updates_declared_entry_only`、
  `set_dependency_version_is_noop_when_unchanged`、
  `set_dependency_version_rejects_non_object_group`；
- 应用自身 `version` 仍由 `update_app_info` 合并远程清单写回，不受影响。

### 5. 备注

issue 只有标题；若实际现象是应用自身 `version` 字段未变，需要提交
`dytes update` 输出与 `dytes.json` / `dytes.lock` 的前后内容进一步确认。

---

## I-17 GitHub #33：`dytes publish` 对无导出的 `app` 包直接报缺少 exports

**发现日期**：2026-11-24（GitHub #33）
**影响版本**：0.2.22 及更早
**严重程度**：P2 —— 纯依赖集合的 app 包无法直接发布，有命令行绕过方式
**状态**：0.2.23 已修复 —— app 类型无导出时默认按仅清单发布；非 app 仍报错并提示 `--manifest-only`；GitHub #33 已于 2026-11-27 关闭

### 1. 现象

`train_chat` 是 `type: app` 的包，只有 `dependencies-core` / `dependencies-sys`，
没有任何导出文件：

```bash
$ dytes publish
dytes.json 缺少 exports / platform-exports
```

`dytes publish --dry-run`、`dytes check` 同样在读取清单阶段失败。issue 提到
app 类型只是各种应用的集合，可能不需要导出任何文件，目前却无法发布。

### 2. 原因

`src/publish/cli.rs::run` 用 `DytesConfig::read_with(&config_path, args.manifest_only)`
读取清单，而 `src/utils/dytes.rs::validate_with_options(false)` 在 `exports` 与
`platform-exports` 都为空时直接报错。因此还没走到 publish 自身对“仅清单包”的
处理，也没有机会提示 `--manifest-only`。

“仅清单”发布本身已经支持：

- `src/publish/cli.rs` / `src/publish/dytes.rs`：`--manifest-only` 时跳过导出文件，
  只上传 `dytes.json` 与索引；
- `src/test.rs` 也有同名参数。

### 3. 当前绕过

```bash
dytes publish --manifest-only
```

### 4. 优化建议

- `app` 类型天然可以只是应用 / 依赖集合：无导出时默认允许仅清单发布，或在
  `validate_with_options` 中按 `type: app` 放宽，不必显式加参数；
- 即使保留显式参数，也应在错误信息中提示
  “需求 / 训练 / 纯依赖类包请使用 `--manifest-only`”，与
  `src/publish/cli.rs` 中“没有可导出的文件”的提示保持一致；
- 补充集成测试：无 exports 的 app 包在默认与 `--manifest-only` 下的行为。

### 5. 修复（0.2.23）

- `src/utils/dytes.rs::validate_with_options`：`type: app` 且 `exports` /
  `platform-exports` 均为空时不再报错；其他类型仍报错，错误信息补上
  `--manifest-only` 提示；
- `src/publish/cli.rs`：`run` 与 `evaluate_check` 对“app 且无导出”自动按
  `--manifest-only` 处理，`publish --dry-run` / `check` / 实际发布行为一致；
- `src/publish/dytes.rs::publish_with_options`：入口兜底，直接调用该函数也会
  按仅清单发布；
- 测试：`tests/cli_publish.rs`（app 无导出默认成功、非 app 仍失败）、
  `tests/cli_check.rs`（app 无导出 check 通过、非 app 需 `--manifest-only`）；
- 顺带修复 `src/utils/s3.rs` 的 `clippy::unnecessary_map_or`，使
  `cargo clippy --all-targets --locked -- -D warnings` 通过。

验证：使用 issue 中的 `train_chat` 清单执行 `dytes check` 与
`dytes publish --dry-run` 均成功，输出识别为 app 类型、索引
`app/packages.json`、无导出文件。

---

## I-18 0.2.21–0.2.23 发布构建丢失内置 S3 凭据，self-update / update 失效

**发现日期**：2026-11-24
**影响版本**：0.2.21 / 0.2.22 / 0.2.23（0.2.20 及更早正常）
**严重程度**：P0 —— 发布二进制无法读取 S3，自更新与在线更新全部失败
**状态**：0.2.24 已修复

### 1. 现象

用 0.2.23 发布版在无用户配置的环境中执行：

```text
$ dytes self-update --check
自更新失败: 读取 S3 dytes 清单失败；... S3 凭证未配置 ...
```

`dytes config list` 显示 `s3.access_key` / `s3.secret_key = (未设置)`，说明
二进制没有内置凭据。

### 2. 原因

`build.rs` 通过 `UPYUN_API_KEY` / `UPYUN_API_SECRET`（以及 `UPYUN_BUCKET` /
`UPYUN_ENDPOINT`）把 S3 配置编译进二进制。0.2.21 精简 release workflow 时，
`build` job 的 env 从 `UPYUN_*` 改成了 `AWS_*`（供 aws cli 上传），构建步骤
因此读不到 `UPYUN_*`，三平台发布资产不再内置凭据。

### 3. 修复（0.2.24）

`build` job 同时提供两组变量：

- `UPYUN_API_KEY` / `UPYUN_API_SECRET` / `UPYUN_BUCKET` / `UPYUN_ENDPOINT`：
  供 `build.rs` 注入发布二进制；
- `AWS_ACCESS_KEY_ID` / `AWS_SECRET_ACCESS_KEY`：供 aws cli 上传。

`build-action` 仍不注入凭据：Action 资产供 CI 使用，凭据由调用方通过环境变量
提供（见 [action.md](action.md)）。

### 4. 验证

本机（2026-11-24）无法直连 `s3.api.upyun.com`（连接超时），无法完成端到端
自更新验证。改用发布产物的错误信息对照：

| 版本 | `self-update --check` 结果 |
| --- | --- |
| 0.2.23 | `S3 凭证未配置`（内置凭据缺失） |
| 0.2.24 | `S3 对象读取失败: core/dytes/dytes.json`（已通过凭证检查，失败于网络） |
| 0.2.20（正常基线） | 与 0.2.24 相同 |

结论：0.2.24 已重新内置 UPYUN 凭据；剩余失败与本机到又拍云 S3 的网络有关。
发布 workflow 的 `Verify published files` 已通过 CDN 校验版本化产物、`latest`
别名与清单内容。

---

## I-19 GitHub #34：安装器 / 更新器硬编码 `app/<name>/dytes.json`，`type: core` 的应用无法升级

**发现日期**：2026-11-25（GitHub #34）
**影响版本**：0.2.25（Linux）；Piksel 1.1.32 → 1.1.34 升级
**严重程度**：P1 —— 默认应用（Piksel）的安装 / 升级链路在发布类型改为 `core` 后断链
**状态**：GitHub #34 已于 2026-11-27 关闭 —— Piksel 1.1.36 把 `dytes.json` 改回
`type: app` 并在 release.yml 强制 `app`，实测 1.1.32 可升级到 1.1.36。**通用问题
（安装 / 更新 `type: core` 的应用）仍未修复**，代码仍硬编码 `app/<name>/dytes.json`，
建议另开 issue 跟踪。

### 1. 现象

Piksel 1.1.34 把 `dytes.json` 的 `type` 从 `app` 改为 `core`，发布路径随之变为
`core/piksel/dytes.json`。已安装 1.1.32 的环境执行 `dytes update --check --json`
时，Piksel 被识别为 `type: app`、`remote_version: null`、`needs_update: false`：

```json
{"local_version":null,"name":"piksel","needs_update":false,"remote_version":null,"type":"app"}
```

`install.log` 显示安装器读取的仍是旧路径与旧清单：
`获取应用配置: app/piksel/dytes.json`，内容仍为 1.1.32，于是更新引擎认为无需更新。
同时已安装的 `cyctron`、`dytes` 等 core 包显示 `local_version: null`。

### 2. 原因

安装器、应用更新与更新检查都写死了 `app/<name>/dytes.json`，不参考清单里的
`type`，因此 `core/piksel/dytes.json` 永远不会被查询：

- `src/install/installer.rs:116,123`（0.2.25 仍为
  `run_install_named` 内的 `fetch_text(&format!("app/{}/dytes.json", app))`）；
- `src/update/update_app.rs:31-33` 的 `fetch_remote_app_info` 固定
  `format!("app/{}/dytes.json", name)`；
- `src/update/dytes.rs:664-680` 的 `app_need_update` 用本地清单 `version` 与上述
  远端版本比较，`update --check` 同样走这条路径。

反向的依赖更新则按类型拼接路径（`src/update/dytes.rs:97` 的
`{dep_type.remote_dir()}/{name}/dytes.json`）。当应用自身从 `app` 变为 `core`
后，它落在两套逻辑之间的空隙里：应用更新只认 `app/`，依赖更新又不接管
“应用自身”。另外 core 包没有本地清单时缺少版本来源，`local_version` 只能输出
`null`。

按安装方式区分（0.2.25 实测代码路径）：

- `dytes install`（不指定类型）默认按 core 依赖安装 piksel，走
  `install_dependency` → `update_os`，读 `core/piksel/dytes.json`，能装到 1.1.34；
- `dytes install app piksel` 与内置安装器 `run_install[_named]` 仍读
  `app/piksel/dytes.json`，会装成 1.1.32（`type: app`）；
- 既有的 1.1.32 安装执行 `dytes update` 时走 `app_need_update`，远端仍是
  `app/piksel/dytes.json` 的 1.1.32，永远判定无需更新；依赖也沿用旧清单
  （`dytes 0.1.30`、`chatllm 1.0.16`），整条升级链停在迁移前。

### 3. 线上核实（2026-11-25，CDN `packages.funxdata.com`）

| 对象 | 实际内容 |
| --- | --- |
| `app/piksel/dytes.json` | 200，`version: 1.1.32`、`type: app`；Last-Modified 2026-11-12 |
| `app/packages.json` | 200，piksel 仍为 1.1.13（索引早于清单 1.1.32）；Last-Modified 2026-11-21 |
| `core/piksel/dytes.json` | 200，`version: 1.1.34`、`type: core`；Last-Modified 2026-11-24 |
| `core/packages.json` | 200，`packages` 中 **没有** piksel（只有 cyctron / chatbot / update） |
| `core/piksel/1.1.34/{windows,linux,macos}/piksel.{exe,bin,app}` | 均 200，约 1.23 / 5.65 / 1.70 MB |
| `core/piksel/1.1.33/dytes.json` 及产物 | 404 |

结论：1.1.33 没有成功发布，1.1.34 是迁移到 `core/` 后第一个完整产物；发布侧只
上传了新路径，没有清理 `app/` 旧清单、没有把 piksel 登记进 `core/packages.json`，
`app/packages.json` 里甚至还是更早的 1.1.13。依赖更新因为索引缺条目会回退到
`core/piksel/dytes.json`，仍能读到 1.1.34；而应用自身更新固定读 `app/`，所以只有
“已安装的旧应用”会断链。

### 4. 修复建议

- 远端清单路径按本地 `dytes.json` 的 `type` 解析（`PackageType` 已有
  `app` / `core` 判定，`DependencyType::remote_dir()` 已有目录映射），或同时探测
  `app/<name>` 与 `core/<name>` 并按类型优先匹配；
- 过渡期兼容旧安装：本地 `type: app` 但远端只有 `core/<name>`（或 `core/` 版本
  更高）时回退并迁移——按 core 依赖安装 1.1.34，同时更新本地清单 `type` 与
  `dytes.lock` 记录，避免下次又回到 `app/`；
- 明确应用自身为 `core` 时的更新归属：由应用更新逻辑按类型取清单，还是转为
  依赖更新；避免两条路径都不处理；
- 下载基址也不要用 `format!("app/{}", name)`
  （`src/update/update_app.rs` 第 4 步的 `remote_base`），否则即使读到 core 清单
  也会去 `app/piksel/1.1.34/...` 下载；
- `is_missing_remote_manifest`（`src/update/dytes.rs:1220-1226`）里的
  `text.contains("下载 app/")` 需要随路径一起放宽，否则 core 包缺失会被算作
  更新失败；
- 发布侧补齐数据：把 piksel 登记到 `core/packages.json`，清理或冻结
  `app/piksel`（保留旧安装读取即可，不再作为新版本来源），修正
  `app/packages.json` 里过期的 1.1.13；
- `local_version` 为 `null` 时从安装记录 / 锁文件 / 二进制元数据读取版本，至少在
  `--json` 输出中给出原因；
- 补充集成测试：`type: core` 应用的安装、`update --check`、旧 `app` 安装迁移到
  `core` 以及实际升级。

### 5. 备注

处理意见已回复在 issue 评论：
<https://github.com/dytescn/dytes/issues/34#issuecomment-7150796971>。

issue 提到 Piksel 侧发布配置问题已同步到 `dytescn/Piksel`。本次仅同步记录与
线上核实，未修改代码。

---

## I-20 GitHub #35：exFAT/FAT32 上 symlink 返回 EPERM，install / update 在创建命令入口时中断

**发现日期**：2026-11-27（GitHub #35）
**影响版本**：0.2.26（Linux；安装目录位于 exFAT / FAT32）
**严重程度**：P1 —— 安装目录放在 exFAT / FAT32 移动硬盘的用户无法完成安装或更新
**状态**：已修复（0.2.27 发布）：符号链接失败时依次回退为 shell 包装脚本、复制；chmod 失败不再中断安装。

### 1. 现象

安装目录位于 exFAT 移动硬盘（`/media/funx/dytes`，挂载参数
`rw,nosuid,nodev,relatime,uid=1000,gid=1000,fmask=0022,dmask=0022`）时，
`dytes install core piksel` 的下载与校验都成功，但在创建无后缀命令入口时失败：

```text
  下载完成: /media/funx/dytes/cyctron.bin
  校验通过: cyctron.bin (6756b3e2...)
安装失败: 创建命令入口失败: /media/funx/dytes/cyctron: Operation not permitted (os error 1)
```

改用 `dytes install core piksel --gui` 后，Piksel 自身更新和 cyctron 依赖更新仍然失败；
最终 `piksel.bin`、`cyctron.bin` 虽已落盘，但没有 `piksel` / `cyctron` 命令入口，
安装被判定为失败。

### 2. 原因

`src/update/executable.rs:25` 的 `install_command_alias` 在 Unix 上统一创建符号链接：

```rust
// src/update/executable.rs:35
std::os::unix::fs::symlink(&target, &link)
    .with_context(|| format!("创建命令入口失败: {}", link.display()))?;
```

exFAT / FAT32 不支持符号链接，`symlink(2)` 返回 `EPERM`。该错误被当作致命错误
向上传播，`install` / `update` 因此中断，尽管产物已经下载并通过校验。

### 3. 影响

- 所有把安装目录放在 exFAT / FAT32 移动硬盘上的 Linux 用户都无法完成安装或更新；
- 与 Piksel #4「移动硬盘（exFAT）安装场景」直接相关。

### 4. 修复方案（已实现）

`src/update/executable.rs` 的 `install_command_alias` 改为三级回退：

1. 优先创建符号链接（支持 symlink 的文件系统）；
2. `symlink` 失败时写入 `#!/bin/sh` 包装脚本，用 `$(dirname "$0")/<filename>`
   定位同目录真实产物并透传参数；
3. 包装脚本也失败时复制产物。

同时：

- 给产物 / 入口设置 `0755` 改为尽力而为：exFAT / FAT32 不支持 `chmod`，
  失败不再中断安装或更新；
- 入口创建阶段不再把 `symlink` 的 `EPERM` 当作致命错误；
- 新增 `wrapper_script_execs_sibling_binary` 单测校验脚本内容。

### 5. 备注

GitHub issue：<https://github.com/dytescn/dytes/issues/35>。已修复并随 0.2.27 发布。

---

## I-21 GitHub #36：S3 下载错误被 anyhow 字符串化，重试判定失效，10 次重试实际只执行 1 次

**发现日期**：2026-11-27（GitHub #36）
**影响版本**：0.2.26（Linux）
**严重程度**：P1 —— 临时网络抖动 / 服务端 5xx 会直接导致安装与更新失败
**状态**：已修复（0.2.27 发布）：新增 `RetryableError` 标记，并按 `SdkError` 类型分类可重试错误。

### 1. 现象

更新系统库 `designsoft` 时 S3 HEAD 返回 service error，日志显示“尝试 1/10”后立即
“下载最终失败”，没有任何重试：

```text
  从 S3 下载: system/designsoft/1.0.2/designsoft.so
  下载失败 (尝试 1/10): S3 HEAD 请求失败: system/designsoft/1.0.2/designsoft.so (service error)
  下载最终失败: S3 HEAD 请求失败: system/designsoft/1.0.2/designsoft.so (service error)
```

### 2. 原因

`src/update/loader.rs:208` 的 `is_retryable_error` 只识别保留了原始类型的
`reqwest::Error`：

```rust
fn is_retryable_error(e: &anyhow::Error) -> bool {
    if let Some(req_err) = e.downcast_ref::<reqwest::Error>() {
        if req_err.is_timeout() || req_err.is_connect() || req_err.is_body() {
            return true;
        }
        if let Some(status) = req_err.status() {
            if status.is_server_error() {
                return true;
            }
        }
    }
    false
}
```

但 `download_from_s3` 在 `head_object().send()` 失败时先 `format!` 再 `anyhow!`，
把原始错误转成了字符串（`src/update/loader.rs:236-249`）：

```rust
.map_err(|error| {
    let detail = format!("{error}");
    ...
    anyhow!("S3 HEAD 请求失败: {object_name} ({detail})")
})?;
```

类型信息丢失后，`downcast_ref::<reqwest::Error>()` 永远返回 `None`，所有 S3 错误都被
判为不可重试，循环在第一次失败后 `break`，`MAX_RETRIES = 10`
（`src/update/loader.rs:173`）实际只执行 1 次。

### 3. 影响

短暂网络抖动或服务端 5xx 会直接导致安装 / 更新失败，与“10 次重试”的设计预期不符，
也掩盖了真实的临时性故障。

### 4. 修复方案（已实现）

- 新增 `RetryableError` 标记类型：`is_retryable_error` 沿 `anyhow` 错误链查找该
  标记，即使错误被 `format!` / 多层 `context` 处理也不会丢失重试意图；
- `download_from_s3` 的 HEAD 与 Range GET 用 `is_retryable_s3_error` 按
  `SdkError` 变体分类：超时、`DispatchFailure`、`ResponseError` 与 5xx 可重试，
  4xx / 404 / NoSuchKey 仍快速失败；
- S3 数据流读取遇到 `TimedOut` / `ConnectionReset` / `ConnectionAborted` /
  `BrokenPipe` / `UnexpectedEof` 时标记为可重试；
- HTTP 下载的 5xx 响应同样标记为可重试；
- 重试日志改为显示实际生效的 `max_retries`（启动模式为 2，不再误报 10）；
- 新增 `retryable_marker_survives_context`、`plain_error_is_not_retryable` 单测。

### 5. 备注

GitHub issue：<https://github.com/dytescn/dytes/issues/36>。已修复并随 0.2.27 发布。

---

## I-22 GitHub #37：旧版 Windows 启动即报 `SystemParametersInfoForDpi` 入口点缺失

**发现日期**：2026-11-28（GitHub #37）
**影响版本**：0.2.28（Windows GUI 发布产物）
**严重程度**：P1 —— 受影响的系统上任何命令（含 `dytes --version`）都无法启动
**状态**：已定位（2026-11-28 评论补充环境与导入表分析）；修复方向定为方案 A
（声明最低 Windows 10，由下载页 / 安装脚本提示）；**程序自身无法在旧系统上提示**，
因为失败发生在加载器阶段；**无 GUI 构建也无法回到 Win7**，需要旧工具链
（rustc ≤ 1.77）全量重建才可能，当前不作为目标

### 1. 现象

双击 / 运行发布产物（`dytes-windows.exe`）时，进程尚未进入 `main`，Windows 加载器
直接弹出错误框：

```text
dytes-windows.exe - 无法找到入口
无法定位程序输入点 SystemParametersInfoForDpi 于动态链接库 user32.dll 上。
```

issue 评论补充的实测环境（Windows 7 SP1 x64）：

| 项 | 值 |
| --- | --- |
| OS | Windows 7 Ultimate SP1 x64（6.1.7601.65536） |
| 程序 | `dytes-windows.exe`（31,905,280 字节，`F:\soft\`） |
| 运行库 | VC++ 2015–2022 x64 14.44.35211、UCRT 10.0.14393、.NET 4.8 均已安装 |
| 启动方式 | 双击 / 命令行直接运行 |
| 退出码 | `0xC0000139`（`STATUS_ENTRYPOINT_NOT_FOUND`，cmd 中为 `-1073741511`） |

系统日志（`Application Popup`，事件 ID 26）记录同一条“无法定位程序输入点”；
进程约 0.5 秒退出，窗口不出现。

### 2. 原因

Windows 7 的 `user32.dll` 没有该导出；发布产物在 `windows-latest`（MSVC）上编译，
PE 导入表带上这些符号后，加载器在启动阶段就失败（`0xC0000139`），与 dytes 自身逻辑
无关。评论中的导入表分析显示**不止 Slint 一处**：

| DLL | 导入符号 | 最低系统要求 |
| --- | --- | --- |
| `user32.dll` | `SystemParametersInfoForDpi` | Windows 10（评论记为 1703） |
| `bcryptprimitives.dll` | `ProcessPrng` | Windows 10 |
| `combase.dll` | `CoTaskMemFree`、`CoCreateFreeThreadedMarshaler` | Windows 8（Win7 无 `combase.dll`） |
| `api-ms-win-core-winrt-error-l1-1-0.dll` | `RoOriginateErrorW` | Windows 8 |
| `kernel32.dll` | `GetSystemTimePreciseAsFileTime` | Windows 8 |
| `api-ms-win-core-synch-l1-2-0.dll` | `WaitOnAddress`、`WakeByAddressSingle` | Windows 8（Win7 仅装 UCRT 后有 downlevel 版本） |

来源分两层：

1. **Slint**：`i-slint-backend-winit-1.18.1/lib.rs:865` 直接
   `use windows::Win32::UI::HiDpi::SystemParametersInfoForDpi;` 并调用，没有回退；
   对照 winit 自己的 `GetDpiForWindow` 是 `get_function!("user32.dll", ...)`
   动态解析（`winit-0.30.13/src/platform_impl/windows/util.rs:257`），旧系统可降级；
2. **Rust 标准库与依赖链**：`ProcessPrng`、`WaitOnAddress`、`combase`、WinRT 错误
   API 等来自 std / `windows` crate 等底层依赖。Rust 1.78 起官方最低支持 Windows 10，
   这些调用在 std 内部，**与是否启用 `gui` feature 无关**。

因此：

- 装 VC++ 运行库 / UCRT / .NET 都无法补上这些系统级 API（评论已实测 Win7 的
  `bcryptprimitives.dll` 没有 `ProcessPrng` 导出）；
- 即使产出不带 `gui` 的 `dytes-action.bin` 同类产物，只要仍用当前工具链编译，
  Win7 依然起不来；回到 Win7 需要 rustc ≤ 1.77 全量重建并规避 WinRT / Win10 API；
- `Cargo.toml` 的 `gui` feature 会让问题更早暴露（`SystemParametersInfoForDpi`），
  但不是唯一原因。

### 3. 影响

- Windows 7 / 8 / 8.1、Windows Server 2008 R2 / 2012 (R2)、Windows 10 RTM / 1511
  等系统的用户无法使用 dytes（含 `install` / `update` / `self-update`）；
- 错误发生在加载器阶段，没有 dytes 自己的错误提示，用户容易误判为文件损坏、
  下载不完整或“缺运行库”，从而反复安装 VC++ / .NET 运行库（实测无效）；
- 与「换一台电脑就不能运行」的反馈直接相关，属于产物层面的系统依赖问题
  （详见 [packaging.md](packaging.md) 的运行时依赖审计）。

### 4. 已确认信息

- 系统：Windows 7 Ultimate SP1 x64（6.1.7601.65536）；
- 产物：`dytes-windows.exe`（0.2.28 构建，`TimeDateStamp 2026-11-27 07:32 UTC`，
  `Subsystem = WINDOWS_CUI`，无版本资源）；
- 运行库齐全（VC++ 14.44 / UCRT 14393 / .NET 4.8），排除“缺运行库”这一常见误判；
- 双击与命令行启动均失败，事件日志与退出码一致。

若后续还有用户反馈，仍需补充：系统架构（ARM64 / x86）、dytes 版本与产物来源
（GitHub Release / `packages.funxdata.com` / 随 Piksel 分发）、是否安装了安全软件拦截。

### 5. 修复方向

| 方案 | 内容 | 成本 | 取舍 |
| --- | --- | --- | --- |
| A. 明确并声明基线 | 官网 / README / 下载页 / 安装脚本标注最低 Windows 10，并在下载页与脚本入口检测系统版本 | 低 | 不扩大兼容范围；程序自身仍会以 `0xC0000139` 退出，但用户在下下载 / 安装前就能看到要求 |
| B. 旧工具链重建 | 用 rustc ≤ 1.77 重新构建（并规避 WinRT / Win10 API），可同时产出无 GUI 版本 | 高 | 需锁定旧工具链、验证所有依赖的 MSRV，失去新版本修复；旧系统可用 CLI |
| C. patch Slint | fork winit 后端，用 `GetProcAddress` 探测 `SystemParametersInfoForDpi` | 高 | **不足以解决**：std / 其它依赖的 `ProcessPrng` 等仍要求 Win10 |
| D. 仅降级 Slint | 回退到不含该调用的版本 | 低—中 | 同样不足，且可能回退窗口 / 渲染修复 |

**决策（2026-11-28）**：按 A 处理——README / 下载页 / 安装脚本声明最低 Windows 10
并在脚本入口检测版本；**不承诺程序自身弹提示**（加载器阶段失败，Rust 代码不会执行），
也不做 B（旧工具链）、C（patch Slint）。评论进一步表明“无 GUI 产物即可兼容 Win7”
的假设不成立，方案 B 的取舍已据此修正。
详见 [requirements/windows-installer.md](requirements/windows-installer.md) D2。

### 6. 验证方法

```powershell
# 产物导入表（任意 Windows 机器）
dumpbin /imports dytes-windows.exe | findstr /i "SystemParametersInfoForDpi ProcessPrng WaitOnAddress"
# 旧系统回归：Windows 7 SP1 / Windows 8.1 虚拟机执行 install 脚本，
# 期望脚本给出“需要 Windows 10”的提示并退出，而不是让用户下载后双击失败
```

CI 可增加 `dumpbin /imports` 白名单检查，阻止其它 Win10-only API 再次混入。

注意：**旧系统上的提示只能放在下载页 / 安装脚本**（例如 `install.ps1` 用
`[Environment]::OSVersion` 判断），不能放在 dytes 自己的二进制里——PE 加载器在
`main` 之前就会因导入表缺失而失败。

### 7. 备注

GitHub issue：<https://github.com/dytescn/dytes/issues/37>。
2026-11-28 评论（funxsayhi）补充了 Windows 7 SP1 x64 的环境、PE 导入表与退出码，
并指出“安装任何运行库都无法解决”；该结论已被采纳，I-22 的原因与方案表据此更新。
