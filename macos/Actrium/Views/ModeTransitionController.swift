//
//  ModeTransitionController.swift
//  Actrium
//
//  Controls mode transition animation state between Focus and Overview modes.
//  Works with VirtualGrid (AnimationLayerNSView) for high-performance animation.
//
//  VirtualGrid Approach:
//  - VirtualGrid mirrors the real Grid layout exactly (positions, cardSize)
//  - Each card slot displays Tab thumbnail from TabSnapshotCache
//  - Animation drives scale/offset transform via CVDisplayLink
//  - VirtualGrid shows during transition, hides when complete
//
//  Focus → Overview:
//    Pure Zoom: Zoom out from targetScale (card fills viewport) to 1.0 (full grid)
//
//  Overview → Focus:
//    Pure Zoom: Zoom in from 1.0 to targetScale (target card fills viewport)
//

import SwiftUI
import Combine
import QuartzCore

// MARK: - Transition Phase

/// Represents the current phase of mode transition
/// NOTE: Does NOT contain progress to avoid triggering SwiftUI redraws every frame
enum TransitionPhase: Equatable {
    /// No transition in progress
    case idle

    /// Transitioning from Focus to Overview mode
    case focusToOverview(fromTabId: UUID)

    /// Transitioning from Overview to Focus mode
    case overviewToFocus(toTabId: UUID)

    var isTransitioning: Bool {
        switch self {
        case .idle: return false
        default: return true
        }
    }
}


// MARK: - Mode Transition Controller

/// Controls mode transition animation state
/// Uses CALayer for high-performance animation without SwiftUI redraw overhead
@MainActor
final class ModeTransitionController: ObservableObject {

    // MARK: - Animation Toggle

    /// 🔧 Debug switches: set as false Disables the animation completely, switching directly to mode
    static var animationEnabled: Bool = true

    // MARK: - Published Properties (only for state that SwiftUI needs to observe)

    /// Current transition phase
    @Published private(set) var phase: TransitionPhase = .idle

    /// Whether user interaction should be locked during transition
    @Published private(set) var isInteractionLocked: Bool = false

    // MARK: - CALayer Animation View

    /// Reference to the animation layer NSView (set by AnimationLayerRepresentable)
    weak var animationLayerView: AnimationLayerNSView?

    // MARK: - Private Animation State (NOT @Published to avoid SwiftUI redraw)

    /// Current animation scale
    private var currentScale: CGFloat = 1.0

    /// Current offset for translation (in points)
    private var currentOffset: CGPoint = .zero

    // MARK: - Private Properties

    /// Animation start time
    private var animationStartTime: CFTimeInterval = 0

    /// Total animation duration in seconds
    private var totalDuration: CFTimeInterval = ModeTransitionConstants.totalTransitionDuration

    /// Completion callback
    private var onTransitionComplete: (() -> Void)?

    /// Target tab ID for Overview → Focus transition
    private var targetTabId: UUID?

    /// Source tab ID for Focus → Overview transition
    private var sourceTabId: UUID?

    /// Target scale for zoom animation
    private var targetScale: CGFloat = 1.0

    /// Card rect in viewport (for calculating zoom)
    private var cardRectInViewport: CGRect = .zero

    /// Viewport size
    private var viewportSize: CGSize = .zero

    /// Overview transform (scale and offset for centered Overview mode)
    private var overviewTransform: (scale: CGFloat, offset: CGPoint) = (1.0, .zero)

    /// CVDisplayLink for vsync-aligned animation
    private var displayLink: CVDisplayLink?

    /// Frame counter for performance logging
    private var frameCount: Int = 0
    private var lastLogTime: CFTimeInterval = 0

    // MARK: - Initialization

    init() {}

    deinit {
        // Stop display link directly without going through main actor
        if let link = displayLink {
            CVDisplayLinkStop(link)
        }
    }

    // MARK: - Public API

    /// Start Focus → Overview transition
    /// - Parameters:
    ///   - tabId: The tab being transitioned from
    ///   - cardRect: The card's rect in actrium coordinates (before transform)
    ///   - viewportSize: The viewport size
    ///   - overviewTransform: The final transform for Overview mode (scale and centered offset)
    ///   - overviewSnapshot: Snapshot of the Overview grid (target state)
    ///   - onComplete: Callback when animation completes
    func startFocusToOverviewTransition(
        tabId: UUID,
        cardRect: CGRect,
        viewportSize: CGSize,
        overviewTransform: (scale: CGFloat, offset: CGPoint),
        overviewSnapshot: NSImage?,
        onComplete: @escaping () -> Void
    ) {
        guard !phase.isTransitioning else {
            NSLog("⚠️ ModeTransitionController: Already transitioning, ignoring")
            return
        }

        // 🔧 Directly call back when animated
        if !Self.animationEnabled {
            NSLog("🎬 ModeTransitionController: Animation DISABLED, direct switch Focus → Overview")
            onComplete()
            return
        }

        NSLog("🎬 ModeTransitionController: Starting Focus → Overview for tab %@",
              String(tabId.uuidString.prefix(8)))
        NSLog("🎬 ModeTransitionController: overviewSnapshot=%@, cardRect=%@, viewportSize=%@",
              overviewSnapshot != nil ? "YES (\(overviewSnapshot!.size))" : "nil",
              NSStringFromRect(NSRect(origin: cardRect.origin, size: cardRect.size)),
              NSStringFromSize(NSSizeFromCGSize(viewportSize)))

        sourceTabId = tabId
        self.cardRectInViewport = cardRect
        self.viewportSize = viewportSize
        self.overviewTransform = overviewTransform

        // Lock interaction
        isInteractionLocked = true

        // Calculate zoom scale (card should fill viewport)
        let scaleX = viewportSize.width / max(cardRect.width, 1)
        let scaleY = viewportSize.height / max(cardRect.height, 1)
        targetScale = min(scaleX, scaleY)

        // Calculate offset: when scale=targetScale, card should be at origin
        // offset = -cardRect.origin * scale
        let targetOffset = CGPoint(
            x: -cardRect.origin.x * targetScale,
            y: -cardRect.origin.y * targetScale
        )

        // Setup initial state: Start zoomed in (card fills viewport)
        currentScale = targetScale
        currentOffset = targetOffset

        // 🔑 Key: Set before the animation layer is shown phase
        // I'll do it. showWithTransform() Make the animation visible, even SwiftUI Trigger Update
        // updateNSView And because... phase.isTransitioning And skip, avoid reset.
        phase = .focusToOverview(fromTabId: tabId)

        // Setup VirtualGrid: Use atomic methods to avoid flashing
        // Focus → Overview: The initial rounded angle is 0(Focus Mode No Rounded Angle)
        // 🔑 showWithTransform In one. CATransaction In which to complete all settings
        if let animView = animationLayerView {
            NSLog("🎬 ModeTransitionController: animationLayerView OK, calling showWithTransform")
            animView.setAnimationDirection(.focusToOverview)
            animView.showWithTransform(scale: currentScale, offset: currentOffset, cornerRadius: 0)
        } else {
            NSLog("⚠️ ModeTransitionController: animationLayerView is NIL! Animation will fail")
        }

        NSLog("🎬 ModeTransitionController: Focus→Overview targetScale=%.2f, targetOffset=(%.1f, %.1f)",
              targetScale, targetOffset.x, targetOffset.y)

        // Switch actriumModeReal Overview The view is below the animation layer.
        onComplete()

        // Start animation
        startAnimation()
    }

    /// Start Overview → Focus transition
    /// - Parameters:
    ///   - tabId: The tab to focus on
    ///   - cardRect: The card's rect in actrium coordinates (before transform)
    ///   - viewportSize: The viewport size
    ///   - overviewTransform: The current transform for Overview mode (scale and centered offset)
    ///   - overviewSnapshot: Snapshot of the current Overview grid
    ///   - onComplete: Callback when animation completes
    func startOverviewToFocusTransition(
        tabId: UUID,
        cardRect: CGRect,
        viewportSize: CGSize,
        overviewTransform: (scale: CGFloat, offset: CGPoint),
        overviewSnapshot: NSImage?,
        onComplete: @escaping () -> Void
    ) {
        guard !phase.isTransitioning else {
            NSLog("⚠️ ModeTransitionController: Already transitioning, ignoring")
            return
        }

        // 🔧 Directly call back when animated
        if !Self.animationEnabled {
            NSLog("🎬 ModeTransitionController: Animation DISABLED, direct switch Overview → Focus")
            onComplete()
            return
        }

        NSLog("🎬 ModeTransitionController: Starting Overview → Focus for tab %@",
              String(tabId.uuidString.prefix(8)))
        NSLog("🎬 ModeTransitionController: overviewSnapshot=%@, cardRect=%@, viewportSize=%@",
              overviewSnapshot != nil ? "YES (\(overviewSnapshot!.size))" : "nil",
              NSStringFromRect(NSRect(origin: cardRect.origin, size: cardRect.size)),
              NSStringFromSize(NSSizeFromCGSize(viewportSize)))

        targetTabId = tabId
        self.cardRectInViewport = cardRect
        self.viewportSize = viewportSize
        self.overviewTransform = overviewTransform

        // Lock interaction
        isInteractionLocked = true

        // Calculate target zoom scale (card should fill viewport)
        let scaleX = viewportSize.width / max(cardRect.width, 1)
        let scaleY = viewportSize.height / max(cardRect.height, 1)
        targetScale = min(scaleX, scaleY)

        // Setup initial state: Start at Overview transform (centered)
        currentScale = overviewTransform.scale
        currentOffset = overviewTransform.offset

        // 🔑 Key: Set before the animation layer is shown phase
        // I'll do it. showWithTransform() Make the animation visible, even SwiftUI Trigger Update
        // updateNSView And because... phase.isTransitioning And skip, avoid reset.
        phase = .overviewToFocus(toTabId: tabId)

        // Setup VirtualGrid: Use atomic methods to avoid flashing
        // Overview → Focus: The initial rounded angle is 10(Overview Mode has round angles)
        // 🔑 showWithTransform In one. CATransaction In which to complete all settings
        animationLayerView?.setAnimationDirection(.overviewToFocus)
        animationLayerView?.showWithTransform(scale: currentScale, offset: currentOffset, cornerRadius: 10)

        NSLog("🎬 ModeTransitionController: Overview→Focus targetScale=%.2f, cardRect=(%.1f, %.1f, %.1f, %.1f)",
              targetScale, cardRect.origin.x, cardRect.origin.y, cardRect.width, cardRect.height)

        // Switch actriumModeReal Focus The view is below the animation layer.
        onComplete()

        // Start animation
        startAnimation()
    }

    /// Cancel any in-progress transition
    func cancelTransition() {
        guard phase.isTransitioning else { return }

        NSLog("⚠️ ModeTransitionController: Cancelling transition")

        stopDisplayLink()
        resetState()
    }

    // MARK: - Animation Control

    private func startAnimation() {
        animationStartTime = CACurrentMediaTime()
        frameCount = 0
        lastLogTime = 0

        NSLog("🎞️ Animation starting, duration=%.2fs, targetScale=%.2f", totalDuration, targetScale)

        startDisplayLink()
    }

    /// Start CVDisplayLink for vsync-aligned animation
    private func startDisplayLink() {
        stopDisplayLink()

        var link: CVDisplayLink?
        CVDisplayLinkCreateWithActiveCGDisplays(&link)

        guard let displayLink = link else {
            NSLog("⚠️ Failed to create CVDisplayLink")
            return
        }

        self.displayLink = displayLink

        // Store weak self in a context pointer
        let context = Unmanaged.passUnretained(self).toOpaque()

        CVDisplayLinkSetOutputCallback(displayLink, { (_, _, _, _, _, context) -> CVReturn in
            guard let context = context else { return kCVReturnSuccess }
            let controller = Unmanaged<ModeTransitionController>.fromOpaque(context).takeUnretainedValue()

            // Dispatch to main thread for UI updates
            DispatchQueue.main.async {
                controller.displayLinkCallback()
            }

            return kCVReturnSuccess
        }, context)

        CVDisplayLinkStart(displayLink)
    }

    /// Stop CVDisplayLink
    private func stopDisplayLink() {
        if let link = displayLink {
            CVDisplayLinkStop(link)
            displayLink = nil
        }
    }

    /// CVDisplayLink callback (called on main thread via dispatch)
    private func displayLinkCallback() {
        guard phase.isTransitioning else {
            stopDisplayLink()
            return
        }
        updateAnimation()
    }

    private func updateAnimation() {
        let now = CACurrentMediaTime()
        let elapsed = now - animationStartTime
        let totalProgress = min(elapsed / totalDuration, 1.0)

        // Performance logging: log every 10 frames
        frameCount += 1
        if frameCount % 10 == 0 {
            let fps = lastLogTime > 0 ? 10.0 / (now - lastLogTime) : 0
            NSLog("🎞️ Animation frame %d: progress=%.2f, scale=%.3f, fps=%.1f",
                  frameCount, totalProgress, currentScale, fps)
            lastLogTime = now
        }

        // Calculate eased progress
        let easedProgress = easeOutCubic(totalProgress)

        switch phase {
        case .focusToOverview:
            updateFocusToOverviewAnimation(easedProgress: easedProgress)

        case .overviewToFocus:
            updateOverviewToFocusAnimation(easedProgress: easedProgress)

        case .idle:
            return
        }

        // 🔑 New scheme: the animation layer remains visible and disappears at the last minute
        // opacity: Full 1, change when the animation is finished 0
        // cornerRadius: Overview(10) → Focus(0) or Focus(0) → Overview(10)
        let opacity: Double = totalProgress < 1.0 ? 1.0 : 0.0

        let cornerRadius: CGFloat
        switch phase {
        case .overviewToFocus:
            // Overview → Focus: Rounded Angle From 10 And become 0
            cornerRadius = 10.0 * (1.0 - CGFloat(easedProgress))
        case .focusToOverview:
            // Focus → Overview: Rounded Angle From 0 And become 10
            cornerRadius = 10.0 * CGFloat(easedProgress)
        case .idle:
            cornerRadius = 10.0
        }

        animationLayerView?.applyTransform(
            scale: currentScale,
            offset: currentOffset,
            opacity: CGFloat(opacity),
            cornerRadius: cornerRadius
        )

        // Check if animation is complete
        if totalProgress >= 1.0 {
            NSLog("🎞️ Animation completed: %d frames in %.3fs (avg %.1f fps)",
                  frameCount, elapsed, Double(frameCount) / elapsed)
            completeTransition()
        }
    }

    /// Focus → Overview animation update
    /// Pure Zoom animation: Zoom out from targetScale to overviewTransform.scale
    /// NOTE: easedProgress Calculated by the Caller, directly here
    private func updateFocusToOverviewAnimation(easedProgress: Double) {
        // Start state (Focus): zoomed in on card
        let startScale = targetScale
        let startOffset = CGPoint(
            x: -cardRectInViewport.origin.x * targetScale,
            y: -cardRectInViewport.origin.y * targetScale
        )

        // End state (Overview): overviewTransform (includes centering offset)
        let endScale = overviewTransform.scale
        let endOffset = overviewTransform.offset

        // Interpolate scale: startScale → endScale
        currentScale = startScale + (endScale - startScale) * CGFloat(easedProgress)

        // Interpolate offset: startOffset → endOffset
        currentOffset = CGPoint(
            x: startOffset.x + (endOffset.x - startOffset.x) * CGFloat(easedProgress),
            y: startOffset.y + (endOffset.y - startOffset.y) * CGFloat(easedProgress)
        )
    }

    /// Overview → Focus animation update
    /// Pure Zoom: Zoom in from overviewTransform to Focus (targetScale)
    /// NOTE: easedProgress Calculated by the Caller, directly here
    private func updateOverviewToFocusAnimation(easedProgress: Double) {
        // Start state (Overview): overviewTransform (includes centering offset)
        let startScale = overviewTransform.scale
        let startOffset = overviewTransform.offset

        // End state (Focus): zoomed in on card
        let endScale = targetScale
        let endOffset = CGPoint(
            x: -cardRectInViewport.origin.x * targetScale,
            y: -cardRectInViewport.origin.y * targetScale
        )

        // Interpolate scale: startScale → endScale
        currentScale = startScale + (endScale - startScale) * CGFloat(easedProgress)

        // Interpolate offset: startOffset → endOffset
        currentOffset = CGPoint(
            x: startOffset.x + (endOffset.x - startOffset.x) * CGFloat(easedProgress),
            y: startOffset.y + (endOffset.y - startOffset.y) * CGFloat(easedProgress)
        )
    }

    /// Ease-out cubic: Quick Start, Slow End (Similar) Safari)
    private func easeOutCubic(_ t: Double) -> Double {
        let t1 = 1.0 - t
        return 1.0 - t1 * t1 * t1
    }

    private func completeTransition() {
        NSLog("✅ ModeTransitionController: Transition complete")

        // 🔑 New programmes:actriumMode Switched at the start of the animation
        // The animation layer is completely transparent (opacity=0) directly hide and reset the state
        stopDisplayLink()
        animationLayerView?.hide()
        resetState()
    }

    /// Reset status after fadeout (no call) hide/reset, because fadeOut (Cased)
    private func resetStateAfterFadeOut() {
        phase = .idle
        isInteractionLocked = false
        currentScale = 1.0
        currentOffset = .zero
        onTransitionComplete = nil
        sourceTabId = nil
        targetTabId = nil
        targetScale = 1.0
        cardRectInViewport = .zero
        viewportSize = .zero
        overviewTransform = (1.0, .zero)
        // Note: No call animationLayerView?.reset(), because fadeOut We've handled the hidden and transform Reset
    }

    private func resetState() {
        phase = .idle
        isInteractionLocked = false
        currentScale = 1.0
        currentOffset = .zero
        onTransitionComplete = nil
        sourceTabId = nil
        targetTabId = nil
        targetScale = 1.0
        cardRectInViewport = .zero
        viewportSize = .zero
        overviewTransform = (1.0, .zero)

        // Reset VirtualGrid
        animationLayerView?.reset()
    }
}

// MARK: - Transition Helper Extension

extension ModeTransitionController {
    /// Check if currently transitioning to/from a specific tab
    func isTransitioning(tabId: UUID) -> Bool {
        switch phase {
        case .focusToOverview(let fromTabId):
            return fromTabId == tabId
        case .overviewToFocus(let toTabId):
            return toTabId == tabId
        case .idle:
            return false
        }
    }
}
