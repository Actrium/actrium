import React from 'react';
import { Pressable, StyleProp, StyleSheet, View, ViewStyle } from 'react-native';
import { useTranslation } from 'react-i18next';
import { AppText } from '../../../components';
import { getTypographyStyle } from '../../../theme/tokens';
import { useColors } from '../../../theme/useColors';
import type { ColorTokens } from '../../../theme/colors';
import { useAuthTaskStore } from '../../store/useAuthTaskStore';
import { useAppStore } from '../../store/useAppStore';
import { useUserStore } from '../../store/useUserStore';

interface AuthGateOverlayProps {
  appId: string;
  style?: StyleProp<ViewStyle>;
  /** Authorise pre-checking for recall, not authorized return false */
  checkInputAuth?: () => Promise<boolean>;
}

/**
 * Authorized intercept layer: Covers above input box, intercepts touch and triggers authorized inspection if not authorized.
 * Auto remove after authorization, allow bottom TextInput Normal interaction.
 * Exiting login (%1)userInfo And become nullThe police are now in the process of re-establishing the interception.
 * Pad End and Sum Mobile Port share.
 */
export function AuthGateOverlay({
  appId,
  style,
  checkInputAuth,
}: AuthGateOverlayProps) {
  const { t } = useTranslation();
  const colors = useColors();
  const styles = React.useMemo(() => makeStyles(colors), [colors]);
  const userInfo = useUserStore((state) => state.userInfo);
  const appStatus = useAppStore(state => state.apps[appId]?.app_status)
  const allowAnonymousAccess = useAppStore(
    (state) => state.apps[appId]?.allow_anonymous_access,
  );
  const authStatus = useAuthTaskStore(
    (state) => state.alreadyAuthorizedAppIds[appId] || 'unknown',
  );
  const isLoggedIn = !!userInfo?.user?.cn;
  const shouldBlockInput =
    !allowAnonymousAccess && (!isLoggedIn || authStatus !== 'authorized') && appStatus === 'alive';

  const handlePress = React.useCallback(async () => {
    if (!checkInputAuth) {
      return;
    }
    await checkInputAuth();
  }, [checkInputAuth]);

  // overlay Directly bind the login and authorized cache to the visible, update the status and automatically update UI.
  if (!shouldBlockInput) return null;

  // Distinguish the blocked reason: not logged in vs logged in but not
  // authorized for this app. Wording mirrors the modal the tap opens.
  const hintText = !isLoggedIn
    ? t('actr.auth.signInToChat')
    : t('actr.auth.authorizeToChat');

  return (
    <Pressable
      style={[styles.overlay, style]}
      onPressIn={handlePress}
    >
      {/* Inline hint so the gated input reads as "sign in first" instead of
          a surprise login modal. Styled like the input placeholder; the pill
          background masks the underlying placeholder text. */}
      <View pointerEvents='none' style={styles.hintPill}>
        <AppText
          variant='editorInputCompact'
          numberOfLines={1}
          ellipsizeMode='tail'
          style={styles.hintText}
        >
          {hintText}
        </AppText>
      </View>
    </Pressable>
  );
}

const makeStyles = (colors: ColorTokens) => StyleSheet.create({
  // Overlay that intercepts touches meant for the underlying TextInput.
  // zIndex must stay above the mobile mobileInputGestureOverlay (zIndex: 20).
  overlay: {
    position: 'absolute',
    left: 0,
    right: 0,
    top: 0,
    bottom: 0,
    zIndex: 30,
    alignItems: 'center',
    justifyContent: 'center',
  },
  hintPill: {
    backgroundColor: colors.bgInputPill,
    borderRadius: 12,
    paddingHorizontal: 12,
    paddingVertical: 4,
  },
  hintText: {
    color: colors.textPlaceholader,
    ...getTypographyStyle('editorInputCompact'),
  },
});
