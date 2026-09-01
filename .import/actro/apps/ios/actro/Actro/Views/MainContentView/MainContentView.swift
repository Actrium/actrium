import SwiftUI

private let logger = AppLogger.ui
import WebKit
import UIKit

@MainActor
struct MainContentView: View {
    @ObservedObject var viewModel: BrowserViewModel
    @ObservedObject var historyVM: HistoryViewModel
    @Binding var isTabOverviewVisible: Bool
    @State private var isRightPanelVisible: Bool = true
    @State private var addressInputText: String = ""
    @State private var isAddressBarEditing: Bool = false
    @State private var isAddressBarFirstResponder: Bool = false

    init(viewModel: BrowserViewModel, historyVM: HistoryViewModel, isTabOverviewVisible: Binding<Bool>) {
        self.viewModel = viewModel
        self.historyVM = historyVM
        _isTabOverviewVisible = isTabOverviewVisible
    }

    var body: some View {
        GeometryReader { geometry in
            let isLeftPanelVisible = viewModel.isLeftPanelVisible(for: viewModel.currentTabId)
            TabContentView(
                viewModel: viewModel,
                historyVM: historyVM,
                isLeftPanelVisible: isLeftPanelVisible,
                isRightPanelVisible: isRightPanelVisible
            )
            .frame(maxWidth: .infinity, maxHeight: .infinity)
            .animation(.easeInOut(duration: 0.2), value: isRightPanelVisible)
            .contentShape(Rectangle())
            .onTapGesture {
                // 在 Overview 模式下，不拦截点击，交给外层处理
                guard !isTabOverviewVisible else { return }
                if isAddressBarEditing {
                    logger.debug("⌨️ 取消地址栏编辑")
                    isAddressBarEditing = false
                    isAddressBarFirstResponder = false
                }
            }
            // 注意：拖拽处理已移至 ReactNativeTabHostView 的 Glass View
            .toolbar {
                ToolbarItemGroup(placement: .principal) {
                    let totalWidth = geometry.size.width
                    // 地址栏容器宽度为整个窗口宽度的 50%
                    let addressBarContainerWidth = totalWidth * 0.5
                    
                    HStack {
                        Spacer(minLength: 0)
                        
                        AddressBarContainer(
                            titleText: .constant(viewModel.currentTab?.title ?? TM.active.blackTab()),
                            inputText: $addressInputText,
                            isEditing: $isAddressBarEditing,
                            isFirstResponder: $isAddressBarFirstResponder,
                            toggleLeftPanel: {
                                withAnimation(.easeInOut(duration: 0.2)) {
                                    viewModel.toggleLeftPanelForCurrentTab()
                                }
                            },
                            refreshAction: {
                                // TODO: 刷新逻辑
                            }
                        )
                        .frame(width: addressBarContainerWidth)
                        // 测量 Toolbar 高度
                        .background(
                            GeometryReader { toolbarGeometry in
                                Color.clear.onAppear {
                                    // 获取 toolbar 的实际高度
                                    let height = toolbarGeometry.size.height
                                    if height > 0 {
                                        viewModel.toolbarHeight = height
                                        logger.debug("📏 Toolbar 高度: \(height)")
                                    }
                                }
                            }
                        )
                        
                        Spacer(minLength: 0)
                    }
                    .frame(maxWidth: .infinity)
                }
                
                ToolbarItemGroup(placement: .primaryAction) {
                    Button(action: {}) {
                        Image(systemName: "square.and.arrow.up")
                    }
                    .help("分享")
                    
                    Button(action: {
                        withAnimation(.easeInOut(duration: 2.0)) {
                            _ = viewModel.addNewTab()
                        }
                    }) {
                        Image(systemName: "plus")
                    }
                    .help("新建标签页")
                    
                    Button(action: {
                        isTabOverviewVisible.toggle()
                    }) {
                        Image(systemName: "square.grid.2x2")
                    }
                    .help("标签总览")
                }
            }
        }
    }
}

// MARK: - Address Bar Container

private struct AddressBarContainer: View {
    @Binding var titleText: String
    @Binding var inputText: String
    @Binding var isEditing: Bool
    @Binding var isFirstResponder: Bool
    var toggleLeftPanel: () -> Void
    var refreshAction: () -> Void
    
    private let placeholder = "Search from bookmark or enter app id"
    @State private var focusRingScale: CGFloat = 1.0
    @State private var focusRingOpacity: Double = 0.0
    
    var body: some View {
        Group {
            if isEditing {
                // 编辑状态：放大镜 + 输入框
                HStack(spacing: 10) {
                    Image(systemName: "magnifyingglass")
                        .foregroundStyle(Color.white.opacity(0.85))
                        .frame(width: 16, height: 16)
                    
                    FocusableTextField(
                        text: $inputText,
                        placeholder: placeholder,
                        isFirstResponder: $isFirstResponder
                    )
                }
            } else {
                // 非编辑状态：左右图标 + 中间标题
                HStack(spacing: 10) {
                    Button(action: toggleLeftPanel) {
                        Image(systemName: "list.bullet")
                            .frame(width: 20, height: 20)
                    }
                    .buttonStyle(.borderless)
                    .help("切换左侧面板")
                    
                    Text(titleText)
                        .foregroundColor(.white)
                        .multilineTextAlignment(.center)
                        .frame(maxWidth: .infinity)
                        .contentShape(Rectangle())
                        .onTapGesture {
                            if !isEditing {
                                isEditing = true
                                inputText = ""
                                DispatchQueue.main.async {
                                    isFirstResponder = true
                                }
                            }
                        }
                    
                    Button(action: refreshAction) {
                        Image(systemName: "arrow.clockwise")
                    }
                    .buttonStyle(.borderless)
                    .help("刷新")
                }
            }
        }
        .padding(.horizontal, 12)
        .padding(.vertical, 4)
        .frame(maxWidth: .infinity, minHeight: 28)
        .overlay(
            Capsule()
                .stroke(
                    Color(red: 0.18, green: 0.56, blue: 0.96)
                        .opacity(focusRingOpacity),
                    lineWidth: 2.8
                )
                .scaleEffect(focusRingScale)
                .padding(.vertical, -4)
        )
        .onChange(of: isFirstResponder) {
            if !isFirstResponder && isEditing {
                isEditing = false
            }
        }
        .onChange(of: isEditing) {
            if isEditing {
                // 从略大的、透明的蓝圈收缩到正常大小并渐显
                focusRingScale = 1.08
                focusRingOpacity = 0.0
                withAnimation(.spring(response: 0.25, dampingFraction: 0.7, blendDuration: 0.1)) {
                    focusRingScale = 1.0
                    focusRingOpacity = 1.0
                }
            } else {
                withAnimation(.easeOut(duration: 0.15)) {
                    focusRingOpacity = 0.0
                }
            }
        }
    }
    
private func endEditing() {
    isEditing = false
    isFirstResponder = false
}
}

// MARK: - Tab Content View

struct TabContentView: View {
    @ObservedObject var viewModel: BrowserViewModel
    @ObservedObject var historyVM: HistoryViewModel
    let isLeftPanelVisible: Bool
    let isRightPanelVisible: Bool
    @State private var reactCoordinator: ReactNativeRootRepresentable.Coordinator?

    var body: some View {
        // All tabs (including system tabs like History/Bookmarks) are rendered by React Native
        GeometryReader { geometry in
            ReactNativeTabHostView(
                viewModel: viewModel,
                historyVM: historyVM,
                isLeftPanelVisible: isLeftPanelVisible,
                isRightPanelVisible: isRightPanelVisible,
                containerSize: geometry.size,
                onCoordinatorReady: { coordinator in
                    reactCoordinator = coordinator
                    logger.debug("✅ React Native Coordinator 已就绪")
                }
            )
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }
}

// MARK: - Tab Overview

// TabOverviewView 已移至独立文件 TabOverviewView.swift
private struct FocusableTextField: UIViewRepresentable {
    @Binding var text: String
    var placeholder: String
    @Binding var isFirstResponder: Bool
    
    func makeCoordinator() -> Coordinator {
        Coordinator(self)
    }
    
    func makeUIView(context: Context) -> UITextField {
        let field = UITextField()
        field.borderStyle = .none
        field.backgroundColor = .clear
        field.font = UIFont.systemFont(ofSize: UIFont.systemFontSize)
        field.placeholder = placeholder
        field.delegate = context.coordinator
        field.returnKeyType = .search
        field.autocorrectionType = .no
        field.autocapitalizationType = .none
        field.textColor = .white
        
        // Add done button to keyboard
        let toolbar = UIToolbar()
        toolbar.sizeToFit()
        let flexSpace = UIBarButtonItem(barButtonSystemItem: .flexibleSpace, target: nil, action: nil)
        let doneButton = UIBarButtonItem(barButtonSystemItem: .done, target: context.coordinator, action: #selector(Coordinator.doneButtonTapped))
        toolbar.items = [flexSpace, doneButton]
        field.inputAccessoryView = toolbar
        
        return field
    }
    
    func updateUIView(_ uiView: UITextField, context: Context) {
        if uiView.text != text {
            uiView.text = text
        }
        uiView.placeholder = placeholder
        
        if isFirstResponder && !uiView.isFirstResponder {
            uiView.becomeFirstResponder()
        } else if !isFirstResponder && uiView.isFirstResponder {
            uiView.resignFirstResponder()
        }
    }
    
    @MainActor
    final class Coordinator: NSObject, UITextFieldDelegate {
        var parent: FocusableTextField
        
        init(_ parent: FocusableTextField) {
            self.parent = parent
        }
        
        func textField(_ textField: UITextField, shouldChangeCharactersIn range: NSRange, replacementString string: String) -> Bool {
            // 过滤掉换行符
            let filtered = string.components(separatedBy: .newlines).joined(separator: " ")
            if filtered != string {
                let currentText = textField.text ?? ""
                let newText = (currentText as NSString).replacingCharacters(in: range, with: filtered)
                textField.text = newText
                parent.text = newText
                return false
            }
            return true
        }
        
        func textFieldDidChangeSelection(_ textField: UITextField) {
            parent.text = textField.text ?? ""
        }
        
        func textFieldDidEndEditing(_ textField: UITextField) {
            parent.isFirstResponder = false
        }
        
        func textFieldShouldReturn(_ textField: UITextField) -> Bool {
            textField.resignFirstResponder()
            return true
        }
        
        @objc func doneButtonTapped() {
            parent.isFirstResponder = false
        }
    }
}

// MARK: - Draggable Divider

struct DraggableDivider: View {
    @Binding var panelWidth: CGFloat
    let minWidth: CGFloat
    let maxWidth: CGFloat
    var isTrailing: Bool = false
    
    @State private var isDragging = false
    @State private var startWidth: CGFloat = 0
    
    var body: some View {
        ResizableHandleView(
            panelWidth: $panelWidth,
            minWidth: minWidth,
            maxWidth: maxWidth,
            isTrailing: isTrailing,
            isDragging: $isDragging,
            startWidth: $startWidth
        )
        .frame(width: 3) // 直接让 NSView 占据 3pt 宽度
    }
}

// Use UIViewRepresentable for touch handling
private struct ResizableHandleView: UIViewRepresentable {
    @Binding var panelWidth: CGFloat
    let minWidth: CGFloat
    let maxWidth: CGFloat
    let isTrailing: Bool
    @Binding var isDragging: Bool
    @Binding var startWidth: CGFloat
    
    func makeUIView(context: Context) -> ResizableHandle {
        let view = ResizableHandle()
        let gesture = UIPanGestureRecognizer(target: context.coordinator, action: #selector(Coordinator.handlePan(_:)))
        view.addGestureRecognizer(gesture)
        return view
    }
    
    func updateUIView(_ uiView: ResizableHandle, context: Context) {
        uiView.isDragging = isDragging
        context.coordinator.parent = self
    }
    
    func makeCoordinator() -> Coordinator {
        Coordinator(self)
    }
    
    @MainActor
    class Coordinator: NSObject {
        var parent: ResizableHandleView
        
        init(_ parent: ResizableHandleView) {
            self.parent = parent
        }
        
        @objc func handlePan(_ gesture: UIPanGestureRecognizer) {
            switch gesture.state {
            case .began:
                logger.debug("🎯 开始拖拽 - isTrailing: \(self.parent.isTrailing), 初始宽度: \(self.parent.panelWidth)")
                parent.isDragging = true
                parent.startWidth = parent.panelWidth
                
            case .changed:
                let translation = gesture.translation(in: gesture.view)
                let delta = parent.isTrailing ? -translation.x : translation.x
                let newWidth = parent.startWidth + delta
                let clampedWidth = min(max(newWidth, parent.minWidth), parent.maxWidth)
                
                // 直接更新，不通过 DispatchQueue
                parent.panelWidth = clampedWidth
                
                // 每隔一段时间打印一次调试信息
                if Int(translation.x) % 10 == 0 {
                    logger.debug("📏 拖拽中 - translation: \(translation.x), delta: \(delta), newWidth: \(clampedWidth)")
                }
                
            case .ended, .cancelled, .failed:
                logger.debug("✅ 拖拽结束 - 最终宽度: \(self.parent.panelWidth)")
                parent.isDragging = false
                
            default:
                break
            }
        }
    }
}

private class ResizableHandle: UIView {
    var isDragging = false {
        didSet {
            if isDragging != oldValue {
                setNeedsDisplay()
            }
        }
    }
    
    override init(frame: CGRect) {
        super.init(frame: frame)
        self.backgroundColor = .clear
        self.isUserInteractionEnabled = true
    }
    
    required init?(coder: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }
    
    override func draw(_ rect: CGRect) {
        super.draw(rect)
        
        guard let context = UIGraphicsGetCurrentContext() else { return }
        
        // 绘制渐变分隔线（上下淡出，中间明显）
        // 线条在 NSView 的中心位置
        let lineX = (bounds.width - 1) / 2
        let lineWidth: CGFloat = 1
        let height = bounds.height
        
        // 创建渐变色（上下透明，中间不透明）
        // 使用更柔和的透明度
        let colorSpace = CGColorSpaceCreateDeviceRGB()
        let baseAlpha: CGFloat = 0.3  // 基础透明度降低到 30%
        let draggingAlpha: CGFloat = 0.5  // 拖拽时透明度 50%
        let alpha: CGFloat = isDragging ? draggingAlpha : baseAlpha
        let separatorColor = UIColor.separator.withAlphaComponent(alpha)
        
        let colors = [
            UIColor.clear.cgColor,                                    // 顶部透明
            separatorColor.cgColor,                                   // 20% 处开始显示
            separatorColor.cgColor,                                   // 80% 处结束显示
            UIColor.clear.cgColor                                     // 底部透明
        ] as CFArray
        
        let locations: [CGFloat] = [0.0, 0.2, 0.8, 1.0]
        
        guard let gradient = CGGradient(colorsSpace: colorSpace, colors: colors, locations: locations) else {
            return
        }
        
        // 绘制渐变（在中心位置画 1pt 宽的线）
        context.saveGState()
        context.clip(to: CGRect(x: lineX, y: 0, width: lineWidth, height: height))
        context.drawLinearGradient(
            gradient,
            start: CGPoint(x: lineX, y: 0),
            end: CGPoint(x: lineX, y: height),
            options: []
        )
        context.restoreGState()
    }
}

// MARK: - Panel Views
