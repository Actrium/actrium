import CryptoKit
import Foundation

private let actrAuthorityLogger = AppLogger.viewModel

enum ActrAuthorityError: LocalizedError {
    case invalidURL(String)
    case insecureEndpoint(String)
    case invalidResponse(String)
    case unsupportedVersion(Int)
    case authorityMismatch(expected: String, actual: String)
    case missingPackageService
    case targetMismatch(expected: String, actual: String)
    case packageTooLarge(UInt64)
    case packageSizeMismatch(expected: UInt64, actual: UInt64)
    case packageDigestMismatch

    var errorDescription: String? {
        switch self {
        case .invalidURL(let value):
            return "Invalid Actr URL: \(value)"
        case .insecureEndpoint(let value):
            return "Insecure non-loopback Actr endpoint: \(value)"
        case .invalidResponse(let message):
            return "Invalid Actr authority response: \(message)"
        case .unsupportedVersion(let version):
            return "Unsupported Actr response version: \(version)"
        case .authorityMismatch(let expected, let actual):
            return "Authority mismatch: expected \(expected), received \(actual)"
        case .missingPackageService:
            return "The authority does not advertise actr-packages/1"
        case .targetMismatch(let expected, let actual):
            return "Target mismatch: expected \(expected), received \(actual)"
        case .packageTooLarge(let size):
            return "The resolved Actor package is too large: \(size) bytes"
        case .packageSizeMismatch(let expected, let actual):
            return "Actor package size mismatch: expected \(expected), received \(actual)"
        case .packageDigestMismatch:
            return "Actor package SHA-256 verification failed"
        }
    }
}

private struct ActrServiceLink: Decodable {
    let rel: String
    let href: String
    let `protocol`: String?
}

private struct ActrAuthorityDescriptor: Decodable {
    let version: Int
    let authority: String
    let services: [ActrServiceLink]
}

private struct ActrResolvedPackage: Decodable {
    let href: String
    let mediaType: String
    let sha256: String
    let size: UInt64
    let profile: String?

    enum CodingKeys: String, CodingKey {
        case href
        case mediaType = "media_type"
        case sha256
        case size
        case profile
    }
}

private struct ActrPackageResolution: Decodable {
    let version: Int
    let target: String
    let package: ActrResolvedPackage
}

private struct ActrPublicTarget {
    let authority: String
    let host: String
    let port: Int?
    let target: String

    var authorityURL: String {
        "actr://\(authority)"
    }

    var allowsPlainHTTP: Bool {
        host == "localhost" || host == "127.0.0.1" || host == "::1"
    }

    var discoveryURL: URL? {
        var components = URLComponents()
        components.scheme = allowsPlainHTTP ? "http" : "https"
        components.host = host.contains(":") ? "[\(host)]" : host
        components.port = port
        components.path = "/.well-known/actr"
        return components.url
    }

    init(url: URL, requireTarget: Bool = true) throws {
        guard url.scheme?.lowercased() == "actr",
              url.user == nil,
              url.password == nil,
              url.query == nil,
              url.fragment == nil,
              let rawHost = url.host,
              !rawHost.isEmpty else {
            throw ActrAuthorityError.invalidURL(url.absoluteString)
        }

        let normalizedHost = rawHost.lowercased().trimmingCharacters(in: CharacterSet(charactersIn: "[]"))
        let normalizedPort = url.port == 443 ? nil : url.port
        let displayHost = normalizedHost.contains(":") ? "[\(normalizedHost)]" : normalizedHost
        authority = normalizedPort.map { "\(displayHost):\($0)" } ?? displayHost
        host = normalizedHost
        port = normalizedPort

        let encodedPath = url.path(percentEncoded: true)
        let rawTarget = encodedPath.hasPrefix("/") ? String(encodedPath.dropFirst()) : encodedPath
        target = try Self.normalizeTarget(rawTarget)
        if requireTarget && target.isEmpty {
            throw ActrAuthorityError.invalidURL("Actr URL must contain a Target")
        }
        if !requireTarget && !target.isEmpty {
            throw ActrAuthorityError.invalidURL("Actr authority must not contain a Target")
        }
    }

    private static func normalizeTarget(_ value: String) throws -> String {
        if value.isEmpty {
            return ""
        }

        let allowedRaw = CharacterSet(charactersIn: "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-._~!$&'()*+,;=:@")
        let unreserved = CharacterSet(charactersIn: "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-._~")
        var normalizedSegments: [String] = []

        for segment in value.split(separator: "/", omittingEmptySubsequences: false) {
            guard !segment.isEmpty else {
                throw ActrAuthorityError.invalidURL("Target contains an empty segment")
            }
            let bytes = Array(segment.utf8)
            var index = 0
            var normalized = ""
            var decoded = Data()
            while index < bytes.count {
                let byte = bytes[index]
                if byte == 0x25 {
                    guard index + 2 < bytes.count,
                          let hex = String(bytes: bytes[(index + 1)...(index + 2)], encoding: .utf8),
                          let octet = UInt8(hex, radix: 16) else {
                        throw ActrAuthorityError.invalidURL("Target contains invalid percent encoding")
                    }
                    decoded.append(octet)
                    if octet < 0x80,
                       let scalar = UnicodeScalar(Int(octet)),
                       unreserved.contains(scalar) {
                        normalized.append(Character(scalar))
                    } else {
                        normalized += String(format: "%%%02X", octet)
                    }
                    index += 3
                    continue
                }

                guard byte < 0x80,
                      let scalar = UnicodeScalar(Int(byte)),
                      allowedRaw.contains(scalar) else {
                    throw ActrAuthorityError.invalidURL("Target contains a character that must be percent encoded")
                }
                decoded.append(byte)
                normalized.append(Character(scalar))
                index += 1
            }

            guard decoded != Data(".".utf8), decoded != Data("..".utf8) else {
                throw ActrAuthorityError.invalidURL("Target contains a dot segment")
            }
            normalizedSegments.append(normalized)
        }
        return normalizedSegments.joined(separator: "/")
    }
}

final class ActrAuthorityClient {
    static let shared = ActrAuthorityClient()

    private static let descriptorLimit = 64 * 1024
    private static let resolutionLimit = 64 * 1024
    private static let packageLimit: UInt64 = 256 * 1024 * 1024

    private let session: URLSession
    private let cacheDirectory: URL

    init(session: URLSession = .shared, cacheDirectory: URL? = nil) {
        self.session = session
        self.cacheDirectory = cacheDirectory ?? Self.defaultCacheDirectory()
    }

    func resolveAndDownload(_ publicURL: URL) async throws -> URL {
        let requested = try ActrPublicTarget(url: publicURL)
        guard let discoveryURL = requested.discoveryURL else {
            throw ActrAuthorityError.invalidURL(publicURL.absoluteString)
        }

        actrAuthorityLogger.notice("[ActrAuthority] Discovering \(requested.authorityURL)")
        let descriptorData = try await get(discoveryURL, accept: "application/json", limit: Self.descriptorLimit)
        let descriptor = try decode(ActrAuthorityDescriptor.self, from: descriptorData)
        guard descriptor.version == 1 else {
            throw ActrAuthorityError.unsupportedVersion(descriptor.version)
        }
        guard let describedURL = URL(string: descriptor.authority) else {
            throw ActrAuthorityError.invalidResponse("authority is not a URL")
        }
        let described = try ActrPublicTarget(url: describedURL, requireTarget: false)
        guard described.authorityURL == requested.authorityURL else {
            throw ActrAuthorityError.authorityMismatch(
                expected: requested.authorityURL,
                actual: described.authorityURL
            )
        }

        guard let packageService = descriptor.services.first(where: {
            $0.rel == "packages" && $0.protocol == "actr-packages/1"
        }), let endpoint = URL(string: packageService.href) else {
            throw ActrAuthorityError.missingPackageService
        }
        try ActrAuthorityTransport.validate(endpoint)

        guard var resolutionComponents = URLComponents(url: endpoint, resolvingAgainstBaseURL: false) else {
            throw ActrAuthorityError.invalidResponse("packages href is malformed")
        }
        resolutionComponents.percentEncodedPath = resolutionComponents.percentEncodedPath.trimmingCharacters(in: CharacterSet(charactersIn: "/"))
        resolutionComponents.percentEncodedPath = "/\(resolutionComponents.percentEncodedPath)/resolve"
            .replacingOccurrences(of: "//", with: "/")
        resolutionComponents.queryItems = [URLQueryItem(name: "target", value: requested.target)]
        guard let resolutionURL = resolutionComponents.url else {
            throw ActrAuthorityError.invalidResponse("could not construct the Target resolution URL")
        }

        actrAuthorityLogger.notice("[ActrAuthority] Resolving Target \(requested.target)")
        let resolutionData = try await get(resolutionURL, accept: "application/json", limit: Self.resolutionLimit)
        let resolution = try decode(ActrPackageResolution.self, from: resolutionData)
        guard resolution.version == 1 else {
            throw ActrAuthorityError.unsupportedVersion(resolution.version)
        }
        guard resolution.target == requested.target else {
            throw ActrAuthorityError.targetMismatch(expected: requested.target, actual: resolution.target)
        }
        guard resolution.package.mediaType == "application/zip" else {
            throw ActrAuthorityError.invalidResponse("unsupported package media type")
        }
        guard resolution.package.size <= Self.packageLimit else {
            throw ActrAuthorityError.packageTooLarge(resolution.package.size)
        }
        guard let expectedDigest = Self.normalizedDigest(resolution.package.sha256),
              let artifactURL = URL(string: resolution.package.href) else {
            throw ActrAuthorityError.invalidResponse("package href or SHA-256 is malformed")
        }
        try ActrAuthorityTransport.validate(artifactURL)

        // Stream with the declared size as the hard limit: one byte over and
        // we reject before buffering more than the server claimed. The
        // equality check below still catches under-delivery.
        let packageData = try await get(
            artifactURL,
            accept: "application/zip",
            limit: Int(resolution.package.size)
        )
        let actualSize = UInt64(packageData.count)
        guard actualSize == resolution.package.size else {
            throw ActrAuthorityError.packageSizeMismatch(
                expected: resolution.package.size,
                actual: actualSize
            )
        }
        let actualDigest = SHA256.hash(data: packageData).map { String(format: "%02x", $0) }.joined()
        guard actualDigest == expectedDigest else {
            throw ActrAuthorityError.packageDigestMismatch
        }

        try FileManager.default.createDirectory(at: cacheDirectory, withIntermediateDirectories: true)
        let packageURL = cacheDirectory.appendingPathComponent("\(actualDigest).actr", isDirectory: false)
        try packageData.write(to: packageURL, options: .atomic)
        actrAuthorityLogger.notice("[ActrAuthority] Cached verified package \(actualDigest.prefix(16))")
        return packageURL
    }

    nonisolated private func get(_ url: URL, accept: String, limit: Int) async throws -> Data {
        try ActrAuthorityTransport.validate(url)
        var request = URLRequest(url: url)
        request.httpMethod = "GET"
        request.timeoutInterval = 30
        request.setValue(accept, forHTTPHeaderField: "Accept")
        let (bytes, response) = try await session.bytes(for: request, delegate: ActrAuthorityRedirectDelegate())
        defer { bytes.task.cancel() }
        guard let http = response as? HTTPURLResponse,
              (200..<300).contains(http.statusCode) else {
            let status = (response as? HTTPURLResponse)?.statusCode ?? 0
            throw ActrAuthorityError.invalidResponse("GET \(url.absoluteString) returned HTTP \(status)")
        }
        if response.expectedContentLength > Int64(limit) {
            throw ActrAuthorityError.invalidResponse("response exceeded \(limit) bytes")
        }
        var data = Data()
        for try await byte in bytes {
            guard data.count < limit else {
                throw ActrAuthorityError.invalidResponse("response exceeded \(limit) bytes")
            }
            data.append(byte)
        }
        return data
    }

    private func decode<T: Decodable>(_ type: T.Type, from data: Data) throws -> T {
        do {
            return try JSONDecoder().decode(type, from: data)
        } catch {
            throw ActrAuthorityError.invalidResponse(error.localizedDescription)
        }
    }

    private static func normalizedDigest(_ value: String) -> String? {
        let normalized = value.lowercased()
        guard normalized.count == 64,
              normalized.allSatisfy({ $0.isHexDigit }) else {
            return nil
        }
        return normalized
    }

    private static func defaultCacheDirectory() -> URL {
        let root = FileManager.default.urls(for: .cachesDirectory, in: .userDomainMask).first
            ?? FileManager.default.temporaryDirectory
        return root
            .appendingPathComponent("io.actrium.app", isDirectory: true)
            .appendingPathComponent("ActorPackages", isDirectory: true)
    }
}

// Apply the same policy to advertised URLs and every redirect before sending it.
enum ActrAuthorityTransport {
    nonisolated static func validate(_ url: URL) throws {
        guard let scheme = url.scheme?.lowercased(),
              let rawHost = url.host, !rawHost.isEmpty,
              url.user == nil, url.password == nil else {
            throw ActrAuthorityError.invalidURL(url.absoluteString)
        }
        let host = rawHost.lowercased().trimmingCharacters(in: CharacterSet(charactersIn: "[]"))
        if scheme == "https" {
            return
        }
        if scheme == "http", host == "localhost" || host == "127.0.0.1" || host == "::1" {
            return
        }
        throw ActrAuthorityError.insecureEndpoint(url.absoluteString)
    }
}

final class ActrAuthorityRedirectDelegate: NSObject, URLSessionTaskDelegate {
    nonisolated override init() {
        super.init()
    }

    nonisolated func urlSession(
        _ session: URLSession,
        task: URLSessionTask,
        willPerformHTTPRedirection response: HTTPURLResponse,
        newRequest request: URLRequest,
        completionHandler: @escaping (URLRequest?) -> Void
    ) {
        guard let url = request.url,
              (try? ActrAuthorityTransport.validate(url)) != nil else {
            completionHandler(nil)
            return
        }
        completionHandler(request)
    }
}
