//
//  AnimationLayer.swift
//  Canvas
//
//  Animation Layer for two-phase transition between Focus and Overview modes.
//  Uses CALayer for high-performance animation without SwiftUI redraw overhead.
//

import SwiftUI
import QuartzCore

// MARK: - Animation Layer NSView

/// High-performance animation layer using CALayer
/// Bypasses SwiftUI state updates to avoid view tree redraw during animation
///
/// Layer hierarchy:
/// - backgroundLayer: Solid color background filling entire window
/// - containerLayer: Contains the screenshot content, positioned at viewport offset
///   - primaryLayer: Primary screenshot (Overview or Focus)
///   - secondaryLayer: Secondary screenshot (for cross-dissolve)
final class AnimationLayerNSView: NSView {

    // MARK: - Layers

    private let backgroundLayer = CALayer()
    private let containerLayer = CALayer()
    private let primaryLayer = CALayer()
    private let secondaryLayer = CALayer()

    // MARK: - Configuration

    /// Viewport offset from left edge (sidebar width)
    private var viewportOffset: CGFloat = 0

    /// Viewport size (content area size)
    private var viewportSize: CGSize = .zero

    // MARK: - Coordinate System

    /// Use flipped coordinate system to match SwiftUI (origin at top-left)
    override var isFlipped: Bool { true }

    // MARK: - Initialization

    override init(frame frameRect: NSRect) {
        super.init(frame: frameRect)
        setupLayers()
    }

    required init?(coder: NSCoder) {
        super.init(coder: coder)
        setupLayers()
    }

    private func setupLayers() {
        wantsLayer = true
        layer?.masksToBounds = true

        // Background layer - fills entire window with window background color
        backgroundLayer.backgroundColor = NSColor.windowBackgroundColor.cgColor
        layer?.addSublayer(backgroundLayer)

        // Container layer for viewport content + transform
        containerLayer.masksToBounds = false  // Allow content to overflow during zoom
        containerLayer.anchorPoint = .zero
        containerLayer.position = .zero
        layer?.addSublayer(containerLayer)

        // Primary and secondary image layers
        primaryLayer.contentsGravity = .topLeft
        primaryLayer.masksToBounds = false
        containerLayer.addSublayer(primaryLayer)

        secondaryLayer.contentsGravity = .topLeft
        secondaryLayer.masksToBounds = false
        containerLayer.addSublayer(secondaryLayer)

        // Disable implicit animations
        CATransaction.begin()
        CATransaction.setDisableActions(true)
        primaryLayer.opacity = 0
        secondaryLayer.opacity = 0
        CATransaction.commit()
    }

    override func layout() {
        super.layout()
        updateLayerFrames()
    }

    private func updateLayerFrames() {
        CATransaction.begin()
        CATransaction.setDisableActions(true)

        let bounds = self.bounds

        // Background fills entire view
        backgroundLayer.frame = bounds

        // Container starts at viewport offset, sized to viewport
        containerLayer.frame = CGRect(
            x: viewportOffset,
            y: 0,
            width: viewportSize.width > 0 ? viewportSize.width : bounds.width - viewportOffset,
            height: viewportSize.height > 0 ? viewportSize.height : bounds.height
        )

        // Image layers fill container
        let containerBounds = containerLayer.bounds
        primaryLayer.frame = containerBounds
        secondaryLayer.frame = containerBounds

        CATransaction.commit()
    }

    // MARK: - Configuration

    /// Configure viewport layout
    /// - Parameters:
    ///   - offset: Horizontal offset from left edge (sidebar width)
    ///   - size: Viewport size
    func configureViewport(offset: CGFloat, size: CGSize) {
        self.viewportOffset = offset
        self.viewportSize = size
        updateLayerFrames()
    }

    // MARK: - Public API

    /// Set primary image
    func setPrimaryImage(_ image: NSImage?) {
        CATransaction.begin()
        CATransaction.setDisableActions(true)
        primaryLayer.contents = image
        CATransaction.commit()
    }

    /// Set secondary image
    func setSecondaryImage(_ image: NSImage?) {
        CATransaction.begin()
        CATransaction.setDisableActions(true)
        secondaryLayer.contents = image
        CATransaction.commit()
    }

    /// Update animation state (called every frame)
    /// - Parameters:
    ///   - primaryOpacity: Primary layer opacity (0-1)
    ///   - secondaryOpacity: Secondary layer opacity (0-1)
    ///   - scale: Scale factor for zoom
    ///   - offset: Translation offset in points (after scaling)
    func updateAnimationState(
        primaryOpacity: Float,
        secondaryOpacity: Float,
        scale: CGFloat,
        offset: CGPoint
    ) {
        CATransaction.begin()
        CATransaction.setDisableActions(true)

        primaryLayer.opacity = primaryOpacity
        secondaryLayer.opacity = secondaryOpacity

        // Apply transform: scale first (around origin), then translate by offset
        // Matrix order: Translate * Scale, applied as Scale first, then Translate
        // Point (x, y) becomes: (x * scale + offset.x, y * scale + offset.y)
        // For cardRect.origin to map to (0, 0): offset = -cardRect.origin * scale
        var transform = CATransform3DIdentity
        transform = CATransform3DScale(transform, scale, scale, 1.0)
        transform = CATransform3DTranslate(transform, offset.x, offset.y, 0)
        containerLayer.transform = transform

        CATransaction.commit()
    }

    /// Reset animation state (hide layers, reset transform)
    func resetState() {
        CATransaction.begin()
        CATransaction.setDisableActions(true)

        primaryLayer.opacity = 0
        secondaryLayer.opacity = 0
        primaryLayer.contents = nil
        secondaryLayer.contents = nil
        containerLayer.transform = CATransform3DIdentity

        CATransaction.commit()
    }

    // MARK: - Debug API

    /// Show animation layer with specified scale (for debugging)
    /// - Parameters:
    ///   - scale: Scale factor
    ///   - cardRect: Target card rect in viewport coordinates
    func debugShowWithScale(_ scale: CGFloat, cardRect: CGRect) {
        CATransaction.begin()
        CATransaction.setDisableActions(true)

        primaryLayer.opacity = 1.0
        secondaryLayer.opacity = 0

        // Calculate offset to center the card in viewport
        // When scale = 1.0: offset = (0, 0) - show full overview
        // When scale = targetScale: offset = -cardRect.origin * scale - card fills viewport
        let offset = CGPoint(
            x: -cardRect.origin.x * scale,
            y: -cardRect.origin.y * scale
        )

        var transform = CATransform3DIdentity
        transform = CATransform3DScale(transform, scale, scale, 1.0)
        transform = CATransform3DTranslate(transform, offset.x, offset.y, 0)
        containerLayer.transform = transform

        CATransaction.commit()

        NSLog("🎚️ AnimationLayer debug: scale=%.3f, cardRect=(%.1f, %.1f, %.1f, %.1f), offset=(%.1f, %.1f)",
              scale, cardRect.origin.x, cardRect.origin.y, cardRect.width, cardRect.height,
              offset.x, offset.y)
    }

    /// Hide debug view
    func debugHide() {
        resetState()
    }
}

// MARK: - SwiftUI Representable

/// SwiftUI wrapper for AnimationLayerNSView
struct AnimationLayerRepresentable: NSViewRepresentable {
    /// Reference to the controller for accessing the NSView
    let controller: ModeTransitionController

    /// Viewport offset from left edge (sidebar width)
    let viewportOffset: CGFloat

    /// Viewport size
    let viewportSize: CGSize

    /// Debug mode: show animation layer with slider-controlled scale
    let debugMode: Bool

    /// Debug scale value (1.0 = overview, targetScale = focus)
    let debugScale: CGFloat

    /// Card rect for debug mode (first tab card position in viewport)
    let debugCardRect: CGRect

    /// Screenshot image for debug mode
    let debugImage: NSImage?

    func makeNSView(context: Context) -> AnimationLayerNSView {
        let view = AnimationLayerNSView(frame: .zero)
        // Store reference in controller
        DispatchQueue.main.async {
            controller.animationLayerView = view
        }
        return view
    }

    func updateNSView(_ nsView: AnimationLayerNSView, context: Context) {
        // Update viewport configuration
        nsView.configureViewport(offset: viewportOffset, size: viewportSize)

        // Handle debug mode
        if debugMode {
            if let image = debugImage {
                nsView.setPrimaryImage(image)
            }
            nsView.debugShowWithScale(debugScale, cardRect: debugCardRect)
        }
    }
}

// MARK: - Convenience initializer for non-debug mode
extension AnimationLayerRepresentable {
    init(controller: ModeTransitionController, viewportOffset: CGFloat, viewportSize: CGSize) {
        self.controller = controller
        self.viewportOffset = viewportOffset
        self.viewportSize = viewportSize
        self.debugMode = false
        self.debugScale = 1.0
        self.debugCardRect = .zero
        self.debugImage = nil
    }
}

// MARK: - Animation Constants

/// Constants for mode transition animations
enum ModeTransitionConstants {
    /// Total transition duration (both phases combined)
    /// DEBUG: 10x slower for observation (normal: 0.6)
    static let totalTransitionDuration: Double = 6.0

    /// Spring animation response (for reference, not used in manual animation)
    static let springResponse: Double = 0.35

    /// Spring animation damping
    static let springDamping: Double = 1.0

    /// Legacy: Animation Layer duration (equals total duration)
    static let animationDuration: Double = totalTransitionDuration

    /// Blur radius for Overview mode thumbnails
    static let overviewBlurRadius: CGFloat = 4.0

    /// Saturation for Overview mode thumbnails (0.0-1.0)
    static let overviewSaturation: Double = 0.7

    /// Blur radius for Focus mode (clear)
    static let focusBlurRadius: CGFloat = 0.0

    /// Saturation for Focus mode (full color)
    static let focusSaturation: Double = 1.0
}

// MARK: - Legacy SwiftUI View (for reference/fallback)

/// Legacy SwiftUI Animation Layer (kept for reference)
/// Use AnimationLayerRepresentable instead for better performance
struct AnimationLayerLegacy: View {
    let primaryImage: NSImage?
    let secondaryImage: NSImage?
    let primaryOpacity: Double
    let secondaryOpacity: Double
    let scale: CGFloat
    let anchor: UnitPoint
    let frameSize: CGSize

    var body: some View {
        ZStack {
            if let image = primaryImage {
                Image(nsImage: image)
                    .resizable()
                    .aspectRatio(contentMode: .fill)
                    .frame(width: frameSize.width, height: frameSize.height)
                    .clipped()
                    .opacity(primaryOpacity)
            }
            if let image = secondaryImage {
                Image(nsImage: image)
                    .resizable()
                    .aspectRatio(contentMode: .fill)
                    .frame(width: frameSize.width, height: frameSize.height)
                    .clipped()
                    .opacity(secondaryOpacity)
            }
        }
        .scaleEffect(scale, anchor: anchor)
        .frame(width: frameSize.width, height: frameSize.height)
        .clipped()
        .allowsHitTesting(false)
    }
}
