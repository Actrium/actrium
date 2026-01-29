import Foundation
import Actr
import SwiftProtobuf
import ActrBindings

/// React Native bridge for Actr High Level API
/// This class provides a bridge between React Native and the Actr Swift SDK,
/// exposing the high-level API (ActrSystem, ActrNode, ActrRef) to JavaScript.
@objc(ActrBridge)
@preconcurrency
public class ActrBridge: NSObject, @unchecked Sendable {
    
    // MARK: - Storage
    
    /// Stores ActrSystem instances by ID
    private var systems: [String: ActrSystem] = [:]
    
    /// Stores ActrNode instances by ID
    private var nodes: [String: ActrNode] = [:]
    
    /// Stores ActrRef instances by ID
    private var refs: [String: ActrRef] = [:]
    
    /// Serial queue for thread-safe access to storage
    private let queue = DispatchQueue(label: "com.actr.bridge.queue")
    
    private var actrService: ActrService?
    
    // MARK: - ActrSystem Methods
    
    /// Create an ActrSystem from a TOML config file path
    /// @param configPath: Path to the TOML configuration file
    /// @param resolve: Promise resolve callback with system ID
    /// @param reject: Promise reject callback
    @objc
    public func createSystemFromPath(
        _ configPath: String,
        resolve: @escaping RCTPromiseResolveBlock,
        reject: @escaping RCTPromiseRejectBlock
    ) {
        Task {
            do {
                let system = try await ActrSystem.from(tomlConfig: configPath)
                let systemId = UUID().uuidString
                
                queue.sync {
                    self.systems[systemId] = system
                }
                
                resolve(systemId)
            } catch let error as ActrError {
                reject("ACTR_ERROR", self.formatActrError(error), error)
            } catch {
                reject("UNKNOWN_ERROR", error.localizedDescription, error)
            }
        }
    }
    
    /// Create an ActrSystem from a TOML config file URL
    /// @param configUrl: File URL string to the TOML configuration file
    /// @param resolve: Promise resolve callback with system ID
    /// @param reject: Promise reject callback
    @objc
    public func createSystemFromUrl(
        _ configUrl: String,
        resolve: @escaping RCTPromiseResolveBlock,
        reject: @escaping RCTPromiseRejectBlock
    ) {
        Task {
            do {
                guard let url = URL(string: configUrl) else {
                    reject("INVALID_URL", "Invalid URL: \(configUrl)", nil)
                    return
                }
                let system = try await ActrSystem.from(tomlConfig: url)
                let systemId = UUID().uuidString
                
                queue.sync {
                    self.systems[systemId] = system
                }
                
                resolve(systemId)
            } catch let error as ActrError {
                reject("ACTR_ERROR", self.formatActrError(error), error)
            } catch {
                reject("UNKNOWN_ERROR", error.localizedDescription, error)
            }
        }
    }
    
    /// Spawn a node with a workload
    /// @param systemId: The ID of the ActrSystem
    /// @param workloadId: The ID of the workload (must be registered via setWorkload)
    /// @param resolve: Promise resolve callback with node ID
    /// @param reject: Promise reject callback
    @objc
    public func spawnNode(
        _ systemId: String,
        workloadId: String,
        resolve: @escaping RCTPromiseResolveBlock,
        reject: @escaping RCTPromiseRejectBlock
    ) {
        queue.async {
            guard let system = self.systems[systemId] else {
                reject("SYSTEM_NOT_FOUND", "ActrSystem with ID \(systemId) not found", nil)
                return
            }
            
            guard let workload = WorkloadRegistry.shared.getWorkload(workloadId) else {
                reject("WORKLOAD_NOT_FOUND", "Workload with ID \(workloadId) not found", nil)
                return
            }
            
            do {
                let node = try system.spawn(workload: workload)
                let nodeId = UUID().uuidString
                
                self.queue.sync {
                    self.nodes[nodeId] = node
                }
                
                resolve(nodeId)
            } catch let error as ActrError {
                reject("ACTR_ERROR", self.formatActrError(error), error)
            } catch {
                reject("UNKNOWN_ERROR", error.localizedDescription, error)
            }
        }
    }
    
    // MARK: - ActrNode Methods
    
    /// Start a node and get an ActrRef
    /// @param nodeId: The ID of the ActrNode
    /// @param resolve: Promise resolve callback with actor reference ID
    /// @param reject: Promise reject callback
    @objc
    public func startNode(
        _ nodeId: String,
        resolve: @escaping RCTPromiseResolveBlock,
        reject: @escaping RCTPromiseRejectBlock
    ) {
        queue.async {
            guard let node = self.nodes[nodeId] else {
                reject("NODE_NOT_FOUND", "ActrNode with ID \(nodeId) not found", nil)
                return
            }
            
            Task {
                do {
                    let actrRef = try await node.start()
                    let refId = UUID().uuidString
                    
                    self.queue.sync {
                        self.refs[refId] = actrRef
                    }
                    
                    resolve(refId)
                } catch let error as ActrError {
                    reject("ACTR_ERROR", self.formatActrError(error), error)
                } catch {
                    reject("UNKNOWN_ERROR", error.localizedDescription, error)
                }
            }
        }
    }
    
    // MARK: - ActrRef Methods
    
    /// Get the actor ID of an ActrRef
    /// @param refId: The ID of the ActrRef
    /// @param resolve: Promise resolve callback with actor ID dictionary
    /// @param reject: Promise reject callback
    @objc
    public func getActorId(
        _ refId: String,
        resolve: @escaping RCTPromiseResolveBlock,
        reject: @escaping RCTPromiseRejectBlock
    ) {
        queue.async {
            guard let actrRef = self.refs[refId] else {
                reject("REF_NOT_FOUND", "ActrRef with ID \(refId) not found", nil)
                return
            }
            
            Task {
                let actorId = actrRef.actorId()
                let actorIdDict: [String: Any] = [
                    "serialNumber": actorId.serialNumber,
                    "type": [
                        "manufacturer": actorId.type.manufacturer,
                        "name": actorId.type.name
                    ],
                    "realm": self.serializeRealm(actorId.realm)
                ]
                resolve(actorIdDict)
            }
        }
    }
    
    /// Perform an RPC call
    /// @param refId: The ID of the ActrRef
    /// @param routeKey: The route key for the RPC call
    /// @param requestData: The request payload as base64 encoded string
    /// @param payloadType: The payload type (0: rpcReliable, 1: rpcSignal, etc.)
    /// @param timeoutMs: Timeout in milliseconds
    /// @param resolve: Promise resolve callback with response data as base64 string
    /// @param reject: Promise reject callback
    @objc
    public func call(
        _ refId: String,
        routeKey: String,
        requestData: String,
        payloadType: Int32,
        timeoutMs: Int64,
        resolve: @escaping RCTPromiseResolveBlock,
        reject: @escaping RCTPromiseRejectBlock
    ) {
        queue.async {
            guard let actrRef = self.refs[refId] else {
                reject("REF_NOT_FOUND", "ActrRef with ID \(refId) not found", nil)
                return
            }
            
            guard let requestBytes = Data(base64Encoded: requestData) else {
                reject("INVALID_DATA", "Invalid base64 encoded request data", nil)
                return
            }
            
            Task {
                do {
                    let responseData = try await actrRef.rawCall(
                        routeKey: routeKey,
                        requestPayload: requestBytes,
                        timeoutMs: timeoutMs
                    )
                    let responseBase64 = responseData.base64EncodedString()
                    resolve(responseBase64)
                } catch let error as ActrError {
                    reject("ACTR_ERROR", self.formatActrError(error), error)
                } catch {
                    reject("UNKNOWN_ERROR", error.localizedDescription, error)
                }
            }
        }
    }
    
    /// Discover actors of a specific type
    /// @param refId: The ID of the ActrRef
    /// @param manufacturer: The manufacturer of the actor type
    /// @param name: The name of the actor type
    /// @param limit: Maximum number of actors to discover
    /// @param resolve: Promise resolve callback with array of actor IDs
    /// @param reject: Promise reject callback
    @objc
    public func discover(
        _ refId: String,
        manufacturer: String,
        name: String,
        limit: Int,
        resolve: @escaping RCTPromiseResolveBlock,
        reject: @escaping RCTPromiseRejectBlock
    ) {
        queue.async {
            guard let actrRef = self.refs[refId] else {
                reject("REF_NOT_FOUND", "ActrRef with ID \(refId) not found", nil)
                return
            }
            
            Task {
                do {
                    let actorType = ActrType(manufacturer: manufacturer, name: name)
                    let actorIds = try await actrRef.discover(
                        targetType: actorType,
                        count: UInt32(limit)
                    )
                    
                    let actorIdDicts = actorIds.map { actorId -> [String: Any] in
                        return [
                            "serialNumber": actorId.serialNumber,
                            "type": [
                                "manufacturer": actorId.type.manufacturer,
                                "name": actorId.type.name
                            ],
                            "realm": self.serializeRealm(actorId.realm)
                        ]
                    }
                    
                    resolve(actorIdDicts)
                } catch let error as ActrError {
                    reject("ACTR_ERROR", self.formatActrError(error), error)
                } catch {
                    reject("UNKNOWN_ERROR", error.localizedDescription, error)
                }
            }
        }
    }
    
    /// Stop an actor reference
    /// @param refId: The ID of the ActrRef
    /// @param resolve: Promise resolve callback
    /// @param reject: Promise reject callback
    @objc
    public func stopActor(
        _ refId: String,
        resolve: @escaping RCTPromiseResolveBlock,
        reject: @escaping RCTPromiseRejectBlock
    ) {
        queue.async {
            guard let actrRef = self.refs[refId] else {
                reject("REF_NOT_FOUND", "ActrRef with ID \(refId) not found", nil)
                return
            }
            
            Task {
                await actrRef.stop()
                
                let _ = self.queue.sync {
                    self.refs.removeValue(forKey: refId)
                }
                
                resolve(nil)
            }
        }
    }
    
    // MARK: - Cleanup Methods
    
    /// Remove a system from storage
    @objc
    public func removeSystem(
        _ systemId: String,
        resolve: @escaping RCTPromiseResolveBlock,
        reject: @escaping RCTPromiseRejectBlock
    ) {
        queue.async {
            self.systems.removeValue(forKey: systemId)
            resolve(nil)
        }
    }
    
    /// Remove a node from storage
    @objc
    public func removeNode(
        _ nodeId: String,
        resolve: @escaping RCTPromiseResolveBlock,
        reject: @escaping RCTPromiseRejectBlock
    ) {
        queue.async {
            self.nodes.removeValue(forKey: nodeId)
            resolve(nil)
        }
    }
    
    // MARK: - Helper Methods
    
    private nonisolated func formatActrError(_ error: ActrError) -> String {
        return error.errorDescription ?? ""
    }
    
    private nonisolated func serializeRealm(_ realm: Realm) -> [String: Any] {
        // Realm serialization - adjust based on actual Realm structure
        return ["id": "\(realm)"]
    }
}

// MARK: - Workload Registry

/// Registry for managing workload instances
/// Workloads must be registered before they can be used with spawnNode
public class WorkloadRegistry {
    public static let shared = WorkloadRegistry()
    
    private var workloads: [String: WorkloadBridge] = [:]
    private let queue = DispatchQueue(label: "com.actr.workload.registry.queue")
    
    private init() {}
    
    /// Register a workload with an ID
    public func registerWorkload(_ id: String, workload: WorkloadBridge) {
        queue.sync {
            workloads[id] = workload
        }
    }
    
    /// Get a workload by ID
    public func getWorkload(_ id: String) -> WorkloadBridge? {
        queue.sync {
            return workloads[id]
        }
    }
    
    /// Remove a workload by ID
    public func removeWorkload(_ id: String) {
        let _ = queue.sync {
            workloads.removeValue(forKey: id)
        }
    }
}

// MARK: - ActrService Methods

extension ActrBridge {
    /// Initialize ActrService
    @objc
    public func initializeService(
        _ resolve: @escaping RCTPromiseResolveBlock,
        reject: @escaping RCTPromiseRejectBlock
    ) {
        Task { @MainActor in
            do {
                let service = ActrService()
                try await service.initialize()
                
                queue.sync {
                    self.actrService = service
                }
                
                resolve(["status": "connected"])
            } catch let error as ActrError {
                reject("ACTR_ERROR", self.formatActrError(error), error)
            } catch {
                reject("UNKNOWN_ERROR", error.localizedDescription, error)
            }
        }
    }
    
    /// Shutdown ActrService
    @objc
    public func shutdownService(
        _ resolve: @escaping RCTPromiseResolveBlock,
        reject: @escaping RCTPromiseRejectBlock
    ) {
        Task { @MainActor in
            guard let service = queue.sync(execute: { self.actrService }) else {
                reject("SERVICE_NOT_FOUND", "ActrService not initialized", nil)
                return
            }
            
            await service.shutdown()
            
            queue.sync {
                self.actrService = nil
            }
            
            resolve(["status": "disconnected"])
        }
    }
    
    /// Send echo message through ActrService
    @objc
    public func sendEcho(
        _ message: String,
        resolve: @escaping RCTPromiseResolveBlock,
        reject: @escaping RCTPromiseRejectBlock
    ) {
        Task { @MainActor in
            guard let service = queue.sync(execute: { self.actrService }) else {
                reject("SERVICE_NOT_FOUND", "ActrService not initialized", nil)
                return
            }
            
            do {
                let reply = try await service.sendEcho(message)
                resolve(["reply": reply])
            } catch let error as ActrError {
                reject("ACTR_ERROR", self.formatActrError(error), error)
            } catch {
                reject("UNKNOWN_ERROR", error.localizedDescription, error)
            }
        }
    }
    
    /// Get ActrService connection status
    @objc
    public func getServiceStatus(
        _ resolve: @escaping RCTPromiseResolveBlock,
        reject: @escaping RCTPromiseRejectBlock
    ) {
        Task { @MainActor in
            guard let service = queue.sync(execute: { self.actrService }) else {
                resolve(["status": "disconnected"])
                return
            }
            
            let status = service.connectionStatus.description
            let result: [String: Any] = [
                "status": status,
                "errorMessage": service.errorMessage ?? NSNull()
            ]
            resolve(result)
        }
    }
}

// MARK: - React Native Bridge Support

/// Type alias for React Native promise callbacks
public typealias RCTPromiseResolveBlock = (Any?) -> Void
public typealias RCTPromiseRejectBlock = (String?, String?, Error?) -> Void
