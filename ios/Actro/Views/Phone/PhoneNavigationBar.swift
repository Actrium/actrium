import SwiftUI

/// Reusable custom navigation bar:
/// - Show a large header on the left (optional subheading)
/// - Right through. ViewBuilder Import any button or view
struct PhoneNavigationBar<LeftItems: View, RightItems: View>: View {
    let title: String
    let subtitle: String?
    let showLargeTitle: Bool
    let leftItems: () -> LeftItems
    let rightItems: () -> RightItems

    init(title: String,
         subtitle: String? = nil,
         showLargeTitle: Bool = true,
         @ViewBuilder leftItems: @escaping () -> LeftItems,
         @ViewBuilder rightItems: @escaping () -> RightItems) {
        self.title = title
        self.subtitle = subtitle
        self.showLargeTitle = showLargeTitle
        self.leftItems = leftItems
        self.rightItems = rightItems
    }
    
    var body: some View {
        VStack {
            HStack(alignment: .center, spacing: 12) {
                // Left Custom Area (buttonable)
                if !(LeftItems.self == EmptyView.self) {
                    HStack(spacing: 8) {
                        leftItems()
                    }
                }
                
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
                
                // Right button area
                HStack(spacing: 10) {
                    rightItems()
                }
            }
            .padding(.horizontal, 16)
            .frame(height: 44)
        }
        .padding(.top, UIApplication.shared.statusBarHeight + (showLargeTitle ? 12 : 8))
        .padding(.bottom, showLargeTitle ? 10 : 6)
        .background(
            LinearGradient(
                gradient: Gradient(colors: [Color.black.opacity(1.0), Color.black.opacity(0.0)]),
                startPoint: UnitPoint(x: 0.5, y: 0.45),
                endPoint: .bottom
            )
            .mask(Rectangle().opacity(1.0))
        )
        .ignoresSafeArea(edges: .top)
    }
}

// Convenience initializers
extension PhoneNavigationBar where LeftItems == EmptyView {
    init(title: String,
         subtitle: String? = nil,
         showLargeTitle: Bool = true,
         @ViewBuilder rightItems: @escaping () -> RightItems) {
        self.init(title: title, subtitle: subtitle, showLargeTitle: showLargeTitle, leftItems: { EmptyView() }, rightItems: rightItems)
    }
}

extension PhoneNavigationBar where RightItems == EmptyView {
    init(title: String,
         subtitle: String? = nil,
         showLargeTitle: Bool = true,
         @ViewBuilder leftItems: @escaping () -> LeftItems) {
        self.init(title: title, subtitle: subtitle, showLargeTitle: showLargeTitle, leftItems: leftItems, rightItems: { EmptyView() })
    }
}

extension PhoneNavigationBar where LeftItems == EmptyView, RightItems == EmptyView {
    init(title: String, subtitle: String? = nil, showLargeTitle: Bool = true) {
        self.init(title: title, subtitle: subtitle, showLargeTitle: showLargeTitle, leftItems: { EmptyView() }, rightItems: { EmptyView() })
    }
}
