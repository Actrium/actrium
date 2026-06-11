import React, { useCallback, useEffect, useRef, useState } from 'react';
import { LayoutAnimation, View, StyleSheet, type ViewStyle } from 'react-native';
import { Supramark } from '@supramark/rn';
import type { SupramarkStyles } from '@supramark/rn';
import {
  SUPRAMARK_CONTAINER_RENDERERS,
  SUPRAMARK_FULL_CONFIG,
} from '../../libs/supramarkConfig';

export interface TypewriterProps {
  /** 完整目标文本（流式累积后的全量内容），组件会逐步显示到该内容 */
  content: string;
  /** Supramark 样式配置 */
  styles?: SupramarkStyles;
  /** 每个 tick 推进的字符数，默认 2 */
  charsPerTick?: number;
  /** tick 间隔（毫秒），默认 30 */
  tickIntervalMs?: number;
  /** 换行后额外延迟（毫秒），默认 150；
   *  确保换行前后的文本分成两次显示，不会同时出现多行 */
  lineBreakDelayMs?: number;
  /** 是否启用打字机效果，默认 true；
   *  关闭时直接显示完整内容，不逐字推进 */
  enabled?: boolean;
  /** 打字完成后回调（仅当 cursor 追上 content 时触发） */
  onComplete?: () => void;
  /** 容器样式 */
  containerStyle?: ViewStyle;
}

/**
 * 打字机效果组件
 *
 * 核心行为：
 * 1. content 作为目标文本，组件内部逐 tick 推进显示
 * 2. 每次推进 charsPerTick 个字符
 * 3. 遇到换行符时，不在同一 tick 中跨过，确保新旧行分开显示
 * 4. 换行后插入额外延迟（lineBreakDelayMs），营造"新行出现"的视觉节奏
 * 5. 流式追加场景：content 变长时，cursor 自动继续推进，无需重启
 * 6. 内容替换场景：content 不以已显示文本为前缀时，自动重置 cursor
 *
 * 注意：Supramark 每次接收的是逐步增长的 markdown 子串，
 * 中间态的 markdown 可能不完整（如代码块未闭合），
 * 但最终态传入的是完整合法的 markdown，Supramark 会正确解析。
 */
export function Typewriter({
  content,
  styles: supramarkStyles,
  charsPerTick = 2,
  tickIntervalMs = 30,
  lineBreakDelayMs = 150,
  enabled = true,
  onComplete,
  containerStyle,
}: TypewriterProps) {
  const [displayedText, setDisplayedText] = useState(() =>
    enabled ? '' : content,
  );

  // --- 所有可变值都通过 ref 管理，避免闭包陈旧和 effect 频繁重入 ---
  const cursorRef = useRef(enabled ? 0 : content.length);
  const contentRef = useRef(content);
  const displayedTextRef = useRef(enabled ? '' : content);
  const timerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const isRunningRef = useRef(false);
  const onCompleteRef = useRef(onComplete);
  const enabledRef = useRef(enabled);

  // 配置值也存入 ref，tick 函数从中读取，无需放入 useCallback 依赖
  const configRef = useRef({ charsPerTick, tickIntervalMs, lineBreakDelayMs });

  // 同步最新值到 ref
  contentRef.current = content;
  onCompleteRef.current = onComplete;
  enabledRef.current = enabled;
  configRef.current = { charsPerTick, tickIntervalMs, lineBreakDelayMs };

  // 清理定时器
  const clearTimer = useCallback(() => {
    if (timerRef.current !== null) {
      clearTimeout(timerRef.current);
      timerRef.current = null;
    }
  }, []);

  // 停止打字循环
  const stopLoop = useCallback(() => {
    isRunningRef.current = false;
    clearTimer();
  }, [clearTimer]);

  // 核心 tick 函数 —— 通过 ref 读取所有值，无外部依赖，永不陈旧
  const tickRef = useRef<() => void>(null as unknown as () => void);

  tickRef.current = () => {
    const target = contentRef.current;
    const cursor = cursorRef.current;
    const { charsPerTick: cpt, tickIntervalMs: interval, lineBreakDelayMs: lbDelay } =
      configRef.current;

    // 已追上目标内容，停止循环
    if (cursor >= target.length) {
      isRunningRef.current = false;
      onCompleteRef.current?.();
      return;
    }

    let advance = Math.min(cpt, target.length - cursor);
    let crossedLineBreak = false;

    // 不在同一个 tick 中跨过换行符
    const nextChunk = target.slice(cursor, cursor + advance);
    const lineBreakIdx = nextChunk.indexOf('\n');

    if (lineBreakIdx > 0) {
      // chunk 中包含换行符，截断到换行符之前
      advance = lineBreakIdx;
    } else if (lineBreakIdx === 0) {
      // 下一个字符就是换行符：
      // 将换行符与后续 charsPerTick 个字符一起推进，
      // 使"换行"和"新行开头"在同一个 tick 出现，
      // 避免出现只有换行符而无新行内容的空白闪烁。
      const afterBreak = target.slice(cursor + 1, cursor + 1 + cpt);
      const nextBreakInNewLine = afterBreak.indexOf('\n');
      if (nextBreakInNewLine !== -1) {
        // 新行中也有换行符，只推进到那个换行符之前
        advance = 1 + nextBreakInNewLine;
      } else {
        advance = Math.min(1 + cpt, target.length - cursor);
      }
      crossedLineBreak = true;
    }

    const nextCursor = cursor + advance;
    cursorRef.current = nextCursor;
    const newText = target.slice(0, nextCursor);
    displayedTextRef.current = newText;

    // 判断下一个字符是否是换行符（用于延迟计算和动画判断）
    const nextCharIsLineBreak =
      nextCursor < target.length && target[nextCursor] === '\n';

    // 跨过换行符或即将到达换行符时，气泡高度即将变化，
    // 用 LayoutAnimation 让高度增长和列表滚动平滑过渡，避免底部闪跳。
    if (crossedLineBreak || nextCharIsLineBreak) {
      LayoutAnimation.configureNext(LAYOUT_ANIMATION_CONFIG);
    }

    setDisplayedText(newText);

    // 刚跨过换行符 或 下一个字符是换行符 → 使用更长延迟
    const delay =
      crossedLineBreak || nextCharIsLineBreak
        ? Math.max(lbDelay, interval)
        : interval;

    timerRef.current = setTimeout(() => tickRef.current?.(), delay);
  };

  // 启动打字循环
  const startLoop = useCallback(() => {
    if (isRunningRef.current) return;
    isRunningRef.current = true;
    timerRef.current = setTimeout(
      () => tickRef.current?.(),
      configRef.current.tickIntervalMs,
    );
  }, []);

  // 监听 enabled 变化
  const prevEnabledRef = useRef(enabled);
  useEffect(() => {
    const prevEnabled = prevEnabledRef.current;
    prevEnabledRef.current = enabled;

    if (!enabled) {
      if (!prevEnabled) {
        // 一直是 disabled（旧消息，挂载时 progress 已为 2）→ 直接显示全部内容
        stopLoop();
        cursorRef.current = content.length;
        displayedTextRef.current = content;
        setDisplayedText(content);
      } else {
        // 从 enabled 变为 disabled（流式回答刚完成）→
        // 不倾倒文本，让打字循环自然追上 content 后自行停止，
        // 避免瞬间显示剩余全部文本的视觉跳变。
        if (cursorRef.current < content.length && !isRunningRef.current) {
          startLoop();
        }
      }
      return;
    }

    // enabled 从 false → true：重置 cursor，从头开始打字
    if (!prevEnabled && enabled) {
      cursorRef.current = 0;
      displayedTextRef.current = '';
      setDisplayedText('');
    }

    // 有内容要显示且循环未运行，启动
    if (cursorRef.current < content.length && !isRunningRef.current) {
      startLoop();
    }
  }, [enabled, content.length, startLoop, stopLoop, content]);

  // 监听 content 变化，检测"追加 vs 替换"
  useEffect(() => {
    // 新内容不以已显示文本为前缀 → 内容被替换（如切换消息），重置 cursor
    const isAppend =
      content.startsWith(displayedTextRef.current) ||
      displayedTextRef.current === '';

    if (!isAppend) {
      stopLoop();
      cursorRef.current = 0;
      displayedTextRef.current = '';
      setDisplayedText('');

      // 重置后如果 enabled 且有内容，重启循环
      if (enabled && content.length > 0) {
        startLoop();
      }
    } else {
      // 追加场景：有新内容且循环未运行，确保循环启动
      if (enabled && cursorRef.current < content.length && !isRunningRef.current) {
        startLoop();
      }
    }
  }, [content, enabled, startLoop, stopLoop]);

  // 组件卸载时清理定时器；
  // 如果 typewriter 还在运行（cursor 还没追上 content），触发 onComplete 兜底，
  // 保证滚走卸载时外部回调（如震动）不丢失。
  useEffect(() => {
    return () => {
      const wasRunning =
        isRunningRef.current || cursorRef.current < contentRef.current.length;
      stopLoop();
      if (wasRunning) {
        onCompleteRef.current?.();
      }
    };
  }, [stopLoop]);

  const renderedContent = enabled ? displayedText : content;

  return (
    <View style={[styles.container, containerStyle]}>
      <Supramark
        markdown={renderedContent}
        config={SUPRAMARK_FULL_CONFIG}
        containerRenderers={SUPRAMARK_CONTAINER_RENDERERS as any}
        styles={supramarkStyles}
      />
    </View>
  );
}

const styles = StyleSheet.create({
  container: {
    flexShrink: 1,
    width: '100%',
    minWidth: 0,
  },
});

// 气泡高度变化时的布局过渡动画配置。
// - update 不指定 property，使布局变化（高度、位置）做帧动画平滑过渡，
//   而非 opacity 交叉淡入（opacity 模式下 Flexbox 重排会瞬间跳动）。
// - create/delete 保留 opacity 效果，新视图淡入、旧视图淡出。
// duration 较短（120ms），在打字机效果的速度下不会产生明显延迟感，
// 但足以让高度增长和列表贴底滚动平滑过渡，消除闪跳。
const LAYOUT_ANIMATION_CONFIG = {
  duration: 120,
  create: {
    type: 'easeInEaseOut' as const,
    property: 'opacity' as const,
  },
  update: {
    type: 'easeInEaseOut' as const,
  },
  delete: {
    type: 'easeInEaseOut' as const,
    property: 'opacity' as const,
  },
};
