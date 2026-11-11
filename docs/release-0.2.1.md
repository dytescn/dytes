# 0.2.1

## 修复 Linux glibc 基线过高

- Linux 构建机从 `ubuntu-latest`（Ubuntu 24.04 / glibc 2.39）固定为
  `ubuntu-22.04`（glibc 2.35），避免产物要求过高的 GLIBC 符号版本。
- 构建后新增 GLIBC 检查：`readelf --version-info` 中最大符号版本超过 2.35 时
  发布失败，防止以后 runner 升级导致基线再次漂移。

## 兼容范围

- 支持 glibc ≥ 2.35：Ubuntu 22.04+、Debian 12+、RHEL 9+ 等。
- CentOS 7/8、Ubuntu 20.04、Debian 10/11 仍无法运行；如需覆盖，后续改用
  manylinux 2.28 / 2.17 容器构建（见 `docs/issues.md` I-05）。

## 升级

0.2.0 的 Linux 产物受该问题影响，请升级到 0.2.1：

```bash
dytes self-update
```
