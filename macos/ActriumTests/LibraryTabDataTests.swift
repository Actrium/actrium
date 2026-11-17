import Foundation
import Testing
@testable import Actrium

struct LibraryTabDataTests {
    @Test func historyIncludesOlderVisitsAndPreservesAddresses() throws {
        let packageURL = try #require(ActriumAppURLScheme.appURL(filePath: "/tmp/front.actr"))
        let items = (0..<12).map { index in
            HistoryItem(
                title: "Visit \(index)",
                appURL: index == 11 ? packageURL : URL(string: "https://example.com/\(index)"),
                visitTime: Date(timeIntervalSince1970: TimeInterval(100 - index))
            )
        }
        let config = LibraryTabData.configuration(for: .history, history: items, bookmarks: [])
        let rows = try #require(config["historyItems"] as? [[String: Any]])

        #expect(rows.count == 12)
        #expect(rows.first?["id"] as? String == items.first?.id.uuidString)
        #expect(rows.last?["appURL"] as? String == packageURL.absoluteString)
        #expect(rows.last?["visitTime"] as? Int == 89_000)
        #expect(config["bookmarks"] == nil)
    }

    @Test func onlyMatchingSystemPageReceivesItsList() throws {
        let bookmark = Bookmark(
            title: "Example",
            appURL: URL(string: "https://example.com/path"),
            createdAt: Date(timeIntervalSince1970: 123)
        )
        let history = [HistoryItem(title: "Private visit")]
        let config = LibraryTabData.configuration(for: .bookmarks, history: history, bookmarks: [bookmark])
        let rows = try #require(config["bookmarks"] as? [[String: Any]])

        #expect(rows.first?["id"] as? String == bookmark.id.uuidString)
        #expect(rows.first?["appURL"] as? String == "https://example.com/path")
        #expect(rows.first?["createdAt"] as? Int == 123_000)
        #expect(config["historyItems"] == nil)
        for type: SystemTabType? in [nil, .debug, .startPage] {
            #expect(LibraryTabData.configuration(for: type, history: history, bookmarks: [bookmark]).isEmpty)
        }
    }

    @Test func emptySnapshotsReplacePreviouslyDisplayedItems() {
        let history = LibraryTabData.configuration(for: .history, history: [], bookmarks: [])
        let bookmarks = LibraryTabData.configuration(for: .bookmarks, history: [], bookmarks: [])

        #expect((history["historyItems"] as? [[String: Any]])?.isEmpty == true)
        #expect((bookmarks["bookmarks"] as? [[String: Any]])?.isEmpty == true)
    }

    @Test func historySnapshotsBoundEachPayloadAndClampStalePageIndices() throws {
        let pageSize = LibraryTabData.historyPageSize
        let history = (0..<(pageSize * 2 + 1)).map { HistoryItem(title: "Visit \($0)") }

        let first = LibraryTabData.configuration(for: .history, history: history, bookmarks: [])
        let middle = LibraryTabData.configuration(for: .history, history: history, bookmarks: [], historyPage: 1)
        let last = LibraryTabData.configuration(for: .history, history: history, bookmarks: [], historyPage: Int.max)
        let firstRows = try #require(first["historyItems"] as? [[String: Any]])
        let middleRows = try #require(middle["historyItems"] as? [[String: Any]])
        let lastRows = try #require(last["historyItems"] as? [[String: Any]])

        #expect(firstRows.count == pageSize)
        #expect(middleRows.count == pageSize)
        #expect(middleRows.first?["id"] as? String == history[pageSize].id.uuidString)
        #expect(lastRows.map { $0["id"] as? String } == [history.last?.id.uuidString])
        #expect(last["historyPage"] as? [String: Int] == ["index": 2, "count": 3])
    }
}
