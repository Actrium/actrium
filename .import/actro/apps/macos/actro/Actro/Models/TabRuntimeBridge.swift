import Foundation
import ActroCore

enum RuntimeBridgeContract {
    static let tabId = runtimeBridgeTabIdKey()
    static let title = runtimeBridgeTitleKey()
    static let containerWidth = runtimeBridgeContainerWidthKey()
    static let containerHeight = runtimeBridgeContainerHeightKey()
    static let systemAppType = "systemAppType"
}

extension String {
    /// Stable content fingerprint used to identify equivalent runtime bundles.
    var stableHash: String {
        stableContentFingerprint(input: self)
    }
}
