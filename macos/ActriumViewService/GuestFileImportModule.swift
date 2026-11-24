import ActriumIPC
import Foundation
import React

@objc(GuestFileImport)
final class GuestFileImportModule: NSObject {
    @objc(pick:reloadKey:source:accept:multiple:resolver:rejecter:)
    func pick(
        _ tabId: String, reloadKey: NSNumber, source: String, accept: [String], multiple: Bool,
        resolver resolve: @escaping RCTPromiseResolveBlock, rejecter reject: @escaping RCTPromiseRejectBlock
    ) {
        pickFiles(tabId, reloadKey: reloadKey, source: source, accept: accept, multiple: multiple,
                  cache: false, resolve: resolve, reject: reject)
    }

    @objc(pickCached:reloadKey:source:accept:multiple:resolver:rejecter:)
    func pickCached(
        _ tabId: String, reloadKey: NSNumber, source: String, accept: [String], multiple: Bool,
        resolver resolve: @escaping RCTPromiseResolveBlock, rejecter reject: @escaping RCTPromiseRejectBlock
    ) {
        pickFiles(tabId, reloadKey: reloadKey, source: source, accept: accept, multiple: multiple,
                  cache: true, resolve: resolve, reject: reject)
    }

    private func pickFiles(
        _ tabId: String, reloadKey: NSNumber, source: String, accept: [String], multiple: Bool, cache: Bool,
        resolve: @escaping RCTPromiseResolveBlock, reject: @escaping RCTPromiseRejectBlock
    ) {
        guard let source = GuestFileImportCall.Source(rawValue: source) else {
            reject("invalid_file_import", "Invalid picker source.", nil)
            return
        }
        let call = GuestFileImportCall(reloadKey: reloadKey.intValue, source: source, accept: accept, multiple: multiple)
        guard call.hasValidArguments else {
            reject("invalid_file_import", "Invalid picker arguments.", nil)
            return
        }
        DispatchQueue.main.async {
            RendererProcessController.pickFiles(call, tabId: tabId) { response in
                if let error = response.error {
                    reject("file_import_failed", error, nil)
                    return
                }
                do {
                    let files = cache ? try Self.cached(response.files, tabId: tabId, reloadKey: call.reloadKey)
                        : response.files.map { file in
                            Self.withThumbnail(["name": file.name, "base64": file.data.base64EncodedString()], file.thumbnail)
                        }
                    resolve(["files": files])
                } catch {
                    reject("file_import_failed", "Could not cache the selected items: \(error.localizedDescription)", error as NSError)
                }
            }
        }
    }

    @MainActor
    private static func cached(_ files: [GuestFileImportReply.File], tabId: String, reloadKey: Int) throws -> [[String: Any]] {
        try RendererProcessController.withAttachmentCache(tabId: tabId) { cache in
            let originals = files.map { RendererAttachmentCache.File(name: $0.name, data: $0.data, thumbnail: $0.thumbnail) }
            return try cache.store(originals, tabId: tabId, reloadKey: reloadKey).map { file in
                withThumbnail(["fileId": file.id.uuidString, "name": file.name, "size": file.size], file.thumbnail)
            }
        }
    }

    private static func withThumbnail(_ item: [String: Any], _ thumbnail: Data?) -> [String: Any] {
        var output = item
        if let thumbnail { output["thumbnailBase64"] = thumbnail.base64EncodedString() }
        return output
    }

    @objc(cachedAttachment:reloadKey:action:fileId:resolver:rejecter:)
    func cachedAttachment(
        _ tabId: String, reloadKey: NSNumber, action: String, fileId: String,
        resolver resolve: @escaping RCTPromiseResolveBlock, rejecter reject: @escaping RCTPromiseRejectBlock
    ) {
        guard let id = UUID(uuidString: fileId), let operation = CachedOperation(rawValue: action) else {
            reject("invalid_cached_attachment", "Invalid attachment request.", nil)
            return
        }
        let revision = reloadKey.intValue
        DispatchQueue.main.async {
            do {
                resolve(try Self.perform(operation, id: id, tabId: tabId, reloadKey: revision))
            } catch {
                reject("cached_attachment_failed", error.localizedDescription, error as NSError)
            }
        }
    }

    private enum CachedOperation: String {
        case read, preview, release
    }

    @MainActor
    private static func perform(_ operation: CachedOperation, id: UUID, tabId: String, reloadKey: Int) throws -> Any? {
        try RendererProcessController.withAttachmentCache(tabId: tabId) { cache in
            switch operation {
            case .read:
                let file = try cache.read(id, tabId: tabId, reloadKey: reloadKey)
                return ["filename": file.name, "base64": file.data.base64EncodedString()]
            case .preview:
                return try cache.previewURL(id, tabId: tabId, reloadKey: reloadKey).absoluteString
            case .release:
                try cache.release(id, tabId: tabId, reloadKey: reloadKey)
                return nil
            }
        }
    }

    @objc static func requiresMainQueueSetup() -> Bool { false }
}
