import { db } from '../libs/Sqlite';
import { RUNTIME_DB_QUERIES } from '../actr-host/coreContract';

// 1. Bookmark Type: Corresponding bookmark_items Table + apps Table Associated Fields
export type TBookmarkItem = {
  id: number;
  source?: 'runtime' | 'browser';
  category_id: number | null;
  app_id: string;
  created_at: number;
  sort_order: number;
  // Here's what I'm talking about. JOIN apps Fields received after tables
  app_name: string | null; // Correspond apps.app_name
  instance_name: string | null; // Correspond apps.instance_name
  app_status: 'alive' | 'dead' | null; // Correspond apps.app_status
  logo: string | null; // Correspond apps.logo
  url: string | null;  // Correspond apps.url
  last_question: string | null; // The question of the latest message.
  last_answer: string | null; // The answer to the latest message
};

// 2. Group type: Corresponding bookmark_categories Table
export type TBookmarkGroup = {
  id: number;
  title: string;
  sort_order: number;
  items: TBookmarkItem[];
};

export class BookmarkRepository {
  /**
   * Get All Bookmark Levels (Classification + Bookmarks)
   * Include unclassified bookmarks (%2)category_id Yes null)
   */
  static async getBookmarksHierarchy(): Promise<TBookmarkGroup[]> {
    try {
      // 1. Get All Categories
      const categories = await db.selectAll<{
        id: number;
        title: string;
        sort_order: number;
      }>(
        RUNTIME_DB_QUERIES.bookmarks.selectCategories
      );

      // 2. Get All Bookmarks (Strict Match TBookmarkItem Fields)
      const items = await db.selectAll<TBookmarkItem>(
        RUNTIME_DB_QUERIES.bookmarks.selectItemsWithAppInfo
      );

      // 3. Embedding nesting structures
      const result = categories.map((cat) => ({
        ...cat,
        items: items.filter((item) => item.category_id === cat.id),
      }));

      // 4. Add unclassified bookmarks as a special grouping (if any)
      const uncategorizedItems = items.filter((item) => item.category_id === null);
      if (uncategorizedItems.length > 0) {
        result.push({
          id: -1, // Special ID Not classified
          title: 'Not classified',
          sort_order: 999999,
          items: uncategorizedItems,
        });
      }

      return result;
    } catch (error) {
      console.error(
        '[BookmarkRepository] Failed to fetch bookmarks hierarchy',
        error
      );
      return [];
    }
  }

  /**
   * Fetch Pure Class List
   */
  static async getCategories(): Promise<Omit<TBookmarkGroup, 'items'>[]> {
    try {
      return await db.selectAll<Omit<TBookmarkGroup, 'items'>>(
        RUNTIME_DB_QUERIES.bookmarks.selectCategories
      );
    } catch (error) {
      console.error('[BookmarkRepository] Failed to fetch categories', error);
      return [];
    }
  }

  /**
   * Add a Category
   */
  static async addCategory(title: string, sort_order: number = 0) {
    return await db.executeSql(
      RUNTIME_DB_QUERIES.bookmarks.insertCategory,
      [title, sort_order]
    );
  }

  /**
   * Remove a Category
   */
  static async deleteCategory(id: number) {
    return await db.executeSql(RUNTIME_DB_QUERIES.bookmarks.deleteCategoryById, [
      id,
    ]);
  }

  /**
   * Add a bookmark
   * category_id Optional, is empty for unclassified
   */
  static async addBookmark({
    app_id,
    category_id,
    sort_order = 0,
  }: {
    app_id: string;
    category_id?: number | null;
    sort_order?: number;
  }) {
    return await db.executeSql(
      RUNTIME_DB_QUERIES.bookmarks.insertBookmark,
      [category_id ?? null, app_id, sort_order]
    );
  }

  /**
   * Remove a bookmark (key-click)
   */
  static async removeBookmarkById(id: number) {
    return await db.executeSql(RUNTIME_DB_QUERIES.bookmarks.deleteBookmarkById, [id]);
  }

  /**
   * Press app_id Delete Bookmarks
   */
  static async removeBookmarkByAppId(appId: string) {
    return await db.executeSql(RUNTIME_DB_QUERIES.bookmarks.deleteBookmarkByAppId, [appId]);
  }

  /**
   * Based on app_id Query Collection Items
   */
  static async getBookmarkByAppId(appId: string): Promise<TBookmarkItem | null> {
    try {
      const rows = await db.selectAll<TBookmarkItem>(
        RUNTIME_DB_QUERIES.bookmarks.selectBookmarkByAppId,
        [appId],
      );
      return rows[0] ?? null;
    } catch (error) {
      console.error('[BookmarkRepository] Failed to get bookmark by appId', error);
      return null;
    }
  }
}
