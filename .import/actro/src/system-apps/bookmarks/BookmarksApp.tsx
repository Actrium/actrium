/**
 * Bookmarks System App - Built-in app similar to chrome://bookmarks
 */
import React from 'react';
import { useWindowDimensions } from 'react-native';
import { useTranslation } from 'react-i18next';
import { isTabletWidth } from '../../utils/device';
import BookmarksPadView from './BookmarksPadView';
import BookmarksPhoneView from './BookmarksPhoneView';
import { useBookmarksPage } from './useBookmarksPage';

export default function BookmarksApp() {
  const { t } = useTranslation();
  const { width } = useWindowDimensions();
  const isPad = isTabletWidth(width);
  const bookmarksPage = useBookmarksPage({ t });

  if (isPad) {
    return <BookmarksPadView {...bookmarksPage} />;
  }

  return <BookmarksPhoneView {...bookmarksPage} />;
}
