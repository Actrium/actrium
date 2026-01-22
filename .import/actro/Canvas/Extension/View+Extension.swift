import SwiftUI

extension View {
    @ViewBuilder
    func asButtonStyle(_ color: Color = .primary, _ isProminent: Bool = false) -> some View {
        if #available(iOS 26.0, *) {
            self.buttonStyle(.glass)
        } else {
            self.background(color)
        }
    }
    
    @ViewBuilder
    func dynamicButtonStyle(_ prominent: Bool) -> some View {
        if prominent {
            self.buttonStyle(.borderedProminent)
        } else {
            self.buttonStyle(.plain)
        }
    }
    
    @ViewBuilder
    func asNavigationTitleMode() -> some View {
        if #available(iOS 26, *) {
            self.toolbarTitleDisplayMode(.inlineLarge)
        } else {
            self.navigationBarTitleDisplayMode(.large)
        }
    }
    
    @ViewBuilder
    func asGlassEffect(_ color: Color = .primary, radius: CGFloat? = nil) -> some View {
        if #available(iOS 26.0, *) {
            if let cornerRadius = radius {
                self.glassEffect(.regular, in: .rect(cornerRadius: cornerRadius))
            } else {
                self.glassEffect(.regular)
            }
        } else {
            if let cornerRadius = radius {
                self.background(color).cornerRadius(cornerRadius)
            } else {
                self.background(color)
            }
        }
    }
    
    @ViewBuilder
    func asGlassTintEffect(_ color: Color = .primary, _ cornerRadius: CGFloat? = nil) -> some View {
        if #available(iOS 26.0, *) {
            if let radius = cornerRadius {
                self.glassEffect(.regular.tint(color), in: .rect(cornerRadius: radius))
            } else {
                self.glassEffect(.regular.tint(color))
            }
        } else {
            self.background(color)
        }
    }
    
    @ViewBuilder
    func asGlassClearEffect(_ color: Color = .primary, radius: CGFloat? = nil) -> some View {
        if #available(iOS 26.0, *) {
            if let cornerRadius = radius {
                self.glassEffect(.clear.interactive(), in: .rect(cornerRadius: cornerRadius))
            } else {
                self.glassEffect(.clear.interactive())
            }
        } else {
            if let cornerRadius = radius {
                self.background(color).cornerRadius(cornerRadius)
            } else {
                self.background(color)
            }
        }
    }
    
    @ViewBuilder
    func asGlassEffectID(_ id: (some Hashable & Sendable)?, in namespace: Namespace.ID) -> some View {
        if #available(iOS 26, *) {
            self.glassEffectID(id, in: namespace)
        } else {
            self.id(id)
        }
    }
    
    @ViewBuilder
    func asGlassEffectUnion(id: (some Hashable & Sendable)?, namespace: Namespace.ID) -> some View {
        if #available(iOS 26, *) {
            self.glassEffectUnion(id: id, namespace: namespace)
        } else {
            self.id(id)
        }
    }
    
    @ViewBuilder
    func asMatchedTransitionSource(id: some Hashable, in namespace: Namespace.ID) -> some View {
        if #available(iOS 18, *) {
            self.matchedTransitionSource(id: id, in: namespace)
        } else {
            self.id(id)
        }
    }
    
    @ViewBuilder
    func asNavigationTransition(id: some Hashable, in namespace: Namespace.ID) -> some View {
        if #available(iOS 18, *) {
            self.navigationTransition(.zoom(sourceID: id, in: namespace))
        } else {
            self.id(id)
        }
    }
    
    @ViewBuilder
    func asShadow(_ color: Color, radius: CGFloat, x: CGFloat = 0, y: CGFloat = 0) -> some View {
        let os = ProcessInfo.processInfo.operatingSystemVersion.majorVersion
        if (15...18).contains(os) {
            self.shadow(color: color, radius: radius, x: x, y: y)
        } else {
            self
        }
    }
    
    @ViewBuilder
    func asBorder(_ color: Color = .black, width: CGFloat = 1) -> some View {
        let os = ProcessInfo.processInfo.operatingSystemVersion.majorVersion
        if (15...18).contains(os) {
            self.overlay(
                RoundedRectangle(cornerRadius: 0)
                    .stroke(color, lineWidth: width)
            )
        } else {
            self
        }
    }
    
    @ViewBuilder
    func asClipShape<S>(_ shape: S, style: FillStyle = FillStyle()) -> some View where S : Shape {
        let os = ProcessInfo.processInfo.operatingSystemVersion.majorVersion
        if (15...18).contains(os) {
            self.clipShape(shape, style: style)
        } else {
            self
        }
    }
    
    @ViewBuilder
    func asCornerRadius(_ radius: CGFloat, antialiased: Bool = true) -> some View {
        let os = ProcessInfo.processInfo.operatingSystemVersion.majorVersion
        if (15...18).contains(os) {
            self.cornerRadius(radius, antialiased: antialiased)
        } else {
            self
        }
    }
    
    @ViewBuilder
    func asClipped() -> some View {
        let os = ProcessInfo.processInfo.operatingSystemVersion.majorVersion
        if (15...18).contains(os) {
            self.clipped()
        } else {
            self
        }
    }
}
