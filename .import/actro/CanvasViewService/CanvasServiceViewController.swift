//
//  CanvasServiceViewController.swift
//  CanvasViewService
//
//  NSServiceViewController subclass that hosts content for ViewBridge.
//  Uses Objective-C runtime to properly inherit from private NSServiceViewController.
//

import Cocoa
import ObjectiveC

// MARK: - Dynamic Class Registration

/// Register CanvasServiceViewController as a subclass of NSServiceViewController at runtime
/// This must be called before ViewBridge tries to instantiate the class
@_cdecl("registerCanvasServiceViewController")
public func registerCanvasServiceViewController() {
    NSLog("CanvasServiceViewController: Registering class...")

    // Load ViewBridge framework
    let frameworkPath = "/System/Library/PrivateFrameworks/ViewBridge.framework/ViewBridge"
    guard dlopen(frameworkPath, RTLD_NOW) != nil else {
        NSLog("CanvasServiceViewController: Failed to load ViewBridge")
        return
    }

    // Get NSServiceViewController class
    guard let superclass = NSClassFromString("NSServiceViewController") else {
        NSLog("CanvasServiceViewController: NSServiceViewController not found")
        return
    }

    // Check if already registered
    if NSClassFromString("CanvasServiceViewController") != nil {
        NSLog("CanvasServiceViewController: Already registered")
        return
    }

    // Create new class as subclass of NSServiceViewController
    guard let newClass = objc_allocateClassPair(superclass, "CanvasServiceViewController", 0) else {
        NSLog("CanvasServiceViewController: Failed to allocate class pair")
        return
    }

    // Add instance variable for content view
    class_addIvar(newClass, "_contentView", MemoryLayout<AnyObject>.size, UInt8(log2(Double(MemoryLayout<AnyObject>.alignment))), "@")

    // Add loadView method
    let loadViewBlock: @convention(block) (AnyObject) -> Void = { (self_) in
        NSLog("CanvasServiceViewController: loadView called")

        // Create root view
        let rootView = NSView(frame: NSRect(x: 0, y: 0, width: 800, height: 600))
        rootView.wantsLayer = true
        rootView.layer?.backgroundColor = NSColor.windowBackgroundColor.cgColor

        // Set as view using KVC
        self_.setValue(rootView, forKey: "view")

        // Setup content
        setupContent(in: rootView)
    }
    let loadViewImp = imp_implementationWithBlock(loadViewBlock)
    class_addMethod(newClass, #selector(NSViewController.loadView), loadViewImp, "v@:")

    // Add viewDidAppear method
    let viewDidAppearBlock: @convention(block) (AnyObject) -> Void = { (self_) in
        NSLog("CanvasServiceViewController: viewDidAppear")

        // Call super
        if let superclass = class_getSuperclass(object_getClass(self_)) {
            let selector = #selector(NSViewController.viewDidAppear)
            if let superMethod = class_getInstanceMethod(superclass, selector) {
                let superImp = method_getImplementation(superMethod)
                typealias SuperFunc = @convention(c) (AnyObject, Selector) -> Void
                let superCall = unsafeBitCast(superImp, to: SuperFunc.self)
                superCall(self_, selector)
            }
        }

        // Show test content after delay
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.3) {
            if let view = self_.value(forKey: "view") as? NSView {
                showTestContent(in: view)
            }
        }
    }
    let viewDidAppearImp = imp_implementationWithBlock(viewDidAppearBlock)
    class_addMethod(newClass, #selector(NSViewController.viewDidAppear), viewDidAppearImp, "v@:")

    // Register the class
    objc_registerClassPair(newClass)
    NSLog("CanvasServiceViewController: Class registered successfully")
}

// MARK: - Content Setup

private func setupContent(in view: NSView) {
    let contentView = NSView(frame: view.bounds)
    contentView.wantsLayer = true
    contentView.layer?.backgroundColor = NSColor.controlBackgroundColor.cgColor
    contentView.autoresizingMask = [.width, .height]

    // Loading indicator
    let label = NSTextField(labelWithString: "Loading ViewBridge content...")
    label.font = NSFont.systemFont(ofSize: 14)
    label.textColor = NSColor.secondaryLabelColor
    label.alignment = .center
    label.sizeToFit()
    label.frame.origin = NSPoint(
        x: (contentView.bounds.width - label.bounds.width) / 2,
        y: (contentView.bounds.height - label.bounds.height) / 2
    )
    label.autoresizingMask = [.minXMargin, .maxXMargin, .minYMargin, .maxYMargin]

    contentView.addSubview(label)
    view.addSubview(contentView)
}

private func showTestContent(in view: NSView) {
    NSLog("CanvasServiceViewController: Showing test content")

    // Clear existing content
    view.subviews.forEach { $0.removeFromSuperview() }

    // Create success view
    let contentView = NSView(frame: view.bounds)
    contentView.wantsLayer = true
    contentView.layer?.backgroundColor = NSColor.systemGreen.withAlphaComponent(0.2).cgColor
    contentView.autoresizingMask = [.width, .height]

    // Success label
    let label = NSTextField(labelWithString: "ViewBridge Working!")
    label.font = NSFont.boldSystemFont(ofSize: 28)
    label.textColor = NSColor.labelColor
    label.backgroundColor = .clear
    label.isBezeled = false
    label.isEditable = false
    label.alignment = .center
    label.sizeToFit()
    label.frame.origin = NSPoint(
        x: (contentView.bounds.width - label.bounds.width) / 2,
        y: (contentView.bounds.height - label.bounds.height) / 2 + 20
    )
    label.autoresizingMask = [.minXMargin, .maxXMargin, .minYMargin, .maxYMargin]

    // Info label
    let infoLabel = NSTextField(labelWithString: "Cross-process view from XPC service")
    infoLabel.font = NSFont.systemFont(ofSize: 14)
    infoLabel.textColor = NSColor.secondaryLabelColor
    infoLabel.backgroundColor = .clear
    infoLabel.isBezeled = false
    infoLabel.isEditable = false
    infoLabel.alignment = .center
    infoLabel.sizeToFit()
    infoLabel.frame.origin = NSPoint(
        x: (contentView.bounds.width - infoLabel.bounds.width) / 2,
        y: (contentView.bounds.height - infoLabel.bounds.height) / 2 - 20
    )
    infoLabel.autoresizingMask = [.minXMargin, .maxXMargin, .minYMargin, .maxYMargin]

    contentView.addSubview(label)
    contentView.addSubview(infoLabel)
    view.addSubview(contentView)

    NSLog("CanvasServiceViewController: Test content displayed successfully")
}

// MARK: - Module Initialization

/// Called when the module is loaded - register our class
@_cdecl("CanvasViewServiceModuleInit")
public func moduleInit() {
    registerCanvasServiceViewController()
}
