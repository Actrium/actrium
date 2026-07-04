import { createGuestComponent } from '../guest-factory';

export type GlobalAlertButton = {
  text?: string;
  style?: 'default' | 'cancel' | 'destructive';
  onPress?: () => void;
};

export type GlobalAlertProps = {
  visible?: boolean;
  title?: string;
  message?: string;
  buttons?: GlobalAlertButton[];
  onDismiss?: () => void;
};

export const GlobalAlert = createGuestComponent<GlobalAlertProps>('GlobalAlert');

export default GlobalAlert;
