# Actrix Realm v1 依赖风险登记

状态：Realm v1 发布输入，不是一次性免责清单。依赖或运行模式变化时必须重新评估。

## 范围与原则

本登记只覆盖 Actrix 默认单节点、SQLite profile（当前实现单 Realm；多 Realm 目标契约
不引入新的运行时依赖类别，实现时需复评 authority descriptor 获取与 JCS 规范化依赖） 及其内嵌 Admin UI。仓库中
示例、其他语言 binding 和实验目录的告警由各自 owner 处理，不能被本文替代。

- `Cargo.lock` 的 Rust 审计不允许 advisory ignore；
- Admin UI 对 high/critical npm advisory 默认 fail closed；
- 只有能够由代码和构建方式证明不可达的告警才可暂时例外；
- 例外必须绑定具体版本和架构不变量，变化后自动失败；
- 独立安全评审仍需复核可达性判断。

## 2026-07-29 处置结果

| 依赖/告警 | 处置 | 结果 |
|---|---|---|
| `jsonwebtoken` / GHSA-h395-gr6q-cpjc | 从 9.3.1 升级至 10.4.0；使用已在进程中存在的 AWS-LC backend，不引入存在公开 advisory 的 Rust `rsa` backend | 已修复 |
| `opentelemetry_sdk` / GHSA-w9wp-h8wv-79jx | Actrix、Platform、Signaling 和 OTLP/proto/tracing 组合一起升级至兼容的 0.32.x 版本；删除未使用的旧 Jaeger exporter | 已修复 |
| `lru` / GHSA-rhfx-m35p-ff5j | TURN cache 从 0.12.5 升级至 0.16.4 | 已修复 |
| React Router 的 RCE/XSS/redirect/DoS 历史告警 | `react-router(-dom)` 升级至当前稳定 7.18.2 | 除下述 RSC 告警外已修复 |
| Vite/Rollup/Babel/PostCSS/smol-toml 告警 | 重新解析 npm 与 pnpm 锁文件至已修复版本，生产 bundle 重新构建 | 已修复 |
| 原 CI 忽略的 3 个 Rust advisory | 更新锁文件后 `maxminddb 0.26`、`protobuf 2`、`rsa 0.9` 均不再存在；删除 CI 中全部 `cargo audit --ignore` | 已消除豁免 |

`cargo audit` 仍报告 5 个不会使其失败的 warning，不能把它们伪装成「零风险」：

- `audiopus_sys` unmaintained 不在 Actrix 依赖树中；
- `bincode 1.3.3` unmaintained 由上游 `webrtc -> dtls` 引入；
- `rustls-pemfile 2.2.0` unmaintained 由 Actrix Platform 和 `axum-server` 使用；
- `spin 0.9.8` 被 yanked，由 `sqlx-sqlite -> flume` 引入；
- `anyhow 1.0.102` 的 `RUSTSEC-2026-0190` 只涉及
  `Error::downcast_mut()`；仓库 Rust 源码没有调用该 API。

这些项目目前没有已知可达的 P0/P1 路径，但依赖 owner 仍应跟随上游替换。若
`anyhow` warning 出现修复版，或代码开始调用 `downcast_mut()`，必须立即升级或阻断。

## 暂时接受：React Router RSC-only 告警

`GHSA-qwww-vcr4-c8h2` 将 React Router `>=7.12.0 <8.3.0` 标为 high，但当前 npm registry
的最新稳定版是 7.18.2，尚无其声明的修复版本。该问题要求 React Server Components
Mode 和 server action 执行路径。

Actrix Admin UI 的受评估 profile 是静态 Vite SPA：

- `App.tsx` 只使用 `BrowserRouter`/`Routes`，没有 data router、SSR 或 RSC；
- 依赖中没有 `@react-router/*` RSC tooling 或 `react-server-dom-webpack`；
- Actrix 只嵌入 `vite build` 的静态产物，不执行 React server action。

因此当前攻击路径不可达。`scripts/check-production-audit.mjs` 将例外严格绑定到
`react-router 7.18.2` 和上述静态路由不变量；出现任何其他 high/critical advisory、
升级 Router、引入 RSC tooling 或改变路由模式时，Admin UI CI 会失败。

退出条件满足任一项即可：

1. 上游发布包含修复的稳定版本并完成升级；
2. Admin UI 移除 React Router；
3. 若计划采用 SSR/RSC，先删除例外并完成新的威胁模型与独立评审。

## 仓库级剩余责任

GitHub fork 在本次处置前报告 155 个 open alerts，其中大量属于 Web binding、示例和
实验目录，不能推断为 Actrix 生产二进制可达，也不能因不在 Actrix 内而全局忽略。
合并后应等待 Dependabot 按新锁文件重新计算，并由对应 owner 对剩余 manifest 逐项
确定升级、不可达证明或移除。Realm v1 的独立评审报告必须引用固定 commit，并复核
本文唯一例外。
