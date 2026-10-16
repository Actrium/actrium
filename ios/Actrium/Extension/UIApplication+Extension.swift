import SwiftUI

extension UIApplication {
    func hideKeyboard() {
        sendAction(#selector(UIResponder.resignFirstResponder), to: nil, from: nil, for: nil)
    }
    
    var statusBarHeight: CGFloat {
        let window = UIApplication.shared.connectedScenes
                .compactMap { $0 as? UIWindowScene }
                .flatMap { $0.windows }
                .first { $0.isKeyWindow }
        let height = window?.windowScene?.statusBarManager?.statusBarFrame.height ?? 0
//        Logger.debug("Status bar height: \(height)")
        return [44, 47].contains(height) ? 36 : height
    }
    
    var bottomSafeAreaHeight: CGFloat {
        let windowScene = UIApplication.shared.connectedScenes
            .first as? UIWindowScene
        let window = windowScene?.windows.first
        return window?.safeAreaInsets.bottom ?? 0
    }
}
