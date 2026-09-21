import CryptoKit
import Foundation
import Testing
@testable import Actrium

private final class ActrMockURLProtocol: URLProtocol {
    static var handler: ((URLRequest) throws -> (HTTPURLResponse, Data))?

    override class func canInit(with request: URLRequest) -> Bool {
        true
    }

    override class func canonicalRequest(for request: URLRequest) -> URLRequest {
        request
    }

    override func startLoading() {
        do {
            guard let handler = Self.handler else {
                throw URLError(.resourceUnavailable)
            }
            let (response, data) = try handler(request)
            client?.urlProtocol(self, didReceive: response, cacheStoragePolicy: .notAllowed)
            client?.urlProtocol(self, didLoad: data)
            client?.urlProtocolDidFinishLoading(self)
        } catch {
            client?.urlProtocol(self, didFailWithError: error)
        }
    }

    override func stopLoading() {}
}

@MainActor
@Suite(.serialized)
struct ActrAuthorityClientTests {
    @Test(arguments: ["localhost:8081", "[::1]:8081"], ["/actr/packages", "/actr%2Fpackages"])
    func resolvesAuthorityTargetAndCachesVerifiedPackage(authority: String, servicePath: String) async throws {
        let packageData = Data("signed actor package".utf8)
        let digest = SHA256.hash(data: packageData).map { String(format: "%02x", $0) }.joined()
        let cacheDirectory = FileManager.default.temporaryDirectory
            .appendingPathComponent("actr-authority-test-\(UUID().uuidString)", isDirectory: true)
        defer {
            ActrMockURLProtocol.handler = nil
            try? FileManager.default.removeItem(at: cacheDirectory)
        }

        ActrMockURLProtocol.handler = { request in
            let url = try #require(request.url)
            let body: Data
            let contentType: String
            switch url.path(percentEncoded: true) {
            case "/.well-known/actr":
                body = try JSONSerialization.data(withJSONObject: [
                    "version": 1,
                    "authority": "actr://\(authority)",
                    "services": [[
                        "rel": "packages",
                        "href": "http://\(authority)\(servicePath)",
                        "protocol": "actr-packages/1",
                    ]],
                ])
                contentType = "application/json"
            case "\(servicePath)/resolve":
                let target = URLComponents(url: url, resolvingAgainstBaseURL: false)?
                    .queryItems?.first(where: { $0.name == "target" })?.value
                #expect(target == "echo")
                body = try JSONSerialization.data(withJSONObject: [
                    "version": 1,
                    "target": "echo",
                    "package": [
                        "href": "http://\(authority)\(servicePath)/artifact?target=echo&sha256=\(digest)",
                        "media_type": "application/zip",
                        "sha256": digest,
                        "size": packageData.count,
                        "profile": "keel.guest/1",
                    ],
                ])
                contentType = "application/json"
            case "\(servicePath)/artifact":
                body = packageData
                contentType = "application/zip"
            default:
                throw URLError(.fileDoesNotExist)
            }

            let response = try #require(HTTPURLResponse(
                url: url,
                statusCode: 200,
                httpVersion: "HTTP/1.1",
                headerFields: ["Content-Type": contentType]
            ))
            return (response, body)
        }

        let configuration = URLSessionConfiguration.ephemeral
        configuration.protocolClasses = [ActrMockURLProtocol.self]
        let session = URLSession(configuration: configuration)
        defer { session.invalidateAndCancel() }
        let client = ActrAuthorityClient(session: session, cacheDirectory: cacheDirectory)

        let packageURL = try await client.resolveAndDownload(
            try #require(URL(string: "actr://\(authority)/echo"))
        )

        #expect(packageURL.lastPathComponent == "\(digest).actr")
        #expect(try Data(contentsOf: packageURL) == packageData)
    }

    @Test func rejectsDescriptorForAnotherAuthority() async throws {
        let configuration = URLSessionConfiguration.ephemeral
        configuration.protocolClasses = [ActrMockURLProtocol.self]
        let session = URLSession(configuration: configuration)
        defer {
            ActrMockURLProtocol.handler = nil
            session.invalidateAndCancel()
        }
        ActrMockURLProtocol.handler = { request in
            let url = try #require(request.url)
            let body = try JSONSerialization.data(withJSONObject: [
                "version": 1,
                "authority": "actr://other.example",
                "services": [[
                    "rel": "packages",
                    "href": "https://other.example/packages",
                    "protocol": "actr-packages/1",
                ]],
            ])
            let response = try #require(HTTPURLResponse(
                url: url,
                statusCode: 200,
                httpVersion: "HTTP/1.1",
                headerFields: ["Content-Type": "application/json"]
            ))
            return (response, body)
        }

        let client = ActrAuthorityClient(session: session)
        do {
            _ = try await client.resolveAndDownload(
                try #require(URL(string: "actr://localhost:8081/echo"))
            )
            Issue.record("Expected authority mismatch")
        } catch let error as ActrAuthorityError {
            guard case .authorityMismatch = error else {
                Issue.record("Unexpected error: \(error)")
                return
            }
        }
    }
    @Test(arguments: [false, true])
    func rejectsOversizedDescriptor(advertiseLength: Bool) async throws {
        let configuration = URLSessionConfiguration.ephemeral
        configuration.protocolClasses = [ActrMockURLProtocol.self]
        let session = URLSession(configuration: configuration)
        defer {
            ActrMockURLProtocol.handler = nil
            session.invalidateAndCancel()
        }
        ActrMockURLProtocol.handler = { request in
            let body = Data(repeating: 0x20, count: 64 * 1024 + 1)
            let response = try #require(HTTPURLResponse(
                url: #require(request.url), statusCode: 200, httpVersion: "HTTP/1.1",
                headerFields: advertiseLength ? ["Content-Length": String(body.count)] : [:]
            ))
            return (response, body)
        }
        do {
            _ = try await ActrAuthorityClient(session: session).resolveAndDownload(
                try #require(URL(string: "actr://localhost:8081/echo"))
            )
            Issue.record("Expected descriptor size rejection")
        } catch let error as ActrAuthorityError {
            guard case .invalidResponse(let message) = error else {
                Issue.record("Unexpected error: \(error)")
                return
            }
            #expect(message == "response exceeded 65536 bytes")
        }
    }

    @Test(arguments: ["https://cdn.example/package", "http://[::1]:8081/package", "http://remote.example/package"])
    func validatesRedirectBeforeFollowing(destination: String) throws {
        let session = URLSession(configuration: .ephemeral)
        defer { session.invalidateAndCancel() }
        let source = try #require(URL(string: "https://authority.example/package"))
        let request = URLRequest(url: try #require(URL(string: destination)))
        let response = try #require(HTTPURLResponse(
            url: source, statusCode: 302, httpVersion: "HTTP/1.1", headerFields: nil
        ))
        let task = session.dataTask(with: source)
        defer { task.cancel() }
        ActrAuthorityRedirectDelegate().urlSession(
            session, task: task, willPerformHTTPRedirection: response, newRequest: request
        ) { redirected in
            if destination == "http://remote.example/package" {
                #expect(redirected == nil)
            } else {
                #expect(redirected?.url == request.url)
            }
        }
    }

}
