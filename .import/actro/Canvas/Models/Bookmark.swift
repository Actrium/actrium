import Foundation

struct Bookmark: Identifiable {
    let id = UUID()
    var title: String
    var url: URL
}
