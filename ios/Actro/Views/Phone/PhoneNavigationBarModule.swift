import Foundation
import SwiftUI
import UIKit

/// It's a simplified version. PhoneNavigationBar, for export to React Native Use
/// Unsupported pane ViewBuilder, only basic headings are supported/Subtitle Configuration
struct RNPhoneNavigationBar: View {
    let title: String
    let subtitle: String?
    let showLargeTitle: Bool

    init(title: String,
         subtitle: String? = nil,
         showLargeTitle: Bool = true) {
        self.title = title
        self.subtitle = subtitle
        self.showLargeTitle = showLargeTitle
    }

    var body: some View {
        VStack {
            HStack(alignment: .center, spacing: 12) {
                // Left Large Title Area
                VStack(alignment: .leading, spacing: 2) {
                    if showLargeTitle {
                        Text(title)
                            .font(.largeTitle)
                            .foregroundColor(Color.white)
                            .lineLimit(1)
                    } else {
                        HStack {
                            Spacer()
                            Text(title)
                                .font(.headline)
                                .foregroundColor(Color.white)
                                .lineLimit(1)
                            Spacer()
                        }
                    }

                    if let subtitle = subtitle, !subtitle.isEmpty {
                        Text(subtitle)
                            .font(.subheadline)
                            .foregroundColor(Color.secondary)
                            .lineLimit(1)
                    }
                }

                Spacer()
            }
            .padding(.horizontal, 16)
            .frame(height: 44)
        }
        .padding(.top, UIApplication.shared.statusBarHeight + (showLargeTitle ? 12 : 8))
        .padding(.bottom, showLargeTitle ? 10 : 6)
        .background(
            LinearGradient(
                gradient: Gradient(colors: [Color.black.opacity(1.0), Color.black.opacity(0.0)]),
                startPoint: UnitPoint(x: 0.5, y: 0.55),
                endPoint: .bottom
            )
            .mask(Rectangle().opacity(0.95))
        )
        .ignoresSafeArea(edges: .top)
    }
}

/// UIView Packaging, for use in React Native Show SwiftUI View
@objc(PhoneNavigationBarView)
class PhoneNavigationBarView: UIView {
    private var hostingController: UIHostingController<RNPhoneNavigationBar>?

    @objc var title: String = "" {
        didSet {
            updateView()
        }
    }

    @objc var subtitle: String? {
        didSet {
            updateView()
        }
    }

    @objc var showLargeTitle: Bool = true {
        didSet {
            updateView()
        }
    }

    override init(frame: CGRect) {
        super.init(frame: frame)
        setupView()
    }

    required init?(coder: NSCoder) {
        super.init(coder: coder)
        setupView()
    }

    private func setupView() {
        backgroundColor = .clear
        updateView()
    }

    private func updateView() {
        let swiftUIView = RNPhoneNavigationBar(
            title: title,
            subtitle: subtitle,
            showLargeTitle: showLargeTitle
        )

        if let existingHostingController = hostingController {
            existingHostingController.rootView = swiftUIView
        } else {
            let controller = UIHostingController(rootView: swiftUIView)
            controller.view.backgroundColor = .clear
            controller.view.translatesAutoresizingMaskIntoConstraints = false
            addSubview(controller.view)

            NSLayoutConstraint.activate([
                controller.view.topAnchor.constraint(equalTo: topAnchor),
                controller.view.leadingAnchor.constraint(equalTo: leadingAnchor),
                controller.view.trailingAnchor.constraint(equalTo: trailingAnchor),
                controller.view.bottomAnchor.constraint(equalTo: bottomAnchor)
            ])

            hostingController = controller
        }
    }
}
