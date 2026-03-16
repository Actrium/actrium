import React, { useCallback, useRef, useState } from 'react';
import {
  NativeSyntheticEvent,
  StyleSheet,
  StyleProp,
  Text,
  TextInput,
  TextInputContentSizeChangeEventData,
  TextStyle,
  View,
} from 'react-native';
import { useTranslation } from 'react-i18next';
import { useInputStore } from '../../store/useInputStore';

interface StableTextInputProps {
  tabId: string;
  hasAttachments: boolean;
  isSendOnEnter: boolean;
  handleSendRef: React.MutableRefObject<() => void>;
  mobileMode?: boolean;
  onFocus?: () => void;
  onBlur?: () => void;
  inputRefExternal?: React.MutableRefObject<TextInput | null>;
  desktopInputStyle: StyleProp<TextStyle>;
  mobileInputStyle: StyleProp<TextStyle>;
  editable?: boolean;
}

export const StableTextInput = React.memo(function StableTextInput({
  tabId,
  hasAttachments,
  isSendOnEnter,
  handleSendRef,
  mobileMode = false,
  onFocus,
  onBlur,
  inputRefExternal,
  desktopInputStyle,
  mobileInputStyle,
  editable = true,
}: StableTextInputProps) {
  const { t } = useTranslation();
  const MOBILE_LINE_HEIGHT = 22;
  const MOBILE_MAX_LINES = 5;
  const MOBILE_MIN_HEIGHT = MOBILE_LINE_HEIGHT;
  const MOBILE_MAX_HEIGHT = MOBILE_LINE_HEIGHT * MOBILE_MAX_LINES;
  const inputRef = useRef<TextInput>(null);
  const localTextRef = useRef('');
  const [initialText] = useState(
    () => useInputStore.getState().inputs[tabId]?.text ?? '',
  );
  const [mobileInputHeight, setMobileInputHeight] = useState(MOBILE_MIN_HEIGHT);
  const [isEmpty, setIsEmpty] = useState(initialText.length === 0);
  const setInputText = useInputStore((state) => state.setInputText);

  // 不订阅组件重渲染：仅同步 ref，避免输入中被父级 render 打断。
  React.useEffect(() => {
    localTextRef.current = useInputStore.getState().inputs[tabId]?.text ?? '';
  }, [tabId]);

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
          // Android multiline 对 setNativeProps({ text: '' }) 有时不稳定，clear() 更可靠。
          inputRef.current?.clear();
        } else {
          inputRef.current?.setNativeProps({ text });
        }
      }
    });
    return unsubscribe;
  }, [tabId]);

  React.useEffect(() => {
    if (!inputRefExternal) return;
    inputRefExternal.current = inputRef.current;
    return () => {
      if (inputRefExternal.current === inputRef.current) {
        inputRefExternal.current = null;
      }
    };
  }, [inputRefExternal]);

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
      setInputText(tabId, text);
    },
    [tabId, mobileMode, setInputText],
  );

  const inputStyle = React.useMemo(
    () => [
      mobileMode ? mobileInputStyle : desktopInputStyle,
      mobileMode
        ? {
            height: mobileInputHeight,
            maxHeight: MOBILE_MAX_HEIGHT,
          }
        : null,
    ],
    [
      desktopInputStyle,
      hasAttachments,
      mobileInputHeight,
      mobileInputStyle,
      mobileMode,
    ],
  );

  const handleContentSizeChange = useCallback(
    (e: NativeSyntheticEvent<TextInputContentSizeChangeEventData>) => {
      if (!mobileMode) return;
      // mobile 文本框高度限制在 1~5 行，超过后由 TextInput 内部滚动承载。
      const nextHeight = Math.max(
        MOBILE_MIN_HEIGHT,
        Math.min(e.nativeEvent.contentSize.height, MOBILE_MAX_HEIGHT),
      );
      if (Math.abs(nextHeight - mobileInputHeight) > 1) {
        setMobileInputHeight(nextHeight);
      }
    },
    [mobileInputHeight, mobileMode],
  );

  const shouldShowPlaceholder = isEmpty;

  return (
    <View style={styles.container}>
      {shouldShowPlaceholder && (
        <View pointerEvents='none' style={styles.placeholderOverlay}>
          <Text numberOfLines={1} ellipsizeMode='tail' style={styles.placeholderText}>
            {t('actr.input.placeholder')}
          </Text>
        </View>
      )}
      <TextInput
        ref={inputRef}
        style={inputStyle}
        editable={editable}
        placeholder=''
        multiline={true}
        scrollEnabled={mobileMode && mobileInputHeight >= MOBILE_MAX_HEIGHT}
        defaultValue={initialText}
        onChangeText={handleChangeText}
        onContentSizeChange={handleContentSizeChange}
        // send_on_enter=true 时，Enter 提交发送；false 时，Enter 插入换行。
        blurOnSubmit={isSendOnEnter}
        onSubmitEditing={() => {
          if (isSendOnEnter) {
            handleSendRef.current();
          }
        }}
        onFocus={onFocus}
        onBlur={onBlur}
      />
    </View>
  );
});

const styles = StyleSheet.create({
  container: {
    position: 'relative',
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
    color: '#999',
    fontSize: 17,
    lineHeight: 22,
  },
});
