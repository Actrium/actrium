import CryptoKit
import Foundation
import Testing
@testable import Actro

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
    @Test func resolvesAuthorityTargetAndCachesVerifiedPackage() async throws {
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
            switch url.path {
            case "/.well-known/actr":
                body = try JSONSerialization.data(withJSONObject: [
                    "version": 1,
                    "authority": "actr://localhost:8081",
                    "services": [[
                        "rel": "packages",
                        "href": "http://localhost:8081/actr/packages",
                        "protocol": "actr-packages/1",
                    ]],
                ])
                contentType = "application/json"
            case "/actr/packages/resolve":
                let target = URLComponents(url: url, resolvingAgainstBaseURL: false)?
                    .queryItems?.first(where: { $0.name == "target" })?.value
                #expect(target == "echo")
                body = try JSONSerialization.data(withJSONObject: [
                    "version": 1,
                    "target": "echo",
                    "package": [
                        "href": "http://localhost:8081/actr/packages/artifact?target=echo&sha256=\(digest)",
                        "media_type": "application/zip",
                        "sha256": digest,
                        "size": packageData.count,
                        "profile": "keel.guest/1",
                    ],
                ])
                contentType = "application/json"
            case "/actr/packages/artifact":
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
            try #require(URL(string: "actr://localhost:8081/echo"))
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
}
