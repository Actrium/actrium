# Rill 插件示例

本目录包含 Rill 插件开发示例。

## 示例列表

### simple-plugin

一个完整的 Rill 插件示例，展示:

- **基础组件**: View, Text, TouchableOpacity, ScrollView
- **状态管理**: useState, useEffect
- **宿主通信**:
  - `useHostEvent` - 监听宿主事件
  - `useSendToHost` - 向宿主发送消息
  - `useConfig` - 获取插件配置
- **主题支持**: 明暗主题切换

## 快速开始

```bash
# 进入示例目录
cd examples/simple-plugin

# 安装依赖
npm install

# 构建插件
npm run build

# 开发模式 (不压缩 + sourcemap)
npm run build:dev

# 监听文件变化
npm run watch
```

构建输出: `dist/bundle.js`

## 插件结构

```
simple-plugin/
├── package.json      # 项目配置
├── src/
│   └── index.tsx     # 插件主入口
└── dist/
    └── bundle.js     # 构建输出
```

## 创建新插件

1. 复制 `simple-plugin` 目录
2. 修改 `package.json` 中的名称
3. 编辑 `src/index.tsx` 实现插件逻辑
4. 运行 `npm run build` 构建

## SDK 导入

```tsx
import {
  // 基础组件
  View,
  Text,
  TouchableOpacity,
  ScrollView,
  Image,
  ActivityIndicator,
  Button,

  // Hooks
  useHostEvent,      // 监听宿主事件
  useSendToHost,     // 发送消息到宿主
  useConfig,         // 获取插件配置

  // 类型
  StyleProp,
  LayoutEvent,
  ScrollEvent,
} from 'rill/sdk';
```

## 宿主通信示例

```tsx
// 发送消息到宿主
const sendToHost = useSendToHost();
sendToHost('MY_EVENT', { data: 'value' });

// 监听宿主事件
useHostEvent('REFRESH', () => {
  console.log('Host requested refresh');
});

// 带类型的事件处理
useHostEvent<{ theme: 'light' | 'dark' }>('THEME_CHANGE', (payload) => {
  console.log('Theme:', payload.theme);
});

// 获取配置
const config = useConfig<{ title: string }>();
console.log('Title:', config.title);
```
