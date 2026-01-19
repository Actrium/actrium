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

    func toDictionary(isOverviewMode: Bool) -> [String: Any] {
        var dict: [String: Any] = [
            "tabId": tabId.uuidString,
            "title": title,
            "backgroundColor": backgroundColor,
            "containerWidth": containerSize.width,
            "containerHeight": containerSize.height,
            "isOverviewMode": isOverviewMode,
            "leftPanelVisible": leftPanelVisible,
            "rightPanelVisible": rightPanelVisible
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
    var connectionId: String?  // 用于多连接路由

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
        setupNotificationListeners()
        NSLog("📡 [XPCBridgeService] Initialized")
    }

    // MARK: - 核心状态（单一数据源）

    /// 当前 Canvas 模式：false = Focus, true = Overview
    /// 由 canvasMode 订阅自动更新
    private(set) var isOverviewMode: Bool = false

    /// 每个 Tab 的 XPC 状态
    private var tabStates: [UUID: XPCTabState] = [:]

    /// 待发送的请求队列（目标未就绪时缓存）
    private var pendingRequests: [UUID: [XPCPendingRequest]] = [:]

    /// XPC 服务就绪状态（按 serviceId）
    private var serviceReadyStates: [String: Bool] = [:]

    // MARK: - 订阅

    private var canvasModeSubscription: AnyCancellable?

    // MARK: - Notification Observers

    private var xpcReadyObserver: NSObjectProtocol?
    private var connectionCreatedObserver: NSObjectProtocol?
    private var sizeAppliedObserver: NSObjectProtocol?

    // MARK: - 公开 API：生命周期管理

    /// 绑定到 BrowserViewModel，订阅 canvasMode 变化
    /// 应该在 App 启动时调用一次
    func bindToViewModel(_ viewModel: CanvasModePublisher) {
        canvasModeSubscription?.cancel()

        // 立即同步当前状态
        let newOverviewMode = !viewModel.canvasMode.isInFocusMode
        NSLog("📡 [XPCBridgeService] bindToViewModel: canvasMode.isInFocusMode=%@, isOverviewMode=%@",
              viewModel.canvasMode.isInFocusMode ? "true" : "false",
              newOverviewMode ? "true" : "false")

        updateOverviewMode(newOverviewMode)

        // 订阅后续变化
        canvasModeSubscription = viewModel.canvasModePublisher
            .dropFirst()
            .receive(on: DispatchQueue.main)
            .sink { [weak self] newMode in
                let isOverview = !newMode.isInFocusMode
                self?.updateOverviewMode(isOverview)
            }
    }

    /// 为 Tab 分配 XPC 服务
    /// 返回分配的 serviceId
    func allocateService(for tabId: UUID, fingerprint: String) throws -> String {
        // 检查是否已分配
        if let existing = tabStates[tabId]?.serviceId {
            return existing
        }

        // 委托给 XPCServicePool 进行实际分配
        let serviceId = try XPCServicePool.shared.allocateService(for: tabId, fingerprint: fingerprint)

        // 初始化 Tab 状态
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

        // 检查是否有待发送的请求
        flushPendingRequests(for: tabId)
    }

    /// 释放 Tab 的 XPC 资源
    func releaseTab(_ tabId: UUID) {
        guard let state = tabStates[tabId] else { return }

        // 释放 XPCServicePool 中的分配
        if state.serviceId != nil {
            XPCServicePool.shared.releaseService(for: tabId)
        }

        // 清理状态
        tabStates.removeValue(forKey: tabId)
        pendingRequests.removeValue(forKey: tabId)

        NSLog("📡 [XPCBridgeService] releaseTab: tab=%@", String(tabId.uuidString.prefix(8)))
    }

    // MARK: - 公开 API：发送请求

    /// 发送 Tab 配置
    /// 如果目标未就绪，自动缓存并在就绪后发送
    func sendTabConfig(_ config: XPCTabConfig) {
        let tabId = config.tabId
        let tabIdShort = String(tabId.uuidString.prefix(8))

        guard let state = tabStates[tabId] else {
            NSLog("⚠️ [XPCBridgeService] sendTabConfig: tab=%@ NOT REGISTERED, deferring", tabIdShort)
            appendPendingRequest(tabId: tabId, request: .tabConfig(config))
            return
        }

        guard state.isReady, let serviceId = state.serviceId else {
            NSLog("⚠️ [XPCBridgeService] sendTabConfig: tab=%@ NOT READY (service=%@, connected=%@, serviceReady=%@), deferring",
                  tabIdShort,
                  state.serviceId ?? "nil",
                  state.isConnected ? "true" : "false",
                  state.isServiceReady ? "true" : "false")
            appendPendingRequest(tabId: tabId, request: .tabConfig(config))
            return
        }

        // 发送配置
        doSendTabConfig(config, serviceId: serviceId)
    }

    /// 发送容器尺寸变化
    /// 如果目标未就绪，自动缓存
    func sendContainerSizeChanged(tabId: UUID, size: CGSize, requestId: String = UUID().uuidString.prefix(8).description) {
        let tabIdShort = String(tabId.uuidString.prefix(8))

        guard let state = tabStates[tabId], state.isReady, let serviceId = state.serviceId else {
            NSLog("⚠️ [XPCBridgeService] sendContainerSizeChanged: tab=%@ NOT READY, deferring", tabIdShort)
            appendPendingRequest(tabId: tabId, request: .sizeChange(size, requestId))
            return
        }

        // 检查是否与上次发送的相同
        if let lastSize = state.lastSentSize, lastSize == size {
            NSLog("📡 [XPCBridgeService] sendContainerSizeChanged: tab=%@ SKIP (same size)", tabIdShort)
            return
        }

        // 更新状态
        var updatedState = state
        updatedState.lastSentSize = size
        tabStates[tabId] = updatedState

        // 发送
        doSendContainerSizeChanged(tabId: tabId, size: size, serviceId: serviceId, requestId: requestId)
    }

    // MARK: - 公开 API：状态查询

    /// 获取当前 isOverviewMode（供发送配置时使用）
    var currentOverviewMode: Bool {
        isOverviewMode
    }

    /// 获取 Tab 的 XPC 状态
    func tabState(for tabId: UUID) -> XPCTabState? {
        tabStates[tabId]
    }

    /// 检查 Tab 是否就绪
    func isTabReady(_ tabId: UUID) -> Bool {
        tabStates[tabId]?.isReady ?? false
    }

    /// 获取 Tab 的 serviceId
    func serviceId(for tabId: UUID) -> String? {
        tabStates[tabId]?.serviceId
    }

    // MARK: - 内部方法：状态更新

    private func updateOverviewMode(_ newValue: Bool) {
        guard isOverviewMode != newValue else { return }

        isOverviewMode = newValue
        NSLog("📡 [XPCBridgeService] updateOverviewMode: %@", newValue ? "true" : "false")

        // 通知所有已就绪的 Tab
        for (tabId, state) in tabStates where state.isReady {
            if let serviceId = state.serviceId, let lastSize = state.lastSentSize {
                doSendContainerSizeChanged(
                    tabId: tabId,
                    size: lastSize,
                    serviceId: serviceId,
                    requestId: "mode-\(UUID().uuidString.prefix(8))"
                )
            }
        }
    }

    private func markServiceReady(_ serviceId: String) {
        guard serviceReadyStates[serviceId] != true else { return }

        serviceReadyStates[serviceId] = true
        NSLog("📡 [XPCBridgeService] markServiceReady: %@", serviceId.components(separatedBy: ".").last ?? "")

        // 更新使用该服务的所有 Tab
        for (tabId, var state) in tabStates where state.serviceId == serviceId {
            state.isServiceReady = true
            tabStates[tabId] = state

            // 检查是否可以发送待处理请求
            if state.isReady {
                flushPendingRequests(for: tabId)
            }
        }
    }

    // MARK: - 内部方法：请求队列

    private func appendPendingRequest(tabId: UUID, request: XPCPendingRequest) {
        var requests = pendingRequests[tabId] ?? []

        // 对于 sizeChange，只保留最新的
        if case .sizeChange = request {
            requests.removeAll { if case .sizeChange = $0 { return true } else { return false } }
        }

        requests.append(request)
        pendingRequests[tabId] = requests
    }

    private func flushPendingRequests(for tabId: UUID) {
        guard let requests = pendingRequests[tabId], !requests.isEmpty else { return }
        guard let state = tabStates[tabId], state.isReady, let serviceId = state.serviceId else { return }

        NSLog("📡 [XPCBridgeService] flushPendingRequests: tab=%@ count=%d",
              String(tabId.uuidString.prefix(8)), requests.count)

        pendingRequests.removeValue(forKey: tabId)

        for request in requests {
            switch request {
            case .tabConfig(let config):
                doSendTabConfig(config, serviceId: serviceId)
            case .sizeChange(let size, let requestId):
                doSendContainerSizeChanged(tabId: tabId, size: size, serviceId: serviceId, requestId: requestId)
            }
        }
    }

    // MARK: - 内部方法：实际发送

    private func doSendTabConfig(_ config: XPCTabConfig, serviceId: String) {
        let tabIdShort = String(config.tabId.uuidString.prefix(8))
        let dict = config.toDictionary(isOverviewMode: isOverviewMode)

        guard let jsonData = try? JSONSerialization.data(withJSONObject: dict),
              let jsonString = String(data: jsonData, encoding: .utf8) else {
            NSLog("❌ [XPCBridgeService] doSendTabConfig: JSON serialization failed")
            return
        }

        let notificationName = "com.canvas.tabConfig.\(serviceId)"
        NSLog("📤 [XPCBridgeService] sendTabConfig: tab=%@ service=%@ size=%.0fx%.0f isOverviewMode=%@",
              tabIdShort, serviceId.components(separatedBy: ".").last ?? "",
              config.containerSize.width, config.containerSize.height,
              isOverviewMode ? "true" : "false")

        DistributedNotificationCenter.default().postNotificationName(
            NSNotification.Name(notificationName),
            object: nil,
            userInfo: ["config": jsonString],
            deliverImmediately: true
        )
    }

    private func doSendContainerSizeChanged(tabId: UUID, size: CGSize, serviceId: String, requestId: String) {
        let tabIdShort = String(tabId.uuidString.prefix(8))

        let sizeInfo: [String: Any] = [
            "tabId": tabId.uuidString,
            "width": size.width,
            "height": size.height,
            "requestId": requestId,
            "isOverviewMode": isOverviewMode
        ]

        guard let jsonData = try? JSONSerialization.data(withJSONObject: sizeInfo),
              let jsonString = String(data: jsonData, encoding: .utf8) else {
            NSLog("❌ [XPCBridgeService] doSendContainerSizeChanged: JSON serialization failed")
            return
        }

        let notificationName = "com.canvas.containerSizeChanged.\(serviceId)"
        NSLog("📐 [XPCBridgeService] sendContainerSizeChanged: tab=%@ size=%.0fx%.0f isOverviewMode=%@",
              tabIdShort, size.width, size.height, isOverviewMode ? "true" : "false")

        DistributedNotificationCenter.default().postNotificationName(
            NSNotification.Name(notificationName),
            object: nil,
            userInfo: ["sizeInfo": jsonString],
            deliverImmediately: true
        )
    }

    // MARK: - 通知监听

    private func setupNotificationListeners() {
        // 监听 XPC Service 就绪通知
        xpcReadyObserver = DistributedNotificationCenter.default().addObserver(
            forName: NSNotification.Name("com.canvas.xpcServiceReady"),
            object: nil,
            queue: .main
        ) { [weak self] notification in
            guard let bundleId = notification.userInfo?["bundleId"] as? String else { return }
            Task { @MainActor in
                self?.markServiceReady(bundleId)
            }
        }

        // 监听连接创建通知（用于多连接路由）
        connectionCreatedObserver = DistributedNotificationCenter.default().addObserver(
            forName: NSNotification.Name("com.canvas.xpcConnectionCreated"),
            object: nil,
            queue: .main
        ) { notification in
            guard let bundleId = notification.userInfo?["bundleId"] as? String,
                  let internalId = notification.userInfo?["internalId"] as? String else { return }
            NSLog("📡 [XPCBridgeService] xpcConnectionCreated: bundleId=%@ internalId=%@",
                  bundleId.components(separatedBy: ".").last ?? "", internalId)
            // TODO: 用于多连接路由，目前暂不处理
        }
    }

    deinit {
        if let observer = xpcReadyObserver {
            DistributedNotificationCenter.default().removeObserver(observer)
        }
        if let observer = connectionCreatedObserver {
            DistributedNotificationCenter.default().removeObserver(observer)
        }
        if let observer = sizeAppliedObserver {
            DistributedNotificationCenter.default().removeObserver(observer)
        }
    }
}

// MARK: - 向后兼容：ViewBridgeCoordinatorManager 代理

extension XPCBridgeService {
    /// 为了向后兼容，提供与 ViewBridgeCoordinatorManager 相同的接口
    /// 逐步迁移后可以移除

    /// 获取或创建 coordinator（委托给 ViewBridgeCoordinatorManager）
    func coordinator(for tabId: UUID) -> ViewBridgeCoordinator {
        ViewBridgeCoordinatorManager.shared.coordinator(for: tabId)
    }
}
