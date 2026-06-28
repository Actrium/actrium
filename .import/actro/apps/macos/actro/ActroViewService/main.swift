//
//  main.swift
//  ActroViewService
//
//  ViewBridge XPC service entry point.
//

import Foundation
import Cocoa

// Register our view controller class before starting the service
// This also initializes ServiceReactNativeHost early to set up pendingTabId listener
registerActroServiceViewController()

// Start the ViewBridge service (does not return)
viewBridgeServiceMain(serviceName: "ActroViewService")
