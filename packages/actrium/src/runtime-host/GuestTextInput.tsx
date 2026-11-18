import React from 'react';
import { Platform, TextInput as NativeTextInput } from 'react-native';
import type { KeyEvent, TextInputProps as MacTextInputProps } from 'react-native-macos';
import { TextInput } from 'keel/host/preset';
import { getTextInputSync, useTextInputValue } from '../../../keel/src/shared/text-input';

type PresetProps = React.ComponentPropsWithoutRef<typeof TextInput>;
type InputInstance = React.ComponentRef<typeof TextInput>;
type GuestTextInputProps = PresetProps & { submitOnEnter?: boolean };

const plainEnter = { key: 'Enter', shiftKey: false, altKey: false, ctrlKey: false, metaKey: false };

const nativePropNames = [
  'placeholder', 'placeholderTextColor', 'style', 'numberOfLines', 'maxLength',
  'secureTextEntry', 'autoCapitalize', 'autoCorrect', 'autoFocus', 'keyboardType',
  'returnKeyType', 'editable', 'selectTextOnFocus', 'testID', 'accessible', 'accessibilityLabel',
] as const satisfies readonly (keyof PresetProps)[];

const SubmitOnEnterInput = React.forwardRef<InputInstance, PresetProps>((props, ref) => {
  const { value, defaultValue, __keelTextInput, onChange, onChangeText, onFocus, onBlur, onSubmitEditing } = props;
  const sync = getTextInputSync(__keelTextInput);
  const { value: displayedValue, recordEdit } = useTextInputValue(value, sync?.eventCount, defaultValue);
  const editCount = React.useRef(0);
  const handleChange: NonNullable<MacTextInputProps['onChange']> = ({ nativeEvent }) => {
    const edit = { text: nativeEvent.text, eventCount: ++editCount.current };
    if (sync) {
      recordEdit(edit);
      sync.onEdit(edit);
    } else {
      onChange?.({ nativeEvent: { ...edit, eventCount: nativeEvent.eventCount ?? edit.eventCount } });
    }
  };
  const handleKeyDown = ({ nativeEvent }: KeyEvent) => {
    const { key, shiftKey, altKey, ctrlKey, metaKey } = nativeEvent;
    if (key === plainEnter.key && !shiftKey && !altKey && !ctrlKey && !metaKey && props.editable !== false) {
      onSubmitEditing?.();
    }
  };
  const keyboardProps = {
    // The native input handles marked text before applying this key rule.
    keyDownEvents: [plainEnter],
    onKeyDown: handleKeyDown,
  } satisfies Pick<MacTextInputProps, 'keyDownEvents' | 'onKeyDown'>;
  const nativeProps = Object.fromEntries(nativePropNames.map((name) => [name, props[name]]));

  return (
    <NativeTextInput
      {...nativeProps}
      {...keyboardProps}
      ref={ref}
      multiline
      submitBehavior="newline"
      value={displayedValue}
      defaultValue={defaultValue}
      onChangeText={sync ? undefined : onChangeText}
      onChange={sync || onChange ? handleChange : undefined}
      onFocus={onFocus ? () => onFocus() : undefined}
      onBlur={onBlur ? () => onBlur() : undefined}
    />
  );
});

export const GuestTextInput = React.forwardRef<InputInstance, GuestTextInputProps>(
  ({ submitOnEnter, ...props }, ref) => {
    if (Platform.OS === 'macos' && submitOnEnter === true && props.multiline === true) {
      return <SubmitOnEnterInput {...props} ref={ref} />;
    }
    return <TextInput {...props} ref={ref} />;
  }
);

GuestTextInput.displayName = 'GuestTextInput';
