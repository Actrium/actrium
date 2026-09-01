//
//  AnimationLayer.swift
//  Actro
//
//  ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
//  Animated Layer - VirtualGrid Programme
//  ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
//
//  Structure:
//  ┌─────────────────────────────────────────────────────────────────┐
//  │  AnimationLayerNSView (NSView, isFlipped=true)                  │
//  │  ├── backgroundLayer: Background Colour Filling Layer (Window Background Colour)                   │
//  │  └── virtualGridLayer: Virtual grid layer (And the truth. Grid The layout is completely consistent.)       │
//  │      ├── cardSlot[tab0.id]: Tab 0 Thumbnails                       │
//  │      ├── cardSlot[tab1.id]: Tab 1 Thumbnails                       │
//  │      └── ...                                                    │
//  └─────────────────────────────────────────────────────────────────┘
//
//  Design principles:
//  - VirtualGrid And the truth. Grid Layout fully synchronized (positions, cardWidth, cardHeight)
//  - Direct Use TabSnapshotCache , do not need to intercept the entire view
//  - virtualGridLayer ♪ The transformation and the truth ♪ actro The change is exactly the same.
//  - Animation VirtualGrid Overwrite Real GridHide after animation
//
//  Data stream:
//  1. WorkspaceMainView Calculate gridResult (positions, cardWidth, cardHeight, actroSize)
//  2. Sync to AnimationLayerRepresentable
//  3. AnimationLayerNSView Update cardSlot It's... frame and contents
//  4. Apply and use animated with actro Same transform
//

import SwiftUI
import QuartzCore

// MARK: - Animation Direction

/// Animated directional count
enum AnimationDirection {
    case focusToOverview  // Focus → Overview: Blank from Large
    case overviewToFocus  // Overview → Focus: Blank from small
}

// MARK: - Virtual Grid Card Info

/// Information for single card slots (Double buffer:FocusLarge + OverviewSmall Chart)
struct VirtualGridCardInfo: Equatable {
    let tabId: UUID
    let position: CGPoint           // The card is here. actro Position in the Coordinate System
    let focusSnapshot: NSImage?     // Focus Modeshot (The big picture.viewport Dimensions)
    let overviewSnapshot: NSImage?  // Overview Modeshot (The small picture.card Dimensions)
    let title: String               // Tab Title (For Rendering Suspended Title Bar)
}

// MARK: - Animation Layer NSView

/// High-performance animated layer - VirtualGrid Programme
///
/// Use CALayer Build and True Grid The video shows the video of the video, which is a very good one.
/// The chutes of each card are corresponding Tab .
final class AnimationLayerNSView: NSView {

    // MARK: - CALayer Level

    /// Background Layer - Fill Window Background Colour
    private let backgroundLayer = CALayer()

    /// Virtual grid layer - Load all card slots, accept actro Change
    private let virtualGridLayer = CALayer()

    /// Card slot - Double-Standing Structure (Focus + Overview) + Title Bar
    private struct CardSlot {
        let container: CALayer       // Packaging Layer (Include card contents)
        let focusLayer: CALayer      // Focus Large Layer (Bottom)
        let overviewLayer: CALayer   // Overview Small Layer (Top Level)
        let titleBar: CALayer        // Suspended Title Bar (Above the card)
        let titleLabel: CATextLayer  // Title text
        let closeIcon: CALayer       // Close button icon
    }

    /// Cache Close Icon (Use CardStyle Shared)
    private lazy var cachedCloseIcon = CardStyle.renderCloseIconImage()

    /// Card slot - Press Tab ID Index
    private var cardSlots: [UUID: CardSlot] = [:]

    /// Current crossfade Progress (0.0 = Show Focus, 1.0 = Show Overview)
    private var crossfadeProgress: CGFloat = 0.0

    /// Current Animation Direction (For decision crossfade Orientation)
    private var animationDirection: AnimationDirection = .focusToOverview

    // MARK: - Grid Layout Data

    /// Actro Dimensions (Total size of grid content)
    private var actroSize: CGSize = .zero

    /// Card Size
    private var cardSize: CGSize = .zero

    /// Can not open message
    private var currentCards: [VirtualGridCardInfo] = []

    // MARK: - Status

    /// Visible
    private var isVisible: Bool = false

    // MARK: - Coordinate System

    /// Match with Flip Coordinates SwiftUI (The original is on the top left corner.)
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

    /// 🔑 Rewrite makeBackingLayer Ensure layer The coordinates are right when you create them.
    /// Yes. init Medium Settings layer?.isGeometryFlipped It might not work because layer Maybe it's not there yet.
    override func makeBackingLayer() -> CALayer {
        let layer = CALayer()
        layer.isGeometryFlipped = true
        return layer
    }

    /// Initialize CALayer Level
    private func setupLayers() {
        wantsLayer = true
        layer?.masksToBounds = true

        // 🔑 Flip coordinates: Make sure layer Reset after Existence (Defence Program)
        layer?.isGeometryFlipped = true

        // ━━━━ Background Layer (Consistent with the content area background colour) ━━━━
        // Use and WorkspaceMainView Same dark gray background, blinking when you avoid fade
        backgroundLayer.backgroundColor = NSColor(white: 0.12, alpha: 1.0).cgColor
        backgroundLayer.opacity = 0  // Initial hide, with isVisible=false Consistency
        layer?.addSublayer(backgroundLayer)

        // ━━━━ Virtual grid layer (Include card slots, scale them all) ━━━━
        // anchorPoint = (0,0) This is for scaling change from the top left corner
        virtualGridLayer.anchorPoint = CGPoint(x: 0, y: 0)
        virtualGridLayer.position = CGPoint(x: 0, y: 0)
        virtualGridLayer.opacity = 0  // Initial hide, display when animation
        virtualGridLayer.masksToBounds = true  // Cutting the shears to the layer bounds Scope
        layer?.addSublayer(virtualGridLayer)
    }

    override func layout() {
        super.layout()
        updateLayerFrames()
    }

    private func updateLayerFrames() {
        CATransaction.begin()
        CATransaction.setDisableActions(true)

        // Fill the whole view with the background layer
        backgroundLayer.frame = bounds

        // Virtual grid layer: use actroSizeFrom (0,0) Action.
        // Note: Settings bounds Not frame, because anchorPoint=(0,0) Time frame and bounds It's different.
        virtualGridLayer.bounds = CGRect(origin: .zero, size: actroSize.width > 0 ? actroSize : bounds.size)
        virtualGridLayer.position = .zero

        CATransaction.commit()
    }

    // MARK: - Public API: Grid Configure

    /// Configure Grid Layout
    /// - Parameters:
    ///   - actroSize: Actro Total Size
    ///   - cardSize: Card Size (width, height)
    func configureGrid(actroSize: CGSize, cardSize: CGSize) {
        self.actroSize = actroSize
        self.cardSize = cardSize

        CATransaction.begin()
        CATransaction.setDisableActions(true)

        // Settings virtualGridLayer It's... bounds(No, it's not. frame, because anchorPoint=(0,0))
        virtualGridLayer.bounds = CGRect(origin: .zero, size: actroSize)
        virtualGridLayer.position = .zero

        CATransaction.commit()
    }

    /// Update card data
    /// - Parameter cards: Information on all cards (tabId, position, focusSnapshot, overviewSnapshot, title)
    func updateCards(_ cards: [VirtualGridCardInfo]) {
        CATransaction.begin()
        CATransaction.setDisableActions(true)

        let newTabIds = Set(cards.map { $0.tabId })
        let existingTabIds = Set(cardSlots.keys)

        // Remove non-existent card
        for tabId in existingTabIds.subtracting(newTabIds) {
            cardSlots[tabId]?.container.removeFromSuperlayer()
            cardSlots[tabId]?.titleBar.removeFromSuperlayer()
            cardSlots.removeValue(forKey: tabId)
        }

        // Add or update cards
        for card in cards {
            let slot: CardSlot
            if let existing = cardSlots[card.tabId] {
                slot = existing
            } else {
                // ━━━ Create card container layer ━━━
                // The container is not cut and is used to carry shadows
                let container = CALayer()
                container.masksToBounds = false  // Do not crop, allow shadows to be displayed

                // Add Shadow (with TabPanel Unanimously.: radius=5, x=0, y=2, opacity=0.3)
                container.shadowColor = NSColor.black.cgColor
                container.shadowOpacity = 0.3
                container.shadowOffset = CGSize(width: 0, height: 2)
                container.shadowRadius = 5

                // Create Focus Layer (Bottom, show big picture)
                // Focus No rounded angles in mode (with TabPanel cornerRadius(isFocused ? 0 : 10) Unanimously.)
                let focusLayer = CALayer()
                focusLayer.contentsGravity = .resize  // Full Fill, with SwiftUI .frame() Behavior is consistent.
                focusLayer.masksToBounds = true
                focusLayer.cornerRadius = 0  // Focus Mode without round angles
                focusLayer.opacity = 0  // Initial hides, by crossfade Control
                container.addSublayer(focusLayer)

                // Create Overview Layer (Top Layer, Show Small Chart)
                // Overview Round angles in mode
                let overviewLayer = CALayer()
                overviewLayer.contentsGravity = .resize  // Full Fill, with SwiftUI .frame() Behavior is consistent.
                overviewLayer.masksToBounds = true
                overviewLayer.cornerRadius = CardStyle.cornerRadius  // Overview Mode has round angles
                overviewLayer.opacity = 1  // Initial display by crossfade Control
                container.addSublayer(overviewLayer)

                // ━━━ Create a floating title bar ━━━
                let titleBar = CALayer()
                titleBar.masksToBounds = false

                // Close button icon (Use SwiftUI Rendering, with reality Grid Exactly.)
                let closeIcon = CALayer()
                closeIcon.contents = cachedCloseIcon.image
                closeIcon.contentsGravity = .resize
                closeIcon.contentsScale = NSScreen.main?.backingScaleFactor ?? 2.0
                titleBar.addSublayer(closeIcon)

                // Title text (Use CardStyle Share Styles)
                let titleLabel = CATextLayer()
                titleLabel.font = CardStyle.titleFont
                titleLabel.fontSize = CardStyle.titleFontSize
                titleLabel.foregroundColor = CardStyle.titleColor
                titleLabel.truncationMode = .end
                titleLabel.alignmentMode = .left
                titleLabel.contentsScale = NSScreen.main?.backingScaleFactor ?? 2.0
                titleBar.addSublayer(titleLabel)

                virtualGridLayer.addSublayer(container)
                virtualGridLayer.addSublayer(titleBar)

                slot = CardSlot(
                    container: container,
                    focusLayer: focusLayer,
                    overviewLayer: overviewLayer,
                    titleBar: titleBar,
                    titleLabel: titleLabel,
                    closeIcon: closeIcon
                )
                cardSlots[card.tabId] = slot
            }

            // ━━━ Update Card frame ━━━
            let cardFrame = CGRect(origin: card.position, size: cardSize)
            slot.container.frame = cardFrame
            slot.focusLayer.frame = CGRect(origin: .zero, size: cardSize)
            slot.overviewLayer.frame = CGRect(origin: .zero, size: cardSize)

            // Update Shadow Path (Rounded rectangle, consistent with the content layer)
            slot.container.shadowPath = CGPath(
                roundedRect: CGRect(origin: .zero, size: cardSize),
                cornerWidth: CardStyle.cornerRadius,
                cornerHeight: CardStyle.cornerRadius,
                transform: nil
            )

            // ━━━ Update Title Bar frame ━━━
            let titleBarHeight = CardStyle.titleBarHeight
            let titleBarY = card.position.y - titleBarHeight - CardStyle.titleBarGap
            slot.titleBar.frame = CGRect(
                x: card.position.x,
                y: titleBarY,
                width: cardSize.width,
                height: titleBarHeight
            )

            // Close button position (Left, centre vertically)
            let iconSize = cachedCloseIcon.size
            let hPadding = CardStyle.titleBarHorizontalPadding
            slot.closeIcon.frame = CGRect(
                x: hPadding,
                y: (titleBarHeight - iconSize.height) / 2,
                width: iconSize.width,
                height: iconSize.height
            )

            // Title text position (Close button right)
            // CATextLayer From frame Top rendering text, using shared lineHeight Centred vertically to calculate
            let titleX = hPadding + iconSize.width + CardStyle.titleBarItemSpacing
            let titleWidth = cardSize.width - hPadding - iconSize.width - CardStyle.titleBarItemSpacing - hPadding
            let titleY = (titleBarHeight - CardStyle.titleLineHeight) / 2
            slot.titleLabel.frame = CGRect(
                x: titleX,
                y: titleY,
                width: titleWidth,
                height: CardStyle.titleLineHeight
            )

            // ━━━ Update ━━━
            slot.focusLayer.contents = card.focusSnapshot
            slot.overviewLayer.contents = card.overviewSnapshot

            // Update titlebar contents ([+] buttons, etc., for the hidden titlebar of untitled cards)
            let hasTitle = !card.title.isEmpty
            slot.titleBar.isHidden = !hasTitle
            if hasTitle {
                slot.titleLabel.string = card.title
            }

            // NOTE: Not here. crossfade Status
            // crossfade By applyTransform() or setAnimationDirection() Unified control
            // Avoid SwiftUI Wrong direction due to timing of update
        }

        currentCards = cards
        CATransaction.commit()
    }

    /// Apply Current crossfade Status to specified slot
    private func applyCurrentCrossfade(to slot: CardSlot) {
        switch animationDirection {
        case .focusToOverview:
            // Focus → Overview: From Focus The big picture is gradually moving to Overview Small Chart
            // progress 0.0: Focus 100%, Overview 0%
            // progress 1.0: Focus 0%, Overview 100%
            slot.focusLayer.opacity = Float(1.0 - crossfadeProgress)
            slot.overviewLayer.opacity = Float(crossfadeProgress)

        case .overviewToFocus:
            // Overview → Focus: From Overview Small scale to Focus Large
            // progress 0.0: Overview 100%, Focus 0%
            // progress 1.0: Overview 0%, Focus 100%
            slot.overviewLayer.opacity = Float(1.0 - crossfadeProgress)
            slot.focusLayer.opacity = Float(crossfadeProgress)
        }
    }

    // MARK: - Public API: Animation Direction Settings

    /// Set animation direction (Call before animation starts)
    /// - Parameter direction: Animation Direction
    func setAnimationDirection(_ direction: AnimationDirection) {
        animationDirection = direction

        // Reset crossfade To Start State
        crossfadeProgress = 0.0
        updateAllCrossfade()

        NSLog("🎬 VirtualGrid: Animation direction set to %@",
              direction == .focusToOverview ? "Focus→Overview" : "Overview→Focus")
    }

    // MARK: - Public API: Change Control

    /// Apply Actro Change and crossfade Progress (And the truth. Grid The change is exactly the same.)
    /// - Parameters:
    ///   - scale: Scale factor
    ///   - offset: Offset
    ///   - crossfade: crossfade Progress (0.0 Present. 1.0)
    func applyTransform(scale: CGFloat, offset: CGPoint, crossfade: CGFloat = 0.0) {
        applyTransform(scale: scale, offset: offset, opacity: 1.0 - crossfade)
    }

    /// Application of change, transparency and roundabouts (new scenario: fade out) + Rounded gradient)
    /// - Parameters:
    ///   - scale: Scale
    ///   - offset: Offset
    ///   - opacity: Overall transparency (1.0 = It's totally non-transparent., 0.0 = Full transparency)
    ///   - cornerRadius: Card round corner (Overview=CardStyle.cornerRadius, Focus=0)
    func applyTransform(scale: CGFloat, offset: CGPoint, opacity: CGFloat, cornerRadius: CGFloat = CardStyle.cornerRadius) {
        CATransaction.begin()
        CATransaction.setDisableActions(true)

        // Apply Changes
        var transform = CATransform3DIdentity
        transform = CATransform3DScale(transform, scale, scale, 1.0)
        transform = CATransform3DTranslate(transform, offset.x / scale, offset.y / scale, 0)
        virtualGridLayer.transform = transform

        // Application of overall transparency (fade-out effect)
        backgroundLayer.opacity = Float(opacity)
        virtualGridLayer.opacity = Float(opacity)

        // Apply a round corner to all cards
        for slot in cardSlots.values {
            slot.overviewLayer.cornerRadius = cornerRadius
            slot.focusLayer.cornerRadius = cornerRadius
            slot.container.cornerRadius = cornerRadius
            // Update Shadow Path to Match New Circle
            if let bounds = slot.container.bounds as CGRect? {
                slot.container.shadowPath = CGPath(
                    roundedRect: bounds,
                    cornerWidth: cornerRadius,
                    cornerHeight: cornerRadius,
                    transform: nil
                )
            }
        }

        CATransaction.commit()
    }

    /// Update all card slots crossfade Status
    private func updateAllCrossfade() {
        for slot in cardSlots.values {
            applyCurrentCrossfade(to: slot)
        }
    }

    // MARK: - Public API: Show/Hide

    /// Atoms display animated layers and set initials transform(avoiding scintillation)
    /// In one. CATransaction In which all settings are completed, make sure the first frame is the right state
    func showWithTransform(scale: CGFloat, offset: CGPoint, cornerRadius: CGFloat) {
        // 🔍 Debug: Check the state before the call
        NSLog("🔍 VirtualGrid: BEFORE showWithTransform - bgOpacity=%.1f, gridOpacity=%.1f, isVisible=%d, transform=%.2f,%.2f,%.2f",
              backgroundLayer.opacity, virtualGridLayer.opacity, isVisible ? 1 : 0,
              virtualGridLayer.transform.m11, virtualGridLayer.transform.m41, virtualGridLayer.transform.m42)

        isVisible = true

        CATransaction.begin()
        CATransaction.setDisableActions(true)

        // 1. Set first transform(in settings) opacity Before)
        var transform = CATransform3DIdentity
        transform = CATransform3DScale(transform, scale, scale, 1.0)
        transform = CATransform3DTranslate(transform, offset.x / scale, offset.y / scale, 0)
        virtualGridLayer.transform = transform

        // 2. Set round corner
        for slot in cardSlots.values {
            slot.overviewLayer.cornerRadius = cornerRadius
            slot.focusLayer.cornerRadius = cornerRadius
            slot.container.cornerRadius = cornerRadius
            if let bounds = slot.container.bounds as CGRect? {
                slot.container.shadowPath = CGPath(
                    roundedRect: bounds,
                    cornerWidth: cornerRadius,
                    cornerHeight: cornerRadius,
                    transform: nil
                )
            }
        }

        // 3. Ensure crossfade Correct.
        updateAllCrossfade()

        // 4. Final Settings opacity Make it visible
        backgroundLayer.opacity = 1.0
        virtualGridLayer.opacity = 1.0

        // 5. Make sure the animation layer is above all other content.
        // Sets the highest level zPosition Ensure CALayer Levels are at the top.
        layer?.zPosition = 10000

        CATransaction.commit()

        NSLog("🎬 VirtualGrid: ShowWithTransform scale=%.2f, offset=(%.1f, %.1f), cornerRadius=%.1f, cardSlots=%d, layer=%@",
              scale, offset.x, offset.y, cornerRadius, cardSlots.count,
              layer != nil ? "OK" : "NIL")
    }

    /// Show animated Layer
    func show() {
        guard !isVisible else {
            NSLog("⚠️ VirtualGrid: show() SKIPPED (already visible)")
            return
        }
        isVisible = true

        // 🔍 Debug: Records show() Status when called
        NSLog("🎬 VirtualGrid: show() called - transform=%.2f,%.2f,%.2f",
              virtualGridLayer.transform.m11, virtualGridLayer.transform.m41, virtualGridLayer.transform.m42)

        CATransaction.begin()
        CATransaction.setDisableActions(true)
        backgroundLayer.opacity = 1.0
        virtualGridLayer.opacity = 1.0

        // Make sure all the cards are in there. crossfade Correct state (prevention) SwiftUI (on the timing of updates)
        updateAllCrossfade()

        CATransaction.commit()

        NSLog("🎬 VirtualGrid: Show")
    }

    /// Hide animated Layer (Standing)
    func hide() {
        guard isVisible else { return }
        isVisible = false

        CATransaction.begin()
        CATransaction.setDisableActions(true)
        backgroundLayer.opacity = 0
        virtualGridLayer.opacity = 0
        CATransaction.commit()

        NSLog("🎬 VirtualGrid: Hide")
    }

    /// Fade animated layer
    /// - Parameters:
    ///   - duration: Time of Fade
    ///   - completion: Revert completed
    func fadeOut(duration: CFTimeInterval = 0.15, completion: (() -> Void)? = nil) {
        guard isVisible else {
            completion?()
            return
        }
        isVisible = false

        NSLog("🎬 VirtualGrid: FadeOut (%.2fs)", duration)

        CATransaction.begin()
        CATransaction.setAnimationDuration(duration)
        CATransaction.setAnimationTimingFunction(CAMediaTimingFunction(name: .easeOut))
        CATransaction.setCompletionBlock { [weak self] in
            // Reset after Fade Finish transform
            CATransaction.begin()
            CATransaction.setDisableActions(true)
            self?.virtualGridLayer.transform = CATransform3DIdentity
            CATransaction.commit()

            NSLog("🎬 VirtualGrid: FadeOut complete")
            completion?()
        }

        backgroundLayer.opacity = 0
        virtualGridLayer.opacity = 0

        CATransaction.commit()
    }

    /// Reset Status (Handdown)
    func reset() {
        hide()

        CATransaction.begin()
        CATransaction.setDisableActions(true)
        virtualGridLayer.transform = CATransform3DIdentity
        CATransaction.commit()
    }

    /// Reset Status (Fade-Insist)
    /// - Parameter completion: Revert completed
    func resetWithFadeOut(completion: (() -> Void)? = nil) {
        fadeOut(duration: 0.15, completion: completion)
    }

    // MARK: - Debug API

    /// Debug: Get Current Status Information
    func debugInfo() -> String {
        return """
        VirtualGrid Debug:
        - isVisible: \(isVisible)
        - actroSize: \(actroSize)
        - cardSize: \(cardSize)
        - cardCount: \(cardSlots.count)
        - animationDirection: \(animationDirection)
        - crossfadeProgress: \(crossfadeProgress)
        - transform: scale=\(virtualGridLayer.transform.m11), tx=\(virtualGridLayer.transform.m41), ty=\(virtualGridLayer.transform.m42)
        """
    }
}

// MARK: - SwiftUI Representable

/// SwiftUI Packaging
struct AnimationLayerRepresentable: NSViewRepresentable {

    /// ModeTransitionController References (For storage NSView References)
    let controller: ModeTransitionController

    /// Actro Dimensions
    let actroSize: CGSize

    /// Card Size
    let cardSize: CGSize

    /// Card Info List
    let cards: [VirtualGridCardInfo]

    /// Whether to show
    let isVisible: Bool

    func makeNSView(context: Context) -> AnimationLayerNSView {
        let view = AnimationLayerNSView(frame: .zero)
        // Synchronize references to avoid time-series problems
        // Note: This is already the main route, not required async
        controller.animationLayerView = view
        return view
    }

    func updateNSView(_ nsView: AnimationLayerNSView, context: Context) {
        // 🔑 Do not update configuration during animation to avoid interference ModeTransitionController Set transform
        // Animation by CVDisplayLink Driver, this update may overwrite the animation status
        let isTransitioning = controller.phase.isTransitioning
        NSLog("🔄 AnimationLayer.updateNSView: isTransitioning=%d, isVisible=%d, virtualGridLayer.opacity=%.1f",
              isTransitioning ? 1 : 0, isVisible ? 1 : 0, nsView.layer?.sublayers?.last?.opacity ?? -1)

        guard !isTransitioning else {
            NSLog("🔄 AnimationLayer.updateNSView: SKIPPED (transitioning)")
            return
        }

        // Update grid configuration (for non-animation only)
        nsView.configureGrid(actroSize: actroSize, cardSize: cardSize)

        // Update card data (for non-animation only)
        nsView.updateCards(cards)

        // Show/Hide (only during non-animation)
        if isVisible {
            nsView.show()
        } else {
            nsView.hide()
        }
    }
}

// MARK: - Animation Constants

/// Animation Constant
enum ModeTransitionConstants {
    /// The total length of the animation
    static let totalTransitionDuration: Double = 0.35

    /// Spring Response Parameters
    static let springResponse: Double = 0.35
    static let springDamping: Double = 1.0
}
