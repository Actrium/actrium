import React from 'react';
import {
  View,
  StyleSheet,
  Pressable,
  Image,
  useWindowDimensions,
  ActivityIndicator,
  PermissionsAndroid,
  Platform,
} from 'react-native';
import ViewShot from 'react-native-view-shot';
import { CameraRoll } from '@react-native-camera-roll/camera-roll';
import { useTranslation } from 'react-i18next';
import QRCode from 'qrcode';
import { TextEncoder as PolyfillTextEncoder } from 'text-encoding';

import type { IMessage } from '../../../../repository/MessageRepository';
import { AppText, GlobalAlert, HostIconButton } from '../../../../components';
import { useColors } from '../../../../theme/useColors';
import type { ColorTokens } from '../../../../theme/colors';
import {
  getTypographyStyle,
  inputArea,
  spacing,
} from '../../../../theme/tokens';
import ChatShareBubble from './ChatShareBubble';

interface ChatSharePreviewModalProps {
  visible: boolean;
  message: IMessage | null;
  appName?: string;
  appUrl?: string;
  onClose: () => void;
  onSaved?: () => void;
}

interface ShareQrMatrix {
  size: number;
  data: number[];
}

const SHARE_QR_QUIET_ZONE_MODULES = 4;
const SHARE_QR_TARGET_SIZE = 88;

export default function ChatSharePreviewModal({
  visible,
  message,
  appName,
  appUrl,
  onClose,
  onSaved,
}: ChatSharePreviewModalProps) {
  const { t } = useTranslation();
  const colors = useColors();
  const styles = React.useMemo(() => makeStyles(colors), [colors]);
  const { width, height } = useWindowDimensions();
  const viewShotRef = React.useRef<ViewShot>(null);
  const [previewUri, setPreviewUri] = React.useState<string | null>(null);
  const [isCapturing, setIsCapturing] = React.useState(false);
  const [isSaving, setIsSaving] = React.useState(false);
  const [cardLayoutReady, setCardLayoutReady] = React.useState(false);
  const [captureAttempt, setCaptureAttempt] = React.useState(0);
  const [containerHeight, setContainerHeight] = React.useState(height);
  const [previewAspectRatio, setPreviewAspectRatio] = React.useState<
    number | null
  >(null);
  const [shareQrMatrix, setShareQrMatrix] = React.useState<ShareQrMatrix | null>(
    null,
  );

  React.useEffect(() => {
    if (!visible) {
      setPreviewUri(null);
      setPreviewAspectRatio(null);
      setIsCapturing(false);
      setIsSaving(false);
      setCardLayoutReady(false);
      setCaptureAttempt(0);
      setShareQrMatrix(null);
    }
  }, [visible]);

  React.useEffect(() => {
    if (!visible || !message || !cardLayoutReady) return;

    let cancelled = false;

    const captureCard = async () => {
      setIsCapturing(true);
      try {
        // 中文注释：分享卡片里包含 Markdown 和图片附件，延后一帧再截图，避免首帧布局尚未稳定。
        await new Promise((resolve) => setTimeout(resolve, 160));
        const uri = await viewShotRef.current?.capture?.();
        if (!cancelled && uri) {
          setPreviewUri(uri);
          Image.getSize(
            uri,
            (imageWidth, imageHeight) => {
              if (!cancelled && imageWidth > 0 && imageHeight > 0) {
                setPreviewAspectRatio(imageWidth / imageHeight);
              }
            },
            (error) => {
              console.warn(
                '[ChatSharePreviewModal] Failed to get preview size:',
                error,
              );
              if (!cancelled) {
                setPreviewAspectRatio(null);
              }
            },
          );
        }
      } catch (error) {
        console.error(
          '[ChatSharePreviewModal] Failed to capture share image:',
          error,
        );
        if (!cancelled) {
          GlobalAlert.alert(
            t('common.error'),
            t('actr.chat.shareGenerateFailed'),
          );
          onClose();
        }
      } finally {
        if (!cancelled) {
          setIsCapturing(false);
        }
      }
    };

    void captureCard();

    return () => {
      cancelled = true;
    };
  }, [cardLayoutReady, captureAttempt, message, onClose, t, visible]);

  const ensureAndroidPhotoPermission = React.useCallback(async () => {
    if (Platform.OS !== 'android') return true;

    if (Platform.Version >= 33) {
      const granted = await PermissionsAndroid.request(
        PermissionsAndroid.PERMISSIONS.READ_MEDIA_IMAGES,
      );
      return granted === PermissionsAndroid.RESULTS.GRANTED;
    }

    if (Platform.Version >= 29) {
      return true;
    }

    const granted = await PermissionsAndroid.request(
      PermissionsAndroid.PERMISSIONS.WRITE_EXTERNAL_STORAGE,
    );
    return granted === PermissionsAndroid.RESULTS.GRANTED;
  }, []);

  const handleSave = React.useCallback(async () => {
    if (!previewUri || isSaving) return;

    setIsSaving(true);
    try {
      const hasPermission = await ensureAndroidPhotoPermission();
      if (!hasPermission) {
        GlobalAlert.alert(
          t('common.error'),
          t('actr.chat.shareSavePermissionDenied'),
        );
        return;
      }
      await CameraRoll.saveAsset(previewUri, { type: 'photo' });
      onSaved?.();
    } catch (error) {
      console.error('[ChatSharePreviewModal] Failed to save image:', error);
      GlobalAlert.alert(t('common.error'), t('actr.chat.shareSaveFailed'));
    } finally {
      setIsSaving(false);
    }
  }, [ensureAndroidPhotoPermission, isSaving, onSaved, previewUri, t]);

  const previewWidth = Math.max(width - 68, 0);
  const previewHeight = Math.max(containerHeight - 90 - 30 - 118, 0);
  const captureImageHeight = Math.max(containerHeight, 0);
  const captureOptions = React.useMemo(
    () => ({
      format: 'png' as const,
      quality: 1,
      result: 'tmpfile' as const,
      // 中文注释：iOS 长内容截图时 drawViewHierarchyInRect 容易失败，这里切到 renderInContext 提高稳定性。
      useRenderInContext: Platform.OS === 'ios',
    }),
    [],
  );
  const renderedPreviewHeight =
    previewAspectRatio && previewAspectRatio > 0
      ? Math.min(previewHeight, previewWidth / previewAspectRatio)
      : previewHeight;
  // 中文注释：二维码内容只接受明确存在的 URL，空字符串不参与渲染，避免生成无意义二维码。
  const shareQrValue = React.useMemo(() => {
    const nextUrl = appUrl?.trim();
    return nextUrl ? nextUrl : null;
  }, [appUrl]);
  const shareQrRender = React.useMemo(() => {
    if (!shareQrMatrix) return null;

    const totalModules =
      shareQrMatrix.size + SHARE_QR_QUIET_ZONE_MODULES * 2;
    // 中文注释：二维码模块必须尽量落在整数像素上，避免截图后出现半像素插值导致扫码失败。
    const moduleSize = Math.max(
      2,
      Math.floor(SHARE_QR_TARGET_SIZE / totalModules),
    );
    const gridSize = totalModules * moduleSize;
    const cells: number[] = [];

    for (let row = 0; row < totalModules; row += 1) {
      for (let col = 0; col < totalModules; col += 1) {
        const sourceRow = row - SHARE_QR_QUIET_ZONE_MODULES;
        const sourceCol = col - SHARE_QR_QUIET_ZONE_MODULES;
        const isQuietZone =
          sourceRow < 0 ||
          sourceCol < 0 ||
          sourceRow >= shareQrMatrix.size ||
          sourceCol >= shareQrMatrix.size;

        if (isQuietZone) {
          cells.push(0);
          continue;
        }

        cells.push(
          shareQrMatrix.data[sourceRow * shareQrMatrix.size + sourceCol] ?? 0,
        );
      }
    }

    return {
      moduleSize,
      gridSize,
      totalModules,
      cells,
    };
  }, [shareQrMatrix]);

  React.useEffect(() => {
    if (!visible) return;

    if (!shareQrValue) {
      setShareQrMatrix(null);
      return;
    }

    try {
      // 中文注释：React Native 运行时默认不保证存在 TextEncoder，这里在二维码生成前按需补齐，避免影响其它模块初始化顺序。
      if (typeof global.TextEncoder === 'undefined') {
        global.TextEncoder = PolyfillTextEncoder as typeof TextEncoder;
      }
      // 中文注释：直接生成二维码矩阵并用普通 View 绘制，避免依赖浏览器 canvas 或额外原生 SVG 组件。
      const qrCode = QRCode.create(shareQrValue, {
        // 中文注释：分享图场景优先保证二维码尺寸可读，使用较低纠错级别换取更小矩阵密度。
        errorCorrectionLevel: 'L',
      });
      setShareQrMatrix({
        size: qrCode.modules.size,
        data: Array.from(qrCode.modules.data),
      });
    } catch (error) {
      console.error(
        '[ChatSharePreviewModal] Failed to generate share QR code:',
        error,
      );
      setShareQrMatrix(null);
    }
  }, [shareQrValue, visible]);

  if (!visible) {
    return null;
  }

  return (
    <View
      style={styles.backdrop}
      onLayout={(event) => {
        const nextHeight = event.nativeEvent.layout.height;
        if (nextHeight > 0 && nextHeight !== containerHeight) {
          // 中文注释：分享层挂在 Chat 页内部时，可用高度不一定等于窗口高度，必须以实际布局高度计算图片区。
          setContainerHeight(nextHeight);
        }
      }}
    >
      <View style={styles.modalContainer}>
        <View style={[styles.previewSection, { height: previewHeight }]}>
          <View
            style={[
              styles.previewCard,
              {
                width: previewWidth,
                height: previewHeight,
              },
            ]}
          >
            {previewUri ? (
              <Image
                source={{ uri: previewUri }}
                style={[
                  styles.previewImage,
                  { width: previewWidth, height: renderedPreviewHeight },
                ]}
                resizeMode='contain'
              />
            ) : (
              <View
                style={[
                  styles.previewLoading,
                  { width: previewWidth, height: previewHeight },
                ]}
              >
                <ActivityIndicator size='small' color={colors.textTertiary} />
                <AppText style={styles.previewLoadingText}>
                  {isCapturing
                    ? t('common.loading')
                    : t('actr.chat.shareGenerating')}
                </AppText>
              </View>
            )}
          </View>
        </View>
        <View style={styles.previewFooterGap} />
        <View style={styles.footer}>
          <View style={styles.footerRow}>
            <HostIconButton
              iconType='close'
              iconSize={16}
              iconColor={colors.textSecondary}
              onPress={onClose}
              size={inputArea.minHeight}
              style={styles.dismissButton}
              backgroundColor={colors.bgInputPill}
            />
            <Pressable
              style={({ pressed }) => [
                styles.saveButton,
                (pressed || isSaving) && styles.saveButtonActive,
                !previewUri && styles.saveButtonDisabled,
              ]}
              onPress={() => {
                void handleSave();
              }}
              disabled={isSaving || !previewUri}
            >
              {isSaving ? (
                <ActivityIndicator size='small' color={colors.textInverse} />
              ) : (
                <AppText style={styles.saveButtonText}>
                  {t('actr.chat.shareSaveToAlbum')}
                </AppText>
              )}
            </Pressable>
          </View>
        </View>
        {/* 图片绘制区域 */}
        <View style={styles.captureHost} pointerEvents='none'>
          <View collapsable={false}>
            <ViewShot
              ref={viewShotRef}
              style={[styles.captureCard, { width }]}
              options={captureOptions}
            >
              <View
                style={[styles.captureFrame, { minHeight: captureImageHeight }]}
                onLayout={() => {
                  // 中文注释：卡片每次重新布局后递增计数，驱动重新截图，避免屏幕宽度变化时仍复用旧图。
                  setCardLayoutReady(true);
                  setCaptureAttempt((value) => value + 1);
                }}
              >
                <View style={styles.captureContent}>
                  {message?.attachments ? (
                    <ChatShareBubble
                      type='question'
                      attachments={message.attachments}
                    />
                  ) : null}
                  {message?.question_content || message?.question_voice_payload ? (
                    <ChatShareBubble
                      type='question'
                      content={message.question_content}
                      voicePayload={message.question_voice_payload}
                    />
                  ) : null}
                  {message?.answer_content || message?.answer_voice_payload ? (
                    <ChatShareBubble
                      type='answer'
                      content={message.answer_content}
                      voicePayload={message.answer_voice_payload}
                    />
                  ) : null}
                </View>
                <View style={styles.captureFooter}>
                  <View style={styles.captureFooterDivider} />
                  <View style={styles.captureFooterTextBlock}>
                    <AppText style={styles.captureFooterTitle}>
                      {t('actr.chat.shareAiFooter')}
                    </AppText>
                    <AppText style={styles.captureFooterDesc}>
                      {t('actr.chat.shareAiDisclaimer')}
                    </AppText>
                  </View>
                  <View style={styles.captureFooterLogoBox}>
                    {shareQrRender ? (
                      <View style={styles.captureQrContainer}>
                        <View
                          style={[
                            styles.captureQrGrid,
                            {
                              width: shareQrRender.gridSize,
                              height: shareQrRender.gridSize,
                            },
                          ]}
                        >
                          {shareQrRender.cells.map((cell, index) => (
                            <View
                              key={index}
                              style={[
                                styles.captureQrCell,
                                {
                                  width: shareQrRender.moduleSize,
                                  height: shareQrRender.moduleSize,
                                  backgroundColor:
                                    cell === 1 ? '#111111' : '#FFFFFF',
                                },
                              ]}
                            />
                          ))}
                        </View>
                      </View>
                    ) : (
                      <AppText style={styles.captureFooterFallbackText}>
                        URL
                      </AppText>
                    )}
                  </View>
                </View>
              </View>
            </ViewShot>
          </View>
        </View>
      </View>
    </View>
  );
}

const makeStyles = (colors: ColorTokens) =>
  StyleSheet.create({
    backdrop: {
      ...StyleSheet.absoluteFillObject,
      backgroundColor: colors.bgChatPage,
      justifyContent: 'flex-start',
      zIndex: 1000,
    },
    modalContainer: {
      flex: 1,
      backgroundColor: colors.bgChatPage,
      paddingTop: 90,
    },
    previewSection: {
      paddingHorizontal: 34,
      alignItems: 'center',
      justifyContent: 'flex-end',
      flexShrink: 0,
    },
    previewFooterGap: {
      height: 30,
    },
    previewCard: {
      alignItems: 'center',
      justifyContent: 'center',
    },
    previewImage: {
      backgroundColor: 'transparent',
    },
    previewLoading: {
      alignItems: 'center',
      justifyContent: 'center',
      gap: spacing.sm,
    },
    previewLoadingText: {
      color: colors.textTertiary,
      ...getTypographyStyle('footnote'),
    },
    footer: {
      height: 118,
      justifyContent: 'center',
      width: '100%',
      paddingLeft: 16,
      paddingRight: 16,
      backgroundColor: 'transparent',
    },
    footerRow: {
      width: '100%',
      flexDirection: 'row',
      alignItems: 'center',
      paddingHorizontal: 16,
      gap: 12,
    },
    dismissButton: {
      width: inputArea.minHeight,
      height: inputArea.minHeight,
      borderRadius: inputArea.minHeight / 2,
    },
    saveButton: {
      flex: 1,
      height: inputArea.minHeight,
      borderRadius: inputArea.minHeight / 2,
      backgroundColor: colors.bgInputPill,
      alignItems: 'center',
      justifyContent: 'center',
    },
    saveButtonActive: {
      opacity: 0.88,
    },
    saveButtonDisabled: {
      opacity: 0.5,
    },
    saveButtonText: {
      color: colors.textSecondary,
      ...getTypographyStyle('bodyInput'),
      fontWeight: '600',
    },
    captureHost: {
      position: 'absolute',
      left: -99999,
      top: 0,
      opacity: 0,
    },
    captureCard: {
      backgroundColor: '#FFFFFF',
    },
    captureFrame: {
      backgroundColor: '#FFFFFF',
      position: 'relative',
      overflow: 'hidden',
    },
    captureContent: {
      width: '100%',
      paddingHorizontal: 16,
      paddingTop: 24,
      paddingBottom: 144,
    },
    captureFooter: {
      position: 'absolute',
      left: 0,
      right: 0,
      bottom: 0,
      height: 120,
      paddingTop: 0,
      paddingBottom: 0,
      paddingHorizontal: 16,
      backgroundColor: '#FFFFFF',
      flexDirection: 'row',
      alignItems: 'center',
      justifyContent: 'space-between',
    },
    captureFooterDivider: {
      position: 'absolute',
      top: 0,
      left: 16,
      right: 16,
      height: 1,
      backgroundColor: '#D1D1D6',
    },
    captureFooterTextBlock: {
      flex: 1,
      height: '100%',
      paddingRight: 16,
      justifyContent: 'center',
    },
    captureFooterTitle: {
      color: '#A3A3A3',
      ...getTypographyStyle('body'),
    },
    captureFooterDesc: {
      marginTop: 2,
      color: '#A3A3A3',
      ...getTypographyStyle('footnote'),
    },
    captureFooterLogoBox: {
      width: 100,
      height: 100,
      borderRadius: 18,
      alignItems: 'center',
      justifyContent: 'center',
      flexShrink: 0,
      backgroundColor: '#FFFFFF',
    },
    captureQrContainer: {
      width: '100%',
      height: '100%',
      alignItems: 'center',
      justifyContent: 'center',
      backgroundColor: '#FFFFFF',
    },
    captureQrGrid: {
      flexDirection: 'row',
      flexWrap: 'wrap',
      backgroundColor: '#FFFFFF',
    },
    captureQrCell: {
      minWidth: 1,
      minHeight: 1,
    },
    captureFooterFallbackText: {
      color: '#8E8E93',
      ...getTypographyStyle('caption1'),
      fontWeight: '600',
    },
  });
