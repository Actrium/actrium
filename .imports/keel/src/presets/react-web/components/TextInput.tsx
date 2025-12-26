/**
 * TextInput Component (Web)
 *
 * Maps to input element
 */

import type React from 'react';

export interface TextInputProps {
  style?: React.CSSProperties;
  className?: string;
  value?: string;
  defaultValue?: string;
  placeholder?: string;
  onChangeText?: (text: string) => void;
  onFocus?: () => void;
  onBlur?: () => void;
  onSubmitEditing?: () => void;
  multiline?: boolean;
  editable?: boolean;
  secureTextEntry?: boolean;
  keyboardType?: 'default' | 'email-address' | 'numeric' | 'phone-pad';
}

export function TextInput({
  style,
  className,
  value,
  defaultValue,
  placeholder,
  onChangeText,
  onFocus,
  onBlur,
  onSubmitEditing,
  multiline = false,
  editable = true,
  secureTextEntry = false,
  keyboardType = 'default',
}: TextInputProps): React.ReactElement {
  const inputType = secureTextEntry
    ? 'password'
    : keyboardType === 'email-address'
      ? 'email'
      : keyboardType === 'numeric' || keyboardType === 'phone-pad'
        ? 'tel'
        : 'text';

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === 'Enter' && !multiline && onSubmitEditing) {
      onSubmitEditing();
    }
  };

  if (multiline) {
    return (
      <textarea
        style={style}
        className={className}
        value={value}
        defaultValue={defaultValue}
        placeholder={placeholder}
        onChange={(e) => onChangeText?.(e.target.value)}
        onFocus={onFocus}
        onBlur={onBlur}
        disabled={!editable}
      />
    );
  }

  return (
    <input
      type={inputType}
      style={style}
      className={className}
      value={value}
      defaultValue={defaultValue}
      placeholder={placeholder}
      onChange={(e) => onChangeText?.(e.target.value)}
      onFocus={onFocus}
      onBlur={onBlur}
      onKeyDown={handleKeyDown}
      disabled={!editable}
    />
  );
}

export default TextInput;
