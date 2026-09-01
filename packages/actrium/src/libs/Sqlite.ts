import SQLite, { SQLiteDatabase, ResultSet, Transaction } from 'react-native-sqlite-storage';
import { RUNTIME_DATABASE_NAME } from '../runtime-host/coreContract';

// Enable Promise Support
SQLite.enablePromise(true);

const database_name = RUNTIME_DATABASE_NAME;

export class SQLiteService {
  private static instance: SQLiteService;
  private db: SQLiteDatabase | null = null;
  private _readyPromise: Promise<void>;
  private _resolveReady!: () => void;

  private constructor() {
    this._readyPromise = new Promise((resolve) => {
      this._resolveReady = resolve;
    });
  }

  /**
   * Initialization of notification database completed
   */
  public signalReady() {
    this._resolveReady();
  }

  /**
   * Waiting for database to be ready
   */
  public async waitReady() {
    return this._readyPromise;
  }

  /**
   * Get Single Cases
   */
  public static getInstance(): SQLiteService {
    if (!SQLiteService.instance) {
      SQLiteService.instance = new SQLiteService();
    }
    return SQLiteService.instance;
  }

  /**
   * Open database connection
   */
  public async open(): Promise<SQLiteDatabase> {
    if (this.db) return this.db;

    try {
      this.db = await SQLite.openDatabase({
        name: database_name,
        location: 'default', // iOS/iPad Save automatically to Documents Contents
      });
      console.log('[SQLite] Database opened successfully');
      return this.db;
    } catch (error) {
      console.error('[SQLite] Failed to open database', error);
      throw error;
    }
  }

  /**
   * Implementation SQL Statement (Add, delete, modify, check)
   * @param sql SQLString
   * @param params Arguments array
   * @param skipWait Skip readiness waiting (used only in initialized script)
   */
  public async executeSql(sql: string, params: any[] = [], skipWait: boolean = false): Promise<ResultSet> {
    if (!skipWait) {
      await this.waitReady();
    }
    
    const db = await this.open();
    try {
      const [results] = await db.executeSql(sql, params);
      return results;
    } catch (error) {
      console.error(`[SQLite] Error executing SQL: ${sql}`, error);
      throw error;
    }
  }

  /**
   * Query multiline data
   */
  public async selectAll<T>(sql: string, params: any[] = []): Promise<T[]> {
    const results = await this.executeSql(sql, params);
    const data: T[] = [];
    for (let i = 0; i < results.rows.length; i++) {
      data.push(results.rows.item(i));
    }
    return data;
  }

  /**
   * Query single-line data
   */
  public async selectOne<T>(sql: string, params: any[] = []): Promise<T | null> {
    const results = await this.executeSql(sql, params);
    if (results.rows.length > 0) {
      return results.rows.item(0);
    }
    return null;
  }

  /**
   * Conduct of business
   */
  public async transaction(callback: (tx: Transaction) => void): Promise<Transaction> {
    const db = await this.open();
    return db.transaction(callback);
  }

  /**
   * Close Database
   */
  public async close(): Promise<void> {
    if (this.db) {
      await this.db.close();
      this.db = null;
    }
  }
}

export const db = SQLiteService.getInstance();
