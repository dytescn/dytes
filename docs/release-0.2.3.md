# 0.2.3

## `dytes test` 依赖在线更新

- `dytes test` 不再复制本地已安装依赖：当前调试的包使用本地构建产物，
  依赖在线更新下载到 `.dytes/env/`。
- 下载范围：
  - 清单 `dependencies-core` / `dependencies-sys` / `dependencies-web` /
    `dependencies-design_plugins` / `dependencies-design_soft` /
    `dependencies-model` 声明的全部依赖（含传递依赖）；
  - `test-dependency` 补充声明的包；同名条目以 `test-dependency` 为准。
- 版本规则与 `dytes update` 一致：按声明 semver 安装线上最新版本；
  精确声明高于线上版本时报错。
- 依赖下载或安装失败立即中止，返回退出码 `1`，不进入安装演练，也不回退到
  本地复制。
- `.dytes/env/dytes.json` 保存合并后的依赖清单，
  `.dytes/env/dytes.lock` 记录安装结果。
- 不执行依赖包的 init 脚本，不修改真实安装目录。

示例：chatllm 的 `chatllm.so` 属于当前包产物，不下载；`database.so`、
`kv_lib.so` 是依赖包，会下载到 `.dytes/env/libary/`。

## 测试

- 集成测试更新为在线依赖语义；
- 新增 `test-dependency` 合并与同名覆盖测试；
- 新增依赖下载失败中止测试（不可达 S3 endpoint）。

## 文档

- 项目介绍统一为英文 `Design software package management tool`；
- 同步 `docs/commands/test.md`、`docs/cmd.md`、`docs/dytes.md`、
  `skills/testenv.md` 与 `docs/issues.md` I-06。

## 升级

```bash
dytes self-update
```
