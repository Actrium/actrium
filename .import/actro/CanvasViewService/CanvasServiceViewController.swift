//
//  CanvasServiceViewController.swift
//  CanvasViewService
//
//  NSServiceViewController subclass that hosts React Native content.
//  This runs in the XPC service process and provides views to the host app.
//

import Cocoa

// MARK: - NSServiceViewController Base Class Access

/// Get the NSServiceViewController class dynamically
private func getServiceViewControllerClass() -> AnyClass? {
    let frameworkPath = "/System/Library/PrivateFrameworks/ViewBridge.framework/ViewBridge"
    _ = dlopen(frameworkPath, RTLD_NOW)
    return NSClassFromString("NSServiceViewController")
}

// MARK: - Canvas Service View Controller

/// The view controller that provides React Native content to the host process.
/// This class inherits from NSServiceViewController (loaded dynamically).
@objc(CanvasServiceViewController)
public class CanvasServiceViewController: NSViewController {

    // MARK: - Properties

    private var reactNativeView: NSView?
    private var contentLoaded = false

    // MARK: - Initialization

    public override init(nibName nibNameOrNil: NSNib.Name?, bundle nibBundleOrNil: Bundle?) {
        super.init(nibName: nibNameOrNil, bundle: nibBundleOrNil)
        NSLog("CanvasServiceViewController: Initialized")
    }

    required init?(coder: NSCoder) {
        super.init(coder: coder)
    }

    // MARK: - View Lifecycle

    public override func loadView() {
        // Create the root view
        let rootView = NSView(frame: NSRect(x: 0, y: 0, width: 800, height: 600))
        rootView.wantsLayer = true
        rootView.layer?.backgroundColor = NSColor.windowBackgroundColor.cgColor

        self.view = rootView
        NSLog("CanvasServiceViewController: View loaded")
    }

    public override func viewDidLoad() {
        super.viewDidLoad()
        NSLog("CanvasServiceViewController: viewDidLoad")

        // Setup the view hierarchy
        setupContentView()
    }

    public override func viewDidAppear() {
        super.viewDidAppear()
        NSLog("CanvasServiceViewController: viewDidAppear, frame=\(view.frame)")

        // Load React Native content when view appears
        if !contentLoaded {
            loadReactNativeContent()
        }
    }

    // MARK: - Content Setup

    private func setupContentView() {
        // Create a placeholder view until React Native loads
        let placeholderView = NSView(frame: view.bounds)
        placeholderView.wantsLayer = true
        placeholderView.layer?.backgroundColor = NSColor.controlBackgroundColor.cgColor
        placeholderView.autoresizingMask = [.width, .height]

        // Add a loading indicator
        let label = NSTextField(labelWithString: "Loading React Native...")
        label.font = NSFont.systemFont(ofSize: 14)
        label.textColor = NSColor.secondaryLabelColor
        label.sizeToFit()
        label.frame.origin = NSPoint(
            x: (placeholderView.bounds.width - label.bounds.width) / 2,
            y: (placeholderView.bounds.height - label.bounds.height) / 2
        )
        label.autoresizingMask = [.minXMargin, .maxXMargin, .minYMargin, .maxYMargin]

        placeholderView.addSubview(label)
        view.addSubview(placeholderView)

        reactNativeView = placeholderView
    }

    private func loadReactNativeContent() {
        NSLog("CanvasServiceViewController: Loading React Native content")
        contentLoaded = true

        // TODO: Initialize React Native here
        // For now, just show a colored view to prove the ViewBridge works

        DispatchQueue.main.asyncAfter(deadline: .now() + 0.5) { [weak self] in
            self?.showTestContent()
        }
    }

    private func showTestContent() {
        guard let containerView = reactNativeView else { return }

        // Remove placeholder content
        containerView.subviews.forEach { $0.removeFromSuperview() }

        // Show test content to verify ViewBridge is working
        containerView.layer?.backgroundColor = NSColor.systemBlue.withAlphaComponent(0.3).cgColor

        let successLabel = NSTextField(labelWithString: "ViewBridge Working!")
        successLabel.font = NSFont.boldSystemFont(ofSize: 24)
        successLabel.textColor = NSColor.labelColor
        successLabel.backgroundColor = .clear
        successLabel.isBezeled = false
        successLabel.isEditable = false
        successLabel.sizeToFit()
        successLabel.frame.origin = NSPoint(
            x: (containerView.bounds.width - successLabel.bounds.width) / 2,
            y: (containerView.bounds.height - successLabel.bounds.height) / 2
        )
        successLabel.autoresizingMask = [.minXMargin, .maxXMargin, .minYMargin, .maxYMargin]

        containerView.addSubview(successLabel)

        NSLog("CanvasServiceViewController: Test content displayed")
    }

    // MARK: - Size Management

    public override func viewDidLayout() {
        super.viewDidLayout()
        reactNativeView?.frame = view.bounds
    }
}

// MARK: - Dynamic Subclassing

/// Create a proper NSServiceViewController subclass at runtime
/// This is needed because NSServiceViewController is a private class
public func createCanvasServiceViewControllerClass() -> AnyClass? {
    guard let baseClass = getServiceViewControllerClass() else {
        NSLog("createCanvasServiceViewControllerClass: NSServiceViewController not found")
        return nil
    }

    // Register our class with the Objective-C runtime
    // The @objc(CanvasServiceViewController) attribute should handle this,
    // but we may need runtime class creation for proper inheritance

    NSLog("createCanvasServiceViewControllerClass: Base class found: \(baseClass)")
    return CanvasServiceViewController.self
}
