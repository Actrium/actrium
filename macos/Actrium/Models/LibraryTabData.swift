import Foundation

enum LibraryTabData {
    static let historyPageSize = 200

    static func historyPageCount(itemCount: Int) -> Int {
        max(0, itemCount - 1) / historyPageSize + 1
    }

    static func configuration(
        for type: SystemTabType?,
        history: [HistoryItem],
        bookmarks: [Bookmark],
        historyPage: Int = 0
    ) -> [String: Any] {
        switch type {
        case .history:
            let count = historyPageCount(itemCount: history.count)
            let index = min(max(0, historyPage), count - 1)
            let start = index * historyPageSize
            let end = min(start + historyPageSize, history.count)
            return [
                "historyPage": ["index": index, "count": count],
                "historyItems": history[start..<end].map { item in [
                    "id": item.id.uuidString,
                    "title": item.title,
                    "appURL": item.appURL?.absoluteString ?? "",
                    "visitTime": Int(item.visitTime.timeIntervalSince1970 * 1000)
                ] },
            ]
        case .bookmarks:
            return ["bookmarks": bookmarks.map { item in
                [
                    "id": item.id.uuidString,
                    "title": item.title,
                    "appURL": item.appURL?.absoluteString ?? "",
                    "createdAt": Int(item.createdAt.timeIntervalSince1970 * 1000)
                ]
            }]
        default:
            return [:]
        }
    }
}
