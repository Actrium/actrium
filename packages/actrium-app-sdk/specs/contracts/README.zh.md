# ASK Contracts（Host ↔ Guest 事件契约）

> 目标：用一份**可版本化**的契约（contracts），把 Host↔Guest 的交互事件收敛为“可检查、可生成类型、可演进”的 API。

本目录是 **唯一事实来源（source of truth）**：
- 事件名
- 事件方向（Host→Guest / Guest→Host）
- payload 字段与类型
- 版本号

生成产物（TypeScript 类型）位于：
- `askit/src/contracts/generated.ts`

生成脚本：
- `askit/scripts/generate-contracts.ts`

