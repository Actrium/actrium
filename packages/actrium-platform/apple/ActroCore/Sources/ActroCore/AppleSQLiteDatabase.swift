import Foundation
import SQLite3

public enum AppleSQLiteDatabaseError: Error, CustomStringConvertible {
    case openFailed(path: String, message: String)

    public var description: String {
        switch self {
        case let .openFailed(path, message):
            return "Failed to open SQLite database at \(path): \(message)"
        }
    }
}

public final class AppleSQLiteDatabase {
    public typealias ErrorHandler = (String) -> Void

    private var db: OpaquePointer?
    private let onError: ErrorHandler?

    public init(path: String, onError: ErrorHandler? = nil) throws {
        self.onError = onError

        if sqlite3_open(path, &db) != SQLITE_OK {
            let message = db.map { String(cString: sqlite3_errmsg($0)) } ?? "unknown error"
            throw AppleSQLiteDatabaseError.openFailed(path: path, message: message)
        }
    }

    deinit {
        if let db {
            sqlite3_close(db)
        }
    }

    @discardableResult
    public func execute(_ sql: String, params: [Any?] = []) -> Bool {
        guard let statement = prepare(sql) else { return false }
        defer { sqlite3_finalize(statement) }

        bind(params, to: statement)

        let result = sqlite3_step(statement)
        if result != SQLITE_DONE && result != SQLITE_ROW {
            reportError("Execute failed")
            return false
        }

        return true
    }

    public func executeReturningChanges(_ sql: String, params: [Any?] = []) -> Int64? {
        guard let db, let statement = prepare(sql) else { return nil }
        defer { sqlite3_finalize(statement) }

        bind(params, to: statement)

        let result = sqlite3_step(statement)
        if result != SQLITE_DONE && result != SQLITE_ROW {
            reportError("Execute failed")
            return nil
        }

        return Int64(sqlite3_changes(db))
    }

    @discardableResult
    public func execute(_ sql: String, coreParams: [CoreSqlValueInfo]) -> Bool {
        execute(sql, params: coreParams.map(\.sqliteValue))
    }

    public func query(_ sql: String, params: [Any?] = []) -> [[String: Any]] {
        guard let statement = prepare(sql) else { return [] }
        defer { sqlite3_finalize(statement) }

        bind(params, to: statement)

        var results: [[String: Any]] = []
        let columnCount = sqlite3_column_count(statement)

        while sqlite3_step(statement) == SQLITE_ROW {
            var row: [String: Any] = [:]

            for index in 0..<columnCount {
                let columnName = String(cString: sqlite3_column_name(statement, index))

                switch sqlite3_column_type(statement, index) {
                case SQLITE_INTEGER:
                    row[columnName] = sqlite3_column_int64(statement, index)
                case SQLITE_FLOAT:
                    row[columnName] = sqlite3_column_double(statement, index)
                case SQLITE_TEXT:
                    if let text = sqlite3_column_text(statement, index) {
                        row[columnName] = String(cString: text)
                    }
                case SQLITE_NULL:
                    row[columnName] = NSNull()
                default:
                    break
                }
            }

            results.append(row)
        }

        return results
    }

    public func queryScalar(_ sql: String, params: [Any?] = []) -> Any? {
        query(sql, params: params).first?.values.first
    }

    private func prepare(_ sql: String) -> OpaquePointer? {
        guard let db else { return nil }

        var statement: OpaquePointer?
        guard sqlite3_prepare_v2(db, sql, -1, &statement, nil) == SQLITE_OK else {
            reportError("Prepare failed")
            return nil
        }

        return statement
    }

    private func bind(_ params: [Any?], to statement: OpaquePointer?) {
        for (index, param) in params.enumerated() {
            let sqliteIndex = Int32(index + 1)

            switch param {
            case nil:
                sqlite3_bind_null(statement, sqliteIndex)
            case let value as String:
                sqlite3_bind_text(statement, sqliteIndex, value, -1, unsafeBitCast(-1, to: sqlite3_destructor_type.self))
            case let value as Int64:
                sqlite3_bind_int64(statement, sqliteIndex, value)
            case let value as Int32:
                sqlite3_bind_int64(statement, sqliteIndex, Int64(value))
            case let value as Int:
                sqlite3_bind_int64(statement, sqliteIndex, Int64(value))
            case let value as Double:
                sqlite3_bind_double(statement, sqliteIndex, value)
            case let value as Bool:
                sqlite3_bind_int64(statement, sqliteIndex, value ? 1 : 0)
            default:
                sqlite3_bind_null(statement, sqliteIndex)
            }
        }
    }

    private func reportError(_ prefix: String) {
        guard let db else {
            onError?("\(prefix): database is not open")
            return
        }

        onError?("\(prefix): \(String(cString: sqlite3_errmsg(db)))")
    }
}

private extension CoreSqlValueInfo {
    var sqliteValue: Any? {
        switch kind {
        case "text":
            return text
        case "integer":
            return integer
        case "real":
            return real
        default:
            return nil
        }
    }
}
