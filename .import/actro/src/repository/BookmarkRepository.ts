import { db } from '../libs/Sqlite';

// 1. 书签项类型：对应 bookmark_items 表 + apps 表的关联字段
export type TBookmarkItem = {
  id: number;
  category_id: number;
  app_id: string;
  created_at: number;
  sort_order: number;
  // 以下是 JOIN apps 表后得到的字段
  app_name: string | null; // 对应 apps.app_name
  logo: string | null; // 对应 apps.logo
  last_question: string | null; // 最近一条消息的提问
  last_answer: string | null; // 最近一条消息的回答
};

// 2. 分类分组类型：对应 bookmark_categories 表
export type TBookmarkGroup = {
  id: number;
  title: string;
  sort_order: number;
  items: TBookmarkItem[];
};

export class BookmarkRepository {
  /**
   * 获取所有书签层级 (分类 + 书签项)
   */
  static async getBookmarksHierarchy(): Promise<TBookmarkGroup[]> {
    try {
      // 1. 获取所有分类
      const categories = await db.selectAll<{
        id: number;
        title: string;
        sort_order: number;
      }>(
        'SELECT id, title, sort_order FROM bookmark_categories ORDER BY sort_order ASC'
      );

      // 2. 获取所有书签项 (严格匹配 TBookmarkItem 字段)
      const items = await db.selectAll<TBookmarkItem>(
        `SELECT 
          bi.id, 
          bi.category_id, 
          bi.app_id,
          bi.created_at,
          bi.sort_order,
          a.app_name,
          a.logo,
          (SELECT question_content FROM app_chat_history WHERE app_id = bi.app_id ORDER BY question_time DESC, message_id DESC LIMIT 1) as last_question,
          (SELECT answer_content FROM app_chat_history WHERE app_id = bi.app_id ORDER BY question_time DESC, message_id DESC LIMIT 1) as last_answer
         FROM bookmark_items bi
         LEFT JOIN apps a ON bi.app_id = a.app_id
         ORDER BY bi.sort_order ASC`
      );

      // 3. 组装嵌套结构
      return categories.map((cat) => ({
        ...cat,
        items: items.filter((item) => item.category_id === cat.id),
      }));
    } catch (error) {
      console.error(
        '[BookmarkRepository] Failed to fetch bookmarks hierarchy',
        error
      );
      return [];
    }
  }

  /**
   * 获取纯分类列表
   */
  static async getCategories(): Promise<Omit<TBookmarkGroup, 'items'>[]> {
    try {
      return await db.selectAll<Omit<TBookmarkGroup, 'items'>>(
        'SELECT id, title, sort_order FROM bookmark_categories ORDER BY sort_order ASC'
      );
    } catch (error) {
      console.error('[BookmarkRepository] Failed to fetch categories', error);
      return [];
    }
  }

  /**
   * 添加一个分类
   */
  static async addCategory(title: string, sort_order: number = 0) {
    return await db.executeSql(
      'INSERT INTO bookmark_categories (title, sort_order) VALUES (?, ?)',
      [title, sort_order]
    );
  }

  /**
   * 删除一个分类
   */
  static async deleteCategory(id: number) {
    return await db.executeSql('DELETE FROM bookmark_categories WHERE id = ?', [
      id,
    ]);
  }

  /**
   * 添加一个书签
   */
  static async addBookmark({
    category_id,
    app_id,
    sort_order = 0,
  }: {
    category_id: number;
    app_id: string;
    sort_order?: number;
  }) {
    return await db.executeSql(
      'INSERT INTO bookmark_items (category_id, app_id, sort_order) VALUES (?, ?, ?)',
      [category_id, app_id, sort_order]
    );
  }

  /**
   * 删除一个书签
   */
  static async removeBookmark(id: number) {
    return await db.executeSql('DELETE FROM bookmark_items WHERE id = ?', [id]);
  }
}
