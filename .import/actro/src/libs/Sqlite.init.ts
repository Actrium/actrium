import { db } from './Sqlite';

const LATEST_DB_VERSION = 4;

type UserVersionRow = {
  user_version: number;
};

type TableInfoRow = {
  name: string;
};

/**
 * 初始化阶段直接执行查询，避免在 waitReady 上自锁
 */
const selectOneDuringInit = async <T>(sql: string, params: any[] = []) => {
  const result = await db.executeSql(sql, params, true);
  if (result.rows.length > 0) {
    return result.rows.item(0) as T;
  }
  return null;
};

/**
 * 创建当前版本所需的基础表结构
 */
const createBaseSchema = async () => {
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
      app_status TEXT DEFAULT 'alive', -- 应用可用状态 (alive/dead)
      app_name TEXT,                  -- 应用名称
      instance_name TEXT,             -- 实例名称
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

  await db.executeSql(`
    CREATE UNIQUE INDEX IF NOT EXISTS idx_bookmark_items_app_id_unique
    ON bookmark_items (app_id);
  `, [], true);

  // 4. 创建聊天历史表
  await db.executeSql(`
    CREATE TABLE IF NOT EXISTS app_chat_history (
      message_id TEXT PRIMARY KEY,
      session_id TEXT NOT NULL,       -- 会话 ID
      app_id TEXT NOT NULL,           -- 指向应用信息表
      tab_id TEXT NOT NULL,           -- 所属 Tab ID
      question_type TEXT DEFAULT 'text',          -- 提问类型 (text/voice)
      question_time INTEGER,                      -- 提问时间
      question_content TEXT,                      -- 提问内容
      question_voice_payload TEXT,                -- 提问语音元数据 (JSON，对应 IVoicePayload：{ local_file_uri, local_file_path, duration_ms, mime_type, file_size, sample_rate, channels, bits_per_sample, status })
      answer_type TEXT DEFAULT 'text',            -- 回答类型 (text/voice)
      answer_time INTEGER,                        -- 回答时间
      answer_content TEXT,                        -- 回答内容
      answer_voice_payload TEXT,                  -- 回答语音元数据 (JSON，对应 IVoicePayload：{ local_file_uri, local_file_path, duration_ms, mime_type, file_size, sample_rate, channels, bits_per_sample, status })
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
};

/**
 * 获取数据库 schema 版本
 */
const getUserVersion = async () => {
  const row = await selectOneDuringInit<UserVersionRow>('PRAGMA user_version;', []);
  return row?.user_version ?? 0;
};

/**
 * 更新数据库 schema 版本
 */
const setUserVersion = async (version: number) => {
  await db.executeSql(`PRAGMA user_version = ${version};`, [], true);
};

/**
 * 判断表是否存在
 */
const hasTable = async (tableName: string) => {
  const row = await selectOneDuringInit<TableInfoRow>(
    `SELECT name FROM sqlite_master WHERE type = 'table' AND name = ?;`,
    [tableName]
  );
  return Boolean(row?.name);
};

/**
 * 判断表中是否存在指定列
 */
const hasColumn = async (tableName: string, columnName: string) => {
  const result = await db.executeSql(`PRAGMA table_info(${tableName});`, [], true);
  for (let i = 0; i < result.rows.length; i += 1) {
    const row = result.rows.item(i) as { name: string };
    if (row.name === columnName) {
      return true;
    }
  }
  return false;
};

/**
 * 识别未写入版本号的历史数据库版本
 */
const detectLegacyVersion = async () => {
  const appsTableExists = await hasTable('apps');
  if (!appsTableExists) {
    return 0;
  }

  // 历史库没有 user_version，需要通过列结构反推版本。
  const hasInstanceName = await hasColumn('apps', 'instance_name');
  if (!hasInstanceName) {
    return 1;
  }

  const hasAppStatus = await hasColumn('apps', 'app_status');
  if (!hasAppStatus) {
    return 3;
  }

  return LATEST_DB_VERSION;
};

/**
 * 按版本顺序执行迁移
 */
const runMigrations = async (fromVersion: number) => {
  let currentVersion = fromVersion;

  if (currentVersion < 1) {
    await createBaseSchema();
    currentVersion = 1;
    await setUserVersion(currentVersion);
  }

  if (currentVersion < 2) {
    // 为已有 apps 表补充实例名称字段，兼容旧库升级。
    const instanceNameExists = await hasColumn('apps', 'instance_name');
    if (!instanceNameExists) {
      await db.executeSql('ALTER TABLE apps ADD COLUMN instance_name TEXT;', [], true);
    }
    currentVersion = 2;
    await setUserVersion(currentVersion);
  }

  if (currentVersion < 3) {
    // 旧库可能已经存在同一 app_id 的重复书签，
    // 先保留最早一条记录并删除其余重复项，再补唯一索引，避免升级时建索引失败。
    await db.executeSql(
      `
        DELETE FROM bookmark_items
        WHERE id NOT IN (
          SELECT MIN(id)
          FROM bookmark_items
          GROUP BY app_id
        );
      `,
      [],
      true,
    );
    await db.executeSql(
      `
        CREATE UNIQUE INDEX IF NOT EXISTS idx_bookmark_items_app_id_unique
        ON bookmark_items (app_id);
      `,
      [],
      true,
    );
    currentVersion = 3;
    await setUserVersion(currentVersion);
  }

  if (currentVersion < 4) {
    // 为历史 apps 表补齐应用状态字段，旧数据默认回填为 normal。
    const appStatusExists = await hasColumn('apps', 'app_status');
    if (!appStatusExists) {
      await db.executeSql(
        `ALTER TABLE apps ADD COLUMN app_status TEXT DEFAULT 'alive';`,
        [],
        true,
      );
    }
    currentVersion = 4;
    await setUserVersion(currentVersion);
  }

  // 每次启动都兜底执行一次当前 schema，保证索引和触发器存在。
  await createBaseSchema();
};

/**
 * 初始化数据库表结构
 */
export const initDatabase = async () => {
  try {
    // 0. 删除旧表（开发阶段，确保结构最新）
    // await db.executeSql('DROP TABLE IF EXISTS app_chat_history;', [], true);
    // await db.executeSql('DROP TABLE IF EXISTS bookmark_categories;', [], true);
    // await db.executeSql('DROP TABLE IF EXISTS bookmark_items;', [], true);
    // await db.executeSql('DROP TABLE IF EXISTS apps;', [], true);
    // await db.executeSql('DROP TABLE IF EXISTS auth;', [], true);

    // question_voice_payload / answer_voice_payload mock:
    // {
    //   "local_file_uri": "file:///var/mobile/Containers/Data/Application/xxx/Documents/actr-voices/msg-123-answer.wav",
    //   "local_file_path": "/var/mobile/Containers/Data/Application/xxx/Documents/actr-voices/msg-123-answer.wav",
    //   "duration_ms": 3200,
    //   "mime_type": "audio/wav",
    //   "file_size": 102444,
    //   "sample_rate": 16000,
    //   "channels": 1,
    //   "bits_per_sample": 16,
    //   "status": "ready"
    // }
    // 字段说明：
    // local_file_uri: 本地文件 URI，供播放器直接使用
    // local_file_path: 本地文件绝对路径，供文件存在性检查和删除使用
    // duration_ms: 语音时长，单位毫秒
    // mime_type: 文件 MIME 类型，当前语音消息统一为 audio/wav
    // file_size: 文件大小，单位字节
    // sample_rate: 采样率，当前默认 16000
    // channels: 声道数，当前默认 1（单声道）
    // bits_per_sample: 每个采样点的位深，当前默认 16
    // status: 本地语音文件状态，recording 表示录制中，ready 表示已落盘可播放

    let currentVersion = await getUserVersion();
    if (currentVersion === 0) {
      currentVersion = await detectLegacyVersion();
    }

    await runMigrations(currentVersion);

    console.log('[SQLite] Database schema initialized successfully');
    db.signalReady();
  } catch (error) {
    console.error('[SQLite] Failed to initialize database', error);
    // 即使失败也信号就绪，避免业务层永久挂起，错误会在执行 SQL 时抛出
    db.signalReady();
  }
};
