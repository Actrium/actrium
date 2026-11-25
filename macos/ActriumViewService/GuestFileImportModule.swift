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
                if !cache {
                    resolve(["files": response.files.map { file in
                        Self.withThumbnail(["name": file.name, "base64": file.data.base64EncodedString()], file.thumbnail)
                    }])
                    return
                }
                Task { @MainActor in
                    do {
                        let files = try await Self.cached(response.files, tabId: tabId, reloadKey: call.reloadKey)
                        resolve(["files": files])
                    } catch {
                        reject("file_import_failed", "Could not cache the selected items: \(error.localizedDescription)", error as NSError)
                    }
                }
            }
        }
    }

    @MainActor
    private static func cached(
        _ files: [GuestFileImportReply.File], tabId: String, reloadKey: Int
    ) async throws -> [[String: Any]] {
        let cache = try RendererProcessController.attachmentCache(for: tabId)
        let originals = files.map { file in
            RendererAttachmentCache.File(
                name: file.name, data: file.data, thumbnail: file.thumbnail,
                canPreview: file.canPreview ?? (file.thumbnail != nil)
            )
        }
        return try await cache.store(originals, tabId: tabId, reloadKey: reloadKey).map { file in
            withThumbnail([
                "fileId": file.id.uuidString, "name": file.name, "size": file.size, "canPreview": file.canPreview,
            ], file.thumbnail)
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
        Task { @MainActor in
            do {
                resolve(try await Self.perform(operation, id: id, tabId: tabId, reloadKey: revision))
            } catch {
                reject("cached_attachment_failed", error.localizedDescription, error as NSError)
            }
        }
    }

    private enum CachedOperation: String {
        case read, preview, release
    }

    @MainActor
    private static func perform(
        _ operation: CachedOperation, id: UUID, tabId: String, reloadKey: Int
    ) async throws -> Any? {
        let cache = try RendererProcessController.attachmentCache(for: tabId)
        switch operation {
        case .read:
            let file = try await cache.readForUpload(id, tabId: tabId, reloadKey: reloadKey)
            return ["filename": file.filename, "base64": file.base64]
        case .preview:
            return try await cache.previewURL(id, tabId: tabId, reloadKey: reloadKey).absoluteString
        case .release:
            try await cache.release(id, tabId: tabId, reloadKey: reloadKey)
            return nil
        }
    }

    @objc static func requiresMainQueueSetup() -> Bool { false }
}
