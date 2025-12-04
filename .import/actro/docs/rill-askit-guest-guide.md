# Rill + Askit Guest 应用开发指南

## 📚 概述

本文档详细介绍如何基于 **Rill** 渲染引擎和 **Askit** 组件库开发 Guest 应用（.actr 格式的小程序）。

### 什么是 Rill?

Rill 是一个基于沙箱的 React 渲染引擎：

- **Guest（客体）**: 运行在 QuickJS 沙箱中的应用代码，产生 UI 操作指令
- **Host（宿主）**: React Native 环境，接收操作指令并渲染原生组件
- **通信机制**: Guest 通过序列化的 JSON 操作与 Host 通信

### 什么是 Askit?

Askit 是一个同构 UI 组件库：

- **90% 共享代码**: 绝大部分逻辑在 Guest 和 Host 之间共享
- **10% 核心差异**: 渲染层根据平台适配（iOS/Android/macOS/Web）
- **丰富组件**: ThemeView、StepList、ChatBubble、UserAvatar 等开箱即用

---

## 🏗️ 架构原理

### 1. 数据流

```
┌─────────────────────────────────────────────────────────────┐
│                         Guest 沙箱                           │
│  (QuickJS 执行环境)                                          │
│                                                              │
│  用户代码 (TSX/JSX)                                          │
│         ↓                                                    │
│  React 虚拟 DOM                                              │
│         ↓                                                    │
│  Rill 操作序列化                                             │
│   CREATE / UPDATE / DELETE / CALL                            │
└─────────────────────────────────────────────────────────────┘
                         ↓ (JSON 操作)
┌─────────────────────────────────────────────────────────────┐
│                         Host 环境                            │
│  (React Native)                                              │
│                                                              │
│  Rill Engine 接收操作                                        │
│         ↓                                                    │
│  操作解析和应用                                              │
│         ↓                                                    │
│  原生组件渲染                                                │
│   <View> <Text> <TouchableOpacity> ...                      │
└─────────────────────────────────────────────────────────────┘
```

### 2. 组件注册机制

Host 环境需要预先注册可用组件：

```typescript
// Host: GuestAppView.tsx
const hostComponents: ComponentMap = {
  View: View,
  Text: Text,
  TouchableOpacity: TouchableOpacity,
  ScrollView: ScrollView,
  ThemeView: ThemeView,  // Askit 组件
  StepList: StepList,     // Askit 组件
  ChatBubble: ChatBubble, // Askit 组件
};

const engine = new Engine({ debug: true });
engine.register(hostComponents);
```

Guest 代码中使用这些组件时，它们实际上是**字符串标识符**：

```typescript
// Guest 中，这些不是真实的 React 组件，而是 Rill 的虚拟组件
const View = 'View';
const Text = 'Text';
```

### 3. 函数序列化

Guest 中的回调函数会被序列化为 ID：

```typescript
// Guest 代码
<TouchableOpacity onPress={() => console.log('clicked')}>
  <Text>按钮</Text>
</TouchableOpacity>

// 转换为操作
{
  type: 'CREATE',
  component: 'TouchableOpacity',
  props: {
    onPress: '__function_123__'  // 函数被序列化为 ID
  },
  children: [...]
}
```

当 Host 触发 `onPress` 时，会回调 Guest 中对应 ID 的函数。

---

## 📝 Guest 应用结构

### 基础文件结构

```
my-guest-app/
├── manifest.json       # 应用元数据
├── guest.js           # 主应用代码（打包后）
└── assets/            # 可选：静态资源
    ├── icon.png
    └── data.json
```

### manifest.json

```json
{
  "name": "MyApp",
  "version": "1.0.0",
  "description": "应用描述",
  "author": "作者名",
  "icon": "assets/icon.png"
}
```

### guest.js 基础模板

```javascript
// 方式 1: 使用 React.createElement
(function() {
  'use strict';

  const App = () => {
    const [count, setCount] = React.useState(0);

    return React.createElement(
      View,
      { style: { flex: 1, padding: 20 } },
      React.createElement(
        Text,
        { style: { fontSize: 24, color: '#fff' } },
        `Count: ${count}`
      ),
      React.createElement(
        TouchableOpacity,
        {
          onPress: () => setCount(count + 1),
          style: { padding: 10, backgroundColor: '#007AFF' }
        },
        React.createElement(Text, { style: { color: '#fff' } }, 'Increment')
      )
    );
  };

  // 导出
  if (typeof module !== 'undefined' && module.exports) {
    module.exports = App;
  } else if (typeof global !== 'undefined') {
    global.App = App;
  }
})();
```

```javascript
// 方式 2: 如果支持 JSX 转译
export default function App() {
  const [count, setCount] = useState(0);

  return (
    <View style={{ flex: 1, padding: 20 }}>
      <Text style={{ fontSize: 24, color: '#fff' }}>
        Count: {count}
      </Text>
      <TouchableOpacity
        onPress={() => setCount(count + 1)}
        style={{ padding: 10, backgroundColor: '#007AFF' }}
      >
        <Text style={{ color: '#fff' }}>Increment</Text>
      </TouchableOpacity>
    </View>
  );
}
```

---

## 🎨 使用 Askit 组件

### ThemeView - 主题容器

```javascript
<ThemeView
  theme="dark"
  style={{ flex: 1 }}
>
  <Text>自动适配主题色</Text>
</ThemeView>
```

### StepList - 步骤列表

```javascript
<StepList
  steps={[
    { title: '步骤 1', status: 'completed' },
    { title: '步骤 2', status: 'active' },
    { title: '步骤 3', status: 'pending' },
  ]}
  onStepPress={(index) => console.log('Step', index)}
/>
```

### ChatBubble - 聊天气泡

```javascript
<ChatBubble
  message="你好，这是一条消息"
  sender="user"
  timestamp={Date.now()}
  avatar="https://..."
/>
```

### UserAvatar - 用户头像

```javascript
<UserAvatar
  name="张三"
  size={48}
  backgroundColor="#007AFF"
/>
```

---

## 🔄 Guest ↔ Host 通信

### 1. Guest 发送消息到 Host

```javascript
import { useSendToHost } from 'rill/sdk';

function MyComponent() {
  const sendToHost = useSendToHost();

  const handleAction = () => {
    sendToHost('USER_ACTION', {
      type: 'button_click',
      timestamp: Date.now()
    });
  };

  return (
    <TouchableOpacity onPress={handleAction}>
      <Text>执行操作</Text>
    </TouchableOpacity>
  );
}
```

### 2. Host 发送消息到 Guest

```javascript
import { useHostEvent } from 'rill/sdk';

function MyComponent() {
  useHostEvent('DATA_UPDATED', (payload) => {
    console.log('收到 Host 数据:', payload);
    // 更新 UI
  });

  return <View>...</View>;
}
```

### 3. 使用 Bus（事件总线）

```javascript
import { Bus } from 'askit';

// 发送事件
Bus.emit('REFRESH', { force: true });

// 监听事件
Bus.on('REFRESH', (data) => {
  console.log('刷新事件:', data);
});

// 移除监听
Bus.off('REFRESH', handler);
```

---

## 🎯 实战示例：Todo 应用

```javascript
(function() {
  'use strict';

  const TodoApp = () => {
    const [todos, setTodos] = React.useState([
      { id: 1, text: '学习 Rill', completed: false },
      { id: 2, text: '学习 Askit', completed: false },
    ]);
    const [inputText, setInputText] = React.useState('');

    const addTodo = () => {
      if (inputText.trim()) {
        setTodos([
          ...todos,
          { id: Date.now(), text: inputText, completed: false }
        ]);
        setInputText('');

        // 通知 Host
        if (typeof Toast !== 'undefined') {
          Toast.show('已添加 Todo');
        }
      }
    };

    const toggleTodo = (id) => {
      setTodos(todos.map(todo =>
        todo.id === id ? { ...todo, completed: !todo.completed } : todo
      ));
    };

    return React.createElement(
      View,
      { style: { flex: 1, backgroundColor: '#1a1a1a', padding: 20 } },

      // 标题
      React.createElement(
        Text,
        { style: { fontSize: 32, fontWeight: 'bold', color: '#fff', marginBottom: 20 } },
        'My Todos'
      ),

      // 输入框区域
      React.createElement(
        View,
        { style: { flexDirection: 'row', marginBottom: 20 } },
        React.createElement(
          TextInput,
          {
            value: inputText,
            onChangeText: setInputText,
            placeholder: '输入新任务...',
            placeholderTextColor: '#666',
            style: {
              flex: 1,
              padding: 12,
              backgroundColor: '#2a2a2a',
              color: '#fff',
              borderRadius: 8,
              marginRight: 10
            }
          }
        ),
        React.createElement(
          TouchableOpacity,
          {
            onPress: addTodo,
            style: {
              padding: 12,
              backgroundColor: '#007AFF',
              borderRadius: 8,
              justifyContent: 'center'
            }
          },
          React.createElement(
            Text,
            { style: { color: '#fff', fontWeight: 'bold' } },
            '添加'
          )
        )
      ),

      // Todo 列表
      React.createElement(
        ScrollView,
        { style: { flex: 1 } },
        todos.map(todo =>
          React.createElement(
            TouchableOpacity,
            {
              key: todo.id,
              onPress: () => toggleTodo(todo.id),
              style: {
                flexDirection: 'row',
                alignItems: 'center',
                padding: 16,
                backgroundColor: '#2a2a2a',
                borderRadius: 8,
                marginBottom: 10
              }
            },
            React.createElement(
              View,
              {
                style: {
                  width: 24,
                  height: 24,
                  borderRadius: 12,
                  borderWidth: 2,
                  borderColor: todo.completed ? '#27ae60' : '#666',
                  backgroundColor: todo.completed ? '#27ae60' : 'transparent',
                  marginRight: 12,
                  alignItems: 'center',
                  justifyContent: 'center'
                }
              },
              todo.completed &&
                React.createElement(
                  Text,
                  { style: { color: '#fff', fontSize: 16 } },
                  '✓'
                )
            ),
            React.createElement(
              Text,
              {
                style: {
                  flex: 1,
                  color: todo.completed ? '#666' : '#fff',
                  fontSize: 16,
                  textDecorationLine: todo.completed ? 'line-through' : 'none'
                }
              },
              todo.text
            )
          )
        )
      )
    );
  };

  // 导出
  if (typeof module !== 'undefined' && module.exports) {
    module.exports = TodoApp;
  } else if (typeof global !== 'undefined') {
    global.App = TodoApp;
  }
})();
```

---

## 📦 打包流程

### 方式 1: 使用 Rill CLI（推荐）

#### 安装 Rill CLI

```bash
# 在 Rill 项目目录中
cd /Users/example-dev-2/Projects/example-workspace-2/GoAskAway/rill

# 构建 CLI 工具
npm run build

# 全局链接（可选）
npm link

# 或使用 npx 直接调用
npx rill --help
```

#### 项目结构

```bash
my-rill-app/
├── src/
│   ├── App.tsx          # 主应用组件（支持 TSX/JSX）
│   ├── components/
│   │   ├── TodoItem.tsx
│   │   └── TodoList.tsx
│   └── utils/
│       └── helpers.ts
├── manifest.json        # 应用元数据
├── package.json
└── tsconfig.json
```

#### 构建命令

```bash
# 基础构建
rill build src/App.tsx

# 指定输出路径
rill build src/App.tsx -o dist/bundle.js

# 开发模式（监听文件变化）
rill build src/App.tsx --watch

# 生成 sourcemap（便于调试）
rill build src/App.tsx --sourcemap

# 禁用压缩（便于调试）
rill build src/App.tsx --no-minify

# 输出构建元数据
rill build src/App.tsx --metafile meta.json
```

#### CLI 构建特性

1. **自动打包**: 使用 esbuild 自动打包所有依赖
2. **JSX 转换**: 自动转换 TSX/JSX 为 JavaScript
3. **代码压缩**: 默认启用代码压缩（production mode）
4. **Runtime 注入**: 自动注入 Rill Runtime 代码
   - 回调函数注册/调用机制
   - Host 事件监听机制
   - 配置管理
5. **IIFE 格式**: 输出立即执行函数表达式（沙箱友好）
6. **Watch 模式**: 开发时自动重新构建

#### 构建产物

```javascript
// dist/bundle.js
/* Rill Plugin Bundle - Generated by rill-cli */

// Rill Runtime Inject
(function() {
  // 回调注册机制
  globalThis.__registerCallback = function(fn) { ... };
  globalThis.__invokeCallback = function(fnId, args) { ... };

  // Host 事件机制
  globalThis.__useHostEvent = function(eventName, callback) { ... };
  globalThis.__handleHostEvent = function(eventName, payload) { ... };
})();

// 你的应用代码（已打包）
var __RillPlugin = (function() {
  'use strict';
  // ... 你的应用逻辑
  return App;
})();

/* End of Rill Plugin Bundle */
```

### 方式 2: 手动打包

如果不使用 Rill CLI，可以手动转译：

```bash
# 使用 TypeScript 编译器
tsc src/App.tsx --outDir dist --jsx react

# 或使用 Babel
babel src --out-dir dist --presets @babel/preset-react
```

### 打包为 .actr

```bash
# 使用提供的打包脚本
cd /Users/example-dev-2/Projects/example-workspace/GoAskAway/Canvas/TestData
./pack-actr.sh my-app

# 输出: my-app.actr (实际上是 zip 文件)
```

#### pack-actr.sh 做了什么？

```bash
#!/bin/bash
# 1. 创建临时目录
# 2. 复制 manifest.json 和 guest.js (或 dist/bundle.js)
# 3. 复制 assets/ 目录（如果有）
# 4. 压缩为 .zip 文件
# 5. 重命名为 .actr
```

### 加载到 Canvas

将 `.actr` 文件拖入 Canvas 窗口，系统会：

1. 解压 .actr 文件
2. 读取 manifest.json
3. 加载 guest.js
4. 传递给 Rill Engine 执行

---

## ⚠️ 注意事项和限制

### 1. 沙箱限制

- ❌ 不能访问 Node.js API (fs, path, etc.)
- ❌ 不能访问浏览器 API (window, document, etc.)
- ❌ 不能直接访问原生模块
- ✅ 可以使用 React Hooks
- ✅ 可以使用 JavaScript 标准库
- ✅ 可以通过 Host 通信访问受限功能

### 2. ES6 模块问题

**问题**: QuickJS 沙箱不直接支持 ES6 `import/export` 语法

**解决方案**: 使用 Rill CLI 自动处理

- ✅ Rill CLI 使用 esbuild 打包，自动转换 ESM 为 IIFE 格式
- ✅ 支持在源码中使用 `import/export` 语法
- ✅ 自动打包所有依赖到单个 bundle 文件
- ✅ JSX/TSX 自动转换为 JavaScript

```bash
# 源码可以使用 ES6 模块
// src/App.tsx
import React from 'react';
import { TodoList } from './components/TodoList';

export default function App() {
  return <TodoList />;
}

# Rill CLI 构建后自动转换为 IIFE 格式
rill build src/App.tsx -o dist/guest.js
```

### 3. 组件可用性

Guest 代码只能使用 Host 预先注册的组件：

```typescript
// 这些组件必须在 Host 中注册
<View>         ✅ 已注册
<Text>         ✅ 已注册
<CustomView>   ❌ 未注册，无法使用
```

### 4. 样式系统

使用 React Native 的 StyleSheet 格式：

```javascript
// ✅ 内联样式
<View style={{ flex: 1, padding: 20 }}>

// ✅ 样式对象
const styles = {
  container: { flex: 1, padding: 20 },
  text: { fontSize: 16, color: '#fff' }
};

// ❌ 不支持 CSS 类名
<View className="container">  // 无效
```

### 5. 性能考虑

- Guest 和 Host 之间的通信有序列化开销
- 避免频繁的跨边界通信
- 大量数据更新时考虑批处理
- 使用 `React.memo` 优化组件渲染

---

## 🔍 调试技巧

### 1. 启用 Debug 模式

```typescript
// Host: GuestAppView.tsx
const engine = new Engine({ debug: true });
```

### 2. Guest 日志

```javascript
// Guest 代码中
console.log('Guest: 当前状态', state);
console.error('Guest: 错误信息', error);
```

日志会显示在 Host 的控制台中。

### 3. Host 监听 Guest 事件

```typescript
<EngineView
  engine={engine}
  bundleUrl={script}
  onLoad={() => console.log('✅ Guest 加载成功')}
  onError={(error) => console.error('❌ Guest 错误:', error)}
/>
```

### 4. 诊断模式

在开发阶段，可以添加诊断 UI：

```javascript
const [debugMode, setDebugMode] = React.useState(true);

if (debugMode) {
  return (
    <View style={{ padding: 20, backgroundColor: '#ff0000' }}>
      <Text style={{ color: '#fff', fontSize: 18 }}>
        🔍 调试模式
      </Text>
      <Text style={{ color: '#fff' }}>
        State: {JSON.stringify(state, null, 2)}
      </Text>
    </View>
  );
}
```

---

## 🔧 完整开发工作流

### 步骤 1: 初始化项目

```bash
# 创建项目目录
mkdir my-issue-tracker
cd my-issue-tracker

# 初始化 package.json
npm init -y

# 安装依赖
npm install react --save-dev
npm install typescript @types/react --save-dev
```

### 步骤 2: 配置 TypeScript

创建 `tsconfig.json`:

```json
{
  "compilerOptions": {
    "target": "ES2020",
    "module": "ESNext",
    "jsx": "react-jsx",
    "moduleResolution": "node",
    "strict": true,
    "esModuleInterop": true,
    "skipLibCheck": true,
    "forceConsistentCasingInFileNames": true,
    "resolveJsonModule": true
  },
  "include": ["src/**/*"],
  "exclude": ["node_modules", "dist"]
}
```

### 步骤 3: 编写应用代码

创建 `src/App.tsx`:

```typescript
import React, { useState } from 'react';
import { View, Text, TouchableOpacity, ScrollView } from 'react-native';

export default function IssueTrackerApp() {
  const [issues, setIssues] = useState([
    { id: 1, title: '修复登录问题', status: 'open' },
    { id: 2, title: '优化性能', status: 'in-progress' },
  ]);

  const addIssue = () => {
    const newIssue = {
      id: Date.now(),
      title: `新 Issue #${issues.length + 1}`,
      status: 'open',
    };
    setIssues([...issues, newIssue]);
  };

  return (
    <View style={{ flex: 1, padding: 20, backgroundColor: '#1a1a1a' }}>
      <Text style={{ fontSize: 32, color: '#fff', marginBottom: 20 }}>
        Issue Tracker
      </Text>

      <TouchableOpacity
        onPress={addIssue}
        style={{
          padding: 15,
          backgroundColor: '#007AFF',
          borderRadius: 8,
          marginBottom: 20,
        }}
      >
        <Text style={{ color: '#fff', fontSize: 16, textAlign: 'center' }}>
          + 创建 Issue
        </Text>
      </TouchableOpacity>

      <ScrollView>
        {issues.map((issue) => (
          <View
            key={issue.id}
            style={{
              padding: 15,
              backgroundColor: '#2a2a2a',
              borderRadius: 8,
              marginBottom: 10,
            }}
          >
            <Text style={{ fontSize: 18, color: '#fff', marginBottom: 5 }}>
              {issue.title}
            </Text>
            <Text style={{ fontSize: 14, color: '#888' }}>
              状态: {issue.status}
            </Text>
          </View>
        ))}
      </ScrollView>
    </View>
  );
}
```

### 步骤 4: 创建 manifest.json

```json
{
  "name": "IssueTracker",
  "version": "1.0.0",
  "description": "简单的 Issue 跟踪应用",
  "author": "Your Name"
}
```

### 步骤 5: 使用 Rill CLI 构建

```bash
# 确保 Rill CLI 已构建
cd /Users/example-dev-2/Projects/example-workspace-2/GoAskAway/rill
npm run build

# 返回项目目录
cd /path/to/my-issue-tracker

# 构建应用（开发模式）
/Users/example-dev-2/Projects/example-workspace-2/GoAskAway/rill/dist/cli/index.js build src/App.tsx -o dist/guest.js --no-minify --sourcemap

# 或在 watch 模式下开发
/Users/example-dev-2/Projects/example-workspace-2/GoAskAway/rill/dist/cli/index.js build src/App.tsx -o dist/guest.js --watch --no-minify --sourcemap
```

### 步骤 6: 打包为 .actr

```bash
# 复制文件到 TestData 目录
mkdir -p /Users/example-dev-2/Projects/example-workspace/GoAskAway/Canvas/TestData/my-issue-tracker
cp dist/guest.js /Users/example-dev-2/Projects/example-workspace/GoAskAway/Canvas/TestData/my-issue-tracker/
cp manifest.json /Users/example-dev-2/Projects/example-workspace/GoAskAway/Canvas/TestData/my-issue-tracker/

# 打包
cd /Users/example-dev-2/Projects/example-workspace/GoAskAway/Canvas/TestData
./pack-actr.sh my-issue-tracker

# 生成: my-issue-tracker.actr
```

### 步骤 7: 加载到 Canvas

```bash
# 将 .actr 文件拖入 Canvas 应用窗口
# 或通过命令行（如果 Canvas 支持）
open -a Canvas my-issue-tracker.actr
```

### 步骤 8: 调试

如果遇到问题，查看日志：

```bash
# Xcode Console 会显示：
# ✅ 传递 guestScript 到 RN，长度: XXXX
# 🔍 GuestAppView 渲染，script 长度: XXXX
# 🔧 创建 Rill Engine 实例
# ✅ EngineView 加载成功

# 如果看到错误：
# ❌ EngineView 加载失败: [error message]
```

### 自动化脚本

创建 `scripts/build-and-pack.sh`:

```bash
#!/bin/bash
set -e

PROJECT_NAME="my-issue-tracker"
RILL_CLI="/Users/example-dev-2/Projects/example-workspace-2/GoAskAway/rill/dist/cli/index.js"
TEST_DATA_DIR="/Users/example-dev-2/Projects/example-workspace/GoAskAway/Canvas/TestData"

echo "📦 Building Guest application..."
$RILL_CLI build src/App.tsx -o dist/guest.js

echo "📋 Copying files..."
mkdir -p "$TEST_DATA_DIR/$PROJECT_NAME"
cp dist/guest.js "$TEST_DATA_DIR/$PROJECT_NAME/"
cp manifest.json "$TEST_DATA_DIR/$PROJECT_NAME/"

echo "🗜️ Packing .actr..."
cd "$TEST_DATA_DIR"
./pack-actr.sh "$PROJECT_NAME"

echo "✅ Done! Generated: $PROJECT_NAME.actr"
```

使用：

```bash
chmod +x scripts/build-and-pack.sh
./scripts/build-and-pack.sh
```

---

## 🚀 最佳实践

### 1. 状态管理

```javascript
// ✅ 使用 React Hooks
const [data, setData] = useState(initialData);
const [loading, setLoading] = useState(false);

// ✅ 复杂状态使用 useReducer
const [state, dispatch] = useReducer(reducer, initialState);

// ❌ 避免使用外部状态库（Redux, MobX）
// 因为它们可能依赖 Node.js 模块
```

### 2. 组件拆分

```javascript
// ✅ 拆分为小组件
const TodoItem = ({ todo, onToggle }) => (
  React.createElement(
    TouchableOpacity,
    { onPress: onToggle },
    React.createElement(Text, null, todo.text)
  )
);

const TodoList = ({ todos }) => (
  React.createElement(
    ScrollView,
    null,
    todos.map(todo =>
      React.createElement(TodoItem, { key: todo.id, todo, onToggle: () => {} })
    )
  )
);
```

### 3. 错误处理

```javascript
try {
  // 可能出错的代码
  const result = riskyOperation();
} catch (error) {
  console.error('操作失败:', error);
  if (typeof Toast !== 'undefined') {
    Toast.show('操作失败，请重试');
  }
}
```

### 4. 使用 Askit 工具

```javascript
import { Toast, Haptic, Bus } from 'askit';

// 用户反馈
Toast.show('操作成功');
Haptic.trigger('light');  // 触觉反馈

// 事件通信
Bus.emit('DATA_CHANGED', { id: 123 });
```

---

## 📚 进阶主题

### 1. 动画支持

```javascript
// 如果 Animated 已注册
const [fadeAnim] = useState(new Animated.Value(0));

useEffect(() => {
  Animated.timing(fadeAnim, {
    toValue: 1,
    duration: 500,
  }).start();
}, []);

return (
  <Animated.View style={{ opacity: fadeAnim }}>
    <Text>渐显内容</Text>
  </Animated.View>
);
```

### 2. 异步数据加载

```javascript
const [data, setData] = useState(null);
const [loading, setLoading] = useState(true);

useEffect(() => {
  // 通过 Host 获取数据
  const sendToHost = useSendToHost();
  sendToHost('FETCH_DATA', { endpoint: '/api/todos' });
}, []);

useHostEvent('DATA_RESPONSE', (payload) => {
  setData(payload.data);
  setLoading(false);
});
```

### 3. 持久化存储

```javascript
// Guest 请求保存数据
const saveData = (key, value) => {
  const sendToHost = useSendToHost();
  sendToHost('STORAGE_SET', { key, value });
};

// Guest 请求读取数据
const loadData = (key) => {
  const sendToHost = useSendToHost();
  sendToHost('STORAGE_GET', { key });
};

useHostEvent('STORAGE_RESPONSE', ({ key, value }) => {
  console.log(`读取 ${key}:`, value);
});
```

---

## 📖 参考资源

### 项目路径

- **Rill 引擎**: `/Users/example-dev-2/Projects/example-workspace-2/GoAskAway/rill`
- **Askit 组件**: `/Users/example-dev-2/Projects/example-workspace-2/GoAskAway/askit`
- **Canvas 宿主**: `/Users/example-dev-2/Projects/example-workspace/GoAskAway/Canvas`

### 示例应用

- **HelloWorld**: `Canvas/TestData/helloworld/`
- **Issue Manager**: `Canvas/TestData/issue-manager/`

### 核心文件

- **Rill Engine**: `rill/src/runtime/Engine.ts`
- **Rill EngineView**: `rill/src/runtime/EngineView.native.tsx`
- **GuestAppView**: `Canvas/actr-host/src/GuestAppView.tsx`
- **打包脚本**: `Canvas/TestData/pack-actr.sh`

---

## 🎓 总结

Rill + Askit 体系提供了一个安全、高效的跨平台 Guest 应用开发方案：

1. **沙箱隔离**: Guest 代码运行在受控环境，保证安全性
2. **跨平台**: 一份代码，多端运行（iOS/Android/macOS/Web）
3. **丰富组件**: Askit 提供开箱即用的 UI 组件
4. **灵活通信**: Guest ↔ Host 双向通信机制
5. **简单打包**: .actr 格式便于分发和安装

按照本文档的指导，你可以快速开发出功能完整、体验流畅的 Guest 应用！

---

**文档版本**: 1.0
**最后更新**: 2025-01-27
**作者**: Canvas 开发团队
