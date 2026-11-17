/**
 * TextInput Component
 *
 * Default TextInput component implementation, wrapping React Native TextInput
 * Uses forwardRef to allow parent components to get native view reference
 */

import React from 'react';
import { TextInput as RNTextInput, type TextStyle, type ViewStyle } from 'react-native';
import {
  getTextInputSync,
  type TextInputEdit,
  type TextInputSyncProps,
  useTextInputValue,
} from '../../../shared/text-input';

export interface TextInputProps {
  value?: string;
  defaultValue?: string;
  placeholder?: string;
  placeholderTextColor?: string;
  onChangeText?: (text: string) => void;
  onChange?: (event: { nativeEvent: TextInputEdit }) => void;
  onFocus?: () => void;
  onBlur?: () => void;
  onSubmitEditing?: () => void;
  submitBehavior?: 'submit' | 'blurAndSubmit' | 'newline';
  style?: TextStyle | ViewStyle;
  multiline?: boolean;
  numberOfLines?: number;
  maxLength?: number;
  secureTextEntry?: boolean;
  autoCapitalize?: 'none' | 'sentences' | 'words' | 'characters';
  autoCorrect?: boolean;
  autoFocus?: boolean;
  keyboardType?:
    | 'default'
    | 'email-address'
    | 'numeric'
    | 'phone-pad'
    | 'decimal-pad'
    | 'number-pad'
    | 'url';
  returnKeyType?: 'done' | 'go' | 'next' | 'search' | 'send';
  editable?: boolean;
  selectTextOnFocus?: boolean;
  testID?: string;
  accessible?: boolean;
  accessibilityLabel?: string;
}

export const TextInput = React.forwardRef<
  React.ComponentRef<typeof RNTextInput>,
  TextInputProps & TextInputSyncProps
>(
  (
    {
      value,
      __keelTextInput,
      defaultValue,
      placeholder,
      placeholderTextColor,
      onChangeText,
      onChange,
      onFocus,
      onBlur,
      onSubmitEditing,
      submitBehavior,
      style,
      multiline,
      numberOfLines,
      maxLength,
      secureTextEntry,
      autoCapitalize,
      autoCorrect,
      autoFocus,
      keyboardType,
      returnKeyType,
      editable,
      selectTextOnFocus,
      testID,
      accessible,
      accessibilityLabel,
    },
    ref
  ) => {
    const sync = getTextInputSync(__keelTextInput);
    const { value: displayedValue, recordEdit } = useTextInputValue(
      value,
      sync?.eventCount,
      defaultValue
    );
    const editCount = React.useRef(0);
    const handleChange = (event: { nativeEvent: { text: string; eventCount?: number } }) => {
      const edit = { text: event.nativeEvent.text, eventCount: ++editCount.current };
      if (sync) {
        recordEdit(edit);
        sync.onEdit(edit);
      } else {
        onChange?.({
          nativeEvent: { ...edit, eventCount: event.nativeEvent.eventCount ?? edit.eventCount },
        });
      }
    };

    // Wrap focus/blur/submit callbacks to not pass event objects to Guest
    // RN passes NativeSyntheticEvent which contains native references
    return (
      <RNTextInput
        ref={ref}
        value={displayedValue}
        defaultValue={defaultValue}
        placeholder={placeholder}
        placeholderTextColor={placeholderTextColor}
        onChangeText={sync ? undefined : onChangeText}
        onChange={sync || onChange ? handleChange : undefined}
        onFocus={onFocus ? () => onFocus() : undefined}
        onBlur={onBlur ? () => onBlur() : undefined}
        onSubmitEditing={onSubmitEditing ? () => onSubmitEditing() : undefined}
        submitBehavior={submitBehavior}
        style={style}
        multiline={multiline}
        numberOfLines={numberOfLines}
        maxLength={maxLength}
        secureTextEntry={secureTextEntry}
        autoCapitalize={autoCapitalize}
        autoCorrect={autoCorrect}
        autoFocus={autoFocus}
        keyboardType={keyboardType}
        returnKeyType={returnKeyType}
        editable={editable}
        selectTextOnFocus={selectTextOnFocus}
        testID={testID}
        accessible={accessible}
        accessibilityLabel={accessibilityLabel}
      />
    );
  }
);

TextInput.displayName = 'TextInput';

export default TextInput;
