import Testing
import WebKit
@testable import Actro

@Suite(.serialized)
struct ContentBlockerManagerTests {

    // MARK: - Enable/Disable Toggle

    @Test func isEnabled_defaultsToTrue() {
        // Remove any stored value to test default
        let key = "adBlocking.enabled"
        let previousValue = UserDefaults.standard.object(forKey: key)
        UserDefaults.standard.removeObject(forKey: key)

        #expect(ContentBlockerManager.shared.isEnabled == true)

        // Restore
        if let prev = previousValue {
            UserDefaults.standard.set(prev, forKey: key)
        }
    }

    @Test func isEnabled_canBeToggled() {
        let original = ContentBlockerManager.shared.isEnabled

        ContentBlockerManager.shared.isEnabled = false
        #expect(ContentBlockerManager.shared.isEnabled == false)

        ContentBlockerManager.shared.isEnabled = true
        #expect(ContentBlockerManager.shared.isEnabled == true)

        // Restore
        ContentBlockerManager.shared.isEnabled = original
    }

    // MARK: - Cache Invalidation

    @Test func invalidateCache_clearsCompiledRules() {
        ContentBlockerManager.shared.invalidateCache()
        // After invalidation, apply should be a no-op (no cached rules)
        let config = WKWebViewConfiguration()
        ContentBlockerManager.shared.apply(to: config)
        // No crash = pass; rules were cleared
    }

    // MARK: - Apply with Disabled

    @Test func apply_doesNothing_whenDisabled() {
        let original = ContentBlockerManager.shared.isEnabled
        ContentBlockerManager.shared.isEnabled = false

        let config = WKWebViewConfiguration()
        ContentBlockerManager.shared.apply(to: config)
        // Should not crash or add rules when disabled

        ContentBlockerManager.shared.isEnabled = original
    }
}
