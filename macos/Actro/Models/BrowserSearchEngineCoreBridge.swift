import Foundation
import ActroCore

struct BrowserSearchEngineCoreInfo: Hashable {
    let id: String
    let displayName: String
    let searchUrlPrefix: String
}

enum BrowserSearchEngineCoreBridge {
    static func defaultId() -> String {
        browserSearchEngineDefaultId()
    }

    static func engines() -> [BrowserSearchEngineCoreInfo] {
        browserSearchEngines().map(BrowserSearchEngineCoreInfo.init(coreInfo:))
    }

    static func info(engineId: String?) -> BrowserSearchEngineCoreInfo {
        BrowserSearchEngineCoreInfo(coreInfo: browserSearchEngineInfo(engineId: engineId))
    }

    static func normalize(engineId: String?) -> String {
        browserSearchEngineNormalizeId(engineId: engineId)
    }

    static func searchURL(query: String, engineId: String?) -> URL? {
        URL(string: browserSearchUrl(query: query, engineId: engineId))
    }
}

private extension BrowserSearchEngineCoreInfo {
    init(coreInfo: BrowserSearchEngineInfo) {
        self.id = coreInfo.id
        self.displayName = coreInfo.displayName
        self.searchUrlPrefix = coreInfo.searchUrlPrefix
    }
}
