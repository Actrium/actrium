import SwiftUI
import Combine

/// Manages the state of modal sheets within the main window
@MainActor
class WindowStateManager: ObservableObject {
    static let shared = WindowStateManager()

    @Published var showingHistory: Bool = false
    @Published var showingBookmarks: Bool = false

    private init() {}

    func openHistory() {
        showingHistory = true
    }

    func openBookmarks() {
        showingBookmarks = true
    }

    func closeHistory() {
        showingHistory = false
    }

    func closeBookmarks() {
        showingBookmarks = false
    }
}
