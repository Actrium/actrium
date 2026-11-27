import ActriumIPC
import Foundation
import React

extension RealmService {
    @objc(callBound:route:payloadBase64:timeoutMs:reloadKey:connection:resolver:rejecter:)
    func callBound(
        _ tabId: String, route: String, payloadBase64: String, timeoutMs: NSNumber,
        reloadKey: NSNumber, connection: NSDictionary,
        resolver resolve: @escaping RCTPromiseResolveBlock, rejecter reject: @escaping RCTPromiseRejectBlock
    ) {
        guard let payload = Data(base64Encoded: payloadBase64),
              (1...300_000).contains(timeoutMs.intValue),
              let actionText = connection["action"] as? String,
              let action = RealmServiceCall.Connection.Action(rawValue: actionText) else {
            reject("invalid_service_connection", "Invalid bound Service call arguments.", nil)
            return
        }
        let id = (connection["id"] as? String).flatMap { UUID(uuidString: $0) }
        guard action == .retain ? connection["id"] == nil : id != nil else {
            reject("invalid_service_connection", "Invalid Service connection ID.", nil)
            return
        }
        let call = RealmServiceCall(
            id: UUID(), route: route, payload: payload, timeoutMs: timeoutMs.intValue,
            connection: .init(action: action, id: id), reloadKey: reloadKey.intValue
        )
        submit(tabId, call: call, resolve: resolve, reject: reject)
    }

    @objc(sendBoundServiceStreamChunk:route:streamId:sequence:payloadBase64:reloadKey:connectionId:resolver:rejecter:)
    func sendBoundServiceStreamChunk(
        _ tabId: String, route: String, streamId: String, sequence: String, payloadBase64: String,
        reloadKey: NSNumber, connectionId: String,
        resolver resolve: @escaping RCTPromiseResolveBlock, rejecter reject: @escaping RCTPromiseRejectBlock
    ) {
        let maxBase64Bytes = (RealmServiceCall.maxStreamChunkBytes + 2) / 3 * 4
        guard let sequence = UInt64(sequence), sequence > 0,
              let connection = UUID(uuidString: connectionId),
              payloadBase64.utf8.count <= maxBase64Bytes,
              let payload = Data(base64Encoded: payloadBase64) else {
            reject("invalid_service_stream_chunk", "Invalid bound Service DataStream arguments.", nil)
            return
        }
        let call = RealmServiceCall(
            id: UUID(), route: route, payload: payload, timeoutMs: 10_000,
            streamChunk: .init(id: streamId, sequence: sequence),
            connection: .init(action: .use, id: connection), reloadKey: reloadKey.intValue
        )
        submit(tabId, call: call, resolve: { _ in resolve(nil) }, reject: reject)
    }
}
