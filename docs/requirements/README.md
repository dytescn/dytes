# 需求文档

待实现功能的需求与设计约定。已完成并发布的变更记录在 [../issues.md](../issues.md)
与 `release-*.md`。

| 文档 | 状态 | 说明 |
| --- | --- | --- |
| [windows-installer.md](windows-installer.md) | 关键决策已确认，细节待评审 |
| [windows7-lite.md](windows7-lite.md) | 方案评估，待评审 | Windows 7 兼容 Lite：rustc 1.77 + 无 GUI + 轻量 S3 客户端，含改动清单与 5–7 天估算 | `dytes-windows.exe` 免 irm / PowerShell 双击自安装：安装到用户目录、写用户 `PATH`、校验与回滚、卸载 |
