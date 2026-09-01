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
  /** Chinese Comment: Enter whether to request focus immediately after the box is mounted and to edit the position in Android Upstabilizes the keyboard. */
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
        // multiline Input frame only tot on selected types clear It's still possible to leave behind old content. text Synchronize.
        inputRef.current.clear();
        inputRef.current.setNativeProps({ text: '' });
        return;
      }

      inputRef.current.setNativeProps({ text });
    },
    [],
  );

  // Do not subscribe to component retrolet: only synchronize refavoids being parented in input render Interrupt.
  React.useEffect(() => {
    const text = useInputStore.getState().inputs[tabId]?.text ?? '';
    localTextRef.current = text;
    syncNativeText(text);
  }, [syncNativeText, tabId]);

  React.useEffect(() => {
    const unsubscribe = useInputStore.subscribe((state) => {
      const text = state.inputs[tabId]?.text ?? '';
      if (text !== localTextRef.current) {
        // External emptiness/When you return to fill in, you change the original text directly and do not trigger the input component reconstruction.
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
    // When first mounted if the previous sync occurs in the original TextInput ref The blogger says that the government is not ready to do so.
    // That time. setNativeProps They'll skip; here they are. ref Create and replace it with a sync to ensure that text is displayed with store Unanimously.
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
        // mobile Text box height limit to 1~5 Okay, over and over. TextInput Internal scroll load.
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
        // Pad End Default 1 Line, expand to maximum when entering growth 5 Line, past and then scroll from within the input box.
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
  // Keyboard Action Semantic Follow“Do you want to go back to the car and change your business?”, ensure that the status name, buttons are high and the actual input is consistent.
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
        // Synchronize the right bottom corner syntax of the keyboard according to sending mode to ensure iOS / Android Show the right sending/Back to the car.
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
