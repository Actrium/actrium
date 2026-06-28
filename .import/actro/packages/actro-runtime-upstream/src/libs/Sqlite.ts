import SQLite, { SQLiteDatabase, ResultSet, Transaction } from 'react-native-sqlite-storage';

// 启用 Promise 支持
SQLite.enablePromise(true);

const database_name = 'AppDatabase.db';

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
   * 通知数据库初始化已完成
   */
  public signalReady() {
    this._resolveReady();
  }

  /**
   * 等待数据库准备就绪
   */
  public async waitReady() {
    return this._readyPromise;
  }

  /**
   * 获取单例
   */
  public static getInstance(): SQLiteService {
    if (!SQLiteService.instance) {
      SQLiteService.instance = new SQLiteService();
    }
    return SQLiteService.instance;
  }

  /**
   * 打开数据库连接
   */
  public async open(): Promise<SQLiteDatabase> {
    if (this.db) return this.db;

    try {
      this.db = await SQLite.openDatabase({
        name: database_name,
        location: 'default', // iOS/iPad 会自动保存到 Documents 目录
      });
      console.log('[SQLite] Database opened successfully');
      return this.db;
    } catch (error) {
      console.error('[SQLite] Failed to open database', error);
      throw error;
    }
  }

  /**
   * 执行 SQL 语句 (增、删、改、查)
   * @param sql SQL字符串
   * @param params 参数数组
   * @param skipWait 是否跳过准备就绪等待（仅在初始化脚本中使用）
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
   * 查询多行数据
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
   * 查询单行数据
   */
  public async selectOne<T>(sql: string, params: any[] = []): Promise<T | null> {
    const results = await this.executeSql(sql, params);
    if (results.rows.length > 0) {
      return results.rows.item(0);
    }
    return null;
  }

  /**
   * 事务处理
   */
  public async transaction(callback: (tx: Transaction) => void): Promise<Transaction> {
    const db = await this.open();
    return db.transaction(callback);
  }

  /**
   * 关闭数据库
   */
  public async close(): Promise<void> {
    if (this.db) {
      await this.db.close();
      this.db = null;
    }
  }
}

export const db = SQLiteService.getInstance();