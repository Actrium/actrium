import React from 'react';
import { Alert } from 'react-native';
import type { GlobalAlertProps } from './GlobalAlert.guest';

export const GlobalAlert: React.FC<GlobalAlertProps> = ({
  visible,
  title,
  message,
  buttons,
  onDismiss,
}) => {
  const didShowRef = React.useRef(false);

  React.useEffect(() => {
    if (!visible) {
      didShowRef.current = false;
      return;
    }
    if (didShowRef.current) return;

    didShowRef.current = true;
    Alert.alert(title ?? '', message, buttons, {
      onDismiss,
      cancelable: true,
    });
  }, [visible, title, message, buttons, onDismiss]);

  return null;
};

export default GlobalAlert;
