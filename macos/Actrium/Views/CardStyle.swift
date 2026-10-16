//
//  CardStyle.swift
//  Actrium
//
//  Shared style constants and views for Overview card rendering.
//  Used by both WorkspaceMainView (real Grid) and AnimationLayer (animation overlay).
//  This ensures visual consistency between the two layers.
//

import SwiftUI

// MARK: - Card Style Constants

/// Shared constants for card rendering
enum CardStyle {
    // MARK: Layout Constants

    /// Card corner radius in Overview mode (Focus mode uses 0)
    static let cornerRadius: CGFloat = 10

    /// Floating title bar height
    static let titleBarHeight: CGFloat = 24

    /// Gap between title bar and card top edge
    static let titleBarGap: CGFloat = 4

    /// Horizontal padding inside title bar
    static let titleBarHorizontalPadding: CGFloat = 8

    /// Spacing between title bar items
    static let titleBarItemSpacing: CGFloat = 8

    // MARK: Close Icon Style

    /// Close icon SF Symbol name
    static let closeIconName = "xmark.circle.fill"

    /// Close icon font size
    static let closeIconFontSize: CGFloat = 14

    /// Close icon color opacity
    static let closeIconOpacity: CGFloat = 0.6

    // MARK: Title Text Style

    /// Title font size
    static let titleFontSize: CGFloat = 12

    /// Title line height (for CATextLayer vertical centering)
    /// Approximates system font line height: fontSize * 1.2
    static let titleLineHeight: CGFloat = 12 * 1.2  // 14.4

    /// Title font weight
    static let titleFontWeight: Font.Weight = .medium

    /// Title color opacity
    static let titleOpacity: CGFloat = 0.9
}

// MARK: - Shared SwiftUI Views

extension CardStyle {
    /// Close icon view - use this in SwiftUI views for visual consistency
    @ViewBuilder
    static func closeIcon() -> some View {
        Image(systemName: closeIconName)
            .font(.system(size: closeIconFontSize))
            .foregroundColor(.white.opacity(closeIconOpacity))
    }

    /// Title text view - use this in SwiftUI views for visual consistency
    @ViewBuilder
    static func titleText(_ title: String, placeholder: String = "New Tab") -> some View {
        Text(title.isEmpty ? placeholder : title)
            .font(.system(size: titleFontSize, weight: titleFontWeight))
            .foregroundColor(.white.opacity(titleOpacity))
            .lineLimit(1)
    }

    /// Floating title bar view - complete title bar with close icon and title
    @ViewBuilder
    static func floatingTitleBar(
        title: String,
        placeholder: String = "New Tab",
        onClose: @escaping () -> Void
    ) -> some View {
        HStack(spacing: titleBarItemSpacing) {
            Button(action: onClose) {
                closeIcon()
                    .frame(width: 20, height: 20)
            }
            .buttonStyle(.plain)
            .contentShape(Rectangle())

            titleText(title, placeholder: placeholder)

            Spacer()
        }
        .padding(.horizontal, titleBarHorizontalPadding)
    }
}

// MARK: - NSImage Rendering for CALayer

extension CardStyle {
    /// Render close icon as NSImage for use in CALayer
    /// Returns (image, pointSize) tuple for proper sizing
    @MainActor
    static func renderCloseIconImage() -> (image: NSImage, size: CGSize) {
        let iconView = Image(systemName: closeIconName)
            .font(.system(size: closeIconFontSize))
            .foregroundColor(.white.opacity(closeIconOpacity))

        let scale = NSScreen.main?.backingScaleFactor ?? 2.0
        let renderer = ImageRenderer(content: iconView)
        renderer.scale = scale

        if let cgImage = renderer.cgImage {
            let pointSize = CGSize(
                width: CGFloat(cgImage.width) / scale,
                height: CGFloat(cgImage.height) / scale
            )
            let image = NSImage(cgImage: cgImage, size: pointSize)
            return (image, pointSize)
        }

        // Fallback to NSImage API
        let config = NSImage.SymbolConfiguration(pointSize: closeIconFontSize, weight: .regular)
            .applying(NSImage.SymbolConfiguration(paletteColors: [NSColor.white.withAlphaComponent(closeIconOpacity)]))
        let fallbackImage = NSImage(systemSymbolName: closeIconName, accessibilityDescription: nil)?
            .withSymbolConfiguration(config) ?? NSImage()
        return (fallbackImage, CGSize(width: closeIconFontSize, height: closeIconFontSize))
    }

    /// Title text attributes for CATextLayer
    static var titleFont: NSFont {
        NSFont.systemFont(ofSize: titleFontSize, weight: .medium)
    }

    /// Title text color for CATextLayer
    static var titleColor: CGColor {
        NSColor.white.withAlphaComponent(titleOpacity).cgColor
    }
}
