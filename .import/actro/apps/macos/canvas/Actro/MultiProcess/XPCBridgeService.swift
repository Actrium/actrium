//
//  XPCBridgeService.swift
//  Canvas
//
//  集中管理所有 XPC 服务通信的核心组件
//
//  架构原则：
//  1. 单一数据源 (SSOT) - 所有 XPC 相关状态集中在此管理
//  2. 单向数据流 - 外部通过 API 请求，内部处理时序和就绪检查
//  3. 自动 Defer - 目标未就绪时自动缓存，就绪后自动发送
//  4. Pull 模式 - XPC 服务启动时主动注册，发送时从状态管理器读取最新状态
//

import Foundation
import Combine
import AppKit

// MARK: - 进程隔离标识符

/// 主进程唯一标识符，用于隔离多个 Canvas 实例
let CanvasHostProcessId = ProcessInfo.processInfo.processIdentifier

// MARK: - 数据结构

/// Tab 配置数据
struct XPCTabConfig {
    let tabId: UUID
    let title: String
    let backgroundColor: String
    let containerSize: CGSize
    let systemAppType: String?
    let unifiedScript: String?
    let leftPanelScript: String?
    let leftPanelVisible: Bool
    let rightPanelVisible: Bool
    let bookmarks: [[String: Any]]
    let recentHistory: [[String: Any]]
    let reloadKey: Int

    func toDictionary(isOverviewMode: Bool) -> [String: Any] {
        var dict: [String: Any] = [
            "tabId": tabId.uuidString,
            "title": title,
            "backgroundColor": backgroundColor,
            "containerWidth": containerSize.width,
            "containerHeight": containerSize.height,
            "isOverviewMode": isOverviewMode,
            "leftPanelVisible": leftPanelVisible,
            "rightPanelVisible": rightPanelVisible,
            "bookmarks": bookmarks,
            "recentHistory": recentHistory,
            "reloadKey": reloadKey
        ]
        if let systemAppType = systemAppType {
            dict["systemAppType"] = systemAppType
        }
        if let unifiedScript = unifiedScript {
            dict["unifiedScript"] = unifiedScript
        }
        if let leftPanelScript = leftPanelScript {
            dict["leftPanelScript"] = leftPanelScript
        }
        return dict
    }
}

/// Tab 就绪状态
struct XPCTabState {
    var serviceId: String?
    var isServiceReady: Bool = false
    var isConnected: Bool = false
    var lastSentSize: CGSize?
    var connectionId: String?

    var isReady: Bool {
        serviceId != nil && isServiceReady && isConnected
    }
}

/// 待发送的请求（目标未就绪时缓存）
enum XPCPendingRequest {
    case tabConfig(XPCTabConfig)
    case sizeChange(CGSize, String)  // size, requestId
}

// MARK: - XPCBridgeService

/// 集中管理所有 XPC 服务通信的核心组件
///
/// 使用方式：
/// 1. App 启动时调用 `bindToViewModel()` 订阅 canvasMode 变化
/// 2. Tab 创建时调用 `allocateService()` 分配 XPC 服务
/// 3. 连接建立后调用 `markTabConnected()` 标记就绪
/// 4. 发送配置时调用 `sendTabConfig()`，内部自动处理就绪检查和 defer
/// 5. Tab 关闭时调用 `releaseTab()` 清理资源
@MainActor
final class XPCBridgeService: ObservableObject {

    // MARK: - Singleton

    static let shared = XPCBridgeService()

    private init() {
        NSLog("📡 [XPCBridgeService] Initialized")
    }

    // MARK: - IPC Integration

    /// Adapter for IPC module
    private let ipcAdapter = IPCBridgeAdapter.shared

    // MARK: - 核心状态（单一数据源）

    /// 当前 Canvas 模式：false = Focus, true = Overview
    private(set) var isOverviewMode: Bool = false

    /// 每个 Tab 的 XPC 状态
    private var tabStates: [UUID: XPCTabState] = [:]

    /// 待发送的请求队列（目标未就绪时缓存）
    private var pendingRequests: [UUID: [XPCPendingRequest]] = [:]

    /// XPC 服务就绪状态（按 serviceId）
    private var serviceReadyStates: [String: Bool] = [:]

    // MARK: - 订阅

    private var canvasModeSubscription: AnyCancellable?

    // MARK: - 公开 API：生命周期管理

    /// 绑定到 BrowserViewModel，订阅 canvasMode 变化
    func bindToViewModel(_ viewModel: CanvasModePublisher) {
        canvasModeSubscription?.cancel()

        let newOverviewMode = !viewModel.canvasMode.isInFocusMode
        NSLog("📡 [XPCBridgeService] bindToViewModel: isInFocusMode=%@, isOverviewMode=%@",
              viewModel.canvasMode.isInFocusMode ? "true" : "false",
              newOverviewMode ? "true" : "false")

        updateOverviewMode(newOverviewMode)

        canvasModeSubscription = viewModel.canvasModePublisher
            .dropFirst()
            .receive(on: DispatchQueue.main)
            .sink { [weak self] newMode in
                let isOverview = !newMode.isInFocusMode
                self?.updateOverviewMode(isOverview)
            }
    }

    /// 为 Tab 分配 XPC 服务
    func allocateService(for tabId: UUID, fingerprint: String) throws -> String {
        if let existing = tabStates[tabId]?.serviceId {
            return existing
        }

        let serviceId = try XPCServicePool.shared.allocateService(for: tabId, fingerprint: fingerprint)

        var state = tabStates[tabId] ?? XPCTabState()
        state.serviceId = serviceId
        state.isServiceReady = serviceReadyStates[serviceId] ?? false
        tabStates[tabId] = state

        NSLog("📡 [XPCBridgeService] allocateService: tab=%@ service=%@ isServiceReady=%@",
              String(tabId.uuidString.prefix(8)), serviceId.components(separatedBy: ".").last ?? "",
              state.isServiceReady ? "true" : "false")

        return serviceId
    }

    /// 标记 Tab 连接已建立
    func markTabConnected(_ tabId: UUID, connectionId: String? = nil) {
        var state = tabStates[tabId] ?? XPCTabState()
        state.isConnected = true
        state.connectionId = connectionId
        tabStates[tabId] = state

        NSLog("📡 [XPCBridgeService] markTabConnected: tab=%@ connectionId=%@ isReady=%@",
              String(tabId.uuidString.prefix(8)), connectionId ?? "nil",
              state.isReady ? "true" : "false")

        ipcAdapter.markTabConnected(tabId, connectionId: connectionId)
        flushPendingRequests(for: tabId)
    }

    /// 释放 Tab 的 XPC 资源
    func releaseTab(_ tabId: UUID) {
        guard let state = tabStates[tabId] else { return }

        if state.serviceId != nil {
            XPCServicePool.shared.releaseService(for: tabId)
        }

        ipcAdapter.releaseTab(tabId)

        tabStates.removeValue(forKey: tabId)
        pendingRequests.removeValue(forKey: tabId)

        NSLog("📡 [XPCBridgeService] releaseTab: tab=%@", String(tabId.uuidString.prefix(8)))
    }

    // MARK: - 公开 API：发送请求

    /// 发送 Tab 配置
    func sendTabConfig(_ config: XPCTabConfig) {
        let tabId = config.tabId
        let tabIdShort = String(tabId.uuidString.prefix(8))

        guard let state = tabStates[tabId] else {
            NSLog("⚠️ [XPCBridgeService] sendTabConfig: tab=%@ NOT REGISTERED, deferring", tabIdShort)
            appendPendingRequest(tabId: tabId, request: .tabConfig(config))
            return
        }

        guard state.isReady else {
            NSLog("⚠️ [XPCBridgeService] sendTabConfig: tab=%@ NOT READY (service=%@, connected=%@, serviceReady=%@), deferring",
                  tabIdShort,
                  state.serviceId ?? "nil",
                  state.isConnected ? "true" : "false",
                  state.isServiceReady ? "true" : "false")
            appendPendingRequest(tabId: tabId, request: .tabConfig(config))
            return
        }

        doSendTabConfig(config)
    }

    /// 发送容器尺寸变化
    func sendContainerSizeChanged(tabId: UUID, size: CGSize, requestId: String = UUID().uuidString.prefix(8).description) {
        let tabIdShort = String(tabId.uuidString.prefix(8))

        guard let state = tabStates[tabId], state.isReady else {
            NSLog("⚠️ [XPCBridgeService] sendContainerSizeChanged: tab=%@ NOT READY, deferring", tabIdShort)
            appendPendingRequest(tabId: tabId, request: .sizeChange(size, requestId))
            return
        }

        if let lastSize = state.lastSentSize, lastSize == size {
            NSLog("📡 [XPCBridgeService] sendContainerSizeChanged: tab=%@ SKIP (same size)", tabIdShort)
            return
        }

        var updatedState = state
        updatedState.lastSentSize = size
        tabStates[tabId] = updatedState

        doSendContainerSizeChanged(tabId: tabId, size: size, requestId: requestId)
    }

    // MARK: - 公开 API：状态查询

    var currentOverviewMode: Bool {
        isOverviewMode
    }

    func tabState(for tabId: UUID) -> XPCTabState? {
        tabStates[tabId]
    }

    func isTabReady(_ tabId: UUID) -> Bool {
        tabStates[tabId]?.isReady ?? false
    }

    func serviceId(for tabId: UUID) -> String? {
        tabStates[tabId]?.serviceId
    }

    // MARK: - 内部方法：状态更新

    private func updateOverviewMode(_ newValue: Bool) {
        guard isOverviewMode != newValue else { return }

        isOverviewMode = newValue
        NSLog("📡 [XPCBridgeService] updateOverviewMode: %@", newValue ? "true" : "false")

        for (tabId, state) in tabStates where state.isReady {
            if let lastSize = state.lastSentSize {
                doSendContainerSizeChanged(
                    tabId: tabId,
                    size: lastSize,
                    requestId: "mode-\(UUID().uuidString.prefix(8))"
                )
            }
        }
    }

    func markServiceReady(_ serviceId: String) {
        guard serviceReadyStates[serviceId] != true else { return }

        serviceReadyStates[serviceId] = true
        NSLog("📡 [XPCBridgeService] markServiceReady: %@", serviceId.components(separatedBy: ".").last ?? "")

        ipcAdapter.markServiceReady(serviceId)

        for (tabId, var state) in tabStates where state.serviceId == serviceId {
            state.isServiceReady = true
            tabStates[tabId] = state

            if state.isReady {
                flushPendingRequests(for: tabId)
            }
        }
    }

    // MARK: - 内部方法：请求队列

    private func appendPendingRequest(tabId: UUID, request: XPCPendingRequest) {
        var requests = pendingRequests[tabId] ?? []

        if case .sizeChange = request {
            requests.removeAll { if case .sizeChange = $0 { return true } else { return false } }
        }

        requests.append(request)
        pendingRequests[tabId] = requests
    }

    private func flushPendingRequests(for tabId: UUID) {
        guard let requests = pendingRequests[tabId], !requests.isEmpty else { return }
        guard let state = tabStates[tabId], state.isReady else { return }

        NSLog("📡 [XPCBridgeService] flushPendingRequests: tab=%@ count=%d",
              String(tabId.uuidString.prefix(8)), requests.count)

        pendingRequests.removeValue(forKey: tabId)

        for request in requests {
            switch request {
            case .tabConfig(let config):
                doSendTabConfig(config)
            case .sizeChange(let size, let requestId):
                doSendContainerSizeChanged(tabId: tabId, size: size, requestId: requestId)
            }
        }
    }

    // MARK: - 内部方法：实际发送

    private func doSendTabConfig(_ config: XPCTabConfig) {
        let tabIdShort = String(config.tabId.uuidString.prefix(8))

        NSLog("📤 [XPCBridgeService] sendTabConfig: tab=%@ size=%.0fx%.0f isOverviewMode=%@",
              tabIdShort, config.containerSize.width, config.containerSize.height,
              isOverviewMode ? "true" : "false")

        ipcAdapter.sendTabConfig(config, isOverviewMode: isOverviewMode)
    }

    private func doSendContainerSizeChanged(tabId: UUID, size: CGSize, requestId: String) {
        let tabIdShort = String(tabId.uuidString.prefix(8))

        NSLog("📐 [XPCBridgeService] sendContainerSizeChanged: tab=%@ size=%.0fx%.0f isOverviewMode=%@",
              tabIdShort, size.width, size.height, isOverviewMode ? "true" : "false")

        let reqIdInt = Int(requestId.filter { $0.isNumber }) ?? 0
        ipcAdapter.sendContainerSizeChanged(tabId: tabId, size: size, requestId: reqIdInt)
    }
}

// MARK: - 向后兼容：ViewBridgeCoordinatorManager 代理

extension XPCBridgeService {
    func coordinator(for tabId: UUID) -> ViewBridgeCoordinator {
        ViewBridgeCoordinatorManager.shared.coordinator(for: tabId)
    }
}
