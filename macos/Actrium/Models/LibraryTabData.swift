import Foundation

enum LibraryTabData {
    static func configuration(
        for type: SystemTabType?,
        history: [HistoryItem],
        bookmarks: [Bookmark]
    ) -> [String: [[String: Any]]] {
        switch type {
        case .history:
            return ["historyItems": history.map { item in
                [
                    "id": item.id.uuidString,
                    "title": item.title,
                    "appURL": item.appURL?.absoluteString ?? "",
                    "visitTime": Int(item.visitTime.timeIntervalSince1970 * 1000)
                ]
            }]
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
