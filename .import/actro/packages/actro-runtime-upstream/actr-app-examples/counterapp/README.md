#### ACTR 快速上手

`actr` 最终会被放到沙箱中运行，所以需要的能力需要 上层（Host）提供

只需要关心库 `askit`

##### API 能力

**现有能力见 `askit 库` `specs/contracts/ask.contracts.v1.json`**

在 `actr` 中使用方式，以 清除历史记录 为例

```ts
const { request: clearChatHistory } = useEventBridge(
    'CLEAR_CHAT_HISTORY',
    'CLEAR_CHAT_HISTORY_RESULT',
  );

const result = await clearChatHistory({
  requestId: 'xxx', // 需要唯一
  ... // 其他参数，和 ask.contracts.v1.json 中 CLEAR_CHAT_HISTORY payload 相对应
})
// 返回的 result 类型和  ask.contracts.v1.json 中 CLEAR_CHAT_HISTORY_RESULT payload 相对应
```


##### UI 组件

基础组件在 `rill/let` 中引用，比如 `View` `Text` 等
ACTR 提供了部分封装后的 UI 组件，位于 `actr` 中的 `src/ui` 目录下

##### 打包

```bash
yarn run build:actr
```


#### ACTR 能力开发

只需要关心两个关键的库 `askit` 和 `loom` 即可

新增能力步骤

1. 在 `askit` 中定义能力接口（`specs/contracts/ask.contracts.v1.json`），后执行 `bun run generate:contracts` 生成
2. 如是基础能力，在 `askit` 中 `src/api/EventHandler.host.ts` `handlers` 实现
3. 如是业务相关，在 `loom` 中（`src/actr-host/features/app/useExtensionEvent.ts`）实现

#### ACTR UI 组件开发

1. 在 `askit` 库 `src/ui` 下创建，注意在外层文件导出
2. 在 `loom` 库 `src/actr-host/features/app/ActrTabView.tsx:hostComponents` 中注册

