import React, { useCallback, useMemo } from 'react';
import {
  View,
  Text,
  StyleSheet,
  FlatList,
  useWindowDimensions,
} from 'react-native';
import { useSafeAreaInsets } from 'react-native-safe-area-context';
import {
  MessageRepository,
  IMessage,
  Progress,
} from '../../../repository/MessageRepository';
import ChatBubble from './ChatBubble';
import { getHostLayoutTokens } from '../../../utils/layout';
import { getTypographyStyle, spacing } from '../../../theme/tokens';

const TEMP_MOCK_APP_ID = '__temp_supramark_mock_app__';

const TEMP_SUPRAMARK_MOCK_MESSAGES: any[] = [
  {
    message_id: 'temp-mock-weather',
    app_id: TEMP_MOCK_APP_ID,
    session_id: 'temp-mock-session',
    is_preset: 0,
    question_time: 1710000000000,
    question_content: '天气',
    answer_time: 1710000001000,
    answer_content: `:::weather yaml
location: Beijing
units: metric
:::`,
    attachments: '',
    is_edited: 0,
    is_read: 1,
    progress: Progress.completed,
  },
  {
    message_id: 'temp-mock-admonition',
    app_id: TEMP_MOCK_APP_ID,
    session_id: 'temp-mock-session',
    is_preset: 0,
    question_time: 1710000002000,
    question_content: '提示框',
    answer_time: 1710000003000,
    answer_content: `# 提示框示例

::: note 提示
这是一个普通提示框，用于展示一般性说明。
:::

::: warning 警告
请勿在生产环境中直接使用测试密钥。
:::`,
    attachments: '',
    is_edited: 0,
    is_read: 1,
    progress: Progress.completed,
  },
  {
    message_id: 'temp-mock-gfm',
    app_id: TEMP_MOCK_APP_ID,
    session_id: 'temp-mock-session',
    is_preset: 0,
    question_time: 1710000004000,
    question_content: 'GFM',
    answer_time: 1710000005000,
    answer_content: `# GFM 功能示例

## 删除线

使用 \`~~文本~~\` 语法来创建~~删除线~~效果。

例如：这是一段~~错误的~~正确的文本。

## 任务列表

使用 \`- [ ]\` 和 \`- [x]\` 来创建任务列表：

- [x] 已完成的任务
- [ ] 未完成的任务
- [x] 另一个已完成的任务
- [ ] 待办事项

## 组合使用

你可以将删除线与其他格式组合使用：

- **粗体**和~~删除线~~
- *斜体*和~~删除线~~
- \`代码\`和~~删除线~~

~~**整段粗体删除线**~~

## 表格

使用 GFM 表格语法创建表格，支持列对齐：

| 功能 | 状态 | 说明 |
| --- | :---: | ---: |
| 删除线 | ✅ | 使用 \`~~\` 语法 |
| 任务列表 | ✅ | 使用 \`[ ]\` 和 \`[x]\` |
| 表格 | ✅ | 标准 GFM 表格 |
| 对齐方式 | ✅ | 左对齐、居中、右对齐 |`,
    attachments: '',
    is_edited: 0,
    is_read: 1,
    progress: Progress.completed,
  },
  {
    message_id: 'temp-mock-emoji',
    app_id: TEMP_MOCK_APP_ID,
    session_id: 'temp-mock-session',
    is_preset: 0,
    question_time: 1710000006000,
    question_content: 'Emoji',
    answer_time: 1710000007000,
    answer_content: `# Emoji 示例

支持 GitHub 风格短代码：

- :smile: :joy: :wink:
- :rocket: :tada: :warning:

也可以直接输入原生 Emoji 😄🚀🎉。`,
    attachments: '',
    is_edited: 0,
    is_read: 1,
    progress: Progress.completed,
  },
{
    message_id: 'temp-mock-dot',
    app_id: TEMP_MOCK_APP_ID,
    session_id: 'temp-mock-session',
    is_preset: 0,
    question_time: 1710000014000,
    question_content: 'DOT / Graphviz diagram',
    answer_time: 1710000015000,
    answer_content: `# DOT / Graphviz diagram 示例

\`\`\`dot
digraph G {
  A -> B;
  B -> C;
}
\`\`\``,
    attachments: '',
    is_edited: 0,
    is_read: 1,
    progress: Progress.completed,
  },
{
    message_id: 'temp-mock-mermaid',
    app_id: TEMP_MOCK_APP_ID,
    session_id: 'temp-mock-session',
    is_preset: 0,
    question_time: 1710000022000,
    question_content: 'Mermaid diagram 示例',
    answer_time: 1710000023000,
    answer_content: `# Mermaid diagram 示例

\`\`\`mermaid
graph TD
  A[开始] --> B{条件判断}
  B -->|是| C[执行操作]
  B -->|否| D[跳过]
  C --> E[结束]
  D --> E
\`\`\``,
    attachments: '',
    is_edited: 0,
    is_read: 1,
    progress: Progress.completed,
  },
  {
    message_id: 'temp-mock-math',
    app_id: TEMP_MOCK_APP_ID,
    session_id: 'temp-mock-session',
    is_preset: 0,
    question_time: 1710000008000,
    question_content: '数学公式',
    answer_time: 1710000009000,
    answer_content: `# 数学公式示例
# 数学公式测试

## 行内公式

supramark 会识别行内公式 $E = mc^2$，并生成 \`math_inline\` 节点。

综合行内示例：
$E=mc^2,\\quad \\sum_{i=1}^{n} i=\\frac{n(n+1)}{2},\\quad \\sqrt{a^2+b^2}$

## 块级公式

下面是一个块级公式（\`math_block\`）：

### 分式与根号

$$
\\frac{1}{\\sqrt{2\\pi\\sigma^2}} e^{-\\frac{(x-\\mu)^2}{2\\sigma^2}}
$$

### 积分、矩阵、希腊字母

$$
\\int_0^\\infty e^{-x^2}\\,dx = \\frac{\\sqrt{\\pi}}{2},\\quad
A=\\begin{bmatrix}
1 & 2 \\\\
3 & 4
\\end{bmatrix}
$$

### 极限与可伸缩箭头

$$
\\left( \\frac{\\alpha+\\beta}{\\gamma} \\right)^{n} \\xrightarrow[n\\to\\infty]{} 0,\\quad
\\lim_{x\\to 0}\\frac{\\sin x}{x}=1
$$

### 分段函数

$$
f(x)=
\\begin{cases}
x^2, & x \\ge 0 \\\\
-x, & x < 0
\\end{cases}
$$

### 多行对齐

$$
\\begin{aligned}
(a+b)^2 &= a^2+2ab+b^2 \\\\
\\prod_{i=1}^{n} x_i &= x_1x_2\\cdots x_n
\\end{aligned}
$$

### 字体与装饰

$$
\\hat{x},\\ \\bar{y},\\ \\overline{AB},\\ \\mathbb{R},\\ \\mathcal{F},\\ \\mathrm{d}x
$$

### 范数与绝对值

$$
\\left\\lVert A \\right\\rVert =
\\sqrt{\\sum_{i=1}^{n} \\lvert x_i \\rvert^2}
$$
`,
    attachments: '',
    is_edited: 0,
    is_read: 1,
    progress: Progress.completed,
  },
//   {
//     message_id: 'temp-mock-footnote',
//     app_id: TEMP_MOCK_APP_ID,
//     session_id: 'temp-mock-session',
//     is_preset: 0,
//     question_time: 1710000010000,
//     question_content: '脚注',
//     answer_time: 1710000011000,
//     answer_content: `# 脚注示例

// 这是一段包含脚注的文本[^1]。你可以在同一段落中添加多个脚注[^2]。

// 脚注可以让你添加补充说明而不打断正文流程[^note]。

// [^1]: 这是第一个脚注的内容。

// [^2]: 这是第二个脚注，可以包含更详细的解释。

// [^note]: 脚注标识符可以是数字或文本。`,
//     attachments: '',
//     is_edited: 0,
//     is_read: 1,
//     progress: Progress.completed,
//   },
//   {
//     message_id: 'temp-mock-deflist',
//     app_id: TEMP_MOCK_APP_ID,
//     session_id: 'temp-mock-session',
//     is_preset: 0,
//     question_time: 1710000012000,
//     question_content: '定义列表',
//     answer_time: 1710000013000,
//     answer_content: `# 定义列表示例

// HTTP
// :   一种应用层协议，用于超文本传输。
// :   目前最常见的 Web 协议。

// HTTPS
// :   在 HTTP 之上加入 TLS 加密的安全协议。`,
//     attachments: '',
//     is_edited: 0,
//     is_read: 1,
//     progress: Progress.completed,
//   },
  
  {
    message_id: 'temp-mock-echarts',
    app_id: TEMP_MOCK_APP_ID,
    session_id: 'temp-mock-session',
    is_preset: 0,
    question_time: 1710000016000,
    question_content: 'ECharts diagram 示例',
    answer_time: 1710000017000,
    answer_content: `# ECharts diagram 示例

\`\`\`echarts
{
  "xAxis": { "type": "category", "data": ["Mon", "Tue", "Wed"] },
  "yAxis": { "type": "value" },
  "series": [
    { "type": "line", "data": [150, 230, 224] }
  ]
}
\`\`\``,
    attachments: '',
    is_edited: 0,
    is_read: 1,
    progress: Progress.completed,
  },
  {
    message_id: 'temp-mock-plantuml',
    app_id: TEMP_MOCK_APP_ID,
    session_id: 'temp-mock-session',
    is_preset: 0,
    question_time: 1710000018000,
    question_content: 'PlantUML diagram 示例',
    answer_time: 1710000019000,
    answer_content: `# PlantUML diagram 示例

\`\`\`plantuml
@startuml
Alice -> Bob: Hello
Bob --> Alice: World
@enduml
\`\`\``,
    attachments: '',
    is_edited: 0,
    is_read: 1,
    progress: Progress.completed,
  },
  {
    message_id: 'temp-mock-vega-lite',
    app_id: TEMP_MOCK_APP_ID,
    session_id: 'temp-mock-session',
    is_preset: 0,
    question_time: 1710000020000,
    question_content: 'Vega-Lite diagram 示例',
    answer_time: 1710000021000,
    answer_content: `# Vega-Lite diagram 示例

下面的围栏代码块会被 supramark 识别为 \`diagram\` 节点（engine = "vega-lite"）：

\`\`\`vega-lite
{
  "mark": "bar",
  "encoding": {
    "x": { "field": "category", "type": "ordinal" },
    "y": { "field": "value", "type": "quantitative" }
  },
  "data": {
    "values": [
      { "category": "A", "value": 1 },
      { "category": "B", "value": 2 }
    ]
  }
}
\`\`\``,
    attachments: '',
    is_edited: 0,
    is_read: 1,
    progress: Progress.completed,
  },
  
];

export default function TemporarySupramarkMockChat() {
  const { width } = useWindowDimensions();
  const insets = useSafeAreaInsets();
  const layoutTokens = useMemo(() => getHostLayoutTokens(width), [width]);

  const renderItem = useCallback(
    ({ item }: { item: IMessage }) => (
      <View
        style={[
          styles.messageListWrapper,
          {
            maxWidth: layoutTokens.contentMaxWidth,
            paddingHorizontal: layoutTokens.messageHorizontalPadding,
          },
        ]}
      >
        <View style={styles.messageGroup}>
          <View style={styles.bubbleWithCheckbox}>
            <View style={styles.questionBubbleWrapper}>
              <ChatBubble
                type='question'
                msgId={item.message_id}
                appId={item.app_id}
                content={item.question_content}
              />
            </View>
          </View>
          <View style={styles.bubbleWithCheckbox}>
            <ChatBubble
              type='answer'
              msgId={item.message_id}
              appId={item.app_id}
              content={item.answer_content}
            />
          </View>
        </View>
      </View>
    ),
    [layoutTokens.contentMaxWidth, layoutTokens.messageHorizontalPadding],
  );

  return (
    <View
      style={[
        styles.mainContainer,
        { backgroundColor: layoutTokens.isTablet ? '#ffffff' : '#F2F2F6' },
      ]}
    >
      <FlatList
        data={TEMP_SUPRAMARK_MOCK_MESSAGES}
        renderItem={renderItem}
        keyExtractor={(item) => item.message_id}
        contentContainerStyle={[
          styles.chatContent,
          {
            paddingTop: 24 + insets.top,
            paddingBottom: 24,
          },
        ]}
        showsVerticalScrollIndicator={true}
      />
    </View>
  );
}

const styles = StyleSheet.create({
  mainContainer: {
    flex: 1,
    backgroundColor: '#ffffff',
    // 移除 paddingBottom，由内部组件处理
  },
  contentArea: {
    flex: 1,
    width: '100%',
  },
  inputContainer: {
    width: '100%',
    backgroundColor: '#fff',
  },
  inputInner: {
    width: '100%', // 移除 maxWidth，让 ActrInput 容器占满全宽
  },
  scrollToBottomContainer: {
    position: 'absolute',
    left: 0,
    right: 0,
    bottom: 90, // 在输入框上方
    alignItems: 'center',
    zIndex: 99,
    pointerEvents: 'box-none',
  },
  chatScrollView: {
    flex: 1,
    width: '100%', // 确保滚动条在窗口最边缘
  },
  chatContent: {
    paddingTop: 24,
    width: '100%',
    alignItems: 'stretch', // 允许子项宽度自动扩展，然后由子项自己的 alignSelf: 'center' 处理居中
  },
  messageListWrapper: {
    width: '100%',
    alignSelf: 'center', // 确保在 FlatList 内部居中
  },
  messageGroup: {
    width: '100%',
    marginBottom: spacing.md,
  },
  messageGroupWithCheckbox: {
    paddingLeft: 30, // 为复选框留出空间
  },
  bubbleWithCheckbox: {
    flexDirection: 'row',
    alignItems: 'flex-start',
    position: 'relative',
    marginBottom: spacing.xxs,
    width: '100%',
  },
  questionBubbleWrapper: {
    flex: 1,
    alignItems: 'flex-end',
    minWidth: 0, // 允许 flex 子元素收缩
  },
  checkboxContainer: {
    position: 'absolute',
    left: -30,
    top: 14, // 与气泡框第一行的 paddingVertical 对齐
    zIndex: 10,
  },
  loadingContainer: {
    flexDirection: 'row',
    alignItems: 'center',
    paddingVertical: spacing.md,
    paddingHorizontal: spacing.xxs,
  },
  loadingText: {
    marginLeft: spacing.sm,
    ...getTypographyStyle('body'),
    color: '#999',
  },
  deleteButtonContainer: {
    position: 'absolute',
    left: 0,
    right: 0,
    alignItems: 'center',
    justifyContent: 'center',
    zIndex: 1001,
    pointerEvents: 'box-none',
  },
});
