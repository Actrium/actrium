//
//  main.swift
//  ActroRenderer
//
//  Browser-style Renderer Helper entry point.
//

import Foundation
import Cocoa

// The synchronous executable entry point runs on the main thread.
MainActor.assumeIsolated {
    RendererProcessController.runFromCommandLine()
}
