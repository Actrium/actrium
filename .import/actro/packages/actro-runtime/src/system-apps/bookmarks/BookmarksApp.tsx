/**
 * Bookmarks System App - Built-in app similar to chrome://bookmarks
 */
import { useTranslation } from 'react-i18next';
import { isTablet } from '../../utils/device';
import BookmarksPadView from './BookmarksPadView';
import BookmarksPhoneView from './BookmarksPhoneView';
import { useBookmarksPage } from './useBookmarksPage';

export default function BookmarksApp() {
  const { t } = useTranslation();

  const bookmarksPage = useBookmarksPage({ t });

  if (isTablet) {
    return <BookmarksPadView {...bookmarksPage} />;
  }

  return <BookmarksPhoneView {...bookmarksPage} />;
}
