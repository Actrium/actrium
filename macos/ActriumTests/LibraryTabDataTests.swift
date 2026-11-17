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
        let rows = try #require(config["historyItems"])

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
        let rows = try #require(config["bookmarks"])

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

        #expect(history["historyItems"]?.isEmpty == true)
        #expect(bookmarks["bookmarks"]?.isEmpty == true)
    }
}
