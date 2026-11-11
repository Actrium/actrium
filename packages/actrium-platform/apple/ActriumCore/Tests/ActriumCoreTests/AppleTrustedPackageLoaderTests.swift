import Foundation
import XCTest
@testable import ActriumCore

private final class PackageMockProtocol: URLProtocol {
    static var handler: ((URLRequest) throws -> (Int, Data))?
    override class func canInit(with request: URLRequest) -> Bool { true }
    override class func canonicalRequest(for request: URLRequest) -> URLRequest { request }
    override func startLoading() {
        do {
            let (status, data) = try Self.handler!(request)
            let response = HTTPURLResponse(url: request.url!, statusCode: status, httpVersion: "HTTP/1.1", headerFields: nil)!
            client?.urlProtocol(self, didReceive: response, cacheStoragePolicy: .notAllowed)
            client?.urlProtocol(self, didLoad: data)
            client?.urlProtocolDidFinishLoading(self)
        } catch { client?.urlProtocol(self, didFailWithError: error) }
    }
    override func stopLoading() {}
}

final class AppleTrustedPackageLoaderTests: XCTestCase {
    private var root: URL!
    private var session: URLSession!
    override func setUp() {
        root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        let config = URLSessionConfiguration.ephemeral
        config.protocolClasses = [PackageMockProtocol.self]
        session = URLSession(configuration: config)
    }
    override func tearDown() {
        PackageMockProtocol.handler = nil
        session.invalidateAndCancel()
        try? FileManager.default.removeItem(at: root)
    }
    private func fixture() throws -> URL {
        guard let path = ProcessInfo.processInfo.environment["ACTRIUM_PACKAGE_TEST_FIXTURE"] else {
            throw XCTSkip("Generate the source-built fixture with cargo test; see trusted-package-loading.md")
        }
        return URL(fileURLWithPath: path)
    }
    private func savedPins() throws -> URL {
        let files = try FileManager.default.contentsOfDirectory(at: root.appendingPathComponent("publisher-pins"),
                                                               includingPropertiesForKeys: nil)
        return try XCTUnwrap(files.first { $0.pathExtension == "json" })
    }
    private func mock(_ fixture: URL, invalidSignature: Bool = false) throws {
        let archive = try Data(contentsOf: fixture.appendingPathComponent(invalidSignature ? "invalid-signature.actr" : "program.actr"))
        let descriptor = try Data(contentsOf: fixture.appendingPathComponent("publisher.json"))
        PackageMockProtocol.handler = { request in
            if request.url?.absoluteString == "https://cdn.example.test/program.actr" { return (200, archive) }
            if request.url?.absoluteString == "https://publisher.example.test/.well-known/actr" { return (200, descriptor) }
            return (404, Data())
        }
    }
    func testProductionLoaderAuthenticatesAndStagesOnlyVerifiedBytesAcrossRestart() async throws {
        let fixture = try fixture()
        try mock(fixture)
        let source = PackageSourceInfo(href: "https://cdn.example.test/program.actr", expectedSha256: nil, expectedActorType: nil)
        let loader = AppleTrustedPackageLoader(session: session, root: root)
        let result = try await loader.load(source: source)
        XCTAssertEqual(result.guestCode, "globalThis.verifiedProgram = true;")
        XCTAssertFalse(result.serviceBytes.isEmpty)
        let staged = root.appendingPathComponent("packages/\(result.digest).actr")
        XCTAssertEqual(try Data(contentsOf: staged), try Data(contentsOf: fixture.appendingPathComponent("program.actr")))
        XCTAssertTrue(FileManager.default.fileExists(atPath: try savedPins().path))
        let restarted = AppleTrustedPackageLoader(session: session, root: root)
        let repeated = try await restarted.load(source: source)
        XCTAssertEqual(repeated.digest, result.digest)
    }
    func testBadSignatureAdvancesPublisherHistoryButPublishesNoPackage() async throws {
        try mock(fixture(), invalidSignature: true)
        let loader = AppleTrustedPackageLoader(session: session, root: root)
        do {
            _ = try await loader.load(source: PackageSourceInfo(href: "https://cdn.example.test/program.actr", expectedSha256: nil, expectedActorType: nil))
            XCTFail("accepted invalid signature")
        } catch {
            XCTAssertTrue(FileManager.default.fileExists(atPath: try savedPins().path))
            XCTAssertFalse(FileManager.default.fileExists(atPath: root.appendingPathComponent("packages").path))
        }
    }
    func testCorruptedSavedPinsFailClosed() async throws {
        try mock(fixture())
        let loader = AppleTrustedPackageLoader(session: session, root: root)
        let source = PackageSourceInfo(href: "https://cdn.example.test/program.actr", expectedSha256: nil, expectedActorType: nil)
        _ = try await loader.load(source: source)
        try Data("corrupt".utf8).write(to: savedPins())
        do {
            _ = try await loader.load(source: source)
            XCTFail("ignored corrupted trust state")
        } catch {
            XCTAssertTrue(String(describing: error).contains("PUBLISHER_PINS_INVALID"))
        }
    }
    func testInvalidSourceAndCancelledRequestDoNotReachNetworkOrPublish() async throws {
        PackageMockProtocol.handler = { _ in XCTFail("network must not be reached"); return (500, Data()) }
        let loader = AppleTrustedPackageLoader(session: session, root: root)
        do {
            _ = try await loader.load(source: PackageSourceInfo(href: "http://remote.example/package.actr", expectedSha256: nil, expectedActorType: nil))
            XCTFail("accepted HTTP")
        } catch {}
        let task = Task {
            withUnsafeCurrentTask { $0?.cancel() }
            return try await loader.load(source: PackageSourceInfo(href: "https://cdn.example.test/program.actr", expectedSha256: nil, expectedActorType: nil))
        }
        task.cancel()
        do { _ = try await task.value; XCTFail("accepted cancellation") } catch {}
        XCTAssertFalse(FileManager.default.fileExists(atPath: root.path))
    }
    func testResponseIsLimitedWhileReceivingEvenWithoutContentLength() async throws {
        // The descriptor has a small independent bound. A valid package gets
        // us to this endpoint without allocating a 256 MiB test artifact.
        let fixture = try fixture()
        let archive = try Data(contentsOf: fixture.appendingPathComponent("program.actr"))
        PackageMockProtocol.handler = { request in
            (200, request.url?.host == "cdn.example.test" ? archive : Data(repeating: 32, count: 64 * 1024 + 1))
        }
        do {
            _ = try await AppleTrustedPackageLoader(session: session, root: root).load(
                source: PackageSourceInfo(href: "https://cdn.example.test/program.actr", expectedSha256: nil, expectedActorType: nil))
            XCTFail("accepted oversized descriptor")
        } catch {}
        XCTAssertFalse(FileManager.default.fileExists(atPath: root.appendingPathComponent("packages").path))
    }
    func testRedirectPolicyRejectsDowngradesCredentialsAndExcessHops() throws {
        let delegate = PackageDownloadRedirects()
        let source = URL(string: "https://publisher.example.test/.well-known/actr")!
        let response = HTTPURLResponse(url: source, statusCode: 302, httpVersion: "HTTP/1.1", headerFields: nil)!
        let task = session.dataTask(with: source)
        defer { task.cancel() }
        for value in ["http://example.test/next", "https://user:password@example.test/next"] {
            delegate.urlSession(session, task: task, willPerformHTTPRedirection: response,
                                newRequest: URLRequest(url: URL(string: value)!)) { XCTAssertNil($0) }
        }
        let hops = PackageDownloadRedirects()
        for index in 1...6 {
            hops.urlSession(session, task: task, willPerformHTTPRedirection: response,
                            newRequest: URLRequest(url: source)) { XCTAssertEqual($0 == nil, index > 5) }
        }
    }
}
