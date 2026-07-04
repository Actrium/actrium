# Input Hooks 说明

本文档只描述语音输入相关的两个 controller hook，以及当前 `ActrInput` 的接入方案。

## 1. `useActrVoiceController`

文件：[`useActrVoiceController.ts`](/Users/example-dev-4/Desktop/work/github/ipad/actro-runtime/src/actr-host/features/input/hooks/useActrVoiceController.ts)

职责：

- 处理“按住直接发送语音消息”的完整链路
- 管理录音启动、切片缓存、阈值提交、远端语音会话开启与结束
- 在发送完成后把本地语音文件落盘，并通过消息回调补回消息流

实现要点：

- 通过 `useAudioRecordingStream` 持续拿到音频切片
- 录音开始后先进入本地缓冲，达到 1 秒阈值后才真正创建语音消息并启动远端语音会话
- 松手正常结束时，先等待切片发送完成，再调用 `endVoice`
- 上滑取消或阈值前松手时，会直接丢弃当前录音
- 语音消息相关的会话能力不在本 hook 内部生成，而是依赖 `useActrMessageFlow` 传入：
  - `processVoiceMessageSend`
  - `sendVoiceChunk`
  - `startVoice`
  - `endVoice`
  - `onVoiceMessageReady`
  - `onVoiceMessageCanceled`

适用场景：

- 需要把用户语音作为“语音消息”直接发送，而不是转成文本时

## 2. `useActrSpeechRecognitionController`

文件：[`useActrSpeechRecognitionController.ts`](/Users/example-dev-4/Desktop/work/github/ipad/actro-runtime/src/actr-host/features/input/hooks/useActrSpeechRecognitionController.ts)

职责：

- 处理“按住开始语音识别，松手后按文本发送”的链路
- 管理识别开始、取消、停止，以及最终 transcript 的发送时机

实现要点：

- 基于 `SpeechRecognitionService` 启动和停止识别
- 识别过程中只缓存 transcript，不直接把文本写入输入框
- 非取消松手时，先从 `SpeechRecognitionService` 读取当前 transcript 作为兜底，再调用 `stop`
- 等待识别服务触发 `onEnd` 后，统一调用 `processMessageSend` 发送文本消息
- 取消时调用 `abort`，不会发送任何消息

适用场景：

- 需要保留“按住说话”的交互，但最终发送的是文本消息时

## 3. 当前接入方案

当前 `ActrInput` 使用的是 `useActrSpeechRecognitionController`。

接入位置：

- 文件：[`ActrInput.shared.tsx`](/Users/example-dev-4/Desktop/work/github/ipad/actro-runtime/src/actr-host/features/input/ActrInput.shared.tsx)

当前行为：

- Phone 和 Pad 的语音按钮、长按手势入口仍然保留原有 UI 结构
- 但上层传入的 `handleVoiceStart / handleVoiceEnd / handleVoiceChangeState` 已经切到 `useActrSpeechRecognitionController`
- 因此当前实际行为是：
  - 按住开始语音识别
  - 上滑取消则不发送
  - 松手时结束识别
  - 识别出的文本直接走 `processMessageSend` 发送

## 4. 如果要恢复原始语音消息发送

恢复点主要在 [`ActrInput.shared.tsx`](/Users/example-dev-4/Desktop/work/github/ipad/actro-runtime/src/actr-host/features/input/ActrInput.shared.tsx)：

- 把当前接入的 `useActrSpeechRecognitionController` 改回 `useActrVoiceController`
- 同时重新把 `useActrMessageFlow` 返回的语音会话能力传回去

Phone / Pad 的视图层通常不用重写，因为它们主要只是手势入口，真正的语音业务逻辑在 controller hook 里。
