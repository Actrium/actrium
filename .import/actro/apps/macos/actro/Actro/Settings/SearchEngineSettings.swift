import Foundation

/// Default search engine for address bar queries
struct SearchEngine: RawRepresentable, CaseIterable, Identifiable, Hashable {
    let rawValue: String
    let displayName: String
    let searchUrlPrefix: String

    var id: String { rawValue }

    static let google = SearchEngine(rawValue: "google")!
    static let bing = SearchEngine(rawValue: "bing")!
    static let duckDuckGo = SearchEngine(rawValue: "duckduckgo")!

    static var allCases: [SearchEngine] {
        BrowserSearchEngineCoreBridge.engines().map(SearchEngine.init(info:))
    }

    /// Search URL template — `%s` is replaced with the percent-encoded query
    var searchTemplate: String {
        "\(searchUrlPrefix)%s"
    }

    var icon: String {
        switch rawValue {
        case "bing": return "globe"
        case "duckduckgo": return "shield"
        default: return "magnifyingglass"
        }
    }

    /// Build a search URL for the given query
    func searchURL(for query: String) -> URL? {
        BrowserSearchEngineCoreBridge.searchURL(query: query, engineId: rawValue)
    }

    init?(rawValue: String) {
        self.init(info: BrowserSearchEngineCoreBridge.info(engineId: rawValue))
    }

    private init(info: BrowserSearchEngineCoreInfo) {
        self.rawValue = info.id
        self.displayName = info.displayName
        self.searchUrlPrefix = info.searchUrlPrefix
    }

    // MARK: - UserDefaults persistence

    private static let defaultsKey = "browser.defaultSearchEngine"

    static var current: SearchEngine {
        get {
            let raw = UserDefaults.standard.string(forKey: defaultsKey)
            let normalized = BrowserSearchEngineCoreBridge.normalize(engineId: raw)
            return SearchEngine(rawValue: normalized) ?? .google
        }
        set {
            let normalized = BrowserSearchEngineCoreBridge.normalize(engineId: newValue.rawValue)
            UserDefaults.standard.set(normalized, forKey: defaultsKey)
        }
    }
}
