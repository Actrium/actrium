//
//  ChromeStyleTabView.swift
//  Canvas
//
//  Chrome-style tab grid view using UICollectionView for iPad
//  - Fixed minimum card width (180-200pt)
//  - Auto-adjust number of columns based on container width
//  - Smooth transitions between overview and focused modes
//

import SwiftUI
import UIKit
import Kingfisher

// MARK: - Display Mode
enum ChromeTabMode {
    case overview
    case focus(tabId: UUID)
}

// MARK: - Chrome Style Tab View
struct ChromeStyleTabView: View {
    @ObservedObject var viewModel: BrowserViewModel
    @ObservedObject var historyVM: HistoryViewModel

    @State private var mode: ChromeTabMode = .overview

    var body: some View {
        ZStack {
            // Background
            Color.black.ignoresSafeArea()

            switch mode {
            case .overview:
                ChromeTabOverviewView(
                    tabs: viewModel.tabsSortedBySlotPosition().map { $0.tab },
                    viewModel: viewModel,
                    historyVM: historyVM,
                    onSelectTab: { tabId in
                        withAnimation(.spring(response: 0.35, dampingFraction: 1.0)) {
                            mode = .focus(tabId: tabId)
                            viewModel.currentTabId = tabId
                            viewModel.canvasMode = .focus(tabId: tabId)
                        }
                    },
                    onNewTab: {
                        viewModel.addNewTab(focus: false)
                    },
                    onCloseTab: { tabId in
                        withAnimation {
                            viewModel.closeTab(id: tabId)
                        }
                    }
                )
                .padding(.top, 24)
                .transition(.opacity.combined(with: .scale(scale: 0.95)))

            case .focus(let tabId):
                ChromeTabFocusView(
                    tabId: tabId,
                    viewModel: viewModel,
                    historyVM: historyVM,
                    onBackToOverview: {
                        withAnimation(.spring(response: 0.35, dampingFraction: 1.0)) {
                            mode = .overview
                            viewModel.canvasMode = .overview
                        }
                    }
                )
                .transition(.opacity.combined(with: .scale(scale: 1.05)))
            }
        }
        .onAppear {
            // Sync with view model's canvas mode
            switch viewModel.canvasMode {
            case .overview:
                mode = .overview
            case .focus(let tabId):
                mode = .focus(tabId: tabId)
            }
        }
        .environment(\.colorScheme, .dark)
        .onChange(of: viewModel.canvasMode) { oldValue, newValue in
            switch newValue {
            case .overview:
                if case .overview = mode { return }
                withAnimation(.spring(response: 0.35, dampingFraction: 1.0)) {
                    mode = .overview
                }
            case .focus(let tabId):
                if case .focus(tabId) = mode { return }
                withAnimation(.spring(response: 0.35, dampingFraction: 1.0)) {
                    mode = .focus(tabId: tabId)
                }
            }
        }
        .onChange(of: viewModel.currentTabId) { oldValue, newValue in
            if let tabId = newValue, case .focus = mode {
                mode = .focus(tabId: tabId)
            }
        }
    }
}

// MARK: - Overview View (Tab Grid)
struct ChromeTabOverviewView: View {
    let tabs: [Tab]
    @ObservedObject var viewModel: BrowserViewModel
    @ObservedObject var historyVM: HistoryViewModel

    let onSelectTab: (UUID) -> Void
    let onNewTab: () -> Void
    let onCloseTab: (UUID) -> Void

    var body: some View {
        // Tab grid using UICollectionView
        ChromeTabCollectionView(
            tabs: tabs,
            viewModel: viewModel,
            onSelectTab: onSelectTab,
            onNewTab: onNewTab,
            onCloseTab: onCloseTab
        )
    }
}

// MARK: - UICollectionView Wrapper for Tab Grid
struct ChromeTabCollectionView: UIViewRepresentable {
    let tabs: [Tab]
    @ObservedObject var viewModel: BrowserViewModel
    let onSelectTab: (UUID) -> Void
    let onNewTab: () -> Void
    let onCloseTab: (UUID) -> Void

    func makeCoordinator() -> Coordinator {
        Coordinator(self)
    }

    func makeUIView(context: Context) -> UICollectionView {
        let layout = ChromeTabGridLayout()
        let collectionView = UICollectionView(frame: .zero, collectionViewLayout: layout)
        collectionView.backgroundColor = .clear
        collectionView.delegate = context.coordinator
        collectionView.dataSource = context.coordinator
        collectionView.register(ChromeTabCell.self, forCellWithReuseIdentifier: "ChromeTabCell")
        collectionView.register(ChromeNewTabCell.self, forCellWithReuseIdentifier: "ChromeNewTabCell")
        collectionView.contentInset = UIEdgeInsets(top: 0, left: 0, bottom: 0, right: 0)
        collectionView.alwaysBounceHorizontal = false
        collectionView.showsHorizontalScrollIndicator = false
        collectionView.dragInteractionEnabled = true
        collectionView.dropDelegate = context.coordinator
        collectionView.dragDelegate = context.coordinator
        return collectionView
    }

    func updateUIView(_ uiView: UICollectionView, context: Context) {
        context.coordinator.tabs = tabs
        context.coordinator.viewModel = viewModel
        uiView.reloadData()
    }

    class Coordinator: NSObject, UICollectionViewDataSource, UICollectionViewDelegate, UICollectionViewDropDelegate, UICollectionViewDragDelegate {
        var parent: ChromeTabCollectionView
        var tabs: [Tab]
        var viewModel: BrowserViewModel

        init(_ parent: ChromeTabCollectionView) {
            self.parent = parent
            self.tabs = parent.tabs
            self.viewModel = parent.viewModel
        }

        func collectionView(_ collectionView: UICollectionView, numberOfItemsInSection section: Int) -> Int {
            return tabs.count + 1
        }

        func collectionView(_ collectionView: UICollectionView, cellForItemAt indexPath: IndexPath) -> UICollectionViewCell {
            if indexPath.item < tabs.count {
                let cell = collectionView.dequeueReusableCell(withReuseIdentifier: "ChromeTabCell", for: indexPath) as! ChromeTabCell
                let tab = tabs[indexPath.item]
                cell.configure(with: tab, viewModel: viewModel)
                cell.onClose = { [weak self] in
                    self?.parent.onCloseTab(tab.id)
                }
                return cell
            } else {
                let cell = collectionView.dequeueReusableCell(withReuseIdentifier: "ChromeNewTabCell", for: indexPath) as! ChromeNewTabCell
                return cell
            }
        }

        func collectionView(_ collectionView: UICollectionView, didSelectItemAt indexPath: IndexPath) {
            if indexPath.item < tabs.count {
                parent.onSelectTab(tabs[indexPath.item].id)
            } else {
                parent.onNewTab()
            }
        }

        func collectionView(_ collectionView: UICollectionView, itemsForBeginning session: UIDragSession, at indexPath: IndexPath) -> [UIDragItem] {
            guard indexPath.item < tabs.count else { return [] }
            let tab = tabs[indexPath.item]
            let itemProvider = NSItemProvider(object: tab.id.uuidString as NSString)
            let dragItem = UIDragItem(itemProvider: itemProvider)
            dragItem.localObject = tab

            if let cell = collectionView.cellForItem(at: indexPath) {
                let originalCellBackgroundColor = cell.backgroundColor
                let originalContentViewBackgroundColor = cell.contentView.backgroundColor
                cell.backgroundColor = .clear
                cell.contentView.backgroundColor = .clear
                let previewParameters = UIDragPreviewParameters()
                previewParameters.visiblePath = UIBezierPath(roundedRect: cell.bounds, cornerRadius: 16)
                previewParameters.backgroundColor = .clear
                dragItem.previewProvider = {
                    cell.backgroundColor = originalCellBackgroundColor
                    cell.contentView.backgroundColor = originalContentViewBackgroundColor
                    return UIDragPreview(view: cell, parameters: previewParameters)
                }
            }
            return [dragItem]
        }

        func collectionView(_ collectionView: UICollectionView, canHandle session: UIDropSession) -> Bool {
            return session.canLoadObjects(ofClass: NSString.self)
        }

        func collectionView(_ collectionView: UICollectionView, dropSessionDidUpdate session: UIDropSession, withDestinationIndexPath destinationIndexPath: IndexPath?) -> UICollectionViewDropProposal {
            if let destinationIndexPath = destinationIndexPath, destinationIndexPath.item >= tabs.count {
                return UICollectionViewDropProposal(operation: .forbidden)
            }
            return UICollectionViewDropProposal(operation: .move, intent: .insertAtDestinationIndexPath)
        }

        func collectionView(_ collectionView: UICollectionView, performDropWith coordinator: UICollectionViewDropCoordinator) {
            guard let destinationIndexPath = coordinator.destinationIndexPath,
                  let item = coordinator.items.first,
                  let sourceIndexPath = item.sourceIndexPath,
                  sourceIndexPath.item < tabs.count,
                  destinationIndexPath.item < tabs.count else {
                return
            }
            let sourceTab = tabs[sourceIndexPath.item]
            let destinationTab = tabs[destinationIndexPath.item]
            viewModel.swapSlotPositions(tabId1: sourceTab.id, tabId2: destinationTab.id)
            var updatedTabs = tabs
            updatedTabs.remove(at: sourceIndexPath.item)
            updatedTabs.insert(sourceTab, at: destinationIndexPath.item)
            self.tabs = updatedTabs
            let tabIds = updatedTabs.map { $0.id.uuidString }
            NotificationCenter.default.post(
                name: .tabsReorderedFromNative,
                object: nil,
                userInfo: ["tabIds": tabIds]
            )
            collectionView.performBatchUpdates {
                collectionView.moveItem(at: sourceIndexPath, to: destinationIndexPath)
            }
        }
    }
}

// MARK: - Custom Grid Layout
class ChromeTabGridLayout: UICollectionViewFlowLayout {
    private let cardAspectRatio: CGFloat = 3.0 / 4.0
    private let interItemSpacing: CGFloat = 16
    private let lineSpacing: CGFloat = 20
    private let minColumns: Int = 2
    private let sideInset: CGFloat = 16

    private var maxColumns: Int {
        guard let collectionView = collectionView else { return 3 }
        let isLandscape = collectionView.bounds.width > collectionView.bounds.height
        return isLandscape ? 4 : 3
    }

    override func prepare() {
        super.prepare()
        updateLayout()
    }

    override var collectionViewContentSize: CGSize {
        let width = collectionView?.bounds.width ?? 0
        let itemCount = collectionView?.numberOfItems(inSection: 0) ?? 0
        let columns = calculateNumberOfColumns()
        let rows = Int(ceil(Double(itemCount) / Double(columns)))
        let cardHeight = calculateCardWidth() / cardAspectRatio
        let height = CGFloat(rows) * cardHeight + CGFloat(rows - 1) * lineSpacing + sectionInset.top + sectionInset.bottom
        return CGSize(width: width, height: height)
    }

    private func updateLayout() {
        let cardWidth = calculateCardWidth()
        let cardHeight = cardWidth / cardAspectRatio
        itemSize = CGSize(width: cardWidth, height: cardHeight)
        minimumInteritemSpacing = interItemSpacing
        minimumLineSpacing = lineSpacing
        sectionInset = UIEdgeInsets(top: 8, left: sideInset, bottom: 16, right: sideInset)
    }

    private func calculateNumberOfColumns() -> Int {
        guard let collectionView = collectionView else { return minColumns }
        let totalWidth = collectionView.bounds.width
        let contentInset = collectionView.contentInset.left + collectionView.contentInset.right
        let availableWidth = totalWidth - contentInset - sideInset * 2
        for columns in stride(from: maxColumns, through: minColumns, by: -1) {
            let cardWidth = (availableWidth - interItemSpacing * CGFloat(columns - 1)) / CGFloat(columns)
            if cardWidth >= 140 {
                return columns
            }
        }
        return minColumns
    }

    private func calculateCardWidth() -> CGFloat {
        guard let collectionView = collectionView else { return 190 }
        let columns = calculateNumberOfColumns()
        let totalWidth = collectionView.bounds.width
        let contentInset = collectionView.contentInset.left + collectionView.contentInset.right
        let availableWidth = totalWidth - contentInset - sideInset * 2
        return (availableWidth - CGFloat(columns - 1) * interItemSpacing) / CGFloat(columns)
    }

    override func shouldInvalidateLayout(forBoundsChange newBounds: CGRect) -> Bool {
        guard let collectionView = collectionView else { return false }
        return collectionView.bounds.width != newBounds.width
    }
}

// MARK: - Tab Card Cell
class ChromeTabCell: UICollectionViewCell {
    static let reuseIdentifier = "ChromeTabCell"

    var onClose: (() -> Void)?

    private let thumbnailView = UIView()
    private let titleBarView = UIView()
    private let titleLabel = UILabel()
    private let iconView = UIImageView()
    private let iconContainerView = UIView()
    let closeButton = UIButton(type: .system)
    let bookmarkButton = UIButton(type: .system)
    private let snapshotImageView = UIImageView()

    private var tab: Tab?
    private weak var viewModel: BrowserViewModel?

    private let iconWidth: CGFloat = 32.0

    override init(frame: CGRect) {
        super.init(frame: frame)
        setupViews()
    }

    required init?(coder: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    private func setupViews() {
        contentView.clipsToBounds = true
        contentView.backgroundColor = UIColor(white: 0.15, alpha: 1.0)
        contentView.layer.borderWidth = 1
        contentView.layer.borderColor = UIColor.white.withAlphaComponent(0.1).cgColor
        contentView.layer.cornerRadius = 16

        thumbnailView.translatesAutoresizingMaskIntoConstraints = false
        thumbnailView.clipsToBounds = true
        contentView.addSubview(thumbnailView)

        snapshotImageView.translatesAutoresizingMaskIntoConstraints = false
        snapshotImageView.contentMode = .scaleAspectFill
        snapshotImageView.clipsToBounds = true
        thumbnailView.addSubview(snapshotImageView)

        titleBarView.translatesAutoresizingMaskIntoConstraints = false
        titleBarView.backgroundColor = UIColor(white: 0.9, alpha: 1.0)
        contentView.addSubview(titleBarView)

        iconContainerView.translatesAutoresizingMaskIntoConstraints = false
        iconContainerView.clipsToBounds = true
        iconContainerView.layer.cornerRadius = iconWidth / 2
        titleBarView.addSubview(iconContainerView)

        iconView.translatesAutoresizingMaskIntoConstraints = false
        iconView.contentMode = .scaleAspectFit
        iconView.tintColor = .white
        iconView.clipsToBounds = true
        iconView.layer.cornerRadius = iconWidth / 2
        iconContainerView.addSubview(iconView)

        titleLabel.translatesAutoresizingMaskIntoConstraints = false
        titleLabel.font = .systemFont(ofSize: 16, weight: .semibold)
        titleLabel.textColor = UIColor(Color(hex: "#404040"))
        titleLabel.lineBreakMode = .byTruncatingTail
        titleBarView.addSubview(titleLabel)

        closeButton.translatesAutoresizingMaskIntoConstraints = false
        closeButton.setImage(UIImage(systemName: "xmark.circle.fill"), for: .normal)
        closeButton.tintColor = UIColor(Color(hex: "#9E9EA7"))
        closeButton.addTarget(self, action: #selector(closeButtonTapped), for: .touchUpInside)
        closeButton.isUserInteractionEnabled = true
        titleBarView.addSubview(closeButton)

        bookmarkButton.translatesAutoresizingMaskIntoConstraints = false
        bookmarkButton.setImage(UIImage(systemName: "star.circle.fill"), for: .normal)
        bookmarkButton.tintColor = UIColor(Color(hex: "#9E9EA7"))
        bookmarkButton.addTarget(self, action: #selector(bookmarkButtonTapped), for: .touchUpInside)
        bookmarkButton.isUserInteractionEnabled = true
        titleBarView.addSubview(bookmarkButton)

        NSLayoutConstraint.activate([
            thumbnailView.topAnchor.constraint(equalTo: contentView.topAnchor),
            thumbnailView.leadingAnchor.constraint(equalTo: contentView.leadingAnchor),
            thumbnailView.trailingAnchor.constraint(equalTo: contentView.trailingAnchor),

            titleBarView.topAnchor.constraint(equalTo: thumbnailView.bottomAnchor),
            titleBarView.leadingAnchor.constraint(equalTo: contentView.leadingAnchor),
            titleBarView.trailingAnchor.constraint(equalTo: contentView.trailingAnchor),
            titleBarView.bottomAnchor.constraint(equalTo: contentView.bottomAnchor),
            titleBarView.heightAnchor.constraint(equalToConstant: 56),

            snapshotImageView.topAnchor.constraint(equalTo: thumbnailView.topAnchor),
            snapshotImageView.leadingAnchor.constraint(equalTo: thumbnailView.leadingAnchor),
            snapshotImageView.trailingAnchor.constraint(equalTo: thumbnailView.trailingAnchor),
            snapshotImageView.bottomAnchor.constraint(equalTo: thumbnailView.bottomAnchor),

            iconContainerView.leadingAnchor.constraint(equalTo: titleBarView.leadingAnchor, constant: 14),
            iconContainerView.centerYAnchor.constraint(equalTo: titleBarView.centerYAnchor),
            iconContainerView.widthAnchor.constraint(equalToConstant: iconWidth),
            iconContainerView.heightAnchor.constraint(equalToConstant: iconWidth),

            iconView.centerXAnchor.constraint(equalTo: iconContainerView.centerXAnchor),
            iconView.centerYAnchor.constraint(equalTo: iconContainerView.centerYAnchor),
            iconView.widthAnchor.constraint(equalToConstant: iconWidth),
            iconView.heightAnchor.constraint(equalToConstant: iconWidth),

            closeButton.trailingAnchor.constraint(equalTo: titleBarView.trailingAnchor, constant: -8),
            closeButton.centerYAnchor.constraint(equalTo: titleBarView.centerYAnchor),
            closeButton.widthAnchor.constraint(equalToConstant: 36),
            closeButton.heightAnchor.constraint(equalToConstant: 36),

            bookmarkButton.trailingAnchor.constraint(equalTo: closeButton.leadingAnchor, constant: -4),
            bookmarkButton.centerYAnchor.constraint(equalTo: titleBarView.centerYAnchor),
            bookmarkButton.widthAnchor.constraint(equalToConstant: 36),
            bookmarkButton.heightAnchor.constraint(equalToConstant: 36),

            titleLabel.leadingAnchor.constraint(equalTo: iconContainerView.trailingAnchor, constant: 12),
            titleLabel.centerYAnchor.constraint(equalTo: titleBarView.centerYAnchor),
            titleLabel.trailingAnchor.constraint(lessThanOrEqualTo: bookmarkButton.leadingAnchor, constant: -8)
        ])
    }

    func configure(with tab: Tab, viewModel: BrowserViewModel) {
        self.tab = tab
        self.viewModel = viewModel

        titleLabel.text = (tab.title.isEmpty || tab.title == "counterapp") ? "" : tab.title

        if let bgColor = Color(hslString: tab.backgroundColor).uiColor {
            iconContainerView.backgroundColor = bgColor
            thumbnailView.backgroundColor = bgColor
        } else {
            iconContainerView.backgroundColor = .white
            thumbnailView.backgroundColor = .white
        }

        if let iconURLString = tab.icon, let iconURL = URL(string: iconURLString) {
            iconView.kf.setImage(with: iconURL)
        } else {
            iconView.image = UIImage(systemName: "plus.viewfinder")
            iconView.tintColor = .white.withAlphaComponent(0.9)
        }

        if tab.title.isEmpty || tab.title == "counterapp" {
            bookmarkButton.isHidden = true
        } else {
            bookmarkButton.isHidden = false
            bookmarkButton.tintColor = tab.isBookmarked ? UIColor(Color(hex: "#EAC15B")) : UIColor(Color(hex: "#9E9EA7"))
        }

        titleBarView.bringSubviewToFront(closeButton)
        titleBarView.bringSubviewToFront(bookmarkButton)

        if let snapshot = viewModel.tabSnapshots[tab.id] {
            snapshotImageView.image = snapshot
            snapshotImageView.isHidden = false
        } else {
            snapshotImageView.image = nil
            snapshotImageView.isHidden = true
        }
    }

    @objc private func closeButtonTapped() {
        onClose?()
    }

    @objc private func bookmarkButtonTapped() {
        guard let tab = tab, let viewModel = viewModel else { return }
        let newBookmarkStatus = !tab.isBookmarked
        if let index = viewModel.tabs.firstIndex(where: { $0.id == tab.id }) {
            viewModel.tabs[index].isBookmarked = newBookmarkStatus
            viewModel.saveTabs()
        }
        bookmarkButton.tintColor = newBookmarkStatus ? UIColor(Color(hex: "#EAC15B")) : UIColor(Color(hex:"#9E9EA7"))
        NotificationCenter.default.post(
            name: .tabBookmarkChangedFromNative,
            object: nil,
            userInfo: [
                "tabId": tab.id.uuidString,
                "isBookmarked": newBookmarkStatus
            ]
        )
    }
}

// MARK: - New Tab Cell
class ChromeNewTabCell: UICollectionViewCell {
    static let reuseIdentifier = "ChromeNewTabCell"

    private let plusImageView = UIImageView()
    private let label = UILabel()

    override init(frame: CGRect) {
        super.init(frame: frame)
        setupViews()
    }

    required init?(coder: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    private func setupViews() {
        contentView.layer.cornerRadius = 16
        contentView.clipsToBounds = true
        contentView.backgroundColor = UIColor(white: 0.12, alpha: 1.0)
        contentView.layer.borderWidth = 1
        contentView.layer.borderColor = UIColor.white.withAlphaComponent(0.15).cgColor

        plusImageView.translatesAutoresizingMaskIntoConstraints = false
        plusImageView.image = UIImage(systemName: "plus")
        plusImageView.tintColor = UIColor.white.withAlphaComponent(0.5)
        plusImageView.contentMode = .scaleAspectFit
        contentView.addSubview(plusImageView)

        label.translatesAutoresizingMaskIntoConstraints = false
        label.text = "New Tab"
        label.font = .systemFont(ofSize: 15, weight: .medium)
        label.textColor = UIColor.white.withAlphaComponent(0.5)
        contentView.addSubview(label)

        NSLayoutConstraint.activate([
            plusImageView.centerXAnchor.constraint(equalTo: contentView.centerXAnchor),
            plusImageView.centerYAnchor.constraint(equalTo: contentView.centerYAnchor, constant: -12),
            plusImageView.widthAnchor.constraint(equalToConstant: 40),
            plusImageView.heightAnchor.constraint(equalToConstant: 40),

            label.centerXAnchor.constraint(equalTo: contentView.centerXAnchor),
            label.topAnchor.constraint(equalTo: plusImageView.bottomAnchor, constant: 12),
        ])
    }
}

// MARK: - Focus View (Single Tab)
struct ChromeTabFocusView: View {
    let tabId: UUID
    @ObservedObject var viewModel: BrowserViewModel
    @ObservedObject var historyVM: HistoryViewModel
    let onBackToOverview: () -> Void

    var tab: Tab? {
        viewModel.tabs.first { $0.id == tabId }
    }

    var body: some View {
        GeometryReader { geometry in
            if let tab = tab {
                SingleTabRootView(
                    tab: tab,
                    viewModel: viewModel,
                    historyVM: historyVM,
                    containerSize: CGSize(
                        width: max(geometry.size.width, 1),
                        height: max(geometry.size.height, 1)
                    )
                )
                .frame(width: geometry.size.width, height: geometry.size.height)
                .ignoresSafeArea()
            }
        }
        .ignoresSafeArea()
    }
}

// MARK: - Color Helpers
extension Color {
    var uiColor: UIColor? {
        let components = self.cgColor?.components
        guard let components = components, components.count >= 3 else { return nil }
        return UIColor(
            red: components[0],
            green: components[1],
            blue: components[2],
            alpha: components.count >= 4 ? components[3] : 1.0
        )
    }
}
