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
    private var actrService: ActrService?

    private let queue = DispatchQueue(label: "com.actr.bridge.queue")
    
}

// MARK: - ActrService Methods

extension ActrBridge {

// MARK: - Helper Methods
    
    private nonisolated func formatActrError(_ error: ActrError) -> String {
        return error.errorDescription ?? ""
    }
    
    private nonisolated func serializeRealm(_ realm: Realm) -> [String: Any] {
        // Realm serialization - adjust based on actual Realm structure
        return ["id": "\(realm)"]
    }

    /// Initialize ActrService
    @objc
    public func initializeService(
        _ path: String,
        resolve: @escaping RCTPromiseResolveBlock,
        reject: @escaping RCTPromiseRejectBlock
    ) {
        Task { @MainActor in
            do {
                let service = ActrService()
                try await service.initialize(path: path)
                
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
