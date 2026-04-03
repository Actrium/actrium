import SwiftUI
import UIKit

struct WindowAccessor: UIViewRepresentable {
    let onWindowFound: (UIWindow) -> Void

    func makeUIView(context: Context) -> UIView {
        let view = UIView()

        DispatchQueue.main.async {
            if let window = view.window {
                onWindowFound(window)
            }
        }

        return view
    }

    func updateUIView(_ uiView: UIView, context: Context) {
        DispatchQueue.main.async {
            if let window = uiView.window {
                onWindowFound(window)
            }
        }
    }
}
