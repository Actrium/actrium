import SwiftUI

/// Chrome-style bookmark popover: edit name/tags, with Remove and Done buttons
struct BookmarkEditSheet: View {
    @Environment(\.dismiss) private var dismiss
    @ObservedObject var viewModel: BookmarkViewModel

    let tab: Tab?
    let existingBookmark: Bookmark?

    @State private var title: String = ""
    @State private var selectedTagIds: Set<UUID> = []
    @State private var showingNewTagField = false
    @State private var newTagName: String = ""
    @FocusState private var isNewTagFieldFocused: Bool

    init(viewModel: BookmarkViewModel, tab: Tab? = nil, existingBookmark: Bookmark? = nil) {
        self.viewModel = viewModel
        self.tab = tab
        self.existingBookmark = existingBookmark
    }

    var body: some View {
        VStack(spacing: 0) {
            // Header
            HStack {
                Text("Bookmark added")
                    .font(.headline)

                Spacer()

                Button(action: { dismiss() }) {
                    Image(systemName: "xmark.circle.fill")
                        .font(.title2)
                        .foregroundColor(.secondary)
                }
                .buttonStyle(.plain)
            }
            .padding()

            Divider()

            // Content area (expands with content)
            VStack(alignment: .leading, spacing: 16) {
                // Name field
                VStack(alignment: .leading, spacing: 4) {
                    Text("Name")
                        .font(.caption)
                        .foregroundColor(.secondary)
                    TextField("Bookmark name", text: $title)
                        .textFieldStyle(.roundedBorder)
                }

                // Tags (as folder substitute)
                VStack(alignment: .leading, spacing: 8) {
                    Text("Tags")
                        .font(.caption)
                        .foregroundColor(.secondary)

                    tagSelectionView
                }

                // URL info (read-only)
                if let url = tab?.url ?? existingBookmark?.appURL {
                    VStack(alignment: .leading, spacing: 4) {
                        Text("URL")
                            .font(.caption)
                            .foregroundColor(.secondary)
                        Text(url.absoluteString)
                            .font(.caption)
                            .foregroundColor(.primary)
                            .lineLimit(2)
                            .truncationMode(.middle)
                    }
                }
            }
            .padding()

            Divider()

            // Actions: Remove and Done (Chrome-style)
            HStack {
                Spacer()

                Button("Remove") {
                    if let bookmark = existingBookmark {
                        viewModel.removeBookmark(id: bookmark.id)
                    }
                    dismiss()
                }
                .foregroundStyle(.secondary)

                Button("Done") {
                    saveBookmark()
                    dismiss()
                }
                .buttonStyle(.borderedProminent)
            }
            .padding()
        }
        .frame(width: 320)
        .fixedSize(horizontal: false, vertical: true)
        .onAppear {
            loadInitialValues()
        }
    }

    // MARK: - Tag Selection

    private var tagSelectionView: some View {
        VStack(alignment: .leading, spacing: 8) {
            FlowLayout(spacing: 6) {
                ForEach(viewModel.userTags) { tag in
                    TagPillView(
                        tag: tag,
                        isSelected: selectedTagIds.contains(tag.id),
                        onTap: {
                            toggleTag(tag.id)
                        },
                        onLongPress: {
                            openTagManager(tag)
                        }
                    )
                }

                // Add new tag button
                Button(action: {
                    showingNewTagField = true
                    isNewTagFieldFocused = true
                }) {
                    HStack(spacing: 4) {
                        Image(systemName: "plus")
                        Text("Add Tag")
                    }
                    .font(.caption)
                    .padding(.horizontal, 8)
                    .padding(.vertical, 4)
                    .background(Color(nsColor: .controlBackgroundColor))
                    .cornerRadius(12)
                }
                .buttonStyle(.plain)
            }

            // New tag input
            if showingNewTagField {
                HStack {
                    TextField("New tag name", text: $newTagName)
                        .textFieldStyle(.roundedBorder)
                        .frame(width: 120)
                        .focused($isNewTagFieldFocused)
                        .onSubmit {
                            createNewTag()
                        }

                    Button("Add") {
                        createNewTag()
                    }
                    .disabled(newTagName.isEmpty)

                    Button("Cancel") {
                        showingNewTagField = false
                        newTagName = ""
                        isNewTagFieldFocused = false
                    }
                }
                .font(.caption)
            }
        }
    }

    // MARK: - Actions

    private func loadInitialValues() {
        if let bookmark = existingBookmark {
            title = bookmark.title
            selectedTagIds = Set(bookmark.tags.map { $0.id })
        } else if let tab = tab {
            title = tab.title.isEmpty ? "Untitled" : tab.title
        }
    }

    private func toggleTag(_ id: UUID) {
        if selectedTagIds.contains(id) {
            selectedTagIds.remove(id)
        } else {
            selectedTagIds.insert(id)
        }
    }

    private func createNewTag() {
        guard !newTagName.isEmpty else { return }

        viewModel.createTag(name: newTagName, color: Tag.defaultColors.randomElement())

        if let newTag = viewModel.tags.first(where: { $0.name == newTagName }) {
            selectedTagIds.insert(newTag.id)
        }

        newTagName = ""
        showingNewTagField = false
    }

    private func saveBookmark() {
        let selectedTags = viewModel.tags.filter { selectedTagIds.contains($0.id) }

        if var bookmark = existingBookmark {
            bookmark.title = title
            bookmark.tags = selectedTags
            viewModel.updateBookmark(bookmark)
        }
    }

    /// Long press on tag -> open tag manager in new window
    /// URI: canvas://bookmarks/tags/{tagId}
    private func openTagManager(_ tag: Tag) {
        dismiss()
        // Post notification to open bookmarks with tag filter
        // URI scheme: canvas://bookmarks/tags/{tagId}
        let uri = "canvas://bookmarks/tags/\(tag.id.uuidString)"
        NotificationCenter.default.post(
            name: NSNotification.Name("com.canvas.openURI"),
            object: nil,
            userInfo: ["uri": uri, "tagId": tag.id.uuidString, "tagName": tag.name]
        )
        NSLog("🏷️ Long press tag: \(tag.name), URI: \(uri)")
    }
}

// MARK: - Tag Pill View

struct TagPillView: View {
    let tag: Tag
    let isSelected: Bool
    var onTap: (() -> Void)?
    var onLongPress: (() -> Void)?

    var body: some View {
        Text(tag.name)
            .font(.caption)
            .padding(.horizontal, 8)
            .padding(.vertical, 4)
            .background(isSelected ? tag.swiftUIColor.opacity(0.2) : Color(nsColor: .controlBackgroundColor))
            .foregroundColor(isSelected ? tag.swiftUIColor : .primary)
            .cornerRadius(12)
            .overlay(
                RoundedRectangle(cornerRadius: 12)
                    .stroke(isSelected ? tag.swiftUIColor : Color.clear, lineWidth: 1)
            )
            .onTapGesture {
                onTap?()
            }
            .onLongPressGesture(minimumDuration: 0.5) {
                onLongPress?()
            }
    }
}

// MARK: - Flow Layout

struct FlowLayout: Layout {
    var spacing: CGFloat = 8

    func sizeThatFits(proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) -> CGSize {
        let result = FlowResult(in: proposal.width ?? 0, subviews: subviews, spacing: spacing)
        return CGSize(width: proposal.width ?? 0, height: result.height)
    }

    func placeSubviews(in bounds: CGRect, proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) {
        let result = FlowResult(in: bounds.width, subviews: subviews, spacing: spacing)
        for (index, subview) in subviews.enumerated() {
            subview.place(at: CGPoint(x: bounds.minX + result.positions[index].x,
                                       y: bounds.minY + result.positions[index].y),
                          proposal: .unspecified)
        }
    }

    struct FlowResult {
        var positions: [CGPoint] = []
        var height: CGFloat = 0

        init(in width: CGFloat, subviews: Subviews, spacing: CGFloat) {
            var x: CGFloat = 0
            var y: CGFloat = 0
            var rowHeight: CGFloat = 0

            for subview in subviews {
                let size = subview.sizeThatFits(.unspecified)

                if x + size.width > width && x > 0 {
                    x = 0
                    y += rowHeight + spacing
                    rowHeight = 0
                }

                positions.append(CGPoint(x: x, y: y))
                rowHeight = max(rowHeight, size.height)
                x += size.width + spacing
            }

            height = y + rowHeight
        }
    }
}

// MARK: - Preview

#Preview("Bookmark Popover") {
    BookmarkEditSheet(
        viewModel: BookmarkViewModel(),
        tab: Tab(title: "Google"),
        existingBookmark: Bookmark(title: "Google", appURL: URL(string: "https://google.com"))
    )
}
