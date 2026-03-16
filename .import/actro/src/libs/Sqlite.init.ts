import { db } from './Sqlite';

/**
 * 初始化数据库表结构
 */
export const initDatabase = async () => {
  try {
    // 0. 删除旧表（开发阶段，确保结构最新）
    await db.executeSql('DROP TABLE IF EXISTS app_chat_history;', [], true);
    await db.executeSql('DROP TABLE IF EXISTS bookmark_items;', [], true);
    await db.executeSql('DROP TABLE IF EXISTS apps;', [], true);
    await db.executeSql('DROP TABLE IF EXISTS bookmark_categories;', [], true);
    await db.executeSql('DROP TABLE IF EXISTS auth;', [], true);

    // 1. 创建书签分类表 (根定义表，不指向外键)
    await db.executeSql(`
      CREATE TABLE IF NOT EXISTS bookmark_categories (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        title TEXT NOT NULL,
        sort_order INTEGER DEFAULT 0
      );
    `, [], true);

    // 2. 创建应用信息表 (根定义表)
    await db.executeSql(`
      CREATE TABLE IF NOT EXISTS apps (
        app_id TEXT PRIMARY KEY,
        app_name TEXT,                  -- 应用名称
        full_description TEXT,          -- 完整描述
        full_title TEXT,                -- 完整标题
        logo TEXT,                      -- 应用图标 (URL 或 Base64)
        url TEXT,                       -- 应用加载来源地址
        details TEXT,                   -- 应用详细信息 (JSON)
        last_opened INTEGER            -- 最近打开时间 (Timestamp)
      );
    `, [], true);

    // 3. 创建书签项表
    await db.executeSql(`
      CREATE TABLE IF NOT EXISTS bookmark_items (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        category_id INTEGER,                -- 指向书签分类表（可为空，表示未分类）
        app_id TEXT NOT NULL,               -- 指向应用信息表
        created_at INTEGER DEFAULT (strftime('%s','now')), -- 创建时间
        sort_order INTEGER DEFAULT 0,
        FOREIGN KEY (category_id) REFERENCES bookmark_categories (id) ON DELETE CASCADE,
        FOREIGN KEY (app_id) REFERENCES apps (app_id) ON DELETE CASCADE
      );
    `, [], true);

    // 4. 创建聊天历史表
    await db.executeSql(`
      CREATE TABLE IF NOT EXISTS app_chat_history (
        message_id TEXT PRIMARY KEY,
        session_id TEXT NOT NULL,       -- 会话 ID
        app_id TEXT NOT NULL,           -- 指向应用信息表
        tab_id TEXT NOT NULL,           -- 所属 Tab ID
        question_time INTEGER,          -- 提问时间
        question_content TEXT,          -- 提问内容
        answer_time INTEGER,            -- 回答时间
        answer_content TEXT,            -- 回答内容
        attachments TEXT,               -- 附件信息 (JSON: images, files, location)
        is_preset INTEGER DEFAULT 0,    -- 是否为预设问题 (0/1)
        is_edited INTEGER DEFAULT 0,    -- 是否编辑过 (0/1)
        is_read INTEGER DEFAULT 0,      -- 是否已读 (0 未读/1 已读)
        progress INTEGER DEFAULT 0,     -- 进度状态 (0 pending/1 processing/2 completed/3 failed/4 canceled/5 overdue)
        FOREIGN KEY (app_id) REFERENCES apps (app_id) ON DELETE CASCADE
      );
    `, [], true);

    // 5. 创建授权信息表 (用于持久化 sessionToken)
    await db.executeSql(`
      CREATE TABLE IF NOT EXISTS auth (
        env TEXT PRIMARY KEY,
        token TEXT,
        updated_at INTEGER
      );
    `, [], true);

    // 6. 为 auth 表添加触发器，以自动维护 updated_at
    await db.executeSql(`
      CREATE TRIGGER IF NOT EXISTS update_auth_updated_at
      AFTER UPDATE ON auth
      BEGIN
        UPDATE auth SET updated_at = (strftime('%s','now') * 1000) WHERE env = OLD.env;
      END;
    `, [], true);

    // 7. 为 app_chat_history 创建索引以优化未读消息查询
    await db.executeSql(`
      CREATE INDEX IF NOT EXISTS idx_chat_history_tab_read
      ON app_chat_history (tab_id, is_read);
    `, [], true);

    console.log('[SQLite] Database tables reset and initialized successfully');
    db.signalReady();
  } catch (error) {
    console.error('[SQLite] Failed to initialize database', error);
    // 即使失败也信号就绪，避免业务层永久挂起，错误会在执行 SQL 时抛出
    db.signalReady();
  }
};
