# dytes 已知问题

> 记录实际使用中遇到的阻塞问题、复现方式与修复方向。
> 最近更新：2026-11-22（同步 GitHub #23 / #24 / #27 / #28 / #29 / #30）

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
**状态**：`--dir` 未修复；`dytes test` 只读 `test-dependency` 的部分已于 0.2.3 修复（见 I-06）

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

### 3. 修复方向

- `set_current_dir` 之前把 `dir` 转成绝对路径（`std::fs::canonicalize` 或
  `current_dir().join(dir)`），后续统一使用绝对路径；
- 或在切换目录前读取清单，切换只影响依赖安装阶段。

### 4. 备注

- GitHub #23 提到的 `dytes test` 只读 `test-dependency` 已在 0.2.3 修复：
  `merge_test_dependencies` 把 `test-dependency` 合并进 `dependencies-*` 后下载（I-06）；
- `dytes init` 默认生成 `exports` 的疑问见 I-10。

---

## I-09 GitHub #24：S3 匿名读取 403 / 缺少凭据导致 update / install / self-update 失效

**发现日期**：2026-11-18（GitHub #24）
**影响版本**：0.2.3 及所有未内置 S3 凭据的构建（0.2.10 本地构建可复现）
**严重程度**：P0 —— 所有在线读写链路中断
**状态**：0.2.11 已改进错误提示；根因（bucket 匿名策略 / 凭证发放）仍待运维确认

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
  （发布 0.2.11 后返回 `v0.2.11`）。

---

## I-10 GitHub #27：`dytes init` 生成的清单出现空 `exports`

**发现日期**：2026-11-21（GitHub #27）
**影响版本**：issue 未注明（报告日期 2026-11-21）
**严重程度**：P2 —— 不影响运行，但生成内容与预期不符
**状态**：0.2.11 未复现，待补充复现步骤

### 1. 现象

issue 描述 `dytes init` 生成的 `dytes.json` 中出现 `"exports": {}`，期望为空或省略。

### 2. 0.2.11 核查

对 app / core / sys / web / design_plugins / design_soft / model 七种类型分别执行
`dytes init`，结果：

- `exports` / `platform-exports` 要么是非空映射，要么完全不出现；
- `--no-exports` 会省略 `exports` 与 `platform-exports`；
- 未生成空对象 `{}`。

相关实现：`src/utils/templates.rs::init_manifest`。

### 3. 需要补充

请提供 `dytes init` 的完整命令、包类型、dytes 版本与生成的 `dytes.json`，
确认是否为旧版本、`--no-exports` 或后续手工编辑导致。

---

## I-11 GitHub #28：`dytes test` 环境需要有可用的 S3 访问

**发现日期**：2026-11-21（GitHub #28）
**影响版本**：0.2.3 起（依赖改为在线下载）
**严重程度**：P2 —— 无凭证 / 无网络时测试环境无法建立
**状态**：符合既有设计，待确认是否补充离线模式

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
**状态**：待复现；部分疑为用法与 README 不一致

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

### 3. 待办

- 请补充实际命令与完整错误输出，确认是否为 I-08 的相对 `--dir` 问题、I-09 的凭证问题，
  或仅仅是命令用法；
- 统一 README / `docs/commands/install.md` 中“安装应用”的入口说明（`update --auto`）。


