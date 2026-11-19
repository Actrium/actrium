import ActriumCore
import CryptoKit
import Foundation
import WebKit
import XCTest

@testable import Actrium

final class RealmAdmissionTests: XCTestCase {
    private let realmName = "echo@realm.example"
    private let realmURI = "actr://realm.example/echo"

    @MainActor
    func testSignedOpenPolicyAllowsInviteFreeAdmission() throws {
        let (body, keys) = try profile(membership: "open")
        let verified = try RealmJoinService.VerifiedProfile(
            body: body, realmName: realmName, signingKeysJSON: keys
        )
        XCTAssertTrue(verified.isOpen)
        XCTAssertEqual(verified.info.realmName, realmName)
    }

    @MainActor
    func testInviteOnlyPolicyCannotUseOpenJoin() async throws {
        let (body, keys) = try profile(membership: "invite-only")
        let verified = try RealmJoinService.VerifiedProfile(
            body: body, realmName: realmName, signingKeysJSON: keys
        )
        XCTAssertFalse(verified.isOpen)
        do {
            try await RealmJoinService.joinOpen(verified, label: "Test")
            XCTFail("An invite-only Realm accepted an open join.")
        } catch {
            XCTAssertEqual((error as NSError).domain, "ActrJoin")
        }
    }

    @MainActor
    func testUnsignedPolicyChangeCannotOpenARealm() throws {
        let (body, keys) = try profile(membership: "invite-only")
        var document = try XCTUnwrap(JSONSerialization.jsonObject(with: body) as? [String: Any])
        document["policy"] = ["membership": "open"]
        let changed = try JSONSerialization.data(withJSONObject: document)
        XCTAssertThrowsError(try RealmJoinService.VerifiedProfile(
            body: changed, realmName: realmName, signingKeysJSON: keys
        ))
    }

    @MainActor
    func testBackgroundInviteSurvivesThePreviousWebPageChangingURL() async throws {
        let documents = try discoveryDocuments(membership: "invite-only")
        let profile = NavigationGate()
        let view = BrowserViewModel(
            realmPreferences: try makePreferences(),
            fetchRealmDocument: { url, _ in
                if url.path == "/realm/echo" { await profile.wait() }
                return try XCTUnwrap(documents[url.absoluteString])
            },
            saveBrowserSession: { _, _ in }
        )
        defer { profile.release() }
        view.navigateCurrentTab(to: "https://old.example/start")
        let origin = try XCTUnwrap(view.currentTabId)
        view.navigateCurrentTab(to: realmURI)
        try await waitUntil { profile.waiting }
        let other = view.addNewTab(focusAddressBar: false).id
        let redirected = try XCTUnwrap(URL(string: "https://old.example/redirect"))
        view.updateTabURL(for: origin, url: redirected)
        XCTAssertEqual(view.tabs.first { $0.id == origin }?.url, redirected)
        profile.release()

        try await waitUntil { view.pendingRealmInvite(for: origin) != nil }
        XCTAssertEqual(view.currentTabId, other)
        XCTAssertNil(view.pendingInviteRedeem)
        view.switchToTab(id: origin)
        let invite = try XCTUnwrap(view.pendingInviteRedeem)
        XCTAssertEqual(invite.uri, realmURI)
        XCTAssertEqual(invite.tabId, origin)
        view.cancelInviteRedeem(invite)
        view.switchToTab(id: other)
        view.switchToTab(id: origin)
        XCTAssertNil(view.pendingInviteRedeem)
    }

    @MainActor
    func testExplicitNavigationSupersedesAPendingRealmLoad() async throws {
        try await assertNoEnrollment { view in
            view.navigateCurrentTab(to: "https://new.example/")
        }
    }

    @MainActor
    func testMainFrameWebLinkSupersedesAPendingRealmLoad() async throws {
        try await assertNoEnrollment { view in
            let tabId = try XCTUnwrap(view.currentTabId)
            let holder = WebViewHolder()
            let pageReady = expectation(description: "The previous page finished loading")
            holder.onNavigationFinished = { _, _ in pageReady.fulfill() }
            let linkActivated = expectation(description: "WebKit allowed the main-frame link")
            var activationCount = 0
            holder.onMainFrameLinkActivated = {
                view.webTabDidActivateLink(tabId)
                activationCount += 1
                linkActivated.fulfill()
            }
            _ = holder.webView.loadHTMLString(#"<a href="about:blank?next">Next</a>"#, baseURL: nil)
            await fulfillment(of: [pageReady], timeout: 5)
            holder.onNavigationFinished = nil
            XCTAssertEqual(activationCount, 0)

            _ = try await holder.webView.evaluateJavaScript("document.querySelector('a').click()")
            await fulfillment(of: [linkActivated], timeout: 5)
        }
    }

    @MainActor
    func testClosingWindowInvalidatesAPendingRealmLoad() async throws {
        try await assertNoEnrollment { view in
            view.invalidateRealmNavigationForWindowClose()
        }
    }

    @MainActor
    func testInviteRedemptionPersistsAfterNavigatingAway() async throws {
        let preferences = try makePreferences()
        let documents = try discoveryDocuments(membership: "invite-only")
        let inviteBody = try JSONSerialization.data(withJSONObject: [
            "schema": "actr.realm-invite/v1",
            "realm": realmName,
            "secret": base64URL(Data(repeating: 1, count: 32))
        ])
        let inviteText = "ri_" + base64URL(inviteBody)
        let instanceURL = try XCTUnwrap(URL(string: realmURI))
        let join = NavigationGate()
        defer { join.release() }
        let unexpectedDiscovery = expectation(description: "A stale redemption must not reopen the Realm")
        unexpectedDiscovery.isInverted = true
        let view = BrowserViewModel(
            realmPreferences: preferences,
            fetchRealmDocument: { url, _ in
                if join.waiting { unexpectedDiscovery.fulfill() }
                return try XCTUnwrap(documents[url.absoluteString])
            },
            joinRealmWithInvite: { code in
                XCTAssertEqual(code, inviteText)
                await join.wait()
                return RealmJoinResult(
                    realmName: self.realmName, memberId: "member", role: "viewer", keyId: "key", instanceURL: instanceURL
                )
            },
            saveBrowserSession: { _, _ in }
        )
        view.navigateCurrentTab(to: realmURI)
        try await waitUntil { view.pendingInviteRedeem != nil }
        view.confirmInviteRedeem(try XCTUnwrap(view.pendingInviteRedeem), code: inviteText)
        try await waitUntil { join.waiting }
        view.navigateCurrentTab(to: "https://new.example/")
        join.release()

        try await waitUntil { preferences.stringArray(forKey: "actr.redeemedRealms") == [self.realmName] }
        await fulfillment(of: [unexpectedDiscovery], timeout: 0.25)
        XCTAssertEqual(view.currentTab?.url?.absoluteString, "https://new.example/")
        XCTAssertNil(view.pendingInviteRedeem)
    }

    @MainActor
    func testOpenRealmImportsOnlyAfterJoinSucceeds() async throws {
        let directory = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: false)
        defer { try? FileManager.default.removeItem(at: directory) }
        let fixture = try makeFrontArchive(in: directory)
        let frontDigest = try digest(Data(contentsOf: fixture.archive))
        let cacheDirectory = directory.appendingPathComponent("cache", isDirectory: true)
        let trustDirectory = directory.appendingPathComponent("trust", isDirectory: true)
        let publisherStore = trustDirectory.appendingPathComponent("verified-publishers.json")
        let apps = AppRepository(database: DatabaseManager(path: ":memory:"))
        let appId = "publisher.example/echo-front@1.0.0"
        var documents = try discoveryDocuments(membership: "open", frontDigest: frontDigest)
        documents["https://publisher.example/.well-known/actr"] = try JSONSerialization.data(withJSONObject: [
            "publisher": ["keys": [[
                "id": fixture.keyId, "public_key": fixture.publicKey.base64EncodedString(), "status": "active"
            ]]]
        ])
        let join = NavigationGate()
        defer { join.release() }
        var joined = false
        var downloads = 0
        let view = BrowserViewModel(
            realmPreferences: try makePreferences(),
            appRepository: apps,
            fetchRealmDocument: { url, _ in
                if url.host == "publisher.example" { XCTAssertTrue(joined) }
                return try XCTUnwrap(documents[url.absoluteString])
            },
            joinOpenRealm: { admission in
                XCTAssertEqual(admission.info.realmName, self.realmName)
                await join.wait()
                joined = true
            },
            downloadRealmArchive: { url in
                XCTAssertEqual(url.absoluteString, "https://publisher.example/echo-front.actr")
                XCTAssertTrue(joined)
                downloads += 1
                return fixture.archive
            },
            saveBrowserSession: { _, _ in }
        )
        view.navigateCurrentTab(to: "https://old.example/")
        let tabId = try XCTUnwrap(view.currentTabId)
        defer { RealmServiceBindings.shared.remove(tabId) }
        let cachedArchive = ActrPackageCache.$rootOverrideForTesting.withValue(cacheDirectory) {
            ActrPackageTrustStore.$supportDirectoryOverrideForTesting.withValue(trustDirectory) {
                view.navigateCurrentTab(to: realmURI)
            }
            return ActrPackageCache.archiveURL(forDigest: frontDigest)
        }
        try await waitUntil { join.waiting }
        XCTAssertEqual(view.currentTab?.url?.absoluteString, "https://old.example/")
        XCTAssertNil(view.currentTab?.appId)
        XCTAssertEqual(downloads, 0)
        join.release()

        try await waitUntil { view.currentTab?.appId != nil }
        XCTAssertEqual(view.currentTabId, tabId)
        XCTAssertEqual(view.currentTab?.url?.absoluteString, realmURI)
        XCTAssertEqual(view.currentTab?.appId, appId)
        XCTAssertEqual(view.currentTab?.unifiedScript, fixture.script)
        XCTAssertEqual(downloads, 1)
        XCTAssertTrue(FileManager.default.fileExists(atPath: cachedArchive.path))
        XCTAssertEqual(apps.getByBundleId(appId)?.sourcePath, cachedArchive.standardizedFileURL.path)
        let pinnedKeys = try JSONDecoder().decode([String: [[String: String]]].self, from: Data(contentsOf: publisherStore))
        XCTAssertEqual(pinnedKeys["publisher.example"], [[
            "public_key": fixture.publicKey.base64EncodedString(), "key_id": fixture.keyId
        ]])

        view.updateTabURL(for: tabId, url: try XCTUnwrap(URL(string: "https://old.example/late")))
        XCTAssertEqual(view.currentTab?.url?.absoluteString, realmURI)
    }

    @MainActor
    private func assertNoEnrollment(after supersede: @MainActor (BrowserViewModel) async throws -> Void) async throws {
        let documents = try discoveryDocuments(membership: "open")
        let profile = NavigationGate()
        let unwantedWork = expectation(description: "A superseded load must not enroll or download a Front")
        unwantedWork.isInverted = true
        let view = BrowserViewModel(
            realmPreferences: try makePreferences(),
            fetchRealmDocument: { url, _ in
                if url.path == "/realm/echo" { await profile.wait() }
                return try XCTUnwrap(documents[url.absoluteString])
            },
            joinOpenRealm: { _ in
                unwantedWork.fulfill()
                throw CancellationError()
            },
            downloadRealmArchive: { _ in
                unwantedWork.fulfill()
                throw CancellationError()
            },
            saveBrowserSession: { _, _ in }
        )
        defer { profile.release() }
        view.navigateCurrentTab(to: "https://old.example/")
        view.navigateCurrentTab(to: realmURI)
        try await waitUntil { profile.waiting }
        try await supersede(view)
        profile.release()
        await fulfillment(of: [unwantedWork], timeout: 0.25)
        XCTAssertNil(view.currentTab?.appId)
        XCTAssertNil(view.pendingInviteRedeem)
    }

    @MainActor
    private func waitUntil(_ condition: () -> Bool) async throws {
        let deadline = ContinuousClock.now + .seconds(3)
        while !condition() && ContinuousClock.now < deadline {
            try await Task.sleep(for: .milliseconds(10))
        }
        _ = try XCTUnwrap(condition() ? true : nil, "Timed out waiting for Realm navigation")
    }

    private func makePreferences() throws -> UserDefaults {
        let suite = "ActriumTests.RealmNavigation.\(UUID())"
        let preferences = try XCTUnwrap(UserDefaults(suiteName: suite))
        addTeardownBlock { UserDefaults(suiteName: suite)?.removePersistentDomain(forName: suite) }
        return preferences
    }

    private func discoveryDocuments(membership: String, frontDigest: String? = nil) throws -> [String: Data] {
        let (body, keys) = try profile(membership: membership, frontDigest: frontDigest)
        let descriptor = try JSONSerialization.data(withJSONObject: [
            "version": 1,
            "authority": "actr://realm.example",
            "services": [[
                "rel": "realms", "href": "https://realm.example/realm", "protocol": "actr-realm-profile/1"
            ]],
            "realm_signing_keys": try JSONSerialization.jsonObject(with: Data(keys.utf8))
        ])
        return [
            "https://realm.example/.well-known/actr": descriptor,
            "https://realm.example/realm/echo": body
        ]
    }

    private func profile(membership: String, frontDigest: String? = nil) throws -> (Data, String) {
        let signer = Curve25519.Signing.PrivateKey()
        let publicKey = signer.publicKey.rawRepresentation.base64EncodedString()
        let keys = try sign([
            "sequence": 1,
            "keys": [["id": "realm-key", "algorithm": "Ed25519", "public_key": publicKey, "status": "active"]]
        ], with: signer)
        var front = archive("echo-front", digest: "b")
        if let frontDigest { front["sha256"] = frontDigest }
        let body = try sign([
            "schema": "actr.realm/v1",
            "sequence": 1,
            "realm": ["name": realmName, "display_name": "Echo", "status": "active"],
            "primary_package": "service",
            "front_package": "front",
            "packages": [
                "service": archive("echo", digest: "a"),
                "front": front
            ],
            "issuer": [
                "url": "https://issuer.example",
                "keys": [["id": "7", "algorithm": "Ed25519", "public_key": publicKey, "status": "active"]]
            ],
            "provider": [
                "id": "provider.example",
                "signaling": [[
                    "url": "wss://signal.provider.example/ws",
                    "priority": 10,
                    "control_plane_id": "provider.example/v1",
                    "ticket_audience": "signal"
                ]],
                "ice": [["urls": ["stun:stun.provider.example:3478"]]]
            ],
            "policy": ["membership": membership],
            "issued_at": "2026-07-29T00:00:00Z"
        ], with: signer)
        return (body, String(decoding: keys, as: UTF8.self))
    }

    private func archive(_ name: String, digest: String) -> [String: String] {
        [
            "href": "https://publisher.example/\(name).actr",
            "type": "publisher.example/\(name)@1.0.0",
            "sha256": String(repeating: digest, count: 64)
        ]
    }

    private func sign(_ payload: [String: Any], with key: Curve25519.Signing.PrivateKey) throws -> Data {
        let body = try JSONSerialization.data(withJSONObject: payload, options: [.sortedKeys, .withoutEscapingSlashes])
        let signature = try key.signature(for: body)
        var document = payload
        document["signature"] = [
            "key_id": "realm-key", "algorithm": "Ed25519", "value": signature.base64EncodedString()
        ]
        return try JSONSerialization.data(withJSONObject: document)
    }

    private func makeFrontArchive(in directory: URL) throws -> (
        archive: URL, script: String, keyId: String, publicKey: Data
    ) {
        let fm = FileManager.default
        let key = Curve25519.Signing.PrivateKey()
        let publicKey = key.publicKey.rawRepresentation
        let keyId = "actr-pub-" + digest(publicKey).prefix(16)
        let script = "globalThis.realmNavigationFixture = true;"
        let files = [
            ("workload/guest.js", "workload", "text/javascript", Data(script.utf8)),
            ("ui/capability-manifest.json", "profile", "application/json", Data(#"{"hostModules":{}}"#.utf8)),
            ("proto/echo.proto", "contract", "text/plain", Data(#"syntax = "proto3"; package echo; service Echo {}"#.utf8))
        ]
        var manifest = """
        format = "actr-package/2"
        [actor]
        publisher = "publisher.example"
        name = "echo-front"
        version = "1.0.0"
        [workload]
        profile = "keel.guest/1"
        entry = "workload/guest.js"
        [workload.keel]
        runtime = "js"
        guest-api = "keel.guest/1"
        capabilities = "ui/capability-manifest.json"
        [host]
        profile = "actr.host/1"
        api = "^1"
        required-capabilities = ["ui.render"]
        [dependencies.service]
        actor = "publisher.example/echo"
        version = "=1.0.0"
        contract = "proto/echo.proto"
        required = true
        [signature]
        algorithm = "ed25519"
        key-id = "\(keyId)"
        """
        for (path, role, mediaType, bytes) in files {
            let url = directory.appendingPathComponent(path)
            try fm.createDirectory(at: url.deletingLastPathComponent(), withIntermediateDirectories: true)
            try bytes.write(to: url)
            manifest += "\n" + """
            [[files]]
            path = "\(path)"
            role = "\(role)"
            media-type = "\(mediaType)"
            size = \(bytes.count)
            sha256 = "\(digest(bytes))"
            """
        }
        let body = Data(manifest.utf8)
        try body.write(to: directory.appendingPathComponent("manifest.toml"))
        try key.signature(for: body).write(to: directory.appendingPathComponent("manifest.sig"))
        let archive = directory.appendingPathComponent("front.actr")
        let zip = Process()
        zip.executableURL = URL(fileURLWithPath: "/usr/bin/zip")
        zip.currentDirectoryURL = directory
        zip.arguments = ["-q", "-X", "-0", archive.lastPathComponent, "manifest.toml", "manifest.sig"] + files.map { $0.0 }
        try zip.run()
        zip.waitUntilExit()
        guard zip.terminationStatus == 0 else { throw URLError(.cannotCreateFile) }
        return (archive, script, keyId, publicKey)
    }

    private func digest(_ bytes: Data) -> String {
        SHA256.hash(data: bytes).map { String(format: "%02x", $0) }.joined()
    }

    private func base64URL(_ bytes: Data) -> String {
        bytes.base64EncodedString()
            .replacingOccurrences(of: "+", with: "-")
            .replacingOccurrences(of: "/", with: "_")
            .replacingOccurrences(of: "=", with: "")
    }
}

@MainActor
private final class NavigationGate {
    private let stream: AsyncStream<Void>
    private let signal: AsyncStream<Void>.Continuation
    private(set) var waiting = false

    init() {
        (stream, signal) = AsyncStream<Void>.makeStream()
    }

    func wait() async {
        waiting = true
        for await _ in stream { break }
    }

    func release() {
        signal.yield(())
        signal.finish()
    }
}
