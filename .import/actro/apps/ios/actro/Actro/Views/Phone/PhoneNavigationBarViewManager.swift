import Foundation
import React

/// React Native View Manager for PhoneNavigationBar
@objc(PhoneNavigationBarViewManager)
class PhoneNavigationBarViewManager: RCTViewManager {

    @objc
    override static func requiresMainQueueSetup() -> Bool {
        return true
    }

    @objc
    override func view() -> UIView! {
        return PhoneNavigationBarView()
    }
}
