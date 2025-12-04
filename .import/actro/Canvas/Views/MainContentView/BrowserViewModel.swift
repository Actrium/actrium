import Foundation
import AppKit // Import AppKit for NSImage
import Combine

@MainActor
class BrowserViewModel: ObservableObject {
    @Published var tabs: [Tab] = []
    @Published var currentTabId: UUID?
    // 默认所有标签页左侧面板可见；仅在用户切换时记录覆盖值
    @Published private var leftPanelVisibility: [UUID: Bool] = [:]

    // MARK: - 螺旋位置管理

    /// Tab ID → 螺旋位置索引的映射
    @Published var tabSpiralPositions: [UUID: Int] = [:]

    // MARK: - 动画状态管理
    
    /// 是否正在执行 Tab ↔ Overview 转场动画
    @Published var isTransitioning = false
    
    /// 正在执行动画的标签页 ID
    @Published var transitioningTabId: UUID?
    
    /// 动画几何信息（位置、尺寸等）
    @Published var animationGeometry = TabAnimationGeometry()
    
    /// Toolbar 实际高度（从视图测量得到）
    @Published var toolbarHeight: CGFloat = 52 // 默认值，会在视图中更新
    
    var currentTab: Tab? {
        guard let currentTabId = currentTabId else { return nil }
        return tabs.first(where: { $0.id == currentTabId })
    }
    
    init() {
        // 创建初始标签页
        let defaultTab = Tab(
            title: "human in loop - coding for ever",
            htmlContent: """
                <!DOCTYPE html>
                <html>
                <head>
                    <meta charset="utf-8">
                    <meta name="viewport" content="width=device-width, initial-scale=1.0">
                    <style>
                        body {
                            font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, Oxygen, Ubuntu, Cantarell, sans-serif;
                            padding: 40px;
                            color: #ffffff;
                            margin: 0;
                            display: flex;
                            justify-content: center;
                            align-items: center;
                            min-height: 100vh;
                            background: linear-gradient(135deg, #1e1e1e 0%, #2d2d2d 100%);
                        }
                        .container {
                            max-width: 800px;
                            text-align: center;
                        }
                        h1 {
                            color: #61afef;
                            font-size: 2.5em;
                            margin-bottom: 20px;
                        }
                        p {
                            line-height: 1.8;
                            font-size: 1.2em;
                            color: #abb2bf;
                        }
                        .highlight {
                            color: #98c379;
                            font-weight: bold;
                        }
                    </style>
                </head>
                <body>
                    <div class="container">
                        <h1>🚀 Welcome to Your Browser</h1>
                        <p>Human in loop - <span class="highlight">coding forever</span></p>
                        <p>Click the + button to create a new tab, or explore the bookmarks in the left panel.</p>
                    </div>
                </body>
                </html>
                """
        )
        tabs.append(defaultTab)
        currentTabId = defaultTab.id

        // 初始化螺旋位置
        initializeSpiralPositions()
    }
    
    @discardableResult
    func addNewTab() -> Tab {
        print("🆕 开始创建新标签页...")
        let newTab = Tab(
            title: "New Tab \(tabs.count + 1)",
            htmlContent: """
                <!DOCTYPE html>
                <html>
                <head>
                    <meta charset="utf-8">
                    <meta name="viewport" content="width=device-width, initial-scale=1.0">
                    <style>
                        body {
                            font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, Oxygen, Ubuntu, Cantarell, sans-serif;
                            padding: 40px;
                            color: #ffffff;
                            margin: 0;
                            display: flex;
                            justify-content: center;
                            align-items: center;
                            min-height: 100vh;
                            background: linear-gradient(135deg, #1e1e1e 0%, #2d2d2d 100%);
                        }
                        .container {
                            max-width: 800px;
                            text-align: center;
                        }
                        h1 {
                            color: #e06c75;
                            font-size: 2.5em;
                            margin-bottom: 20px;
                        }
                        p {
                            line-height: 1.8;
                            font-size: 1.2em;
                            color: #abb2bf;
                        }
                        .tab-number {
                            color: #c678dd;
                            font-weight: bold;
                        }
                    </style>
                </head>
                <body>
                    <div class="container">
                        <h1>📄 New Tab</h1>
                        <p>This is <span class="tab-number">Tab #\(tabs.count + 1)</span></p>
                        <p>Start typing in the address bar to navigate to a new page.</p>
                    </div>
                </body>
                </html>
                """
        )
        tabs.append(newTab)
        currentTabId = newTab.id

        // 简化：直接添加到末尾
        tabSpiralPositions[newTab.id] = tabs.count - 1

        print("✅ 新标签页已创建: \(newTab.title), 位置: \(tabs.count - 1)")
        return newTab
    }
    
    func switchToTab(id: UUID) {
        if let tab = tabs.first(where: { $0.id == id }) {
            print("🔄 切换到标签页: \(tab.title)")
        }
        currentTabId = id
    }
    
    func closeTab(id: UUID) {
        // 获取被删除标签的螺旋位置
        guard let closedPosition = tabSpiralPositions[id] else {
            print("⚠️ 无法找到要关闭的 tab 的位置")
            return
        }

        // 如果这是最后一个标签页，关闭后退出应用
        if tabs.count == 1 {
            print("🚪 关闭最后一个标签页，应用即将退出")
            NSApplication.shared.terminate(nil)
            return
        }

        // 如果关闭的是当前标签，切换到相邻标签
        if currentTabId == id {
            if let index = tabs.firstIndex(where: { $0.id == id }) {
                if index > 0 {
                    currentTabId = tabs[index - 1].id
                } else if index < tabs.count - 1 {
                    currentTabId = tabs[index + 1].id
                }
            }
        }

        // 删除标签页
        tabs.removeAll(where: { $0.id == id })

        // 删除该标签的位置映射
        tabSpiralPositions.removeValue(forKey: id)

        // 重新整理位置：将所有大于被删除位置的位置减1
        for tabId in tabs.map({ $0.id }) {
            if let position = tabSpiralPositions[tabId], position > closedPosition {
                tabSpiralPositions[tabId] = position - 1
            }
        }

        print("🗑️ 关闭标签页，位置 \(closedPosition)，剩余 \(tabs.count) 个标签页")
    }

    func reorderTab(fromIndex source: Int, toIndex destination: Int) {
        guard source >= 0, source < tabs.count,
              destination >= 0, destination <= tabs.count,
              source != destination else { return }

        let movedTab = tabs.remove(at: source)
        let insertIndex = destination > source ? destination - 1 : destination
        tabs.insert(movedTab, at: insertIndex)

        print("🔄 标签页重排: 从索引 \(source) 移动到 \(insertIndex)")
    }

    // MARK: - 螺旋位置管理方法

    /// 初始化所有 tabs 的螺旋位置
    func initializeSpiralPositions() {
        for (index, tab) in tabs.enumerated() {
            tabSpiralPositions[tab.id] = index
        }
        print("📍 初始化螺旋位置: \(tabs.count) 个 tabs")
    }

    /// 交换两个 tab 的螺旋位置（只支持 tab ↔ tab）
    func swapSpiralPositions(tabId1: UUID?, tabId2: UUID?) {
        guard let id1 = tabId1, let id2 = tabId2 else {
            print("⚠️ 无法交换：需要两个有效的 tab ID")
            return
        }

        guard let pos1 = tabSpiralPositions[id1],
              let pos2 = tabSpiralPositions[id2] else {
            print("⚠️ 无法交换：找不到螺旋位置")
            return
        }

        tabSpiralPositions[id1] = pos2
        tabSpiralPositions[id2] = pos1
        print("🔄 交换螺旋位置: \(pos1) ↔ \(pos2)")
    }

    /// 根据螺旋位置获取排序后的 tabs
    func tabsSortedBySpiralPosition() -> [(tab: Tab, position: Int)] {
        return tabs.compactMap { tab in
            guard let position = tabSpiralPositions[tab.id] else { return nil }
            return (tab, position)
        }.sorted { $0.position < $1.position }
    }
    
    func updateCurrentTabTitle(_ title: String) {
        guard let currentTabId = currentTabId,
              let index = tabs.firstIndex(where: { $0.id == currentTabId }) else { return }
        tabs[index].title = title
    }

    func closeCurrentTab() {
        guard let id = currentTabId else { return }
        closeTab(id: id)
    }

    // MARK: - Per-Tab Left Panel Visibility

    func isLeftPanelVisible(for tabId: UUID?) -> Bool {
        guard let id = tabId else { return true }
        return leftPanelVisibility[id] ?? true
    }

    func toggleLeftPanelForCurrentTab() {
        guard let id = currentTabId else { return }
        let current = leftPanelVisibility[id] ?? true
        leftPanelVisibility[id] = !current
    }

    // MARK: - .actr 应用加载

    /// 从本地 .actr 包创建一个新标签并装载应用基础信息
    func importACTRPackage(from fileURL: URL) {
        let resolvedURL = fileURL.standardizedFileURL
        guard FileManager.default.fileExists(atPath: resolvedURL.path) else {
            print("❌ 找不到 .actr 文件：\(resolvedURL.path)")
            return
        }

        let tempDir = FileManager.default.temporaryDirectory
            .appendingPathComponent("actr-\(UUID().uuidString)", isDirectory: true)

        do {
            try FileManager.default.createDirectory(at: tempDir, withIntermediateDirectories: true)
            try unzipACTR(archive: resolvedURL, to: tempDir)

            let manifestInfo = try readManifestJSON(fromRoot: tempDir)
            let tabTitle = manifestInfo.name ?? resolvedURL.deletingPathExtension().lastPathComponent

            // 读取 guest 脚本
            let guestScript = readGuestScript(fromRoot: tempDir)
            let leftPanelScript = readScript(fromRoot: tempDir, fileName: "menu.js")
            let rightPanelScript = readScript(fromRoot: tempDir, fileName: "ext.js")

            if let leftScript = leftPanelScript, let rightScript = rightPanelScript {
                // 新架构：左右分离的面板
                appendACTRTab(
                    title: tabTitle,
                    sourceURL: resolvedURL,
                    leftPanelScript: leftScript,
                    rightPanelScript: rightScript,
                    rightPanelDefaultVisible: manifestInfo.rightPanelDefaultVisible
                )
            } else if let script = guestScript {
                // 旧架构：兼容单一的 guest.js
                appendACTRTab(title: tabTitle, sourceURL: resolvedURL, guestScript: script)
            } else {
                // 没有脚本，显示应用信息页面
                let html = renderACTRInfoHTML(
                    name: manifestInfo.name ?? "Unknown App",
                    version: manifestInfo.version ?? "0.0.0",
                    description: manifestInfo.description ?? "No description provided.",
                    sourcePath: resolvedURL.path
                )
                appendACTRTab(title: tabTitle, sourceURL: resolvedURL, htmlContent: html)
            }
        } catch {
            print("❌ .actr 装载失败：\(error.localizedDescription)")
            let fallbackHTML = renderACTRInfoHTML(
                name: resolvedURL.deletingPathExtension().lastPathComponent,
                version: "unknown",
                description: "加载失败：\(error.localizedDescription)",
                sourcePath: resolvedURL.path
            )
            appendACTRTab(
                title: resolvedURL.deletingPathExtension().lastPathComponent,
                sourceURL: resolvedURL,
                htmlContent: fallbackHTML
            )
        }
    }

    private func appendACTRTab(
        title: String,
        sourceURL: URL,
        htmlContent: String? = nil,
        guestScript: String? = nil,
        leftPanelScript: String? = nil,
        rightPanelScript: String? = nil,
        rightPanelDefaultVisible: Bool = false
    ) {
        let newTab = Tab(
            title: title,
            url: sourceURL,
            htmlContent: htmlContent,
            guestScript: guestScript,
            leftPanelScript: leftPanelScript,
            rightPanelScript: rightPanelScript,
            rightPanelDefaultVisible: rightPanelDefaultVisible
        )
        tabs.append(newTab)
        currentTabId = newTab.id
        tabSpiralPositions[newTab.id] = tabs.count - 1
        print("✅ 已加载 .actr：\(title)")

        if let leftScript = leftPanelScript {
            print("  ├─ 左侧面板脚本: \(leftScript.count) 字节")
        }
        if let rightScript = rightPanelScript {
            print("  ├─ 右侧扩展面板脚本: \(rightScript.count) 字节")
            print("  └─ 右侧面板默认\(rightPanelDefaultVisible ? "显示" : "隐藏")")
        }
    }

    private func unzipACTR(archive: URL, to destination: URL) throws {
        let process = Process()
        process.executableURL = URL(fileURLWithPath: "/usr/bin/unzip")
        process.arguments = ["-q", archive.path, "-d", destination.path]
        try process.run()
        process.waitUntilExit()
        if process.terminationStatus != 0 {
            throw NSError(
                domain: "ACTRUnzipError",
                code: Int(process.terminationStatus),
                userInfo: [NSLocalizedDescriptionKey: "unzip 失败，退出码 \(process.terminationStatus)"]
            )
        }
    }

    private func readManifestJSON(fromRoot root: URL) throws -> (name: String?, version: String?, description: String?, rightPanelDefaultVisible: Bool) {
        guard let manifestURL = findManifestJSON(in: root) else {
            throw NSError(
                domain: "ACTRPackage",
                code: 1,
                userInfo: [NSLocalizedDescriptionKey: "包内未找到 manifest.json"]
            )
        }

        let data = try Data(contentsOf: manifestURL)
        let json = try JSONSerialization.jsonObject(with: data, options: []) as? [String: Any]
        let name = json?["name"] as? String
        let version = json?["version"] as? String
        let description = json?["description"] as? String

        // 读取 layout 配置
        let layout = json?["layout"] as? [String: Any]
        let rightPanelDefaultVisible = layout?["rightPanelDefaultVisible"] as? Bool ?? false

        return (name, version, description, rightPanelDefaultVisible)
    }

    private func findManifestJSON(in root: URL) -> URL? {
        let fm = FileManager.default
        if let direct = try? fm.contentsOfDirectory(at: root, includingPropertiesForKeys: nil).first(where: { $0.lastPathComponent == "manifest.json" }) {
            return direct
        }

        if let enumerator = fm.enumerator(at: root, includingPropertiesForKeys: nil) {
            for case let fileURL as URL in enumerator {
                if fileURL.lastPathComponent == "manifest.json" {
                    return fileURL
                }
            }
        }
        return nil
    }

    /// 通用脚本读取方法
    private func readScript(fromRoot root: URL, fileName: String) -> String? {
        let fm = FileManager.default

        // 首先在根目录查找
        if let direct = try? fm.contentsOfDirectory(at: root, includingPropertiesForKeys: nil).first(where: { $0.lastPathComponent == fileName }) {
            if let script = try? String(contentsOf: direct, encoding: .utf8) {
                print("✅ 找到 \(fileName): \(direct.path)")
                return script
            }
        }

        // 如果根目录没有，递归查找
        if let enumerator = fm.enumerator(at: root, includingPropertiesForKeys: nil) {
            for case let fileURL as URL in enumerator {
                if fileURL.lastPathComponent == fileName {
                    if let script = try? String(contentsOf: fileURL, encoding: .utf8) {
                        print("✅ 找到 \(fileName): \(fileURL.path)")
                        return script
                    }
                }
            }
        }

        return nil
    }

    /// 读取 guest.js (向后兼容)
    private func readGuestScript(fromRoot root: URL) -> String? {
        return readScript(fromRoot: root, fileName: "guest.js")
    }

    private func renderACTRInfoHTML(name: String, version: String, description: String, sourcePath: String) -> String {
        """
        <!DOCTYPE html>
        <html>
        <head>
            <meta charset="utf-8">
            <meta name="viewport" content="width=device-width, initial-scale=1.0">
            <style>
                body {
                    font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, Oxygen, Ubuntu, Cantarell, sans-serif;
                    padding: 32px;
                    color: #e6e8ee;
                    margin: 0;
                    min-height: 100vh;
                    background: linear-gradient(135deg, #0f172a 0%, #111827 50%, #0b1220 100%);
                }
                .card {
                    max-width: 860px;
                    margin: 0 auto;
                    padding: 28px;
                    background: rgba(255, 255, 255, 0.04);
                    border: 1px solid rgba(255, 255, 255, 0.08);
                    border-radius: 16px;
                    box-shadow: 0 15px 50px rgba(0,0,0,0.35);
                    backdrop-filter: blur(10px);
                }
                h1 { margin: 0 0 12px 0; font-size: 28px; letter-spacing: -0.02em; }
                .meta { color: #9ca3af; margin-bottom: 18px; }
                .pill { display: inline-block; padding: 6px 10px; margin-right: 8px;
                        border-radius: 10px; background: rgba(96, 165, 250, 0.12); color: #bfdbfe; font-size: 12px; }
                .desc { line-height: 1.6; color: #d1d5db; margin-bottom: 20px; }
                .path { font-family: 'Menlo', 'SFMono-Regular', ui-monospace, monospace;
                        background: rgba(255,255,255,0.05); padding: 10px 12px; border-radius: 10px; }
            </style>
        </head>
        <body>
            <div class="card">
                <h1>🧩 \(name)</h1>
                <div class="meta">
                    <span class="pill">version \(version)</span>
                    <span class="pill">ACTR package</span>
                </div>
                <div class="desc">\(description)</div>
                <div class="path">\(sourcePath)</div>
            </div>
        </body>
        </html>
        """
    }
    
    // MARK: - Snapshot Management
    
    func updateTabSnapshot(image: NSImage?, forTabId tabId: UUID) {
        guard let index = tabs.firstIndex(where: { $0.id == tabId }) else { return }
        tabs[index].snapshot = image
        print("📸 Tab \(tabId) snapshot updated.")
    }
    
    // MARK: - Animation Control
    
    /// 开始 Tab → Overview 转场动画
    /// 
    /// - Parameters:
    ///   - tabId: 标签页 ID
    ///   - view: 可选的 NSView 实例用于捕获快照
    func startTransitionToOverview(fromTabId tabId: UUID, view: NSView? = nil) {
        guard !isTransitioning else { return }
        
        print("🎬 开始转场到 Overview，Tab: \(tabId)")
        isTransitioning = true
        transitioningTabId = tabId
        
        // 捕获当前标签页的快照
        captureCurrentTabSnapshot(from: view)
    }
    
    /// 开始 Overview → Tab 转场动画
    func startTransitionToTab(_ tabId: UUID) {
        guard !isTransitioning else { return }
        
        print("🎬 开始转场到 Tab: \(tabId)")
        isTransitioning = true
        transitioningTabId = tabId
        currentTabId = tabId
    }
    
    /// 完成转场动画
    func finishTransition() {
        print("✅ 转场动画完成")
        isTransitioning = false
        transitioningTabId = nil
    }
    
    /// 捕获当前标签页的快照
    /// 
    /// - Parameter view: 可选的 NSView 实例（如 RCTRootView），如果提供则直接捕获
    @MainActor
    func captureCurrentTabSnapshot(from view: NSView? = nil) {
        guard let tabId = currentTabId else {
            print("⚠️ 无当前标签页，跳过快照捕获")
            return
        }
        
        guard let view = view else {
            print("⚠️ 需要提供 NSView 实例来捕获快照")
            return
        }
        
        print("📸 开始捕获 Tab \(tabId) 的快照...")
        
        // 使用 Task 异步捕获快照
        Task {
            let snapshot = await view.captureSnapshot()
            
            if let snapshot = snapshot {
                // 可选：缩放到合适的尺寸以节省内存
                let scaled = snapshot.scaledToFit(maxDimension: SnapshotQuality.medium.maxDimension)
                updateTabSnapshot(image: scaled, forTabId: tabId)
                print("✅ Tab \(tabId) 快照已保存")
            } else {
                print("❌ Tab \(tabId) 快照捕获失败")
            }
        }
    }
    
    /// 批量捕获所有标签页的快照
    /// 
    /// - Parameter views: Tab ID 到 NSView 的映射
    @MainActor
    func captureAllTabSnapshots(views: [UUID: NSView]) {
        for (tabId, view) in views {
            Task {
                let snapshot = await view.captureSnapshot()
                if let snapshot = snapshot {
                    let scaled = snapshot.scaledToFit(maxDimension: SnapshotQuality.medium.maxDimension)
                    updateTabSnapshot(image: scaled, forTabId: tabId)
                }
            }
        }
    }
}
