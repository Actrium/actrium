/**
 * Semantic Colours token.
 * min light / dark Two sets, business level through. useColors() Gets the current color, does not directly refer to the object here.
 */

export interface ColorTokens {
  // Text
  textPrimary: string;
  textSecondary: string;
  textTertiary: string;
  textInverse: string;
  textHeading: string;
  textAppName: string;
  textWelcome: string;
  textModalTitle: string;
  textModalTitlePhone: string;
  textMenuLabel: string;
  textTag: string;
  textInput: string;
  textCancelButton: string;
  textError: string;
  textLoading: string;
  textOpenCameraBtn: string;
  textIconPrimary: string;
  textIconSecondary: string;
  textIconDark: string;
  textIconStrong: string;
  textMobileAttachmentPrimary: string;
  textMobileAttachmentSecondary: string;
  textMobileAttachmentSubtitle: string;
  textMobileAttachmentActive: string;
  textMobileAttachmentActiveSubtitle: string;
  iconDefault: string;
  iconChatHeader: string;
  iconMobileAttachment: string;
  iconMobileAttachmentActive: string;

  // HostIconButton
  iconHostIconButton: string;
  iconHostIconButtonActive: string;
  shadowColor: string;

  // Background
  bgPage: string;
  bgChatHeaderBtn: string;
  bgOverlay: string;
  bgHeader: string;
  bgHeaderGradientEnd: string;
  bgChatPage: string;
  bgChatTablet: string;
  bgToast: string;
  bgSurface: string;
  bgDeleteGradientStart: string;
  bgDeleteGradientMid: string;
  bgDeleteGradientEnd: string;
  bgInput: string;
  bgInputPhone: string;
  bgInputPill: string;
  bgInputPadPill: string;
  bgInputMobile: string;
  bgTag: string;
  bgTagSelected: string;
  bgTagSelectedPhone: string;
  bgCancelButton: string;
  bgCancelButtonPhone: string;
  bgSubmitButton: string;
  bgSubmitButtonPhone: string;
  bgMenuItemPressed: string;
  bgMenuItemActive: string;
  bgMenuDivider: string;
  bgToolbarButton: string;
  bgToolbarButtonPlaying: string;
  bgActionButton: string;
  bgActionButtonActive: string;
  bgAttachment: string;
  bgAttachmentItem: string;
  bgMobileAttachmentItem: string;
  bgMobileAttachmentItemActive: string;
  bgFileBlock: string;
  bgLocationPlaceholder: string;
  bgRemoveButton: string;
  bgBubbleQuestion: string;
  bgBubbleAnswer: string;
  bgScreen: string;
  bgColumn: string;
  bgEmpty: string;
  bgBlocked: string;
  bgLoading: string;
  bgScanArea: string;
  bgLoadDark: string;
  bgHostIconButton: string;
  bgHostIconButtonActive: string;
  textPlaceholader: string;
  targetBgColor: string;

  // extensionPanel
  extensionPanelBg: string;
  extensionPanelBorder: string;
  extensionPanelSkeletonBg: string;
  extensionPanelSkeletonBlock: string;

  // auth
  authTagline: string;
  authButtonBg: string;
  authButtonBorder: string;
  authButtonText: string;
  authIconTint: string;
  authPolicyText: string;
  authLinkText: string;
  authTitle: string;
  authTip: string;
  authAppName: string;
  authLoadingIndicator: string;
  authLoadingText: string;
  authPrimaryButtonBg: string;
  authPrimaryButtonText: string;
  authRejectButtonBg: string;
  authRejectButtonBorder: string;
  authRejectButtonText: string;
  authModalLoginBg: string;
  authModalLoginTitle: string;
  authModalLoginCloseBg: string;
  authModalLoginCloseIcon: string;
  authModalLoginSuccessText: string;
  authModalAuthorizeBg: string;
  authModalAuthorizeTitle: string;
  authModalAuthorizeCloseBg: string;
  authModalAuthorizeCloseIcon: string;
  authModalAuthorizePlaceholder: string;
  authProfileBg: string;
  authProfileIconBg: string;
  authProfileName: string;
  authDarkBg: string;

  // Border
  borderPrimary: string;
  borderSecondary: string;
  borderAttachment: string;
  borderToolbarTop: string;
  borderFileBlock: string;
  borderReservedButton: string;
  borderCheckboxUnchecked: string;

  bgToBottom: string,

  // Semantic Colour
  brandPrimary: string;
  accentBlue: string;
  danger: string;
  success: string;
  successIcon: string;
  warning: string;

  // Sound recording state color
  voiceBaseColor: string;
  voiceCancelBaseColor: string;
  voiceTransColor: string;
  voiceCancelTransColor: string;
  voiceTextColor: string;
  voiceCancelTextColor: string;
  voiceRectBgColor: string;
  voiceCancelRectBgColor: string;
}

export const lightColors: ColorTokens = {
  textPrimary: '#222222',
  textSecondary: '#666666',
  textTertiary: '#999999',
  textInverse: '#FFFFFF',
  textHeading: '#252525',
  textAppName: '#1e2939',
  textWelcome: '#364153',
  textModalTitle: '#1A1A1A',
  textModalTitlePhone: '#292929',
  textMenuLabel: '#333333',
  textTag: '#48484A',
  textInput: '#333333',
  textCancelButton: '#48484A',
  textError: '#ff6b6b',
  textLoading: '#888888',
  textOpenCameraBtn: '#000000',
  textIconPrimary: '#333333',
  textIconSecondary: '#666666',
  textIconDark: '#2C2C2C',
  textIconStrong: '#404040',
  textMobileAttachmentPrimary: '#252525',
  textMobileAttachmentSecondary: '#292929',
  textMobileAttachmentSubtitle: '#8F8F8F',
  textMobileAttachmentActive: '#597EF7',
  textMobileAttachmentActiveSubtitle: 'rgba(89, 126, 247, 0.5)',
  iconDefault: '#0a0a0a',
  iconChatHeader: '#333333',
  iconMobileAttachment: '#252525',
  iconMobileAttachmentActive: '#597EF7',
  iconHostIconButton: '#0a0a0a',
  iconHostIconButtonActive: '#FFFFFF',
  shadowColor: '#CCCCCC',
  textPlaceholader: '#d9d9d9',
  targetBgColor: '#00C323',

  bgPage: '#F7F7F7',
  bgChatHeaderBtn: '#007AFF',
  bgOverlay: 'rgba(0, 0, 0, 0.4)',
  bgHeader: '#F2F2F6',
  bgHeaderGradientEnd: 'rgba(242, 242, 246, 0)',
  bgChatPage: '#F2F2F6',
  bgChatTablet: '#FFFFFF',
  bgToast: '#FFFFFF',
  bgSurface: '#FFFFFF',
  bgDeleteGradientStart: 'rgba(255, 255, 255, 0)',
  bgDeleteGradientMid: 'rgba(255, 255, 255, 0.7)',
  bgDeleteGradientEnd: 'rgba(255, 255, 255, 1)',
  bgInput: '#F5F5F7',
  bgInputPhone: '#E3E3E3',
  bgInputPill: '#FFFFFF',
  bgInputPadPill: '#fff',
  bgInputMobile: '#F2F2F6',
  bgTag: '#F5F5F7',
  bgTagSelected: '#007AFF',
  bgTagSelectedPhone: '#34A8FB',
  bgCancelButton: '#F2F2F7',
  bgCancelButtonPhone: '#D9D9D9',
  bgSubmitButton: '#007AFF',
  bgSubmitButtonPhone: '#34A8FB',
  bgMenuItemPressed: '#F2F2F7',
  bgMenuItemActive: '#EDEDED',
  bgMenuDivider: '#E0E0E0',
  bgToolbarButton: '#EAEAEA',
  bgToolbarButtonPlaying: '#e6f4ff',
  bgActionButton: '#7676801F',
  bgActionButtonActive: '#D1D1D6',
  bgAttachment: '#E5E5EA',
  bgAttachmentItem: '#f5f5f5',
  bgMobileAttachmentItem: '#EAEAEA',
  bgMobileAttachmentItemActive: '#E9F0FC',
  bgFileBlock: '#F7F7F7',
  bgLocationPlaceholder: '#F0F8FF',
  bgRemoveButton: 'rgba(0,0,0,0.5)',
  bgBubbleQuestion: '#E6E9F6',
  bgBubbleAnswer: '#F9F9FB',
  bgScreen: '#000',
  bgColumn: '#F2F2F6',
  bgEmpty: '#fff',
  bgBlocked: '#fff',
  bgLoading: '#fff',
  bgScanArea: '#F2F2F6',
  bgLoadDark: '#fff',
  bgHostIconButton: '#DFDFE1',
  bgHostIconButtonActive: '#0088FF',

  // extensionPanel
  extensionPanelBg: '#F2F2F6',
  extensionPanelBorder: '#DADADE',
  extensionPanelSkeletonBg: '#F2F2F6',
  extensionPanelSkeletonBlock: '#e7e7e7',

  // auth
  authTagline: '#000000',
  authButtonBg: '#FFFFFF',
  authButtonBorder: '#E5E5E5',
  authButtonText: '#333',
  authIconTint: '#000',
  authPolicyText: '#999',
  authLinkText: '#1C69FF',
  authTitle: '#333',
  authTip: '#252525',
  authAppName: '#333',
  authLoadingIndicator: '#1C69FF',
  authLoadingText: '#666',
  authPrimaryButtonBg: '#34A8FB',
  authPrimaryButtonText: '#fff',
  authRejectButtonBg: '#78788029',
  authRejectButtonBorder: 'transparent',
  authRejectButtonText: '#333',
  authModalLoginBg: 'rgba(203, 214, 253, 1)',
  authModalLoginTitle: '#fff',
  authModalLoginCloseBg: '#F0F0F0',
  authModalLoginCloseIcon: '#666',
  authModalLoginSuccessText: '#52C41A',
  authModalAuthorizeBg: '#E8E8E9',
  authModalAuthorizeTitle: '#333333',
  authModalAuthorizeCloseBg: '#78788029',
  authModalAuthorizeCloseIcon: '#666',
  authModalAuthorizePlaceholder: '#E5E5E5',
  authProfileBg: '#78788029',
  authProfileIconBg: '#597EF70D',
  authProfileName: '#000000',
  authDarkBg: 'rgba(0, 0, 0, 0)',

  borderPrimary: '#E6E6E6',
  borderSecondary: '#F0F0F0',
  borderAttachment: '#ACACAC',
  borderToolbarTop: '#f1f5f9',
  borderFileBlock: '#E8E8E8',
  borderReservedButton: '#ECECEF',
  borderCheckboxUnchecked: '#D9D9D9',

  bgToBottom: '#fff',

  brandPrimary: '#1677FF',
  accentBlue: '#34A8FB',
  danger: '#FF3B30',
  success: '#2BA471',
  successIcon: '#34C759',
  warning: '#F5A623',

  voiceBaseColor: '#58A6F4',
  voiceCancelBaseColor: '#F2A4A4',
  voiceTransColor: 'rgba(88, 166, 244, 0)',
  voiceCancelTransColor: 'rgba(242, 164, 164, 0)',
  voiceTextColor: '#FFF',
  voiceCancelTextColor: '#FF684E',
  voiceRectBgColor: '#FFF',
  voiceCancelRectBgColor: 'rgba(255, 59, 48, 0.2)',
};

export const darkColors: ColorTokens = {
  textPrimary: '#fff',
  textSecondary: '#fff',
  textTertiary: '#fff',
  textAppName: '#fff',
  textWelcome: '#fff',
  textInverse: '#fff',
  textHeading: '#D5D5D5',

  textIconPrimary: '#fff',
  textPlaceholader: '#ECECED',

  textMenuLabel: '#fff',
  textTag: '#48484A',
  textInput: '#fff',
  textCancelButton: '#fff',
  textError: '#ff6b6b',
  textLoading: '#888888',
  textOpenCameraBtn: '#fff',
  textIconSecondary: '#fff',
  textIconDark: '#fff',
  textIconStrong: '#fff',
  textMobileAttachmentPrimary: '#fff',
  textMobileAttachmentSecondary: '#fff',
  textMobileAttachmentSubtitle: '#fff',
  textMobileAttachmentActive: '#597EF7',
  textMobileAttachmentActiveSubtitle: 'rgba(89, 126, 247, 0.5)',
  iconDefault: '#fff',
  iconChatHeader: '#fff',
  iconMobileAttachment: '#fff',
  iconMobileAttachmentActive: '#597EF7',

  iconHostIconButton: '#78788052',
  iconHostIconButtonActive: '#78788052',
  shadowColor: '#3A3A3C',

  bgPage: '#F7F7F7',
  bgOverlay: 'rgba(0, 0, 0, 0.4)',
  bgHeader: '#3A3A3C',
  bgHeaderGradientEnd: 'rgba(58, 58, 60, 0)',

  bgChatPage: '#3A3A3C',
  bgChatHeaderBtn: '#007AFF',

  bgScanArea: '#3A3A3C', // Background Colour
  bgLoadDark: '#78788052', // Lighter than black.

  // hostIconBtn
  bgHostIconButton: '#48484A',
  bgHostIconButtonActive: '#0088FF',

  // modal
  bgToast: '#4A4A4A',
  bgSurface: '#383838',
  bgDeleteGradientStart: 'rgba(0, 0, 0, 0)',
  bgDeleteGradientMid: 'rgba(0, 0, 0, 0.7)',
  bgDeleteGradientEnd: 'rgba(0, 0, 0, 1)',
  textModalTitle: '#FFF',
  textModalTitlePhone: '#FFF',
  bgCancelButtonPhone: '#4A4A4A',

  // bubble
  bgBubbleQuestion: '#292C38',
  bgBubbleAnswer: '#2C2C2E',

  bgChatTablet: '#FFFFFF',
  bgInput: '#F5F5F7',
  bgInputPhone: '#4A4A4A',
  bgInputPill: '#6A6A6A',
  bgInputPadPill: '#3A3A3C',
  bgInputMobile: '#3A3A3C',
  bgTag: '#7676801F',
  bgTagSelected: '#34A8FB',
  bgTagSelectedPhone: '#34A8FB',
  bgCancelButton: '#4A4A4A',

  bgSubmitButton: '#007AFF',
  bgSubmitButtonPhone: '#34A8FB',
  bgMenuItemPressed: '#4E4E52',
  bgMenuItemActive: '#6A6A6A',
  bgMenuDivider: '#414141',
  bgToolbarButton: '#414141',
  bgToolbarButtonPlaying: '#e6f4ff',
  bgActionButton: '#414141',
  bgActionButtonActive: '#4E4E52',
  bgAttachment: '#E5E5EA',
  bgAttachmentItem: '#f5f5f5',
  bgMobileAttachmentItem: '#565656',
  bgMobileAttachmentItemActive: '#E9F0FC',
  bgFileBlock: '#2C2C2E',
  bgLocationPlaceholder: '#F0F8FF',
  bgRemoveButton: 'rgba(0,0,0,0.5)',

  bgScreen: '#000000',
  bgColumn: '#3A3A3C',
  bgEmpty: '#111111',
  bgBlocked: '#1a1a1a',
  bgLoading: '#0a0a0a',
  targetBgColor: '#00C323',

  // extensionPanel
  extensionPanelBg: '#3A3A3C',
  extensionPanelBorder: '#1a1a1a',
  extensionPanelSkeletonBg: '#3A3A3C',
  extensionPanelSkeletonBlock: '#1a1a1a',

  // auth
  authTagline: '#fff',
  authButtonBg: '#1C1C1E',
  authButtonBorder: 'rgba(255, 255, 255, 0.1)',
  authButtonText: '#fff',
  authIconTint: '#fff',
  authPolicyText: 'rgba(255, 255, 255, 0.8)',
  authLinkText: '#34A8FB',
  authTitle: '#fff',
  authTip: '#ccc',
  authAppName: '#fff',
  authLoadingIndicator: '#fff',
  authLoadingText: '#fff',
  authPrimaryButtonBg: '#34A8FB',
  authPrimaryButtonText: '#fff',
  authRejectButtonBg: 'rgba(255, 255, 255, 0.15)',
  authRejectButtonBorder: 'rgba(255, 255, 255, 0.1)',
  authRejectButtonText: '#fff',
  authModalLoginBg: '#2C2C2E',
  authModalLoginTitle: '#fff',
  authModalLoginCloseBg: 'rgba(255, 255, 255, 0.15)',
  authModalLoginCloseIcon: '#fff',
  authModalLoginSuccessText: '#52C41A',
  authModalAuthorizeBg: '#3A3A3C',
  authModalAuthorizeTitle: '#fff',
  authModalAuthorizeCloseBg: 'rgba(255, 255, 255, 0.15)',
  authModalAuthorizeCloseIcon: '#fff',
  authModalAuthorizePlaceholder: '#48484A',
  authProfileBg: 'rgba(255, 255, 255, 0.15)',
  authProfileIconBg: 'rgba(255, 255, 255, 0.1)',
  authProfileName: '#fff',
  authDarkBg: 'rgba(0, 0, 0, .5)',

  borderPrimary: '#E6E6E6',
  borderSecondary: '#F0F0F0',
  borderAttachment: '#ACACAC',
  borderToolbarTop: '#414141',
  borderFileBlock: '#757575',
  borderReservedButton: '#4A4A4A',
  borderCheckboxUnchecked: '#D9D9D9',

  bgToBottom: '#6A6A6A',

  brandPrimary: '#1677FF',
  accentBlue: '#34A8FB',
  danger: '#FF3B30',
  success: '#2BA471',
  successIcon: '#34C759',
  warning: '#F5A623',

  voiceBaseColor: '#58A6F4',
  voiceCancelBaseColor: '#F2A4A4',
  voiceTransColor: 'rgba(88, 166, 244, 0)',
  voiceCancelTransColor: 'rgba(242, 164, 164, 0)',
  voiceTextColor: '#FFF',
  voiceCancelTextColor: '#FF684E',
  voiceRectBgColor: '#FFF',
  voiceCancelRectBgColor: '#EB4C46',
};
