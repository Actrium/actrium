import React from 'react';
import {
  Animated,
  Easing,
  Modal,
  Pressable,
  StyleSheet,
  Text,
  View,
  type AlertButton,
  type AlertOptions,
} from 'react-native';
import { useColors } from '../theme/useColors';
import type { ColorTokens } from '../theme/colors';

export type GlobalAlertModalButton = AlertButton & {
  variant?: 'primary';
};

interface GlobalAlertModalProps {
  visible: boolean;
  title?: string;
  message?: string;
  buttons?: GlobalAlertModalButton[];
  options?: AlertOptions;
  onClose: (options: { dismissed: boolean }) => void;
}

const defaultButton: GlobalAlertModalButton = { text: 'OK' };

export const GlobalAlertModal: React.FC<GlobalAlertModalProps> = ({
  visible,
  title,
  message,
  buttons,
  options,
  onClose,
}) => {
  // GlobalAlertModal Based on native Modal, displaying a higher level than normal React In the tree. GlobalAlertHost;
  // Just put visible/Content status raised to shared packaging, which can also be reused across pages, even if A Page Trigger,B Page load display.
  // Current for Login protocol confirmation
  const maskOpacity = React.useRef(new Animated.Value(0)).current;
  const dialogOpacity = React.useRef(new Animated.Value(0)).current;
  const dialogScale = React.useRef(new Animated.Value(1.08)).current;
  const isClosingRef = React.useRef(false);

  const colors = useColors();
  const styles = React.useMemo(() => makeStyles(colors), [colors]);

  React.useEffect(() => {
    if (!visible) {
      isClosingRef.current = false;
      maskOpacity.setValue(0);
      dialogOpacity.setValue(0);
      dialogScale.setValue(1.08);
      return;
    }

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
  }, [dialogOpacity, dialogScale, maskOpacity, visible]);

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
        onClose({ dismissed });
        onAfterClose?.();
      });
    },
    [dialogOpacity, maskOpacity, onClose],
  );

  const handleDismiss = React.useCallback(() => {
    if (!options?.cancelable) return;
    dismissWithAnimation(true);
  }, [dismissWithAnimation, options?.cancelable]);

  const handlePressButton = React.useCallback((button: GlobalAlertModalButton) => {
    dismissWithAnimation(false, button.onPress);
  }, [dismissWithAnimation]);

  const resolvedButtons = buttons?.length ? buttons : [defaultButton];
  const isTwoButtons = resolvedButtons.length === 2;

  return (
    <Modal
      visible={visible}
      transparent
      animationType='none'
      onRequestClose={handleDismiss}
    >
      <View style={styles.host} pointerEvents='box-none'>
        <View style={styles.backdrop}>
          <Animated.View style={[styles.mask, { opacity: maskOpacity }]} />
          <Pressable style={styles.maskTouch} onPress={handleDismiss} />
          <View style={styles.dialogWrap} pointerEvents='box-none'>
            <Animated.View
              style={{ opacity: dialogOpacity, transform: [{ scale: dialogScale }] }}
            >
              <Pressable style={styles.dialog} onPress={() => {}}>
                {!!title && (
                  <Text style={styles.title}>{title}</Text>
                )}
                {!!message && (
                  <Text style={styles.message}>{message}</Text>
                )}
                <View
                  style={[
                    styles.buttonContainer,
                    isTwoButtons
                      ? styles.buttonContainerHorizontal
                      : styles.buttonContainerVertical,
                  ]}
                >
                  {resolvedButtons.map((button, index) => (
                    <Pressable
                      key={`${index}-${button.text ?? 'button'}`}
                      onPress={() => handlePressButton(button)}
                      style={({ pressed }) => [
                        styles.button,
                        // Main Operation Relay GlobalAlert , avoids the default for login confirmation button style back to the system.
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
                        {button.text ?? 'Okay.'}
                      </Text>
                    </Pressable>
                  ))}
                </View>
              </Pressable>
            </Animated.View>
          </View>
        </View>
      </View>
    </Modal>
  );
};

const makeStyles = (colors: ColorTokens) => StyleSheet.create({
  host: {
    flex: 1,
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
