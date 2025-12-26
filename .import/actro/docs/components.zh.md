# 组件

askit 提供一组可在 Host 和 Guest 环境中使用的 UI 组件。

## 组件工作原理

askit 采用与 `rill/let` 一致的模型：**Guest 侧组件是 element 标识（string ElementType）**，由 reconciler 负责序列化 props 并生成操作。

在 **Guest** (QuickJS) 中：直接写 JSX。

在 **Host** (React Native) 中：同名 element type 会被注册为真实 RN 组件。

```tsx
import { UserAvatar } from 'askit';

export function App() {
  return <UserAvatar uri="https://..." size={48} />;
}
```

---

## StepList

带状态指示器的步骤列表，常用于引导流程或进度追踪。

### Props

| 属性 | 类型 | 必需 | 描述 |
|------|------|------|------|
| `items` | `StepItem[]` | 是 | 步骤对象数组 |
| `loop` | `boolean` | 否 | 是否循环显示连接线 |
| `onStepPress` | `(item, index) => void` | 否 | 步骤点击回调 |
| `activeColor` | `string` | 否 | 激活状态颜色 |
| `completedColor` | `string` | 否 | 完成状态颜色 |
| `pendingColor` | `string` | 否 | 待处理状态颜色 |
| `errorColor` | `string` | 否 | 错误状态颜色 |
| `lineWidth` | `number` | 否 | 连接线宽度 |
| `style` | `StyleProp` | 否 | 附加样式 |

### StepItem 对象

```typescript
interface StepItem {
  id: string;           // 必需：唯一标识符
  title: string;        // 必需：步骤标题
  subtitle?: string;    // 可选：步骤副标题
  status: StepStatus;   // 必需：步骤状态
  icon?: string;        // 可选：自定义图标
}

type StepStatus = 'pending' | 'active' | 'completed' | 'error';
```

### 示例

```tsx
import { StepList } from 'askit';

export function App() {
  return (
    <StepList
      items={[
        { id: '1', title: '连接钱包', status: 'completed' },
        { id: '2', title: '验证身份', status: 'active' },
        { id: '3', title: '完成设置', status: 'pending' },
      ]}
    />
  );
}
```

---

## ThemeView

适应当前主题的容器视图，提供预设的主题变体。

### Props

| 属性 | 类型 | 必需 | 描述 |
|------|------|------|------|
| `children` | `ReactNode` | 否 | 子元素 |
| `variant` | `'primary' \| 'secondary' \| 'surface' \| 'background'` | 否 | 主题变体 |
| `padding` | `number \| 'none' \| 'small' \| 'medium' \| 'large'` | 否 | 内边距 |
| `style` | `StyleProp` | 否 | 附加样式 |

### 示例

```tsx
import { ThemeView } from 'askit';

export function App() {
  return (
    <ThemeView variant="surface" padding="medium">
      {/* ... */}
    </ThemeView>
  );
}

export function AppNumericPadding() {
  return (
    <ThemeView variant="primary" padding={16}>
      {/* ... */}
    </ThemeView>
  );
}
```

---

## UserAvatar

带降级支持的用户头像显示。

### Props

| 属性 | 类型 | 必需 | 描述 |
|------|------|------|------|
| `uri` | `string` | 否 | 图片 URL |
| `name` | `string` | 否 | 用户名（用于降级显示首字母） |
| `size` | `number \| 'small' \| 'medium' \| 'large'` | 否 | 头像尺寸 |
| `showOnlineStatus` | `boolean` | 否 | 是否显示在线状态指示器 |
| `isOnline` | `boolean` | 否 | 在线状态 |
| `onPress` | `() => void` | 否 | 点击回调 |
| `style` | `StyleProp` | 否 | 附加样式 |

### 示例

```tsx
import { UserAvatar } from 'askit';

export function App() {
  return (
    <>
      <UserAvatar uri="https://example.com/avatar.png" size={48} />
      <UserAvatar uri="https://example.com/avatar.png" size="large" />
      <UserAvatar name="John Doe" size="medium" />
      <UserAvatar
        uri="https://example.com/avatar.png"
        showOnlineStatus
        isOnline
        onPress={() => console.log('头像被点击')}
      />
    </>
  );
}
```

---

## ChatBubble

聊天消息气泡组件。

### Props

| 属性 | 类型 | 必需 | 描述 |
|------|------|------|------|
| `content` | `string` | 否 | 消息文本内容 |
| `children` | `ReactNode` | 否 | 子元素（自定义内容） |
| `isOwn` | `boolean` | 否 | 是否为当前用户的消息 |
| `showTail` | `boolean` | 否 | 是否显示气泡尾部 |
| `timestamp` | `string \| number \| Date` | 否 | 消息时间戳 |
| `status` | `'sending' \| 'sent' \| 'delivered' \| 'read' \| 'error'` | 否 | 消息状态 |
| `renderMarkdown` | `boolean` | 否 | 是否渲染 Markdown |
| `onPress` | `() => void` | 否 | 点击回调 |
| `onLongPress` | `() => void` | 否 | 长按回调 |
| `style` | `StyleProp` | 否 | 附加样式 |

### 示例

```tsx
import { ChatBubble } from 'askit';

export function App() {
  return (
    <>
      <ChatBubble content="你好!" isOwn={false} timestamp="10:30 AM" />
      <ChatBubble content="嗨!" isOwn timestamp={Date.now()} status="delivered" />
      <ChatBubble
        content="长按我试试"
        isOwn
        showTail
        onPress={() => console.log('点击')}
        onLongPress={() => console.log('长按')}
      />
    </>
  );
}
```

---

## 自定义组件

你可以按照 rill/let 的方式创建自定义组件：在 Guest 侧定义 element 标识，在 Host 侧注册实际实现。

### 在 Guest 中（element 标识）

```tsx
// my-component.guest.tsx
import type React from 'react';

export type MyComponentProps = { title: string };

export const MyComponent = 'MyComponent' as unknown as React.ElementType<MyComponentProps>;
```

### 在 Host 中（React Native）

```typescript
// my-component.native.tsx
import React from 'react';
import { View, Text } from 'react-native';

export function MyComponent({ title, children }: MyComponentProps) {
  return (
    <View>
      <Text>{title}</Text>
      {children}
    </View>
  );
}
```

### 在 Core 中注册

```typescript
// 在 Host App 设置中
import { MyComponent } from './MyComponent.native';

engine.register({ MyComponent });
```
