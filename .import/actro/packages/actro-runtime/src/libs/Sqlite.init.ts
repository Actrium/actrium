import { db } from './Sqlite';
import {
  RUNTIME_DB_BASE_SCHEMA_STATEMENTS,
  RUNTIME_DB_COLUMNS,
  RUNTIME_DB_MIGRATION_SQL,
  RUNTIME_DB_SCHEMA_VERSION,
  RUNTIME_DB_TABLES,
} from '../actr-host/coreContract';

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
  for (const statement of RUNTIME_DB_BASE_SCHEMA_STATEMENTS) {
    await db.executeSql(statement, [], true);
  }
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
  const appsTableExists = await hasTable(RUNTIME_DB_TABLES.apps);
  if (!appsTableExists) {
    return 0;
  }

  // 历史库没有 user_version，需要通过列结构反推版本。
  const hasInstanceName = await hasColumn(
    RUNTIME_DB_TABLES.apps,
    RUNTIME_DB_COLUMNS.apps.instanceName,
  );
  if (!hasInstanceName) {
    return 1;
  }

  const hasAppStatus = await hasColumn(
    RUNTIME_DB_TABLES.apps,
    RUNTIME_DB_COLUMNS.apps.appStatus,
  );
  if (!hasAppStatus) {
    return 3;
  }

  return RUNTIME_DB_SCHEMA_VERSION;
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
    const instanceNameExists = await hasColumn(
      RUNTIME_DB_TABLES.apps,
      RUNTIME_DB_COLUMNS.apps.instanceName,
    );
    if (!instanceNameExists) {
      await db.executeSql(
        RUNTIME_DB_MIGRATION_SQL.addAppsInstanceNameColumn,
        [],
        true,
      );
    }
    currentVersion = 2;
    await setUserVersion(currentVersion);
  }

  if (currentVersion < 3) {
    // 旧库可能已经存在同一 app_id 的重复书签，
    // 先保留最早一条记录并删除其余重复项，再补唯一索引，避免升级时建索引失败。
    await db.executeSql(
      RUNTIME_DB_MIGRATION_SQL.removeDuplicateBookmarkItemsByAppId,
      [],
      true,
    );
    await db.executeSql(
      RUNTIME_DB_MIGRATION_SQL.createUniqueBookmarkItemsAppIdIndex,
      [],
      true,
    );
    currentVersion = 3;
    await setUserVersion(currentVersion);
  }

  if (currentVersion < 4) {
    // 为历史 apps 表补齐应用状态字段，旧数据默认回填为 normal。
    const appStatusExists = await hasColumn(
      RUNTIME_DB_TABLES.apps,
      RUNTIME_DB_COLUMNS.apps.appStatus,
    );
    if (!appStatusExists) {
      await db.executeSql(
        RUNTIME_DB_MIGRATION_SQL.addAppsAppStatusColumn,
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
