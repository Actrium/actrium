import { isIOS } from './device';
import { isTablet } from './device';

export interface HostLayoutTokens {
  isTablet: boolean;
  contentMaxWidth: number;
  messageHorizontalPadding: number;
  chatListBottomPadding: number;
  inputHorizontalPadding: number;
  inputVerticalPadding: number;
  deleteButtonBottom: number;
  chatEmptyTitleSize: number;
  loginCardMaxWidth: number;
  loginSpacer: number;
  loginTitleSize: number;
  loginHeaderTop: number;
  loginOverlayHorizontalPadding: number;
}

export function getHostLayoutTokens(width: number): HostLayoutTokens {
  return {
    isTablet,
    contentMaxWidth: isTablet ? 750 : width,
    messageHorizontalPadding: isTablet ? 20 : 12,
    chatListBottomPadding: isTablet ? 40 : 24,
    inputHorizontalPadding: isTablet ? 16 : 12,
    inputVerticalPadding: isTablet ? 20 : 14,
    deleteButtonBottom: isTablet ? 126 : isIOS ? 88 : 74,
    chatEmptyTitleSize: isTablet ? 60 : 36,
    loginCardMaxWidth: isTablet ? 400 : Math.min(width - 32, 360),
    loginSpacer: isTablet ? 100 : 44,
    loginTitleSize: isTablet ? 30 : 24,
    loginHeaderTop: isTablet ? 44 : 24,
    loginOverlayHorizontalPadding: isTablet ? 22 : 16,
  };
}

export function getBookmarkColumns(width: number): number {
  if (width >= 1024) return 4;
  if (width >= 768) return 3;
  if (width >= 420) return 2;
  return 1;
}
