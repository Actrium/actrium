import React, { useCallback, useRef, useState } from 'react';
import {
  NativeSyntheticEvent,
  StyleSheet,
  StyleProp,
  TextInput,
  TextInputContentSizeChangeEventData,
  TextStyle,
  View,
} from 'react-native';
import { useTranslation } from 'react-i18next';
import { AppText } from '../../../components';
import { getTypographyStyle, inputTextBaseStyle, typography } from '../../../theme/tokens';
import { useInputStore } from '../../store/useInputStore';
import { useColors } from '../../../theme/useColors';
import type { ColorTokens } from '../../../theme/colors';

interface StableTextInputProps {
  tabId: string;
  hasAttachments: boolean;
  isEnterForNewLine: boolean;
  handleSendRef: React.MutableRefObject<() => void>;
  mobileMode?: boolean;
  onFocus?: () => void;
  onBlur?: () => void;
  inputRefExternal?: React.MutableRefObject<TextInput | null>;
  desktopInputStyle: StyleProp<TextStyle>;
  mobileInputStyle: StyleProp<TextStyle>;
  editable?: boolean;
  /** 中文注释：输入框挂载后是否立即请求焦点，用于编辑态在 Android 上稳定唤起软键盘。 */
  autoFocus?: boolean;
  autoGrowMaxLines?: number;
  autoGrowLineHeight?: number;
}

export const StableTextInput = React.memo(function StableTextInput({
  tabId,
  hasAttachments,
  isEnterForNewLine,
  handleSendRef,
  mobileMode = false,
  onFocus,
  onBlur,
  inputRefExternal,
  desktopInputStyle,
  mobileInputStyle,
  editable = true,
  autoFocus = false,
  autoGrowMaxLines,
  autoGrowLineHeight,
}: StableTextInputProps) {
  const { t } = useTranslation();
  const colors = useColors();
  const styles = React.useMemo(() => makeStyles(colors), [colors]);
  const MOBILE_LINE_HEIGHT = typography.editorInputCompact.lineHeight;
  const MOBILE_MAX_LINES = 10;
  const MOBILE_MIN_HEIGHT = MOBILE_LINE_HEIGHT;
  const MOBILE_MAX_HEIGHT = MOBILE_LINE_HEIGHT * MOBILE_MAX_LINES;
  const autoGrowEnabled =
    !mobileMode &&
    typeof autoGrowMaxLines === 'number' &&
    autoGrowMaxLines > 1 &&
    typeof autoGrowLineHeight === 'number' &&
    autoGrowLineHeight > 0;
  const DESKTOP_LINE_HEIGHT = autoGrowLineHeight ?? 0;
  const DESKTOP_MIN_HEIGHT = autoGrowEnabled ? DESKTOP_LINE_HEIGHT : 0;
  const DESKTOP_MAX_HEIGHT = autoGrowEnabled
    ? DESKTOP_LINE_HEIGHT * autoGrowMaxLines!
    : 0;
  const inputRef = useRef<TextInput>(null);
  const localTextRef = useRef('');
  const [initialText] = useState(
    () => useInputStore.getState().inputs[tabId]?.text ?? '',
  );
  const [mobileInputHeight, setMobileInputHeight] = useState(MOBILE_MIN_HEIGHT);
  const [desktopInputHeight, setDesktopInputHeight] = useState(
    autoGrowEnabled ? DESKTOP_MIN_HEIGHT : 0,
  );
  const [isEmpty, setIsEmpty] = useState(initialText.length === 0);
  const setInputText = useInputStore((state) => state.setInputText);

  const syncNativeText = useCallback(
    (text: string) => {
      if (!inputRef.current) return;

      if (text.length === 0) {
        // 中文注释：multiline 输入框在部分机型上只调 clear 仍可能残留旧内容，这里补一次原生 text 同步。
        inputRef.current.clear();
        inputRef.current.setNativeProps({ text: '' });
        return;
      }

      inputRef.current.setNativeProps({ text });
    },
    [],
  );

  // 不订阅组件重渲染：仅同步 ref，避免输入中被父级 render 打断。
  React.useEffect(() => {
    const text = useInputStore.getState().inputs[tabId]?.text ?? '';
    localTextRef.current = text;
    syncNativeText(text);
  }, [syncNativeText, tabId]);

  React.useEffect(() => {
    const unsubscribe = useInputStore.subscribe((state) => {
      const text = state.inputs[tabId]?.text ?? '';
      if (text !== localTextRef.current) {
        // 外部清空/回填时，直接改原生文本，不触发输入组件重建。
        localTextRef.current = text;
        setIsEmpty((prev) => {
          const next = text.length === 0;
          return prev === next ? prev : next;
        });
        if (text.length === 0) {
          if (mobileMode) {
            setMobileInputHeight((prev) =>
              prev === MOBILE_MIN_HEIGHT ? prev : MOBILE_MIN_HEIGHT,
            );
          }
          if (autoGrowEnabled) {
            setDesktopInputHeight((prev) =>
              prev === DESKTOP_MIN_HEIGHT ? prev : DESKTOP_MIN_HEIGHT,
            );
          }
        }
        syncNativeText(text);
      }
    });
    return unsubscribe;
  }, [
    DESKTOP_MIN_HEIGHT,
    MOBILE_MIN_HEIGHT,
    autoGrowEnabled,
    mobileMode,
    syncNativeText,
    tabId,
  ]);

  React.useEffect(() => {
    if (!inputRefExternal) return;
    inputRefExternal.current = inputRef.current;
    return () => {
      if (inputRefExternal.current === inputRef.current) {
        inputRefExternal.current = null;
      }
    };
  }, [inputRefExternal]);

  React.useEffect(() => {
    // 中文注释：首次挂载时，如果前一次同步发生在原生 TextInput ref 尚未就绪之前，
    // 那次 setNativeProps 会被跳过；这里在 ref 建立后补做一次同步，确保显示文本与 store 一致。
    syncNativeText(localTextRef.current);
  }, [syncNativeText]);

  const handleChangeText = useCallback(
    (text: string) => {
      localTextRef.current = text;
      setIsEmpty((prev) => {
        const next = text.length === 0;
        return prev === next ? prev : next;
      });
      if (mobileMode && text.length === 0) {
        setMobileInputHeight((prev) =>
          prev === MOBILE_MIN_HEIGHT ? prev : MOBILE_MIN_HEIGHT,
        );
      }
      if (autoGrowEnabled && text.length === 0) {
        setDesktopInputHeight((prev) =>
          prev === DESKTOP_MIN_HEIGHT ? prev : DESKTOP_MIN_HEIGHT,
        );
      }
      setInputText(tabId, text);
    },
    [DESKTOP_MIN_HEIGHT, autoGrowEnabled, mobileMode, setInputText, tabId],
  );

  const inputStyle = React.useMemo(
    () => [
      inputTextBaseStyle,
      mobileMode ? mobileInputStyle : desktopInputStyle,
      mobileMode
        ? {
            height: mobileInputHeight,
            maxHeight: MOBILE_MAX_HEIGHT,
          }
        : autoGrowEnabled
          ? {
              height: desktopInputHeight,
              maxHeight: DESKTOP_MAX_HEIGHT,
            }
          : null,
    ],
    [
      DESKTOP_MAX_HEIGHT,
      autoGrowEnabled,
      desktopInputHeight,
      desktopInputStyle,
      hasAttachments,
      mobileInputHeight,
      mobileInputStyle,
      mobileMode,
    ],
  );

  const handleContentSizeChange = useCallback(
    (e: NativeSyntheticEvent<TextInputContentSizeChangeEventData>) => {
      if (mobileMode) {
        // mobile 文本框高度限制在 1~5 行，超过后由 TextInput 内部滚动承载。
        const nextHeight = Math.max(
          MOBILE_MIN_HEIGHT,
          Math.min(e.nativeEvent.contentSize.height, MOBILE_MAX_HEIGHT),
        );
        if (Math.abs(nextHeight - mobileInputHeight) > 1) {
          setMobileInputHeight(nextHeight);
        }
        return;
      }

      if (autoGrowEnabled) {
        // Pad 端默认 1 行，输入增长时扩展到最多 5 行，超过后改由输入框内部滚动。
        const nextHeight = Math.max(
          DESKTOP_MIN_HEIGHT,
          Math.min(e.nativeEvent.contentSize.height, DESKTOP_MAX_HEIGHT),
        );
        if (Math.abs(nextHeight - desktopInputHeight) > 1) {
          setDesktopInputHeight(nextHeight);
        }
      }
    },
    [
      DESKTOP_MAX_HEIGHT,
      DESKTOP_MIN_HEIGHT,
      autoGrowEnabled,
      desktopInputHeight,
      mobileInputHeight,
      mobileMode,
    ],
  );

  const shouldShowPlaceholder = isEmpty;
  // 中文注释：键盘动作语义跟随“是否回车换行”，确保状态名、按钮高亮和真实输入行为一致。
  const enterKeyHint = isEnterForNewLine ? 'enter' : 'send';
  const returnKeyType = isEnterForNewLine ? 'default' : 'send';
  const submitBehavior = isEnterForNewLine ? 'newline' : 'blurAndSubmit';

  return (
    <View style={styles.container}>
      {shouldShowPlaceholder && (
        <View pointerEvents='none' style={styles.placeholderOverlay}>
          <AppText
            variant='editorInputCompact'
            numberOfLines={1}
            ellipsizeMode='tail'
            style={styles.placeholderText}
          >
            {t('actr.input.placeholder')}
          </AppText>
        </View>
      )}
      <TextInput
        ref={inputRef}
        style={inputStyle}
        editable={editable}
        autoFocus={autoFocus}
        showSoftInputOnFocus={true}
        placeholder=''
        multiline={true}
        scrollEnabled={
          mobileMode
            ? mobileInputHeight >= MOBILE_MAX_HEIGHT
            : autoGrowEnabled && desktopInputHeight >= DESKTOP_MAX_HEIGHT
        }
        defaultValue={initialText}
        onChangeText={handleChangeText}
        onContentSizeChange={handleContentSizeChange}
        // 根据发送模式同步键盘右下角语义，确保 iOS / Android 都显示正确的发送/回车动作。
        enterKeyHint={enterKeyHint}
        returnKeyType={returnKeyType}
        submitBehavior={submitBehavior}
        blurOnSubmit={!isEnterForNewLine}
        onSubmitEditing={() => {
          if (!isEnterForNewLine) {
            handleSendRef.current();
          }
        }}
        onFocus={onFocus}
        onBlur={onBlur}
      />
    </View>
  );
});

const makeStyles = (colors: ColorTokens) => StyleSheet.create({
  container: {
    position: 'relative',
    width: '100%',
  },
  placeholderOverlay: {
    position: 'absolute',
    left: 0,
    right: 0,
    top: 0,
    bottom: 0,
    justifyContent: 'center',
    zIndex: 1,
  },
  placeholderText: {
    color: colors.textPlaceholader,
    ...getTypographyStyle('editorInputCompact'),
  },
});
