import React, { useCallback, useEffect, useRef, useState } from 'react';
import {
  LayoutAnimation,
  Text,
  View,
  StyleSheet,
  type TextStyle,
  type ViewStyle,
} from 'react-native';

export interface TypewriterProps {
  /** Full target text (full content after flow-based accumulation) with components gradually showing it */
  content: string;
  /** Text style used for the progressively rendered content. */
  contentStyle?: TextStyle;
  /** Every one. tick Number of characters advanced, default 2 */
  charsPerTick?: number;
  /** tick Interval (ms), default 30 */
  tickIntervalMs?: number;
  /** Additional delay (ms), default 150;
   *  Ensure that text before and after line breaks is split into two shows, without multiple lines occurring at the same time */
  lineBreakDelayMs?: number;
  /** Whether to enable typewriter effects, default true;
   *  Show full content directly on close without moving word for word */
  enabled?: boolean;
  /** Back after typing (only when) cursor Let's go! content (a) The time trigger) */
  onComplete?: () => void;
  /** Container Styles */
  containerStyle?: ViewStyle;
}

/**
 * Typewriter effect component
 *
 * Core behaviour:
 * 1. content As target text, component internally tick Propulsion Display
 * 2. Every push charsPerTick Character
 * 3. When you encounter a line break, it's not the same tick Crosses to ensure that old and new lines are separated
 * 4. Insert an additional delay after line break (lineBreakDelayMs) , Construction"New line appears"The visual rhythm of the
 * 5. Streaming additional scene:content The blogger adds:cursor Auto-push. No reboot.
 * 6. Content replacement scene:content Autoreset without prefixing already displayed text cursor
 *
 * The component receives a progressively growing plain-text substring and the
 * complete final value once streaming finishes.
 */
export function Typewriter({
  content,
  contentStyle,
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

  // --- All variable values passed. ref Management, to avoid closed-packing obsolete effect Frequent re-entry ---
  const cursorRef = useRef(enabled ? 0 : content.length);
  const contentRef = useRef(content);
  const displayedTextRef = useRef(enabled ? '' : content);
  const timerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const isRunningRef = useRef(false);
  const onCompleteRef = useRef(onComplete);
  const enabledRef = useRef(enabled);

  // Configure also ref, tick Function read from it, no need to be inserted useCallback Dependency
  const configRef = useRef({ charsPerTick, tickIntervalMs, lineBreakDelayMs });

  // Sync the latest value to ref
  contentRef.current = content;
  onCompleteRef.current = onComplete;
  enabledRef.current = enabled;
  configRef.current = { charsPerTick, tickIntervalMs, lineBreakDelayMs };

  // Clean Timer
  const clearTimer = useCallback(() => {
    if (timerRef.current !== null) {
      clearTimeout(timerRef.current);
      timerRef.current = null;
    }
  }, []);

  // Stop typing cycle
  const stopLoop = useCallback(() => {
    isRunningRef.current = false;
    clearTimer();
  }, [clearTimer]);

  // Core tick Function —— Pass. ref Read all values, no external dependency, never old
  const tickRef = useRef<() => void>(null as unknown as () => void);

  tickRef.current = () => {
    const target = contentRef.current;
    const cursor = cursorRef.current;
    const { charsPerTick: cpt, tickIntervalMs: interval, lineBreakDelayMs: lbDelay } =
      configRef.current;

    // We've got target content. Stop the cycle.
    if (cursor >= target.length) {
      isRunningRef.current = false;
      onCompleteRef.current?.();
      return;
    }

    let advance = Math.min(cpt, target.length - cursor);
    let crossedLineBreak = false;

    // Not the same. tick Interline Break
    const nextChunk = target.slice(cursor, cursor + advance);
    const lineBreakIdx = nextChunk.indexOf('\n');

    if (lineBreakIdx > 0) {
      // chunk Include line breaks, cut before line breaks
      advance = lineBreakIdx;
    } else if (lineBreakIdx === 0) {
      // Next character is a line break:
      // Reset Line Breaks and Follow charsPerTick The one character that pushes together,
      // ♪ ♪ And I'm gonna ♪"Line Break"and"New Line Start"In the same one. tick Come on, come on.
      // Avoids the emergence of blanks with line breaks and no new line contents.
      const afterBreak = target.slice(cursor + 1, cursor + 1 + cpt);
      const nextBreakInNewLine = afterBreak.indexOf('\n');
      if (nextBreakInNewLine !== -1) {
        // There are line breaks in the new line, which only advance before the line break.
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

    // Determines whether the next character is a line break (for delayed calculation and animation)
    const nextCharIsLineBreak =
      nextCursor < target.length && target[nextCursor] === '\n';

    // When you cross a line break or are about to reach a line break, the air bubble height is about to change.
    // Use LayoutAnimation To allow a high growth and list smooth transition to avoid a background jump.
    if (crossedLineBreak || nextCharIsLineBreak) {
      LayoutAnimation.configureNext(LAYOUT_ANIMATION_CONFIG);
    }

    setDisplayedText(newText);

    // Just crossed a line break or Next character is a line break → Use of longer delay
    const delay =
      crossedLineBreak || nextCharIsLineBreak
        ? Math.max(lbDelay, interval)
        : interval;

    timerRef.current = setTimeout(() => tickRef.current?.(), delay);
  };

  // Start typering cycle
  const startLoop = useCallback(() => {
    if (isRunningRef.current) return;
    isRunningRef.current = true;
    timerRef.current = setTimeout(
      () => tickRef.current?.(),
      configRef.current.tickIntervalMs,
    );
  }, []);

  // Listening enabled Change
  const prevEnabledRef = useRef(enabled);
  useEffect(() => {
    const prevEnabled = prevEnabledRef.current;
    prevEnabledRef.current = enabled;

    if (!enabled) {
      if (!prevEnabled) {
        // Always. disabled(Old message, mount progress Already 2)→ Show all content directly
        stopLoop();
        cursorRef.current = content.length;
        displayedTextRef.current = content;
        setDisplayedText(content);
      } else {
        // From enabled And become disabled(Flower answers just finished)→
        // No text dumping, let the typing cycle follow. content The blogger says:
        // Avoids visual jumps that display all remaining text in a moment.
        if (cursorRef.current < content.length && !isRunningRef.current) {
          startLoop();
        }
      }
      return;
    }

    // enabled From false → true: Reset cursor, start typing from the beginning
    if (!prevEnabled && enabled) {
      cursorRef.current = 0;
      displayedTextRef.current = '';
      setDisplayedText('');
    }

    // There are elements to show and recycle not running. Start
    if (cursorRef.current < content.length && !isRunningRef.current) {
      startLoop();
    }
  }, [enabled, content.length, startLoop, stopLoop, content]);

  // Listening content Change, test."Append vs Replace"
  useEffect(() => {
    // New content does not prefix to already displayed text → Content replaced (e.g., toggle messages), reset cursor
    const isAppend =
      content.startsWith(displayedTextRef.current) ||
      displayedTextRef.current === '';

    if (!isAppend) {
      stopLoop();
      cursorRef.current = 0;
      displayedTextRef.current = '';
      setDisplayedText('');

      // Reset If enabled And with content, recycle.
      if (enabled && content.length > 0) {
        startLoop();
      }
    } else {
      // Add scene: new content and recycle not running to ensure recycle startup
      if (enabled && cursorRef.current < content.length && !isRunningRef.current) {
        startLoop();
      }
    }
  }, [content, enabled, startLoop, stopLoop]);

  // Clean up the timer when the component is unmounted;
  // If typewriter Running (cursor We're not on it yet. content) , trigger onComplete In the back,
  // Ensure that external backslides (e.g., vibrations) are not lost when rolling off.
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
      <Text style={contentStyle}>{renderedContent}</Text>
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

// Layout transition animation configurations when bubble height changes.
// - update Do Not Assign propertyAnd it's a smooth transition of layout changes (high, high) and frames.
//   Not opacity Cross-Down (opacity Mode Flexbox Re-alignment will jump in a moment.
// - create/delete Reservations opacity effect, new view fades into, old view fades out.
// duration Shorter (120ms) does not produce a significant delay at the speed of the typewriter effect,
// But it is enough to make a smooth transition of high growth and bottom-down roller and roller.
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
