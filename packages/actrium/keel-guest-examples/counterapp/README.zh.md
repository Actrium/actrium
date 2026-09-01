# Counter Keel Guest

Counter 是一个可复用的 `keel.guest/1` 最小 Workload，用于验证 Actrium Actor
Host。它只通过符合 `actr-package/1` 的 `.actr` 包分发。

在当前目录执行：

```bash
bun install
bun run build
bun run build:package
bun run verify:package
```

签名包输出到 `dist/actrium-counter-1.0.0.actr`。这里与 Echo 示例共用仓库内的开发
密钥；该密钥不能用于正式发布。

包结构如下：

```text
actrium-counter-1.0.0.actr
├── manifest.toml
├── manifest.sig
└── workload/
    └── guest.js
```

规范模型见
[`actr/protocols/actor-package.zh.md`](../../../../actr/protocols/actor-package.zh.md)。
