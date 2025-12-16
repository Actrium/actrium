import SwiftUI

/// Sheet for adding or editing a bookmark
struct BookmarkEditSheet: View {
    @Environment(\.dismiss) private var dismiss
    @ObservedObject var viewModel: BookmarkViewModel

    let tab: Tab?
    let existingBookmark: Bookmark?

    @State private var title: String = ""
    @State private var notes: String = ""
    @State private var manufacturer: String = ""
    @State private var selectedTagIds: Set<UUID> = []
    @State private var newTagName: String = ""
    @State private var showingNewTagField = false

    var isEditing: Bool { existingBookmark != nil }

    init(viewModel: BookmarkViewModel, tab: Tab? = nil, existingBookmark: Bookmark? = nil) {
        self.viewModel = viewModel
        self.tab = tab
        self.existingBookmark = existingBookmark
    }

    var body: some View {
        VStack(spacing: 0) {
            // Header
            HStack {
                Text(isEditing ? "Edit Bookmark" : "Add Bookmark")
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

            // Form
            ScrollView {
                VStack(alignment: .leading, spacing: 16) {
                    // Title
                    VStack(alignment: .leading, spacing: 4) {
                        Text("Title")
                            .font(.caption)
                            .foregroundColor(.secondary)
                        TextField("Bookmark title", text: $title)
                            .textFieldStyle(.roundedBorder)
                    }

                    // Tags
                    VStack(alignment: .leading, spacing: 8) {
                        Text("Tags")
                            .font(.caption)
                            .foregroundColor(.secondary)

                        tagSelectionView
                    }

                    // Notes
                    VStack(alignment: .leading, spacing: 4) {
                        Text("Notes")
                            .font(.caption)
                            .foregroundColor(.secondary)
                        TextEditor(text: $notes)
                            .frame(height: 80)
                            .font(.body)
                            .padding(4)
                            .background(Color(nsColor: .controlBackgroundColor))
                            .cornerRadius(6)
                            .overlay(
                                RoundedRectangle(cornerRadius: 6)
                                    .stroke(Color(nsColor: .separatorColor), lineWidth: 1)
                            )
                    }

                    Divider()

                    // Metadata (read-only)
                    VStack(alignment: .leading, spacing: 8) {
                        if let tab = tab {
                            metadataRow(label: "App", value: tab.title)
                        }

                        if !manufacturer.isEmpty || tab != nil {
                            HStack(alignment: .top) {
                                Text("Manufacturer")
                                    .font(.caption)
                                    .foregroundColor(.secondary)
                                    .frame(width: 100, alignment: .leading)

                                TextField("Enter manufacturer", text: $manufacturer)
                                    .textFieldStyle(.roundedBorder)
                                    .font(.caption)
                            }
                        }

                        if let url = tab?.url ?? existingBookmark?.appURL {
                            metadataRow(label: "URL", value: url.path)
                        }
                    }
                }
                .padding()
            }

            Divider()

            // Actions
            HStack {
                Spacer()

                Button("Cancel") {
                    dismiss()
                }
                .keyboardShortcut(.escape)

                Button(isEditing ? "Save Changes" : "Add Bookmark") {
                    saveBookmark()
                    dismiss()
                }
                .keyboardShortcut(.return)
                .buttonStyle(.borderedProminent)
                .disabled(title.isEmpty)
            }
            .padding()
        }
        .frame(width: 400, height: 500)
        .onAppear {
            loadInitialValues()
        }
    }

    // MARK: - Tag Selection

    private var tagSelectionView: some View {
        VStack(alignment: .leading, spacing: 8) {
            // Selected and available tags
            FlowLayout(spacing: 6) {
                ForEach(viewModel.userTags) { tag in
                    TagPillView(
                        tag: tag,
                        isSelected: selectedTagIds.contains(tag.id),
                        onTap: {
                            toggleTag(tag.id)
                        }
                    )
                }

                // Add new tag button
                Button(action: { showingNewTagField = true }) {
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
                        .frame(width: 150)
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
                    }
                }
            }
        }
    }

    private func metadataRow(label: String, value: String) -> some View {
        HStack(alignment: .top) {
            Text(label)
                .font(.caption)
                .foregroundColor(.secondary)
                .frame(width: 100, alignment: .leading)

            Text(value)
                .font(.caption)
                .foregroundColor(.primary)
                .lineLimit(2)

            Spacer()
        }
    }

    // MARK: - Actions

    private func loadInitialValues() {
        if let bookmark = existingBookmark {
            title = bookmark.title
            notes = bookmark.notes ?? ""
            manufacturer = bookmark.manufacturer ?? ""
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

        // Select the new tag
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
            bookmark.notes = notes.isEmpty ? nil : notes
            bookmark.manufacturer = manufacturer.isEmpty ? nil : manufacturer
            bookmark.tags = selectedTags
            viewModel.updateBookmark(bookmark)
        } else if let tab = tab {
            var bookmark = Bookmark.from(tab: tab, manufacturer: manufacturer.isEmpty ? nil : manufacturer)
            bookmark.title = title
            bookmark.notes = notes.isEmpty ? nil : notes
            bookmark.tags = selectedTags
            viewModel.addBookmark(bookmark)
        }
    }
}

// MARK: - Tag Pill View

struct TagPillView: View {
    let tag: Tag
    let isSelected: Bool
    var onTap: (() -> Void)?

    var body: some View {
        Button(action: { onTap?() }) {
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
        }
        .buttonStyle(.plain)
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

#Preview("Add Bookmark") {
    BookmarkEditSheet(
        viewModel: BookmarkViewModel(),
        tab: Tab(title: "Counter App")
    )
}
