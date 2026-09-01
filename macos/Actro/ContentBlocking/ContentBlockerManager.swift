import WebKit

/// Manages WebKit content blocking rules for ad blocking.
/// Compiles rules from a bundled JSON file and caches the result.
final class ContentBlockerManager {
    static let shared = ContentBlockerManager()

    /// UserDefaults key for the ad-blocking toggle
    private static let enabledKey = "adBlocking.enabled"

    /// Cached compiled rule list
    private var cachedRuleList: WKContentRuleList?

    /// Whether rules are currently being compiled
    private var isCompiling = false

    /// Pending callbacks waiting for compilation to finish
    private var pendingCallbacks: [(WKContentRuleList?) -> Void] = []

    /// Whether ad blocking is enabled
    var isEnabled: Bool {
        get { UserDefaults.standard.object(forKey: Self.enabledKey) as? Bool ?? true }
        set { UserDefaults.standard.set(newValue, forKey: Self.enabledKey) }
    }

    private init() {}

    /// Load and compile the bundled ad-blocking rules.
    /// Calls completion on the main thread with the compiled rule list, or nil on failure.
    func loadRules(completion: @escaping (WKContentRuleList?) -> Void) {
        // Return cached result immediately
        if let cached = cachedRuleList {
            completion(cached)
            return
        }

        // Queue up callback if already compiling
        if isCompiling {
            pendingCallbacks.append(completion)
            return
        }

        isCompiling = true
        pendingCallbacks.append(completion)

        guard let url = Bundle.main.url(forResource: "adblock-rules", withExtension: "json"),
              let jsonString = try? String(contentsOf: url, encoding: .utf8) else {
            NSLog("ContentBlockerManager: Failed to load adblock-rules.json from bundle")
            finishCompilation(with: nil)
            return
        }

        WKContentRuleListStore.default().compileContentRuleList(
            forIdentifier: "actro-adblock",
            encodedContentRuleList: jsonString
        ) { [weak self] ruleList, error in
            if let error = error {
                NSLog("ContentBlockerManager: Failed to compile rules: \(error.localizedDescription)")
            }
            DispatchQueue.main.async {
                self?.cachedRuleList = ruleList
                self?.finishCompilation(with: ruleList)
            }
        }
    }

    /// Apply compiled rules to a WKWebViewConfiguration's userContentController.
    /// Does nothing if ad blocking is disabled or rules haven't been compiled yet.
    func apply(to configuration: WKWebViewConfiguration) {
        guard isEnabled, let ruleList = cachedRuleList else { return }
        configuration.userContentController.add(ruleList)
    }

    /// Remove all content rules from a configuration's userContentController.
    func remove(from configuration: WKWebViewConfiguration) {
        configuration.userContentController.removeAllContentRuleLists()
    }

    /// Invalidate the cached rule list (e.g., when rules change).
    func invalidateCache() {
        cachedRuleList = nil
    }

    private func finishCompilation(with ruleList: WKContentRuleList?) {
        isCompiling = false
        let callbacks = pendingCallbacks
        pendingCallbacks.removeAll()
        for callback in callbacks {
            callback(ruleList)
        }
    }
}
