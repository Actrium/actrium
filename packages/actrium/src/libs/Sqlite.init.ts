import { db } from './Sqlite';
import {
  RUNTIME_DB_BASE_SCHEMA_STATEMENTS,
  RUNTIME_DB_COLUMNS,
  RUNTIME_DB_MIGRATION_SQL,
  RUNTIME_DB_SCHEMA_VERSION,
  RUNTIME_DB_TABLES,
} from '../runtime-host/coreContract';

type UserVersionRow = {
  user_version: number;
};

type TableInfoRow = {
  name: string;
};

/**
 * Execute queries directly at initialization, avoiding in- waitReady Lock yourself up.
 */
const selectOneDuringInit = async <T>(sql: string, params: any[] = []) => {
  const result = await db.executeSql(sql, params, true);
  if (result.rows.length > 0) {
    return result.rows.item(0) as T;
  }
  return null;
};

/**
 * Base table structure required to create the current version
 */
const createBaseSchema = async () => {
  for (const statement of RUNTIME_DB_BASE_SCHEMA_STATEMENTS) {
    await db.executeSql(statement, [], true);
  }
};

/**
 * Access to database schema Version
 */
const getUserVersion = async () => {
  const row = await selectOneDuringInit<UserVersionRow>('PRAGMA user_version;', []);
  return row?.user_version ?? 0;
};

/**
 * Update Database schema Version
 */
const setUserVersion = async (version: number) => {
  await db.executeSql(`PRAGMA user_version = ${version};`, [], true);
};

/**
 * Whether the table exists or not
 */
const hasTable = async (tableName: string) => {
  const row = await selectOneDuringInit<TableInfoRow>(
    `SELECT name FROM sqlite_master WHERE type = 'table' AND name = ?;`,
    [tableName]
  );
  return Boolean(row?.name);
};

/**
 * Whether the assigned column is in the judgement table
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
 * Recognize historical database version unwritten version number
 */
const detectLegacyVersion = async () => {
  const appsTableExists = await hasTable(RUNTIME_DB_TABLES.apps);
  if (!appsTableExists) {
    return 0;
  }

  // No history. user_version, a reverse version of the column structure is required.
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
 * Run migration in the order of the version
 */
const runMigrations = async (fromVersion: number) => {
  let currentVersion = fromVersion;

  if (currentVersion < 1) {
    await createBaseSchema();
    currentVersion = 1;
    await setUserVersion(currentVersion);
  }

  if (currentVersion < 2) {
    // As Existing apps Tables add examples of name fields to accommodate old library upgrades.
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
    // The old vault may already be the same. app_id The duplicate bookmarks,
    // First, save the earliest record and remove the remaining duplicate entries and then fill the only index to avoid failure in indexing when you upgrade.
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
    // For history apps Tables fill in status fields, old data default returns to normal.
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

  // Do it every time you start, and do it once. schema, to ensure that the index and trigger exists.
  await createBaseSchema();
};

/**
 * Initialize database table structure
 */
export const initDatabase = async () => {
  try {
    // 0. Delete old table (development phase to ensure up-to-date structure)
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
    // Fields state:
    // local_file_uri: Local File URI, for the player to use directly
    // local_file_path: Absolute path to local files for the file existential check and delete
    // duration_ms: Voice time, milliseconds
    // mime_type: Documentation MIME Type, Current Voice Message Unifiedly to audio/wav
    // file_size: File size, bytes in units
    // sample_rate: Sample Rate, Current Default 16000
    // channels: Channel Number, Current Default 1(Unional channel)
    // bits_per_sample: Depth of each sample point, current default 16
    // status: Local voice file status,recording The video is also available on Facebook.ready Means that the drop-down drive can be played

    let currentVersion = await getUserVersion();
    if (currentVersion === 0) {
      currentVersion = await detectLegacyVersion();
    }

    await runMigrations(currentVersion);

    console.log('[SQLite] Database schema initialized successfully');
    db.signalReady();
  } catch (error) {
    console.error('[SQLite] Failed to initialize database', error);
    // Even if it fails, the signal is ready, and the business level is not hanging up. SQL Time Throw
    db.signalReady();
  }
};
