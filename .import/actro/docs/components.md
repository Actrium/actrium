# Components

askit provides a set of UI components that work across Host and Guest environments.

## How Components Work

askit follows the same model as `rill/let`: **Guest components are plain element identifiers** (string ElementType).
The reconciler serializes props and produces operations.

In **Guest** (QuickJS): write JSX.

In **Host** (React Native): the same element types are registered to real React Native components.

```tsx
import { UserAvatar } from 'askit';

export function App() {
  return <UserAvatar uri="https://..." size={48} />;
}
```

---

## StepList

A step-by-step list with status indicators, commonly used for onboarding or progress tracking.

### Props

| Prop | Type | Required | Description |
|------|------|----------|-------------|
| `items` | `StepItem[]` | Yes | Array of step objects |
| `loop` | `boolean` | No | Whether to show connecting line in loop |
| `onStepPress` | `(item, index) => void` | No | Step press callback |
| `activeColor` | `string` | No | Active status color |
| `completedColor` | `string` | No | Completed status color |
| `pendingColor` | `string` | No | Pending status color |
| `errorColor` | `string` | No | Error status color |
| `lineWidth` | `number` | No | Connecting line width |
| `style` | `StyleProp` | No | Additional styles |

### StepItem Object

```typescript
interface StepItem {
  id: string;           // Required: unique identifier
  title: string;        // Required: step title
  subtitle?: string;    // Optional: step subtitle
  status: StepStatus;   // Required: step status
  icon?: string;        // Optional: custom icon
}

type StepStatus = 'pending' | 'active' | 'completed' | 'error';
```

### Example

```tsx
import { StepList } from 'askit';

export function App() {
  return (
    <StepList
      items={[
        { id: '1', title: 'Connect Wallet', status: 'completed' },
        { id: '2', title: 'Verify Identity', status: 'active' },
        { id: '3', title: 'Complete Setup', status: 'pending' },
      ]}
    />
  );
}
```

---

## ThemeView

A container view that adapts to the current theme with preset theme variants.

### Props

| Prop | Type | Required | Description |
|------|------|----------|-------------|
| `children` | `ReactNode` | No | Child elements |
| `variant` | `'primary' \| 'secondary' \| 'surface' \| 'background'` | No | Theme variant |
| `padding` | `number \| 'none' \| 'small' \| 'medium' \| 'large'` | No | Padding |
| `style` | `StyleProp` | No | Additional styles |

### Example

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

Display user avatar with fallback support.

### Props

| Prop | Type | Required | Description |
|------|------|----------|-------------|
| `uri` | `string` | No | Image URL |
| `name` | `string` | No | User name (for fallback initials) |
| `size` | `number \| 'small' \| 'medium' \| 'large'` | No | Avatar size |
| `showOnlineStatus` | `boolean` | No | Whether to show online status indicator |
| `isOnline` | `boolean` | No | Online status |
| `onPress` | `() => void` | No | Press callback |
| `style` | `StyleProp` | No | Additional styles |

### Example

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
        onPress={() => console.log('Avatar pressed')}
      />
    </>
  );
}
```

---

## ChatBubble

Chat message bubble component.

### Props

| Prop | Type | Required | Description |
|------|------|----------|-------------|
| `content` | `string` | No | Message text content |
| `children` | `ReactNode` | No | Child elements (custom content) |
| `isOwn` | `boolean` | No | Whether message is from current user |
| `showTail` | `boolean` | No | Whether to show bubble tail |
| `timestamp` | `string \| number \| Date` | No | Message timestamp |
| `status` | `'sending' \| 'sent' \| 'delivered' \| 'read' \| 'error'` | No | Message status |
| `renderMarkdown` | `boolean` | No | Whether to render Markdown |
| `onPress` | `() => void` | No | Press callback |
| `onLongPress` | `() => void` | No | Long press callback |
| `style` | `StyleProp` | No | Additional styles |

### Example

```tsx
import { ChatBubble } from 'askit';

export function App() {
  return (
    <>
      <ChatBubble content="Hello!" isOwn={false} timestamp="10:30 AM" />
      <ChatBubble
        content="Hi there!"
        isOwn
        timestamp={Date.now()}
        status="delivered"
      />
      <ChatBubble
        content="Long press me"
        isOwn
        showTail
        onPress={() => console.log('pressed')}
        onLongPress={() => console.log('long pressed')}
      />
    </>
  );
}
```

---

## Custom Components

You can create custom components in the same way as rill/let: define an element type identifier for Guest, and register a host implementation.

### In Guest (element identifier)

```tsx
// my-component.guest.tsx
import type React from 'react';

export type MyComponentProps = { title: string };

export const MyComponent = 'MyComponent' as unknown as React.ElementType<MyComponentProps>;
```

### In Host (React Native)

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

### Register in Core

```typescript
// In your host app setup
import { MyComponent } from './MyComponent.native';

engine.register({ MyComponent });
```
