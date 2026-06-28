import Foundation

/// Default search engine for address bar queries
enum SearchEngine: String, CaseIterable, Identifiable {
    case google
    case bing
    case duckDuckGo

    var id: String { rawValue }

    var displayName: String {
        switch self {
        case .google: return "Google"
        case .bing: return "Bing"
        case .duckDuckGo: return "DuckDuckGo"
        }
    }

    /// Search URL template — `%s` is replaced with the percent-encoded query
    var searchTemplate: String {
        switch self {
        case .google: return "https://www.google.com/search?q=%s"
        case .bing: return "https://www.bing.com/search?q=%s"
        case .duckDuckGo: return "https://duckduckgo.com/?q=%s"
        }
    }

    var icon: String {
        switch self {
        case .google: return "magnifyingglass"
        case .bing: return "globe"
        case .duckDuckGo: return "shield"
        }
    }

    /// Build a search URL for the given query
    func searchURL(for query: String) -> URL? {
        guard let encoded = query.addingPercentEncoding(withAllowedCharacters: .urlQueryAllowed) else {
            return nil
        }
        let urlString = searchTemplate.replacingOccurrences(of: "%s", with: encoded)
        return URL(string: urlString)
    }

    // MARK: - UserDefaults persistence

    private static let defaultsKey = "browser.defaultSearchEngine"

    static var current: SearchEngine {
        get {
            guard let raw = UserDefaults.standard.string(forKey: defaultsKey),
                  let engine = SearchEngine(rawValue: raw) else {
                return .google
            }
            return engine
        }
        set {
            UserDefaults.standard.set(newValue.rawValue, forKey: defaultsKey)
        }
    }
}
