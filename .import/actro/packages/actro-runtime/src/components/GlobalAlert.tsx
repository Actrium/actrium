import React from 'react';
import {
  Animated,
  Easing,
  Pressable,
  StyleSheet,
  Text,
  View,
  type AlertButton,
  type AlertOptions,
} from 'react-native';
import { useColors } from '../theme/useColors';
import type { ColorTokens } from '../theme/colors';

type GlobalAlertOptions = AlertOptions;
type GlobalAlertButton = AlertButton & {
  variant?: 'primary';
};

type AlertRequest = {
  id: number;
  title?: string;
  message?: string;
  buttons?: GlobalAlertButton[];
  options?: GlobalAlertOptions;
};

type QueueState = {
  active: AlertRequest | null;
};

type QueueListener = (state: QueueState) => void;

const listeners = new Set<QueueListener>();
const requestQueue: AlertRequest[] = [];
let activeRequest: AlertRequest | null = null;
let nextRequestId = 1;

function notifyQueue() {
  const state: QueueState = { active: activeRequest };
  listeners.forEach((listener) => listener(state));
}

function pullNext() {
  if (activeRequest || requestQueue.length === 0) return;
  activeRequest = requestQueue.shift() ?? null;
  notifyQueue();
}

function closeCurrent(options: { dismissed: boolean }) {
  if (!activeRequest) return;
  const closing = activeRequest;
  activeRequest = null;
  notifyQueue();
  if (options.dismissed) {
    closing.options?.onDismiss?.();
  }
  pullNext();
}

function enqueue(request: Omit<AlertRequest, 'id'>) {
  requestQueue.push({ ...request, id: nextRequestId++ });
  pullNext();
}

function subscribe(listener: QueueListener) {
  listeners.add(listener);
  listener({ active: activeRequest });
  return () => {
    listeners.delete(listener);
  };
}

const defaultButton: GlobalAlertButton = { text: 'OK' };

export const GlobalAlert = {
  alert(
    title?: string,
    message?: string,
    buttons?: GlobalAlertButton[],
    options?: GlobalAlertOptions,
  ) {
    enqueue({
      title,
      message,
      buttons: buttons?.length ? buttons : [defaultButton],
      options,
    });
  },
};

export const GlobalAlertHost: React.FC = () => {
  // GlobalAlert 依赖当前 React 树里的宿主节点渲染，适合普通页面上的全局提示队列；
  // 但它的层级仍然受宿主挂载位置限制，遇到更高层的原生 Modal 时，视觉上可能被 Modal 压住。
  const [current, setCurrent] = React.useState<AlertRequest | null>(null);
  const maskOpacity = React.useRef(new Animated.Value(0)).current;
  const dialogOpacity = React.useRef(new Animated.Value(0)).current;
  const dialogScale = React.useRef(new Animated.Value(1.08)).current;
  const isClosingRef = React.useRef(false);

  const colors = useColors();
  const styles = React.useMemo(() => makeStyles(colors), [colors]);

  React.useEffect(() => subscribe((state) => setCurrent(state.active)), []);

  React.useEffect(() => {
    if (!current) return;
    isClosingRef.current = false;
    maskOpacity.setValue(1);
    dialogOpacity.setValue(1);
    dialogScale.setValue(1.08);
    Animated.timing(dialogScale, {
      toValue: 1,
      duration: 220,
      easing: Easing.out(Easing.cubic),
      useNativeDriver: true,
    }).start();
  }, [current, dialogOpacity, dialogScale, maskOpacity]);

  const dismissWithAnimation = React.useCallback(
    (dismissed: boolean, onAfterClose?: () => void) => {
      if (isClosingRef.current) return;
      isClosingRef.current = true;
      Animated.parallel([
        Animated.timing(maskOpacity, {
          toValue: 0,
          duration: 140,
          easing: Easing.out(Easing.cubic),
          useNativeDriver: true,
        }),
        Animated.timing(dialogOpacity, {
          toValue: 0,
          duration: 140,
          easing: Easing.out(Easing.cubic),
          useNativeDriver: true,
        }),
      ]).start(() => {
        closeCurrent({ dismissed });
        onAfterClose?.();
      });
    },
    [dialogOpacity, maskOpacity],
  );

  const handleDismiss = React.useCallback(() => {
    if (!current?.options?.cancelable) return;
    dismissWithAnimation(true);
  }, [current, dismissWithAnimation]);

  const handlePressButton = React.useCallback((button: GlobalAlertButton) => {
    dismissWithAnimation(false, button.onPress);
  }, [dismissWithAnimation]);

  if (!current) return null;

  const buttons = current.buttons?.length ? current.buttons : [defaultButton];
  const isTwoButtons = buttons.length === 2;

  return (
    <View style={styles.host} pointerEvents='box-none'>
      <View style={styles.backdrop}>
        <Animated.View style={[styles.mask, { opacity: maskOpacity }]} />
        <Pressable style={styles.maskTouch} onPress={handleDismiss} />
        <View style={styles.dialogWrap} pointerEvents='box-none'>
          <Animated.View
            style={{ opacity: dialogOpacity, transform: [{ scale: dialogScale }] }}
          >
            <Pressable style={styles.dialog} onPress={() => {}}>
            {!!current.title && (
              <Text style={styles.title}>{current.title}</Text>
            )}
            {!!current.message && (
              <Text style={styles.message}>{current.message}</Text>
            )}
            <View
              style={[
                styles.buttonContainer,
                isTwoButtons
                  ? styles.buttonContainerHorizontal
                  : styles.buttonContainerVertical,
              ]}
            >
              {buttons.map((button, index) => (
                <Pressable
                  key={`${current.id}-${index}-${button.text ?? 'button'}`}
                  onPress={() => handlePressButton(button)}
                  style={({ pressed }) => [
                    styles.button,
                    // 用视觉变体区分主操作，避免把“去设置”误做成 destructive/cancel 语义。
                    button.variant === 'primary' ? styles.buttonPrimary : undefined,
                    isTwoButtons ? styles.buttonHalf : styles.buttonFull,
                    pressed ? styles.buttonPressed : undefined,
                  ]}
                >
                  <Text
                    style={[
                      styles.buttonText,
                      button.variant === 'primary'
                        ? styles.buttonTextPrimary
                        : undefined,
                      button.style === 'cancel'
                        ? styles.buttonTextCancel
                        : undefined,
                      button.style === 'destructive'
                        ? styles.buttonTextDestructive
                        : undefined,
                    ]}
                  >
                    {button.text ?? '好'}
                  </Text>
                </Pressable>
              ))}
            </View>
            </Pressable>
          </Animated.View>
        </View>
      </View>
    </View>
  );
};

const makeStyles = (colors: ColorTokens) => StyleSheet.create({
  host: {
    ...StyleSheet.absoluteFillObject,
    zIndex: 9999,
    elevation: 9999,
  },
  backdrop: {
    flex: 1,
  },
  mask: {
    ...StyleSheet.absoluteFillObject,
    backgroundColor: 'rgba(0, 0, 0, 0.1)',
  },
  maskTouch: {
    ...StyleSheet.absoluteFillObject,
  },
  dialogWrap: {
    flex: 1,
    alignItems: 'center',
    justifyContent: 'center',
    paddingHorizontal: 16,
  },
  dialog: {
    width: 300,
    borderRadius: 36,
    backgroundColor: colors.bgSurface,
    padding: 14,
  },
  title: {
    fontSize: 17,
    lineHeight: 22,
    fontWeight: '700',
    color: colors.textModalTitle,
    textAlign: 'left',
    paddingLeft: 8,
    paddingTop: 8,
  },
  message: {
    paddingLeft: 8,
    marginTop: 10,
    fontSize: 17,
    lineHeight: 22,
    color: colors.textPrimary,
    textAlign: 'left',
    paddingBottom: 14,
  },
  buttonContainer: {
    marginTop: 10,
    flexDirection: 'column',
    gap: 10,
  },
  buttonContainerHorizontal: {
    flexDirection: 'row',
  },
  buttonContainerVertical: {
    flexDirection: 'column',
  },
  button: {
    height: 48,
    borderRadius: 26,
    backgroundColor: colors.bgCancelButtonPhone,
    alignItems: 'center',
    justifyContent: 'center',
    paddingHorizontal: 16,
  },
  buttonPressed: {
    opacity: 0.72,
  },
  buttonPrimary: {
    backgroundColor: colors.brandPrimary,
  },
  buttonFull: {
    width: '100%',
  },
  buttonHalf: {
    flex: 1,
  },
  buttonText: {
    fontSize: 17,
    lineHeight: 22,
    color: colors.textPrimary,
  },
  buttonTextPrimary: {
    color: colors.textInverse,
    fontWeight: '600',
  },
  buttonTextCancel: {
    fontWeight: '600',
  },
  buttonTextDestructive: {
    color: colors.danger,
  },
});
