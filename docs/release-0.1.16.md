# dytes 0.1.16 版本更新

## self-update 与 CDN 缓存

- `dytes self-update` 的 CDN 分支优先读取
  `core/dytes/<version>/dytes.json`（版本化不可变路径）；
- 版本化清单不可用时才回退 `core/dytes/dytes.json`；
- 避免未版本化清单被 CDN 缓存成旧版本后，导致自更新判断错误。

这不能替代 CDN purge；未版本化 `dytes.json` / `latest/version.js`
仍建议在发布后刷新缓存。
