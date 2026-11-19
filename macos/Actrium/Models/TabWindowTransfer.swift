import AppKit

@MainActor
final class TabTransferOutlineView: NSOutlineView {
    weak var transferOwner: (any TabWindowTransferOwner)?
}

@MainActor
protocol TabWindowTransferOwner: AnyObject {
    var tabs: [Tab] { get }
    func insertTabFromCrossWindow(_ tab: Tab, at index: Int, groupId: UUID?)
    func removeTabById(_ id: UUID)
    func createGroupFromTabs(_ ids: [UUID], name: String, color: TabGroupColor)
}

/// Only native window owners supply transfer data; pasteboard contents never supply identity.
@MainActor
enum TabWindowTransfer {
    static func source(for draggingSource: Any?) -> (any TabWindowTransferOwner)? {
        (draggingSource as? TabTransferOutlineView)?.transferOwner
    }

    static func moveTab(
        _ id: UUID, from source: any TabWindowTransferOwner, to destination: any TabWindowTransferOwner,
        at index: Int, groupId: UUID?
    ) -> Bool {
        guard source !== destination, let tab = source.tabs.first(where: { $0.id == id }) else { return false }
        destination.insertTabFromCrossWindow(copy(tab), at: index, groupId: groupId)
        source.removeTabById(id)
        return true
    }

    static func moveGroup(
        _ group: TabGroup, from source: any TabWindowTransferOwner, to destination: any TabWindowTransferOwner
    ) -> Bool {
        let originals = source.tabs.filter { $0.groupId == group.id }
        guard source !== destination, !originals.isEmpty else { return false }
        let copies = originals.map(copy)
        for tab in copies {
            destination.insertTabFromCrossWindow(tab, at: destination.tabs.count, groupId: nil)
        }
        destination.createGroupFromTabs(copies.map(\.id), name: group.name, color: group.color)
        for tab in originals { source.removeTabById(tab.id) }
        return true
    }

    private static func copy(_ tab: Tab) -> Tab {
        var result = Tab(
            title: tab.title, url: tab.url, appId: tab.appId, serviceCallUrl: tab.serviceCallUrl,
            legacyActorBridge: tab.legacyActorBridge, htmlContent: tab.htmlContent,
            systemTabType: tab.systemTabType, guestScript: tab.guestScript,
            leftPanelScript: tab.leftPanelScript, rightPanelScript: tab.rightPanelScript,
            unifiedScript: tab.unifiedScript, guestExtractedPath: tab.guestExtractedPath,
            guestEntry: tab.guestEntry, capabilityManifestJSON: tab.capabilityManifestJSON,
            localStorageOrigin: tab.localStorageOrigin,
            localStoragePublisherIdentity: tab.localStoragePublisherIdentity,
            capabilityRevision: tab.capabilityRevision, contractName: tab.contractName,
            contractVersion: tab.contractVersion, permissions: tab.permissions,
            reloadNonce: tab.reloadNonce, leftPanelDefaultVisible: tab.leftPanelDefaultVisible,
            rightPanelDefaultVisible: tab.rightPanelDefaultVisible,
            backgroundColor: tab.backgroundColor, appIcon: tab.appIcon
        )
        result.realmProfileSigner = tab.realmProfileSigner
        result.historyPageIndex = tab.historyPageIndex
        return result
    }
}

/// A native-only request passed directly to the window creator, never through UserDefaults.
@MainActor
final class TabGroupWindowTransfer {
    private weak var source: (any TabWindowTransferOwner)?
    private let group: TabGroup

    init(source: any TabWindowTransferOwner, group: TabGroup) {
        self.source = source
        self.group = group
    }

    func move(to destination: any TabWindowTransferOwner) -> Bool {
        guard let source else { return false }
        return TabWindowTransfer.moveGroup(group, from: source, to: destination)
    }
}
