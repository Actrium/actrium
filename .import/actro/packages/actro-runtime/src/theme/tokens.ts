import { TextStyle } from 'react-native';

/**
 * token 生效平台。
 * 这里把“视觉 token 用哪套值”从运行平台里拆出来，避免为了临时统一视觉而到处改三元表达式。
 * 当前先让 Android 和 iOS 都走 iOS 体系，后续如果要恢复 Android，只需要改这里。
 */
type TokenPlatform = 'ios' | 'android';

const tokenPlatform: TokenPlatform = 'ios';

/**
 * 按 token 平台挑选值。
 * 调用方传入同一语义在 iOS / Android 下各自的标定值，统一由这里决定最终生效哪一个。
 */
function selectTokenValue<T>(values: { ios: T; android: T }): T {
  return values[tokenPlatform];
}

/**
 * 间距 token。
 * 只表达“节奏层级”，不表达具体业务含义。
 * 页面里优先写 spacing.md / spacing.lg，不直接写 12 / 16 / 20。
 */
export const spacing = {
  xxxs: 2,
  xxs: selectTokenValue({ ios: 4, android: 2 }),
  xs: selectTokenValue({ ios: 6, android: 4 }),
  sm: selectTokenValue({ ios: 8, android: 6 }),
  md: selectTokenValue({ ios: 12, android: 10 }),
  lg: selectTokenValue({ ios: 16, android: 14 }),
  xl: selectTokenValue({ ios: 20, android: 18 }),
  xxl: selectTokenValue({ ios: 24, android: 22 }),
  xxxl: selectTokenValue({ ios: 32, android: 30 }),
} as const;

/**
 * 圆角 token。
 * 常见组件的圆角尽量只落在这几个档位内，避免页面里出现 11 / 13 / 18 这种随机值。
 */
export const radius = {
  xs: 4,
  sm: 8,
  md: 12,
  lg: 16,
  xl: 20,
  pill: 999,
} as const;

/**
 * 边框宽度 token。
 * 大多数场景只需要 0.5 / 1 / 2 三档，不建议业务侧继续发散。
 */
export const borderWidth = {
  hairline: 0.5,
  sm: 1,
  md: 2,
} as const;

/**
 * 阴影 token。
 * iOS / Android 的阴影机制不同，这里只先定义语义层级。
 * 真正落地时可以再拆成 shadowColor / shadowOpacity / elevation 的映射。
 */
export const elevation = {
  none: 0,
  sm: 2,
  md: 4,
  lg: 8,
} as const;

/**
 * 图标尺寸 token。
 * 图标和文字一样，不建议在页面里直接写 15 / 17 / 19。
 */
export const iconSize = {
  xs: 12,
  sm: 16,
  md: 20,
  lg: 24,
  xl: 32,
} as const;

/**
 * 宿主按钮内图标的默认尺寸。
 * Android 图标视觉上更“满”，这里默认比 iOS 收一档。
 */
export const hostIconGlyphSize = {
  sm: selectTokenValue({ ios: 16, android: 12 }),
  md: selectTokenValue({ ios: 20, android: 16 }),
  lg: selectTokenValue({ ios: 24, android: 20 }),
} as const;

/**
 * 控件高度 token。
 * 输入框、按钮、工具栏这类交互控件，优先走统一高度体系。
 */
export const controlHeight = {
  xs: selectTokenValue({ ios: 28, android: 26 }),
  sm: selectTokenValue({ ios: 32, android: 30 }),
  md: selectTokenValue({ ios: 36, android: 34 }),
  lg: selectTokenValue({ ios: 44, android: 42 }),
  xl: selectTokenValue({ ios: 52, android: 50 }),
} as const;

/**
 * 宿主圆形图标按钮的默认尺寸。
 * Android 保持更紧凑，避免同一数值下看起来比 iOS 大一号。
 */
export const hostIconButtonSize = {
  md: selectTokenValue({ ios: 44, android: 39 }),
  lg: selectTokenValue({ ios: 48, android: 43 }),
} as const;

/**
 * 输入区域专用尺寸 token。
 * 统一控制输入框内部的图标、按钮等交互元素尺寸，避免散落硬编码。
 */
export const inputArea = {
  // 输入框容器最小高度
  minHeight: selectTokenValue({ ios: 48, android: 41 }),
  // 输入框内小型图标按钮容器尺寸（加号、麦克风、清除等）
  smallButtonSize: selectTokenValue({ ios: 32, android: 28 }),
  // 输入框内小型图标尺寸
  smallIconSize: selectTokenValue({ ios: 22, android: 20 }),
  // 输入框内中型图标尺寸（链接、语音、键盘等）
  mediumIconSize: selectTokenValue({ ios: 22, android: 20 }),
  // 输入框内大型图标尺寸（语音按钮等）
  largeIconSize: selectTokenValue({ ios: 24, android: 22 }),
  // 输入框外侧圆形按钮尺寸（退出、发送等）
  sideButtonSize: selectTokenValue({ ios: 48, android: 41 }),
  // 输入框外侧圆形按钮内图标尺寸
  sideButtonIconSize: selectTokenValue({ ios: 20, android: 18 }),
  // 取消编辑按钮内图标尺寸
  cancelIconSize: selectTokenValue({ ios: 16, android: 14 }),
  // 输入框圆角
  borderRadius: selectTokenValue({ ios: 24, android: 20 }),
} as const;

/**
 * 多选模式底部操作栏尺寸 token。
 * Phone 端会在运行时取 max(safeAreaInsets.bottom, phoneMinBottomPadding)。
 */
export const selectionActionBar = {
  height: 118,
  phoneMinBottomPadding: 22,
} as const;

/**
 * 附件预览条专用尺寸 token。
 * 这类尺寸只服务于附件缩略图场景，避免把局部交互尺寸继续写死在组件里。
 */
export const attachmentPreview = {
  // 附件卡片右上角删除按钮尺寸
  removeButtonSize: selectTokenValue({ ios: 24, android: 22 }),
} as const;

/**
 * 长按菜单专用尺寸 token。
 * 这类浮层尺寸带有明确业务语义，统一收口后便于按平台单独校准。
 */
export const longPressMenu = {
  // 长按菜单容器宽度
  width: selectTokenValue({ ios: 228, android: 198 }),
  // 长按菜单容器内边距
  padding: selectTokenValue({ ios: 10, android: 8 }),
} as const;

/**
 * 通用下拉菜单尺寸 token。
 * DropdownMenu 是基础浮层组件，默认宽度和容器内边距统一收口，避免各处继续散落硬编码。
 */
export const dropdownMenu = {
  // 下拉菜单默认预估宽度
  defaultEstimatedWidth: selectTokenValue({ ios: 228, android: 208 }),
  // 下拉菜单容器内边距
  padding: selectTokenValue({ ios: 10, android: 8 }),
} as const;

/**
 * 聊天气泡底部工具栏专用尺寸 token。
 * 这一组尺寸只服务于最新回答下方的快捷操作按钮，避免组件内部重复写死。
 */
export const chatBubbleToolbar = {
  // 工具栏圆形按钮容器尺寸
  buttonSize: selectTokenValue({ ios: 34, android: 32 }),
  // 工具栏圆形按钮图标尺寸
  iconSize: selectTokenValue({ ios: 16, android: 14 }),
} as const;

/**
 * LoadApp 页面专用尺寸 token。
 * 扫码页底部的输入切换按钮和 URL 输入框属于同一交互域，尺寸统一收口在这里。
 */
export const loadApp = {
  // 切换到输入框模式的圆形按钮尺寸
  inputToggleButtonSize: selectTokenValue({ ios: 48, android: 41 }),
  // 切换按钮内键盘图标尺寸
  inputToggleIconSize: selectTokenValue({ ios: 24, android: 21 }),
  // URL 输入框高度
  inputHeight: selectTokenValue({ ios: 48, android: 42 }),
  // URL 输入框内文字与 placeholder 使用的排版档位
  inputTypographyVariant: 'urlInput',
  // URL 输入框左侧键盘图标尺寸
  inputPrefixIconSize: selectTokenValue({ ios: 24, android: 22 }),
} as const;

/**
 * 扩展面板专用尺寸 token。
 * Phone 侧滑面板需要按平台单独校准展开比例，避免手势路径和渲染宽度各自写死。
 */
export const extensionPanel = {
  // Phone 模式下侧滑面板相对于基准宽度的展开比例
  phoneWidthRatio: selectTokenValue({ ios: 0.8, android: 0.66 }),
} as const;

/**
 * 全局搜索页面专用尺寸 token。
 * 输入框和右侧操作按钮属于同一搜索交互域，统一收口后便于后续单独校准。
 */
export const globalSearch = {
  // 搜索输入框高度
  inputHeight: selectTokenValue({ ios: 48, android: 42 }),
  // 搜索输入框与 placeholder 使用的排版档位
  inputTypographyVariant: 'searchInput',
  // 输入框左侧搜索图标尺寸
  inputPrefixIconSize: selectTokenValue({ ios: 18, android: 16 }),
  // 输入框内清除按钮容器尺寸
  clearButtonSize: selectTokenValue({ ios: 32, android: 30 }),
  // 输入框内清除按钮图标尺寸
  clearButtonIconSize: selectTokenValue({ ios: 19, android: 17 }),
  // 右侧独立关闭按钮尺寸
  closeButtonSize: selectTokenValue({ ios: 48, android: 42 }),
  // 右侧独立关闭按钮图标尺寸
  closeButtonIconSize: selectTokenValue({ ios: 16, android: 14 }),
  // 搜索框与关闭按钮之间的间距
  searchRowGap: spacing.md,
  // 搜索输入框左右内边距
  inputPaddingHorizontal: spacing.md,
  // 左侧搜索图标距离输入框边缘的间距
  inputPrefixMarginLeft: selectTokenValue({ ios: 11, android: 10 }),
  // 清除按钮右侧间距
  clearButtonMarginRight: spacing.xs,
  // 结果列表内容区内边距
  resultsListPadding: selectTokenValue({ ios: 20, android: 18 }),
  // 分组之间的垂直间距
  groupSpacing: spacing.lg,
  // 分组标题底部间距
  groupTitleMarginBottom: spacing.sm,
  // 分组标题左侧微调间距
  groupTitleMarginLeft: spacing.xxs,
  // 结果项左右内边距
  resultItemPaddingHorizontal: selectTokenValue({ ios: 16, android: 14 }),
  // 结果名称与图标的间距
  resultItemTextGap: selectTokenValue({ ios: 11, android: 10 }),
  // 结果项分隔线左侧缩进
  resultDividerInsetLeft: selectTokenValue({ ios: 60, android: 56 }),
  // 无结果态上下内边距
  emptyStatePaddingVertical: selectTokenValue({ ios: 40, android: 36 }),
  // 无结果说明顶部间距
  emptyStateDescriptionMarginTop: spacing.sm,
  // 分组标题排版档位
  groupTitleTypographyVariant: 'searchGroupTitle',
  // 结果项名称排版档位
  resultNameTypographyVariant: 'searchResultName',
  // 无结果标题排版档位
  emptyTitleTypographyVariant: 'searchEmptyTitle',
  // 无结果说明排版档位
  emptyDescriptionTypographyVariant: 'searchEmptyDescription',
} as const;

/**
 * 批量删除操作条专用尺寸 token。
 * 这两个底部操作按钮属于同一交互域，统一收口后便于后续一起校准。
 */
export const deleteActionBar = {
  // 取消/删除按钮最小宽度
  buttonMinWidth: selectTokenValue({ ios: 120, android: 112 }),
  // 按钮左右内边距
  buttonPaddingHorizontal: selectTokenValue({ ios: 32, android: 28 }),
  // 按钮上下内边距
  buttonPaddingVertical: selectTokenValue({ ios: 12, android: 10 }),
  // 按钮文案排版档位
  buttonTypographyVariant: 'scanActionLabel',
} as const;

/**
 * 回到底部悬浮按钮专用尺寸 token。
 * 这颗按钮是独立的悬浮交互，不和其它圆形按钮混用尺寸，避免后续联动调整。
 */
export const scrollToBottomButton = {
  // 按钮容器尺寸
  size: selectTokenValue({ ios: 40, android: 36 }),
  // 按钮内部箭头文案排版档位
  labelTypographyVariant: 'actionLabel',
  // Phone 端按钮距离底部的默认偏移
  bottomOffset: selectTokenValue({ ios: 90, android: 70 }),
  // Pad 端按钮距离底部的默认偏移
  bottomOffsetPad: selectTokenValue({ ios: 160, android: 148 }),
} as const;

/**
 * 字体语义类型。
 * 这里不要按 12 / 14 / 16 命名，而要按“用途”命名。
 * 这样后面安卓和 iOS 校准时，改的是语义，不是全项目找数字。
 */
export type TypographyVariant =
  | 'caption2'
  | 'caption1'
  | 'footnote'
  | 'body'
  | 'bodyStrong'
  | 'subhead'
  | 'helperText'
  | 'statusBody'
  | 'actionLabel'
  | 'scanActionLabel'
  | 'messageBody'
  | 'messageBodySmall'
  | 'menuLabel'
  | 'longPressMenuLabel'
  | 'statusTitle'
  | 'editorInput'
  | 'editorInputCompact'
  | 'urlInput'
  | 'searchInput'
  | 'searchGroupTitle'
  | 'searchResultName'
  | 'searchEmptyTitle'
  | 'searchEmptyDescription'
  | 'title3'
  | 'title2'
  | 'title1'
  | 'pageTitle'
  | 'emptyTitleCompact'
  | 'emptyTitle'
  | 'hero'
  | 'bodyInput';

/**
 * 单个文本 token 的定义。
 * 除字号和行高外，把字重、字间距也一并收口，避免文本表现继续分散。
 */
export interface TypographyToken {
  fontSize: number;
  lineHeight: number;
  fontWeight?: TextStyle['fontWeight'];
  letterSpacing?: number;
}

/**
 * iOS 文本体系。
 * 这是视觉基线，通常更接近设计稿中的默认预期。
 */
const iosTypography: Record<TypographyVariant, TypographyToken> = {
  caption2: {
    fontSize: 10,
    lineHeight: 14,
    fontWeight: '400',
  },
  caption1: {
    fontSize: 11,
    lineHeight: 14,
    fontWeight: '400',
  },
  footnote: {
    fontSize: 12,
    lineHeight: 16,
    fontWeight: '400',
  },
  body: {
    fontSize: 14,
    lineHeight: 20,
    fontWeight: '400',
  },
  bodyStrong: {
    fontSize: 14,
    lineHeight: 20,
    fontWeight: '600',
  },
  subhead: {
    fontSize: 15,
    lineHeight: 22,
    fontWeight: '400',
  },
  // 面板中的辅助说明文案当前使用 16 号字，这里单独建档，避免和正文、输入文案混用。
  helperText: {
    fontSize: 16,
    lineHeight: 22,
    fontWeight: '400',
  },
  // 加载中提示当前使用 15 号字，独立成档位，后续状态文案统一复用。
  statusBody: {
    fontSize: 15,
    lineHeight: 20,
    fontWeight: '400',
  },
  // 主操作按钮文案当前是 17 号半粗体，这里单独定义，避免硬套到菜单标题。
  actionLabel: {
    fontSize: 17,
    lineHeight: 22,
    fontWeight: '600',
  },
  // 扫码入口按钮的文案比通用主按钮更轻一点，单独成档，避免和其它 CTA 共用后一起变大。
  scanActionLabel: {
    fontSize: 16,
    lineHeight: 20,
    fontWeight: '600',
  },
  // 聊天消息正文单独建档，避免和输入框、菜单共用尺寸后互相牵连。
  messageBody: {
    fontSize: 16,
    lineHeight: 22,
    fontWeight: '400',
  },
  // 书签预览等小号消息正文保持更紧凑的行高节奏。
  messageBodySmall: {
    fontSize: 11,
    lineHeight: 15,
    fontWeight: '400',
  },
  // 菜单标题、按钮文案、占位文案里反复出现 17/22，这里单独收口成一档，避免继续散落裸值。
  menuLabel: {
    fontSize: 17,
    lineHeight: 22,
    fontWeight: '400',
  },
  // 长按菜单文案比通用菜单标题小一号，单独建档避免直接硬套 body 导致降太多。
  longPressMenuLabel: {
    fontSize: 17,
    lineHeight: 22,
    fontWeight: '400',
  },
  statusTitle: {
    fontSize: 18,
    lineHeight: 26,
    fontWeight: '600',
  },
  // Pad 输入框正文当前就是 17/24，这里保持原值不变，只把语义收口。
  editorInput: {
    fontSize: 17,
    lineHeight: 24,
    fontWeight: '400',
  },
  // Phone 输入框和 placeholder 当前就是 17/22，这里单独保留，避免和其它 16 号输入字混用。
  editorInputCompact: {
    fontSize: 17,
    lineHeight: 22,
    fontWeight: '400',
  },
  // LoadApp 的 URL 输入框是单行 17 号字，单独建档，后续安卓校准只影响这类地址输入。
  urlInput: {
    fontSize: 17,
    lineHeight: 22,
    fontWeight: '400',
  },
  // 全局搜索输入框与 placeholder 使用统一字号，避免搜索页和其它输入框共用语义后互相牵连。
  searchInput: {
    fontSize: 17,
    lineHeight: 22,
    fontWeight: '400',
  },
  // 搜索分组标题单独建档，避免和通用 caption 混用导致层级变化。
  searchGroupTitle: {
    fontSize: 13,
    lineHeight: 18,
    fontWeight: '400',
  },
  // 搜索结果项名称保持当前 17 号节奏，但独立成档便于系统页单独校准。
  searchResultName: {
    fontSize: 17,
    lineHeight: 22,
    fontWeight: '400',
  },
  // 无结果标题沿用结果项字号但提高字重，避免页面里继续写裸值。
  searchEmptyTitle: {
    fontSize: 17,
    lineHeight: 22,
    fontWeight: '500',
  },
  // 无结果说明文案单独收口，和普通辅助文案解耦。
  searchEmptyDescription: {
    fontSize: 14,
    lineHeight: 20,
    fontWeight: '400',
  },
  bodyInput: {
    fontSize: 16,
    lineHeight: 22,
    fontWeight: '400',
  },
  title3: {
    fontSize: 17,
    lineHeight: 24,
    fontWeight: '600',
  },
  title2: {
    fontSize: 20,
    lineHeight: 28,
    fontWeight: '600',
  },
  title1: {
    fontSize: 24,
    lineHeight: 32,
    fontWeight: '700',
  },
  // 这个档位对应系统页当前已经在线上使用的 34 号大标题，
  // 这里保持原字号不变，只补齐行高，避免接入 AppText 后被默认 body 行高裁切。
  pageTitle: {
    fontSize: 34,
    lineHeight: 41,
    fontWeight: '400',
  },
  // 对话空态标题在 Phone 上使用 36，保持现有视觉大小，只补齐行高。
  emptyTitleCompact: {
    fontSize: 36,
    lineHeight: 44,
    fontWeight: '300',
  },
  // 对话空态标题在 Pad 上使用 60，保持现有视觉大小，只补齐行高。
  emptyTitle: {
    fontSize: 60,
    lineHeight: 72,
    fontWeight: '300',
  },
  hero: {
    fontSize: 30,
    lineHeight: 38,
    fontWeight: '700',
  },
};

/**
 * Android 文本体系。
 * Android 默认字形通常更“撑”，所以常见做法是字号比 iOS 小 1 级左右，
 * 但行高不必同步缩小太多，否则可读性会变差。
 */
const androidTypography: Record<TypographyVariant, TypographyToken> = {
  caption2: {
    fontSize: 8,
    lineHeight: 14,
    fontWeight: '400',
  },
  caption1: {
    fontSize: 9,
    lineHeight: 14,
    fontWeight: '400',
  },
  footnote: {
    fontSize: 10,
    lineHeight: 16,
    fontWeight: '400',
  },
  body: {
    fontSize: 12,
    lineHeight: 20,
    fontWeight: '400',
  },
  bodyStrong: {
    fontSize: 12,
    lineHeight: 20,
    fontWeight: '600',
  },
  subhead: {
    fontSize: 13,
    lineHeight: 22,
    fontWeight: '400',
  },
  helperText: {
    fontSize: 14,
    lineHeight: 22,
    fontWeight: '400',
  },
  statusBody: {
    fontSize: 13,
    lineHeight: 20,
    fontWeight: '400',
  },
  actionLabel: {
    fontSize: 15,
    lineHeight: 22,
    fontWeight: '600',
  },
  scanActionLabel: {
    fontSize: 14,
    lineHeight: 18,
    fontWeight: '600',
  },
  messageBody: {
    fontSize: 14,
    lineHeight: 18,
    fontWeight: '400',
  },
  messageBodySmall: {
    fontSize: 9,
    lineHeight: 13,
    fontWeight: '400',
  },
  menuLabel: {
    fontSize: 15,
    lineHeight: 20,
    fontWeight: '400',
  },
  longPressMenuLabel: {
    fontSize: 14,
    lineHeight: 20,
    fontWeight: '400',
  },
  statusTitle: {
    fontSize: 16,
    lineHeight: 26,
    fontWeight: '600',
  },
  editorInput: {
    fontSize: 15,
    lineHeight: 24,
    fontWeight: '400',
  },
  editorInputCompact: {
    fontSize: 14,
    lineHeight: 18,
    fontWeight: '400',
  },
  urlInput: {
    fontSize: 14,
    lineHeight: 22,
    fontWeight: '400',
  },
  searchInput: {
    fontSize: 14,
    lineHeight: 20,
    fontWeight: '400',
  },
  searchGroupTitle: {
    fontSize: 12,
    lineHeight: 16,
    fontWeight: '400',
  },
  searchResultName: {
    fontSize: 15,
    lineHeight: 20,
    fontWeight: '400',
  },
  searchEmptyTitle: {
    fontSize: 15,
    lineHeight: 20,
    fontWeight: '500',
  },
  searchEmptyDescription: {
    fontSize: 13,
    lineHeight: 18,
    fontWeight: '400',
  },
  bodyInput: {
    fontSize: 14,
    lineHeight: 22,
    fontWeight: '400',
  },
  title3: {
    fontSize: 15,
    lineHeight: 24,
    fontWeight: '600',
  },
  title2: {
    fontSize: 18,
    lineHeight: 28,
    fontWeight: '600',
  },
  title1: {
    fontSize: 22,
    lineHeight: 30,
    fontWeight: '700',
  },
  // Android 的标题字形更撑，这里统一比 iOS 小 2 号。
  pageTitle: {
    fontSize: 32,
    lineHeight: 41,
    fontWeight: '400',
  },
  emptyTitleCompact: {
    fontSize: 32,
    lineHeight: 42,
    fontWeight: '300',
  },
  emptyTitle: {
    fontSize: 50,
    lineHeight: 68,
    fontWeight: '300',
  },
  hero: {
    fontSize: 28,
    lineHeight: 36,
    fontWeight: '700',
  },
};

/**
 * 统一导出的文本 token。
 * 业务层只关心语义，不关心当前平台选的是哪套值。
 */
export const typography = selectTokenValue({
  ios: iosTypography,
  android: androidTypography,
});

/**
 * 文本样式辅助函数。
 * 用于把语义 token 转成 RN TextStyle，避免每个地方手动拆字段。
 */
export function getTypographyStyle(
  variant: TypographyVariant,
  overrides?: Partial<TextStyle>,
): TextStyle {
  const token = typography[variant];

  return {
    fontSize: token.fontSize,
    lineHeight: token.lineHeight,
    fontWeight: token.fontWeight,
    letterSpacing: token.letterSpacing,
    ...overrides,
  };
}

/**
 * Android 文本默认会自带额外上下内边距，视觉上容易显得比 iOS 更大。
 * 这类平台差异不要散落在业务页里，统一由文本基础样式处理。
 */
export const textBaseStyle: TextStyle = {
  includeFontPadding: false,
};

/**
 * 输入框文本基础样式。
 * 输入框通常需要明确 textAlignVertical，避免多行输入在 Android 上表现不稳定。
 */
export const inputTextBaseStyle: TextStyle = {
  ...textBaseStyle,
  textAlignVertical: 'top',
  padding: 0,
  margin: 0,
};
