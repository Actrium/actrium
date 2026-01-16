//
//  ModeTransitionController.swift
//  Canvas
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
enum TransitionPhase: Equatable {
    /// No transition in progress
    case idle

    /// Transitioning from Focus to Overview mode
    /// progress: 0.0 = Focus, 1.0 = Overview
    case focusToOverview(fromTabId: UUID, progress: Double)

    /// Transitioning from Overview to Focus mode
    /// progress: 0.0 = Overview, 1.0 = Focus
    case overviewToFocus(toTabId: UUID, progress: Double)

    var isTransitioning: Bool {
        switch self {
        case .idle: return false
        default: return true
        }
    }
}

// MARK: - Animation Stage

/// Two-stage animation phases
enum AnimationStage {
    case zoom       // Phase 1: Zoom in/out
    case dissolve   // Phase 2: Cross-Dissolve
}

// MARK: - Mode Transition Controller

/// Controls mode transition animation state
/// Uses CALayer for high-performance animation without SwiftUI redraw overhead
@MainActor
final class ModeTransitionController: ObservableObject {

    // MARK: - Published Properties (only for state that SwiftUI needs to observe)

    /// Current transition phase
    @Published private(set) var phase: TransitionPhase = .idle

    /// Whether user interaction should be locked during transition
    @Published private(set) var isInteractionLocked: Bool = false

    /// Current animation stage (for external observers)
    @Published private(set) var currentStage: AnimationStage = .zoom

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
    ///   - cardRect: The card's rect in viewport coordinates (after transform)
    ///   - viewportSize: The viewport size
    ///   - overviewSnapshot: Snapshot of the Overview grid (target state)
    ///   - onComplete: Callback when animation completes
    func startFocusToOverviewTransition(
        tabId: UUID,
        cardRect: CGRect,
        viewportSize: CGSize,
        overviewSnapshot: NSImage?,
        onComplete: @escaping () -> Void
    ) {
        guard !phase.isTransitioning else {
            NSLog("⚠️ ModeTransitionController: Already transitioning, ignoring")
            return
        }

        NSLog("🎬 ModeTransitionController: Starting Focus → Overview for tab %@",
              String(tabId.uuidString.prefix(8)))
        NSLog("🎬 ModeTransitionController: overviewSnapshot=%@, cardRect=%@, viewportSize=%@",
              overviewSnapshot != nil ? "YES (\(overviewSnapshot!.size))" : "nil",
              NSStringFromRect(NSRect(origin: cardRect.origin, size: cardRect.size)),
              NSStringFromSize(NSSizeFromCGSize(viewportSize)))

        sourceTabId = tabId
        onTransitionComplete = onComplete
        self.cardRectInViewport = cardRect
        self.viewportSize = viewportSize

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
        // Note: VirtualGrid uses cached thumbnails directly, no screenshots needed
        currentScale = targetScale
        currentOffset = targetOffset
        currentStage = .zoom

        // Setup VirtualGrid: show and apply initial transform
        animationLayerView?.show()
        animationLayerView?.applyTransform(scale: currentScale, offset: currentOffset)

        NSLog("🎬 ModeTransitionController: Focus→Overview targetScale=%.2f, targetOffset=(%.1f, %.1f)",
              targetScale, targetOffset.x, targetOffset.y)

        // Start animation
        phase = .focusToOverview(fromTabId: tabId, progress: 0)
        startAnimation()
    }

    /// Start Overview → Focus transition
    /// - Parameters:
    ///   - tabId: The tab to focus on
    ///   - cardRect: The card's rect in viewport coordinates (after transform)
    ///   - viewportSize: The viewport size
    ///   - overviewSnapshot: Snapshot of the current Overview grid
    ///   - onComplete: Callback when animation completes
    func startOverviewToFocusTransition(
        tabId: UUID,
        cardRect: CGRect,
        viewportSize: CGSize,
        overviewSnapshot: NSImage?,
        onComplete: @escaping () -> Void
    ) {
        guard !phase.isTransitioning else {
            NSLog("⚠️ ModeTransitionController: Already transitioning, ignoring")
            return
        }

        NSLog("🎬 ModeTransitionController: Starting Overview → Focus for tab %@",
              String(tabId.uuidString.prefix(8)))
        NSLog("🎬 ModeTransitionController: overviewSnapshot=%@, cardRect=%@, viewportSize=%@",
              overviewSnapshot != nil ? "YES (\(overviewSnapshot!.size))" : "nil",
              NSStringFromRect(NSRect(origin: cardRect.origin, size: cardRect.size)),
              NSStringFromSize(NSSizeFromCGSize(viewportSize)))

        targetTabId = tabId
        onTransitionComplete = onComplete
        self.cardRectInViewport = cardRect
        self.viewportSize = viewportSize

        // Lock interaction
        isInteractionLocked = true

        // Calculate target zoom scale (card should fill viewport)
        let scaleX = viewportSize.width / max(cardRect.width, 1)
        let scaleY = viewportSize.height / max(cardRect.height, 1)
        targetScale = min(scaleX, scaleY)

        // Setup initial state: Start at scale 1.0, offset (0,0) - full Overview visible
        // Note: VirtualGrid uses cached thumbnails directly, no screenshots needed
        currentScale = 1.0
        currentOffset = .zero
        currentStage = .zoom

        // Setup VirtualGrid: show and apply initial transform
        animationLayerView?.show()
        animationLayerView?.applyTransform(scale: currentScale, offset: currentOffset)

        NSLog("🎬 ModeTransitionController: Overview→Focus targetScale=%.2f, cardRect=(%.1f, %.1f, %.1f, %.1f)",
              targetScale, cardRect.origin.x, cardRect.origin.y, cardRect.width, cardRect.height)

        // Start animation
        phase = .overviewToFocus(toTabId: tabId, progress: 0)
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

        switch phase {
        case .focusToOverview(let tabId, _):
            updateFocusToOverviewAnimation(tabId: tabId, progress: totalProgress)

        case .overviewToFocus(let tabId, _):
            updateOverviewToFocusAnimation(tabId: tabId, progress: totalProgress)

        case .idle:
            return
        }

        // Update VirtualGrid transform directly (no SwiftUI redraw!)
        animationLayerView?.applyTransform(scale: currentScale, offset: currentOffset)

        // Check if animation is complete
        if totalProgress >= 1.0 {
            NSLog("🎞️ Animation completed: %d frames in %.3fs (avg %.1f fps)",
                  frameCount, elapsed, Double(frameCount) / elapsed)
            completeTransition()
        }
    }

    /// Focus → Overview animation update
    /// Pure Zoom animation: Zoom out from targetScale to 1.0 (Overview snapshot shrinking)
    /// Uses entire duration for zoom, no Cross-Dissolve phase
    private func updateFocusToOverviewAnimation(tabId: UUID, progress: Double) {
        // Apply easing
        let easedProgress = easeInOutCubic(progress)

        phase = .focusToOverview(fromTabId: tabId, progress: easedProgress)

        // Pure Zoom: Use entire animation duration (0% - 100%)
        // VirtualGrid uses thumbnails, no opacity changes needed
        currentStage = .zoom

        // Interpolate scale: targetScale → 1.0
        currentScale = targetScale - (targetScale - 1.0) * CGFloat(easedProgress)

        // Interpolate offset: targetOffset → (0,0)
        // targetOffset = -cardRect.origin * targetScale
        let targetOffset = CGPoint(
            x: -cardRectInViewport.origin.x * targetScale,
            y: -cardRectInViewport.origin.y * targetScale
        )
        currentOffset = CGPoint(
            x: targetOffset.x * (1.0 - CGFloat(easedProgress)),
            y: targetOffset.y * (1.0 - CGFloat(easedProgress))
        )
    }

    /// Overview → Focus animation update
    /// Pure Zoom: Zoom in from scale 1.0 to targetScale
    /// VirtualGrid uses thumbnails, no cross-dissolve needed
    private func updateOverviewToFocusAnimation(tabId: UUID, progress: Double) {
        // Apply easing
        let easedProgress = easeInOutCubic(progress)

        phase = .overviewToFocus(toTabId: tabId, progress: easedProgress)

        // Calculate target offset (card should be at origin when fully zoomed)
        let targetOffset = CGPoint(
            x: -cardRectInViewport.origin.x * targetScale,
            y: -cardRectInViewport.origin.y * targetScale
        )

        // Pure Zoom: Use entire animation duration
        currentStage = .zoom

        // Interpolate scale: 1.0 → targetScale
        currentScale = 1.0 + (targetScale - 1.0) * CGFloat(easedProgress)

        // Interpolate offset: (0,0) → targetOffset
        currentOffset = CGPoint(
            x: targetOffset.x * CGFloat(easedProgress),
            y: targetOffset.y * CGFloat(easedProgress)
        )
    }

    /// Ease-in-out cubic easing function
    private func easeInOutCubic(_ t: Double) -> Double {
        if t < 0.5 {
            return 4 * t * t * t
        } else {
            return 1 - pow(-2 * t + 2, 3) / 2
        }
    }

    private func completeTransition() {
        NSLog("✅ ModeTransitionController: Transition complete")

        let completion = onTransitionComplete
        resetState()
        completion?()
    }

    private func resetState() {
        phase = .idle
        isInteractionLocked = false
        currentScale = 1.0
        currentOffset = .zero
        currentStage = .zoom
        onTransitionComplete = nil
        sourceTabId = nil
        targetTabId = nil
        targetScale = 1.0
        cardRectInViewport = .zero
        viewportSize = .zero

        // Reset VirtualGrid
        animationLayerView?.reset()
    }
}

// MARK: - Transition Helper Extension

extension ModeTransitionController {
    /// Check if currently transitioning to/from a specific tab
    func isTransitioning(tabId: UUID) -> Bool {
        switch phase {
        case .focusToOverview(let fromTabId, _):
            return fromTabId == tabId
        case .overviewToFocus(let toTabId, _):
            return toTabId == tabId
        case .idle:
            return false
        }
    }

    /// Get current transition progress (0-1) or nil if not transitioning
    var currentProgress: Double? {
        switch phase {
        case .focusToOverview(_, let progress):
            return progress
        case .overviewToFocus(_, let progress):
            return progress
        case .idle:
            return nil
        }
    }
}
