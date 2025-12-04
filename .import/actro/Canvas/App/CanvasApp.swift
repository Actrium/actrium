import SwiftUI

@main
struct CanvasApp: App {
    @StateObject private var viewModel = BrowserViewModel()

    init() {
        // Suppress common system warnings in development
        UserDefaults.standard.set(false, forKey: "NSConstraintBasedLayoutLogUnsatisfiable")
        
        // 过滤掉不重要的系统错误日志
        #if DEBUG
        // 这些是系统级别的警告，不影响应用功能
        print("⚠️ 注意：控制台中关于 Metal shader、字体沙盒、WebKit 的警告是系统级警告，不影响应用运行")
        #endif
    }
    
    var body: some Scene {
        WindowGroup {
            ContentView(viewModel: viewModel)
//            ZoomAnimationDemoView()
        }
        .commands {
            // 添加一些有用的调试命令
            CommandGroup(replacing: .help) {
                Button("关于应用状态") {
                    print("✅ 应用运行正常")
                    print("✅ 所有视图已加载")
                }
            }

            // 快捷键：Cmd + T 新建标签页
            CommandGroup(after: .newItem) {
                Button("新建标签页") {
                    withAnimation {
                        _ = viewModel.addNewTab()
                    }
                }
                .keyboardShortcut("t", modifiers: .command)

                Button("关闭当前标签页") {
                    withAnimation {
                        viewModel.closeCurrentTab()
                    }
                }
                .keyboardShortcut("w", modifiers: .command)
            }

            // 快捷键：Cmd + 1...8 / 9 切换标签页
            CommandGroup(after: .sidebar) {
                ForEach(1...8, id: \.self) { index in
                    Button("切换到标签 \(index)") {
                        switchToTab(at: index - 1)
                    }
                    .keyboardShortcut(
                        KeyEquivalent(Character("\(index)")),
                        modifiers: .command
                    )
                }

                Button("切换到最后一个标签") {
                    switchToLastTab()
                }
                .keyboardShortcut("9", modifiers: .command)
            }
        }
    }

    private func switchToTab(at index: Int) {
        guard index >= 0, index < viewModel.tabs.count else { return }
        let tab = viewModel.tabs[index]
        withAnimation {
            viewModel.switchToTab(id: tab.id)
        }
    }

    private func switchToLastTab() {
        guard !viewModel.tabs.isEmpty else { return }
        let lastIndex = viewModel.tabs.count - 1
        let tab = viewModel.tabs[lastIndex]
        withAnimation {
            viewModel.switchToTab(id: tab.id)
        }
    }
}
