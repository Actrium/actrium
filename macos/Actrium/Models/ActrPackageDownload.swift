import Foundation
import CryptoKit

/// Size cap for prototype package downloads (05 §15; doc 20 §2.14 suggests 64 MiB).
enum ActrPackageDownloadLimits {
    static let maxBytes: Int64 = 64 * 1024 * 1024
}

/// Policy deciding which URLs enter the package direct-link download flow.
enum ActrPackageLinkPolicy {
    /// True for package direct links: an https URL whose path ends with `.actr`.
    /// http is accepted only for loopback/localhost hosts (05 §14 dev sources;
    /// formal addresses never fall back to HTTP).
    static func isPackageDownloadURL(_ url: URL) -> Bool {
        guard let host = url.host, !host.isEmpty else { return false }
        guard url.path.lowercased().hasSuffix(".actr") else { return false }
        switch url.scheme?.lowercased() {
        case "https":
            return true
        case "http":
            return isLoopbackHost(host)
        default:
            return false
        }
    }

    static func isLoopbackHost(_ host: String) -> Bool {
        host == "localhost" || host == "::1" || host == "[::1]" || host.hasPrefix("127.")
    }
}

/// Package link awaiting user confirmation — confirm before connect (04 §3).
struct PendingPackageDownload: Identifiable {
    let id = UUID()
    let url: URL

    var host: String { url.host ?? "" }
    var filename: String { url.lastPathComponent }

    var sizeLimitText: String {
        ByteCountFormatter.string(fromByteCount: ActrPackageDownloadLimits.maxBytes, countStyle: .file)
    }
}

/// State of an in-flight (or failed) package download, shown in the progress sheet.
struct ActivePackageDownload: Identifiable {
    let id = UUID()
    let pending: PendingPackageDownload

    var phase: Phase = .downloading
    var errorMessage: String?

    enum Phase {
        case downloading
        case verifying
        case failed
    }
}

/// Notice shown when the user enters a formal `actr://` address.
/// Realm Profile resolution (doc 20 §1.5/§2.13) is not wired yet in the prototype,
/// so the address cannot be opened; the notice tells the user what to do instead.
struct ActrUriNotice: Identifiable {
    let id = UUID()
    let uri: String
    let domain: String
}

enum ActrPackageDownloadError: LocalizedError {
    case sizeExceeded(maxBytes: Int64)
    case cancelled
    case failed(reason: String)

    var errorDescription: String? {
        switch self {
        case .sizeExceeded(let maxBytes):
            let limit = ByteCountFormatter.string(fromByteCount: maxBytes, countStyle: .file)
            return "包大小超过上限 \(limit)，已停止下载"
        case .cancelled:
            return nil
        case .failed(let reason):
            return "下载失败：\(reason)"
        }
    }
}

enum ActrPackageDownloadService {
    struct Result {
        let localURL: URL
        /// 64-char lowercase hex SHA-256 of the downloaded archive.
        let digest: String
    }

    /// Downloads the archive to a stable temporary file, enforcing the size cap.
    /// Supports Swift concurrency cancellation (task cancel aborts the transfer).
    nonisolated static func download(from url: URL) async throws -> URL {
        let tempURL: URL
        let response: URLResponse
        do {
            (tempURL, response) = try await URLSession.shared.download(from: url)
        } catch let error as URLError where error.code == .cancelled {
            throw ActrPackageDownloadError.cancelled
        } catch is CancellationError {
            throw ActrPackageDownloadError.cancelled
        } catch {
            throw ActrPackageDownloadError.failed(reason: error.localizedDescription)
        }

        if let http = response as? HTTPURLResponse {
            guard (200..<300).contains(http.statusCode) else {
                try? FileManager.default.removeItem(at: tempURL)
                throw ActrPackageDownloadError.failed(reason: "HTTP \(http.statusCode)")
            }
        }

        // Pre-check via declared Content-Length when the server provides it.
        let expected = response.expectedContentLength
        if expected > ActrPackageDownloadLimits.maxBytes {
            try? FileManager.default.removeItem(at: tempURL)
            throw ActrPackageDownloadError.sizeExceeded(maxBytes: ActrPackageDownloadLimits.maxBytes)
        }

        let destination = FileManager.default.temporaryDirectory
            .appendingPathComponent("actr-pkg-\(UUID().uuidString).actr")
        do {
            // Move out of the session's scratch location so the file survives the session.
            try? FileManager.default.removeItem(at: destination)
            try FileManager.default.moveItem(at: tempURL, to: destination)
        } catch {
            try? FileManager.default.removeItem(at: tempURL)
            throw ActrPackageDownloadError.failed(reason: error.localizedDescription)
        }

        // Actual size check (covers servers without Content-Length).
        let attributes = try? FileManager.default.attributesOfItem(atPath: destination.path)
        let actualSize = (attributes?[.size] as? Int64) ?? 0
        if actualSize > ActrPackageDownloadLimits.maxBytes {
            try? FileManager.default.removeItem(at: destination)
            throw ActrPackageDownloadError.sizeExceeded(maxBytes: ActrPackageDownloadLimits.maxBytes)
        }

        return destination
    }

    /// Computes the SHA-256 digest of a file off the main actor.
    nonisolated static func sha256Digest(of url: URL) async throws -> String {
        try await withCheckedThrowingContinuation { continuation in
            DispatchQueue.global(qos: .userInitiated).async {
                do {
                    continuation.resume(returning: try sha256File(at: url))
                } catch {
                    continuation.resume(throwing: error)
                }
            }
        }
    }

    /// Synchronous SHA-256 of a file (call off the main actor for large files).
    nonisolated static func sha256FileSync(at url: URL) throws -> String {
        try sha256File(at: url)
    }

    private static func sha256File(at url: URL) throws -> String {
        let handle = try FileHandle(forReadingFrom: url)
        defer { try? handle.close() }
        var hasher = SHA256()
        while let chunk = try handle.read(upToCount: 1024 * 1024), !chunk.isEmpty {
            hasher.update(data: chunk)
        }
        return hasher.finalize().map { String(format: "%02x", $0) }.joined()
    }
}
