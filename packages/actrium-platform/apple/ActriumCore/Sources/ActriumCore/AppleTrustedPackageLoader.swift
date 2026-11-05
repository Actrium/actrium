import Foundation
import Darwin
import CryptoKit

/// Downloads whole signed Programs. Realm callers supply only a selection
/// accepted by F02; explicit HTTPS package selection is also supported.
/// Publisher roots are bootstrapped by Web PKI and then pinned across launches.
public actor AppleTrustedPackageLoader {
    public static let shared = AppleTrustedPackageLoader()
    public static let packageLimit = 256 * 1024 * 1024
    private let session: URLSession
    private let root: URL
    private var publisherDocuments: [String: (json: String, fetchedAt: Date)] = [:]
    private var publisherAttempts: [String: Date] = [:]

    public init(session: URLSession = .shared, root: URL? = nil) {
        self.session = session
        self.root = root ?? FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask)[0]
            .appendingPathComponent("Actrium/TrustedPackages", isDirectory: true)
    }

    /// No executable contents leave Rust until every package check succeeds.
    /// This method deliberately does not reuse an AppInfo hash or URL cache.
    public func load(source: PackageSourceInfo) async throws -> VerifiedProgramInfo {
        try validatePackageSource(source: source)
        try Task.checkCancellation()
        guard let packageURL = URL(string: source.href) else { throw LoaderError.invalidURL }
        let archive = try await receive(packageURL, limit: Self.packageLimit)
        let identity = try inspectDownloadedProgram(source: source, archive: Array(archive))
        guard let bootstrap = URL(string: identity.bootstrapUrl) else { throw LoaderError.invalidURL }
        let descriptorJSON = try await publisherDocument(domain: identity.domain, bootstrap: bootstrap)
        try Task.checkCancellation()

        // Read pins after the await: another install may have advanced the
        // baseline while this download was in flight. Accept + persist has no
        // suspension point and all instances sharing this root use a file lock.
        let pinsDirectory = root.appendingPathComponent("publisher-pins", isDirectory: true)
        try FileManager.default.createDirectory(at: pinsDirectory, withIntermediateDirectories: true,
                                              attributes: [.posixPermissions: 0o700])
        // A canonical DNS name can be 253 bytes; appending an extension would
        // exceed a filesystem component limit. Hash only the storage key.
        let domainKey = SHA256.hash(data: Data(identity.domain.utf8)).map { String(format: "%02x", $0) }.joined()
        let pinsURL = pinsDirectory.appendingPathComponent(domainKey + ".json")
        let publisher = try withPublisherLock(in: pinsDirectory) {
            let existing: String?
            if FileManager.default.fileExists(atPath: pinsURL.path) {
                let handle = try FileHandle(forReadingFrom: pinsURL)
                defer { try? handle.close() }
                let data = try handle.read(upToCount: 64 * 1024 + 1) ?? Data()
                guard data.count <= 64 * 1024, let text = String(data: data, encoding: .utf8) else {
                    throw LoaderError.invalidDocument
                }
                existing = text
            } else { existing = nil }
            let accepted = try acceptPackagePublisher(domain: identity.domain,
                                                     descriptorJson: descriptorJSON, pinsJson: existing)
            // Trust history advances even if this particular package is bad.
            // Otherwise a rejected package could erase an observed revocation.
            try Data(accepted.pinsJson().utf8).write(to: pinsURL, options: .atomic)
            try FileManager.default.setAttributes([.posixPermissions: 0o600], ofItemAtPath: pinsURL.path)
            return accepted
        }
        try Task.checkCancellation()
        let verified = try publisher.verifyProgram(source: source, archive: Array(archive))
        try Task.checkCancellation()

        // Basic verified staging only. Offline reuse, the release ledger and
        // reference-aware garbage collection belong to #367 / #368.
        let packages = root.appendingPathComponent("packages", isDirectory: true)
        try FileManager.default.createDirectory(at: packages, withIntermediateDirectories: true,
                                              attributes: [.posixPermissions: 0o700])
        try Task.checkCancellation()
        try archive.write(to: packages.appendingPathComponent(verified.digest + ".actr"), options: .atomic)
        return verified
    }

    /// Adapter boundary for F02's accepted Realm Profile. This is not itself a
    /// Realm resolver or a trust decision API; both expected fields are required.
    public func loadAcceptedRealmPackage(href: String, sha256: String, actorType: String) async throws -> VerifiedProgramInfo {
        try await load(source: PackageSourceInfo(href: href, expectedSha256: sha256, expectedActorType: actorType))
    }

    private func publisherDocument(domain: String, bootstrap: URL) async throws -> String {
        let now = Date()
        if let cached = publisherDocuments[domain], now.timeIntervalSince(cached.fetchedAt) < 60 {
            // Signature, signed expiry and pins are rechecked on every use.
            return cached.json
        }
        if let attempted = publisherAttempts[domain], now.timeIntervalSince(attempted) < 60 {
            throw LoaderError.publisherRetryBackoff
        }
        // Set before suspension, including failures, to bound concurrent and
        // unknown-key retry fetches to once per Publisher per minute.
        publisherAttempts[domain] = now
        let data = try await receive(bootstrap, limit: 64 * 1024)
        guard let json = String(data: data, encoding: .utf8) else { throw LoaderError.invalidDocument }
        publisherDocuments[domain] = (json, Date())
        return json
    }

    private func receive(_ url: URL, limit: Int) async throws -> Data {
        try PackageDownloadRedirects.validate(url)
        var request = URLRequest(url: url)
        request.timeoutInterval = 30
        let (stream, response) = try await session.bytes(for: request, delegate: PackageDownloadRedirects())
        defer { stream.task.cancel() }
        guard let http = response as? HTTPURLResponse, (200..<300).contains(http.statusCode) else {
            throw LoaderError.httpFailure
        }
        guard response.expectedContentLength <= Int64(limit) else { throw LoaderError.sizeLimit }
        var data = Data()
        for try await byte in stream {
            try Task.checkCancellation()
            guard data.count < limit else { throw LoaderError.sizeLimit }
            data.append(byte)
        }
        try Task.checkCancellation()
        return data
    }

    private func withPublisherLock<T>(in directory: URL, _ operation: () throws -> T) throws -> T {
        let lockURL = directory.appendingPathComponent(".lock")
        let descriptor = open(lockURL.path, O_CREAT | O_RDWR | O_NOFOLLOW, 0o600)
        guard descriptor >= 0 else { throw LoaderError.trustStoreFailure }
        defer { close(descriptor) }
        guard flock(descriptor, LOCK_EX) == 0 else { throw LoaderError.trustStoreFailure }
        defer { flock(descriptor, LOCK_UN) }
        return try operation()
    }

    public enum LoaderError: Error {
        case invalidURL, invalidDocument, httpFailure, sizeLimit, trustStoreFailure, publisherRetryBackoff
    }
}

/// Every redirect is checked before sending it; bootstrap identity remains
/// the original Publisher domain and is checked again inside Rust.
final class PackageDownloadRedirects: NSObject, URLSessionTaskDelegate {
    private var redirects = 0

    static func validate(_ url: URL) throws {
        try validatePackageSource(source: PackageSourceInfo(href: url.absoluteString,
                                                          expectedSha256: nil, expectedActorType: nil))
    }

    func urlSession(_ session: URLSession, task: URLSessionTask,
                    willPerformHTTPRedirection response: HTTPURLResponse, newRequest request: URLRequest,
                    completionHandler: @escaping (URLRequest?) -> Void) {
        redirects += 1
        guard redirects <= 5, let url = request.url, (try? Self.validate(url)) != nil else {
            completionHandler(nil)
            return
        }
        completionHandler(request)
    }
}
