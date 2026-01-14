//
//  ModeTransitionController.swift
//  Canvas
//
//  Controls mode transition animation state between Focus and Overview modes.
//  Uses CALayer for high-performance animation without SwiftUI redraw overhead.
//
//  Focus → Overview:
//    Pure Zoom animation: Zoom out Overview snapshot from targetScale to 1.0
//    (Overview screenshot shrinks from card-fills-screen to full grid view)
//
//  Overview → Focus:
//    Phase 1: Zoom in Overview snapshot until target card fills screen
//    Phase 2: Cross-Dissolve to Focus snapshot
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

    /// Primary image (source snapshot)
    private var primaryImage: NSImage?

    /// Secondary image (target snapshot for cross-dissolve)
    private var secondaryImage: NSImage?

    /// Current primary opacity
    private var primaryOpacity: Float = 0

    /// Current secondary opacity
    private var secondaryOpacity: Float = 0

    /// Current animation scale
    private var currentScale: CGFloat = 1.0

    /// Current offset for translation (in points)
    private var currentOffset: CGPoint = .zero

    // MARK: - Private Properties

    /// Animation start time
    private var animationStartTime: CFTimeInterval = 0

    /// Total animation duration in seconds
    private var totalDuration: CFTimeInterval = ModeTransitionConstants.totalTransitionDuration

    /// Zoom phase duration ratio (0-1)
    private let zoomPhaseRatio: Double = 0.5

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
        primaryImage = overviewSnapshot
        secondaryImage = nil
        primaryOpacity = 1.0
        secondaryOpacity = 0.0
        currentScale = targetScale
        currentOffset = targetOffset
        currentStage = .zoom

        // Setup CALayer
        animationLayerView?.setPrimaryImage(primaryImage)
        animationLayerView?.setSecondaryImage(secondaryImage)
        animationLayerView?.updateAnimationState(
            primaryOpacity: primaryOpacity,
            secondaryOpacity: secondaryOpacity,
            scale: currentScale,
            offset: currentOffset
        )

        NSLog("🎬 ModeTransitionController: primaryImage=%@, targetScale=%.2f, targetOffset=(%.1f, %.1f)",
              primaryImage != nil ? "YES (\(primaryImage!.size))" : "nil",
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
        primaryImage = overviewSnapshot
        secondaryImage = TabSnapshotCacheManager.shared.fullScreenSnapshot(for: tabId)
            ?? TabSnapshotCacheManager.shared.filteredThumbnail(for: tabId)
        primaryOpacity = 1.0
        secondaryOpacity = 0.0
        currentScale = 1.0
        currentOffset = .zero
        currentStage = .zoom

        // Setup CALayer
        animationLayerView?.setPrimaryImage(primaryImage)
        animationLayerView?.setSecondaryImage(secondaryImage)
        animationLayerView?.updateAnimationState(
            primaryOpacity: primaryOpacity,
            secondaryOpacity: secondaryOpacity,
            scale: currentScale,
            offset: currentOffset
        )

        NSLog("🎬 ModeTransitionController: primaryImage=%@, secondaryImage=%@, targetScale=%.2f, cardRect=(%.1f, %.1f, %.1f, %.1f)",
              primaryImage != nil ? "YES (\(primaryImage!.size))" : "nil",
              secondaryImage != nil ? "YES (\(secondaryImage!.size))" : "nil",
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

        // Update CALayer directly (no SwiftUI redraw!)
        animationLayerView?.updateAnimationState(
            primaryOpacity: primaryOpacity,
            secondaryOpacity: secondaryOpacity,
            scale: currentScale,
            offset: currentOffset
        )

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
        currentStage = .zoom
        primaryOpacity = 1.0
        secondaryOpacity = 0.0

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
    /// Phase 1 (0-50%): Zoom in until target card fills screen
    /// Phase 2 (50-100%): Cross-Dissolve to Focus snapshot
    private func updateOverviewToFocusAnimation(tabId: UUID, progress: Double) {
        // Apply easing
        let easedProgress = easeInOutCubic(progress)

        phase = .overviewToFocus(toTabId: tabId, progress: easedProgress)

        // Calculate target offset (card should be at origin when fully zoomed)
        let targetOffset = CGPoint(
            x: -cardRectInViewport.origin.x * targetScale,
            y: -cardRectInViewport.origin.y * targetScale
        )

        if progress < zoomPhaseRatio {
            // Phase 1: Zoom in (0% - 50%)
            currentStage = .zoom
            let phaseProgress = progress / zoomPhaseRatio
            let easedPhaseProgress = easeInOutCubic(phaseProgress)

            primaryOpacity = 1.0
            secondaryOpacity = 0.0

            // Interpolate scale: 1.0 → targetScale
            currentScale = 1.0 + (targetScale - 1.0) * CGFloat(easedPhaseProgress)

            // Interpolate offset: (0,0) → targetOffset
            currentOffset = CGPoint(
                x: targetOffset.x * CGFloat(easedPhaseProgress),
                y: targetOffset.y * CGFloat(easedPhaseProgress)
            )

        } else {
            // Phase 2: Cross-Dissolve (50% - 100%)
            currentStage = .dissolve
            let phaseProgress = (progress - zoomPhaseRatio) / (1.0 - zoomPhaseRatio)
            let easedPhaseProgress = easeInOutCubic(phaseProgress)

            // Cross-dissolve from zoomed Overview to Focus snapshot
            primaryOpacity = Float(1.0 - easedPhaseProgress)
            secondaryOpacity = Float(easedPhaseProgress)

            // Keep at target scale and offset during dissolve
            currentScale = targetScale
            currentOffset = targetOffset
        }
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
        primaryImage = nil
        secondaryImage = nil
        primaryOpacity = 0
        secondaryOpacity = 0
        currentScale = 1.0
        currentOffset = .zero
        currentStage = .zoom
        onTransitionComplete = nil
        sourceTabId = nil
        targetTabId = nil
        targetScale = 1.0
        cardRectInViewport = .zero
        viewportSize = .zero

        // Reset CALayer
        animationLayerView?.resetState()
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
