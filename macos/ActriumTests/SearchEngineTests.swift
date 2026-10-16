import Foundation
import Testing
@testable import Actrium

struct SearchEngineTests {

    // MARK: - Display Name

    @Test func displayName_google() {
        #expect(SearchEngine.google.displayName == "Google")
    }

    @Test func displayName_bing() {
        #expect(SearchEngine.bing.displayName == "Bing")
    }

    @Test func displayName_duckDuckGo() {
        #expect(SearchEngine.duckDuckGo.displayName == "DuckDuckGo")
    }

    // MARK: - Search URL Generation

    @Test func searchURL_google_encodesQuery() {
        let url = SearchEngine.google.searchURL(for: "hello world")
        #expect(url?.absoluteString == "https://www.google.com/search?q=hello%20world")
    }

    @Test func searchURL_bing_encodesQuery() {
        let url = SearchEngine.bing.searchURL(for: "swift testing")
        #expect(url?.absoluteString == "https://www.bing.com/search?q=swift%20testing")
    }

    @Test func searchURL_duckDuckGo_encodesQuery() {
        let url = SearchEngine.duckDuckGo.searchURL(for: "privacy browser")
        #expect(url?.absoluteString == "https://duckduckgo.com/?q=privacy%20browser")
    }

    @Test func searchURL_encodesSpecialCharacters() {
        let url = SearchEngine.google.searchURL(for: "a&b=c")
        #expect(url != nil)
        #expect(url!.absoluteString.contains("a%26b%3Dc") || url!.absoluteString.contains("a&b=c"))
    }

    @Test func searchURL_emptyQuery() {
        let url = SearchEngine.google.searchURL(for: "")
        #expect(url?.absoluteString == "https://www.google.com/search?q=")
    }

    @Test func searchURL_chineseCharacters() {
        let url = SearchEngine.google.searchURL(for: "Hello.")
        #expect(url != nil)
        #expect(url!.absoluteString.hasPrefix("https://www.google.com/search?q="))
    }

    // MARK: - CaseIterable

    @Test func allCases_containsThreeEngines() {
        #expect(SearchEngine.allCases.count == 3)
    }

    // MARK: - Identifiable

    @Test func id_matchesRawValue() {
        for engine in SearchEngine.allCases {
            #expect(engine.id == engine.rawValue)
        }
    }

    // MARK: - Search Template

    @Test func searchTemplate_containsPlaceholder() {
        for engine in SearchEngine.allCases {
            #expect(engine.searchTemplate.contains("%s"))
        }
    }
}
