# Components

askit provides a set of UI components that work across Host and Guest environments.

## How Components Work

In **Guest** (QuickJS): Components return DSL (Domain Specific Language) objects that describe the UI structure.

In **Host** (React Native): Components render as actual React Native views.

```typescript
// Guest side - generates DSL
const avatar = UserAvatar({ uri: 'https://...', size: 48 });
// Returns: { type: 'UserAvatar', props: { uri: '...', size: 48 } }

// Host side - renders React Native component
<UserAvatar uri="https://..." size={48} />
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
| `style` | `ViewStyle` | No | Additional styles |

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

```typescript
import { StepList } from 'askit';

const items = [
  { id: '1', title: 'Connect Wallet', status: 'completed' },
  { id: '2', title: 'Verify Identity', status: 'active' },
  { id: '3', title: 'Complete Setup', status: 'pending' }
];

StepList({ items });
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
| `style` | `ViewStyle` | No | Additional styles |

### Example

```typescript
import { ThemeView } from 'askit';

ThemeView({
  variant: 'surface',
  padding: 'medium',
  children: [/* ... */]
});

// With numeric padding
ThemeView({
  variant: 'primary',
  padding: 16,
  children: [/* ... */]
});
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
| `style` | `ViewStyle` | No | Additional styles |

### Example

```typescript
import { UserAvatar } from 'askit';

// With image
UserAvatar({ uri: 'https://example.com/avatar.png', size: 48 });

// With preset size
UserAvatar({ uri: 'https://example.com/avatar.png', size: 'large' });

// With fallback initials
UserAvatar({ name: 'John Doe', size: 'medium' });
// Shows "JD" as fallback

// With online status
UserAvatar({
  uri: 'https://example.com/avatar.png',
  showOnlineStatus: true,
  isOnline: true,
  onPress: () => console.log('Avatar pressed')
});
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
| `style` | `ViewStyle` | No | Additional styles |

### Example

```typescript
import { ChatBubble } from 'askit';

// Received message
ChatBubble({
  content: 'Hello!',
  isOwn: false,
  timestamp: '10:30 AM'
});

// Sent message
ChatBubble({
  content: 'Hi there!',
  isOwn: true,
  timestamp: Date.now(),
  status: 'delivered'
});

// With interaction
ChatBubble({
  content: 'Long press me',
  isOwn: true,
  showTail: true,
  onPress: () => console.log('pressed'),
  onLongPress: () => console.log('long pressed')
});
```

---

## Custom Components

You can create custom components that follow the same pattern:

### In Guest (DSL Generator)

```typescript
// my-component.remote.tsx
export function MyComponent(props: MyComponentProps) {
  return {
    type: 'MyComponent',
    props
  };
}
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
