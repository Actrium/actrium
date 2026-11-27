import ActriumCore
import AppKit
import CryptoKit
import Foundation
import WebKit
import XCTest

@testable import Actrium

final class RealmNavigationTests: XCTestCase {
    private let realmName = "echo@realm.example"
    private let realmURI = "actr://realm.example/echo"

    private let realmTrustDirectory = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)

    deinit {
        try? FileManager.default.removeItem(at: realmTrustDirectory)
    }

    @MainActor
    func testBackgroundInviteSurvivesThePreviousWebPageChangingURL() async throws {
        let documents = try discoveryDocuments(membership: "invite-only")
        let profile = NavigationGate()
        let view = BrowserViewModel(
            realmPreferences: try makePreferences(),
            realmTrustDirectory: realmTrustDirectory,
            fetchRealmDocument: { url, _ in
                if url.path == "/realm/echo" { await profile.wait() }
                return try XCTUnwrap(documents[url.absoluteString])
            },
            confirmRealmMembership: { _ in false },
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
    func testUserClickOnMainFrameWebLinkSupersedesAPendingRealmLoad() async throws {
        try await assertNoEnrollment { view in
            let tabId = try XCTUnwrap(view.currentTabId)
            let holder = WebViewHolder()
            let window = NSWindow(
                contentRect: NSRect(x: 0, y: 0, width: 240, height: 160),
                styleMask: .borderless, backing: .buffered, defer: false
            )
            window.isReleasedWhenClosed = false
            window.contentView = holder.webView
            defer { window.close() }
            await loadWebLink(in: holder)
            let linkActivated = expectation(description: "WebKit allowed a user-activated main-frame link")
            holder.onUserInitiatedMainFrameNavigation = {
                view.webTabDidActivateLink(tabId)
                linkActivated.fulfill()
            }

            let click = NSPoint(x: 20, y: 140)
            let timestamp = ProcessInfo.processInfo.systemUptime
            let mouseDown = try XCTUnwrap(NSEvent.mouseEvent(
                with: .leftMouseDown, location: click, modifierFlags: [], timestamp: timestamp,
                windowNumber: window.windowNumber, context: nil, eventNumber: 1, clickCount: 1, pressure: 1
            ))
            let mouseUp = try XCTUnwrap(NSEvent.mouseEvent(
                with: .leftMouseUp, location: click, modifierFlags: [], timestamp: timestamp,
                windowNumber: window.windowNumber, context: nil, eventNumber: 2, clickCount: 1, pressure: 0
            ))
            holder.webView.mouseDown(with: mouseDown)
            holder.webView.mouseUp(with: mouseUp)
            await fulfillment(of: [linkActivated], timeout: 5)
        }
    }

    @MainActor
    func testScriptedMainFrameWebLinkKeepsAPendingRealmLoad() async throws {
        let documents = try discoveryDocuments(membership: "invite-only")
        let profile = NavigationGate()
        defer { profile.release() }
        let view = BrowserViewModel(
            realmPreferences: try makePreferences(),
            realmTrustDirectory: realmTrustDirectory,
            fetchRealmDocument: { url, _ in
                if url.path == "/realm/echo" { await profile.wait() }
                return try XCTUnwrap(documents[url.absoluteString])
            },
            confirmRealmMembership: { _ in false },
            saveBrowserSession: { _, _ in }
        )
        view.navigateCurrentTab(to: "https://old.example/")
        let tabId = try XCTUnwrap(view.currentTabId)
        let holder = WebViewHolder()
        await loadWebLink(in: holder)
        let navigationFinished = expectation(description: "The scripted link navigated in the WebView")
        holder.onNavigationFinished = { _, url in
            guard let url, url.absoluteString == "about:blank?next" else { return }
            view.updateTabURL(for: tabId, url: url)
            navigationFinished.fulfill()
        }
        holder.onUserInitiatedMainFrameNavigation = {
            XCTFail("A scripted link was reported as a user click")
            view.webTabDidActivateLink(tabId)
        }
        view.navigateCurrentTab(to: realmURI)
        try await waitUntil { profile.waiting }

        _ = try await holder.webView.evaluateJavaScript("setTimeout(() => document.querySelector('a').click(), 0); void 0")
        await fulfillment(of: [navigationFinished], timeout: 5)
        profile.release()

        try await waitUntil { view.pendingRealmInvite(for: tabId) != nil }
        XCTAssertEqual(view.pendingInviteRedeem?.uri, realmURI)
    }

    @MainActor
    func testClosingWindowInvalidatesAPendingRealmLoad() async throws {
        try await assertNoEnrollment { view in
            view.invalidateRealmNavigationForWindowClose()
        }
    }

    @MainActor
    func testBackFromACompletedBlankPageRestoresTheWebURL() throws {
        let view = BrowserViewModel(
            realmPreferences: try makePreferences(),
            saveBrowserSession: { _, _ in }
        )
        view.navigateCurrentTab(to: "https://page.example/")
        let tabId = try XCTUnwrap(view.currentTabId)
        let blank = try XCTUnwrap(URL(string: "about:blank"))
        view.updateTabURL(for: tabId, url: blank)
        XCTAssertEqual(view.tabs.first { $0.id == tabId }?.url, blank)
        // A completed blank navigation classifies the tab as empty, but its
        // webview still owns it: the URL WebKit reports when the user goes
        // Back must land, or the tab stays blank forever.
        let restored = try XCTUnwrap(URL(string: "https://page.example/"))
        view.updateTabURL(for: tabId, url: restored)
        XCTAssertEqual(view.tabs.first { $0.id == tabId }?.url, restored)
    }

    @MainActor
    func testSystemPageTabRejectsLateWebURLUpdates() throws {
        let view = BrowserViewModel(
            realmPreferences: try makePreferences(),
            saveBrowserSession: { _, _ in }
        )
        view.navigateCurrentTab(to: "https://page.example/")
        let tabId = try XCTUnwrap(view.currentTabId)
        view.navigateCurrentTab(to: "about:history")
        let late = try XCTUnwrap(URL(string: "https://page.example/late"))
        view.updateTabURL(for: tabId, url: late)
        XCTAssertEqual(view.tabs.first { $0.id == tabId }?.url?.absoluteString, "about:history")
    }

    @MainActor
    func testTrackpadHistoryGestureSupersedesAPendingRealmLoad() async throws {
        try await assertNoEnrollment { view in
            let tabId = try XCTUnwrap(view.currentTabId)
            let holder = WebViewHolder()
            // Two distinct URLs so WebKit records a back-forward history entry.
            for page in ["first", "second"] {
                let ready = expectation(description: "Page \(page) finished loading")
                holder.onNavigationFinished = { _, _ in ready.fulfill() }
                holder.webView.load(URLRequest(url: try XCTUnwrap(URL(string: "about:blank?\(page)"))))
                await fulfillment(of: [ready], timeout: 5)
            }
            holder.onNavigationFinished = nil
            let navigatedBack = expectation(description: "WebKit allowed the trackpad history navigation")
            holder.onUserInitiatedMainFrameNavigation = {
                view.webTabDidActivateLink(tabId)
                navigatedBack.fulfill()
            }
            // A trackpad back swipe reaches the web view as a scroll stream
            // right before WebKit performs the traversal; inject the evidence.
            let scroll = try XCTUnwrap(CGEvent(
                scrollWheelEvent2Source: nil, units: .pixel,
                wheelCount: 2, wheel1: 0, wheel2: -10, wheel3: 0))
            scroll.setIntegerValueField(
                .scrollWheelEventScrollPhase, value: Int64(NSEvent.Phase.changed.rawValue))
            holder.webView.scrollWheel(with: try XCTUnwrap(NSEvent(cgEvent: scroll)))
            holder.goBack()
            await fulfillment(of: [navigatedBack], timeout: 5)
        }
    }

    @MainActor
    func testScriptedHistoryTraversalKeepsAPendingRealmLoad() async throws {
        let documents = try discoveryDocuments(membership: "invite-only")
        let profile = NavigationGate()
        defer { profile.release() }
        let view = BrowserViewModel(
            realmPreferences: try makePreferences(),
            realmTrustDirectory: realmTrustDirectory,
            fetchRealmDocument: { url, _ in
                if url.path == "/realm/echo" { await profile.wait() }
                return try XCTUnwrap(documents[url.absoluteString])
            },
            confirmRealmMembership: { _ in false },
            saveBrowserSession: { _, _ in }
        )
        view.navigateCurrentTab(to: "https://old.example/")
        let tabId = try XCTUnwrap(view.currentTabId)
        let holder = WebViewHolder()
        for page in ["first", "second"] {
            let ready = expectation(description: "Page \(page) finished loading")
            holder.onNavigationFinished = { _, _ in ready.fulfill() }
            holder.webView.load(URLRequest(url: try XCTUnwrap(URL(string: "about:blank?\(page)"))))
            await fulfillment(of: [ready], timeout: 5)
        }
        let traversedBack = expectation(description: "The scripted traversal went back")
        holder.onNavigationFinished = { _, url in
            guard url?.absoluteString == "about:blank?first" else { return }
            traversedBack.fulfill()
        }
        holder.onUserInitiatedMainFrameNavigation = {
            XCTFail("A scripted history traversal was reported as a user gesture")
            view.webTabDidActivateLink(tabId)
        }
        view.navigateCurrentTab(to: realmURI)
        try await waitUntil { profile.waiting }

        _ = try await holder.webView.evaluateJavaScript("history.back(); void 0")
        await fulfillment(of: [traversedBack], timeout: 5)
        profile.release()

        try await waitUntil { view.pendingRealmInvite(for: tabId) != nil }
        XCTAssertEqual(view.pendingInviteRedeem?.uri, realmURI)
    }

    @MainActor
    func testFormSubmissionSupersedesAPendingRealmLoad() async throws {
        try await assertNoEnrollment { view in
            let tabId = try XCTUnwrap(view.currentTabId)
            let holder = WebViewHolder()
            let pageReady = expectation(description: "The form page finished loading")
            holder.onNavigationFinished = { _, _ in pageReady.fulfill() }
            holder.webView.loadHTMLString("""
                <form action="about:blank?submitted" method="get"><input type="submit"></form>
                """, baseURL: nil)
            await fulfillment(of: [pageReady], timeout: 5)
            holder.onNavigationFinished = nil
            let submitted = expectation(description: "WebKit allowed the form submission")
            holder.onUserInitiatedMainFrameNavigation = {
                view.webTabDidActivateLink(tabId)
                submitted.fulfill()
            }
            _ = try await holder.webView.evaluateJavaScript("document.querySelector('form').submit()")
            await fulfillment(of: [submitted], timeout: 5)
        }
    }

    @MainActor
    func testUserReloadSupersedesAPendingRealmLoad() async throws {
        try await assertNoEnrollment { view in
            view.reloadCurrentTab()
        }
    }

    @MainActor
    func testInviteRedemptionPersistsAfterNavigatingAway() async throws {
        let preferences = try makePreferences()
        let documents = try discoveryDocuments(membership: "invite-only")
        let inviteText = try makeInviteText()
        let join = NavigationGate()
        defer { join.release() }
        let unexpectedDiscovery = expectation(description: "A stale redemption must not reopen the Realm")
        unexpectedDiscovery.isInverted = true
        let view = BrowserViewModel(
            realmPreferences: preferences,
            realmTrustDirectory: realmTrustDirectory,
            fetchRealmDocument: { url, _ in
                if join.waiting { unexpectedDiscovery.fulfill() }
                return try XCTUnwrap(documents[url.absoluteString])
            },
            inviteMembershipExchange: { profile, code, _, _ in
                XCTAssertEqual(profile.realmName, self.realmName)
                XCTAssertEqual(code, inviteText)
                await join.wait()
                return #"{"member_id":"member","role":"viewer","key_id":"key"}"#
            },
            confirmRealmMembership: { _ in false },
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
    func testOpenRealmImportsInTheBackgroundAfterJoining() async throws {
        try await assertBackgroundFrontImport(membership: "open", withInvite: false)
    }

    @MainActor
    func testOpenRealmRedeemsExplicitInviteBeforeLoadingFront() async throws {
        try await assertBackgroundFrontImport(membership: "open", withInvite: true)
    }

    @MainActor
    func testEmbeddedInviteImportsInTheBackgroundAfterRedemption() async throws {
        try await assertBackgroundFrontImport(membership: "invite-only", withInvite: true)
    }

    @MainActor
    func testRestoredRealmKeepsDynamicTitleAfterFrontActivation() async throws {
        let directory = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: false)
        defer { try? FileManager.default.removeItem(at: directory) }
        let fixture = try makeFrontArchive(in: directory)
        let frontDigest = try digest(Data(contentsOf: fixture.archive))
        var documents = try discoveryDocuments(membership: "open", frontDigest: frontDigest)
        documents["https://publisher.example/.well-known/actr"] = try publisherDescriptor(
            keyId: fixture.keyId, publicKey: fixture.publicKey
        )
        let preferences = try makePreferences()
        preferences.set([realmName], forKey: "actr.redeemedRealms")
        var savedTitles: [String] = []
        let view = BrowserViewModel(
            realmPreferences: preferences,
            realmTrustDirectory: realmTrustDirectory,
            appRepository: AppRepository(database: DatabaseManager(path: ":memory:")),
            guestStorageDatabase: GuestStorageDatabase(database: GuestLocalStorage(path: ":memory:")),
            fetchRealmDocument: { url, _ in
                try XCTUnwrap(documents[url.absoluteString])
            },
            joinOpenRealm: { _ in },
            downloadRealmArchive: { _ in fixture.archive },
            saveBrowserSession: { tabs, _ in savedTitles.append(contentsOf: tabs.map(\.title)) }
        )
        let restoredTabId = UUID()
        defer { RealmServiceBindings.shared.remove(restoredTabId) }
        ActrPackageCache.$rootOverrideForTesting.withValue(directory.appendingPathComponent("cache")) {
            ActrPackageTrustStore.$supportDirectoryOverrideForTesting.withValue(directory.appendingPathComponent("trust")) {
                view.restoreSessionIfNeeded {
                    BrowserDataRepository.RestoredSession(tabs: [
                        .init(id: restoredTabId, title: "Saved Runtime Title", address: realmURI)
                    ], activeTabIndex: 0)
                }
            }
        }

        XCTAssertEqual(view.tabs.count, 1)
        XCTAssertEqual(view.currentTab?.id, restoredTabId)
        XCTAssertEqual(view.currentTab?.title, "Saved Runtime Title")
        try await waitUntil {
            view.tabs.first { $0.id == restoredTabId }?.appId != nil || view.actrUriNotice != nil
        }
        if let notice = view.actrUriNotice {
            XCTFail("Realm activation failed: \(notice.detail ?? "Unknown error")")
            return
        }
        let realmTab = try XCTUnwrap(view.tabs.first { $0.id == restoredTabId })
        XCTAssertEqual(realmTab.unifiedScript, fixture.script)
        XCTAssertEqual(realmTab.title, "Saved Runtime Title")
        XCTAssertEqual(savedTitles.last, "Saved Runtime Title")

        view.updateTabTitle(for: restoredTabId, title: "Later Runtime Title")
        XCTAssertEqual(view.currentTab?.title, "Later Runtime Title")
        XCTAssertEqual(savedTitles.last, "Later Runtime Title")
    }

    @MainActor
    private func assertBackgroundFrontImport(membership: String, withInvite: Bool) async throws {
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
        let invite = withInvite ? try makeInviteText() : nil
        let instanceURL = try XCTUnwrap(URL(string: realmURI))
        let linkURL = try invite.map { try linkCarryingInvite($0, to: instanceURL) } ?? instanceURL
        var documents = try discoveryDocuments(membership: membership, frontDigest: frontDigest)
        documents["https://publisher.example/.well-known/actr"] = try publisherDescriptor(
            keyId: fixture.keyId, publicKey: fixture.publicKey
        )
        let profile = NavigationGate()
        let join = NavigationGate()
        defer {
            profile.release()
            join.release()
        }
        var joined = false
        var joinAttempts = 0
        var downloads = 0
        var savedSelection: UUID?
        var savedAddresses: [String] = []
        let view = BrowserViewModel(
            realmPreferences: try makePreferences(),
            realmTrustDirectory: realmTrustDirectory,
            appRepository: apps,
            guestStorageDatabase: GuestStorageDatabase(database: GuestLocalStorage(path: ":memory:")),
            fetchRealmDocument: { url, _ in
                if url.path == "/realm/echo", !profile.waiting { await profile.wait() }
                if url.host == "publisher.example" { XCTAssertTrue(joined) }
                return try XCTUnwrap(documents[url.absoluteString])
            },
            joinOpenRealm: { admission in
                XCTAssertEqual(membership, "open")
                XCTAssertNil(invite, "An explicit invite must not use invite-free admission")
                XCTAssertEqual(admission.info.realmName, self.realmName)
                joinAttempts += 1
                await join.wait()
                joined = true
            },
            inviteMembershipExchange: { profile, code, _, _ in
                XCTAssertEqual(profile.realmName, self.realmName)
                XCTAssertEqual(code, invite)
                joinAttempts += 1
                await join.wait()
                joined = true
                return #"{"member_id":"member","role":"editor","key_id":"key"}"#
            },
            confirmRealmMembership: { _ in false },
            downloadRealmArchive: { url in
                XCTAssertEqual(url.absoluteString, "https://publisher.example/echo-front.actr")
                XCTAssertTrue(joined)
                downloads += 1
                return fixture.archive
            },
            saveBrowserSession: { tabs, selection in
                savedSelection = selection
                savedAddresses.append(contentsOf: tabs.compactMap { $0.url?.absoluteString })
            }
        )
        view.navigateCurrentTab(to: "https://old.example/")
        let tabId = try XCTUnwrap(view.currentTabId)
        defer { RealmServiceBindings.shared.remove(tabId) }
        let cachedArchive = ActrPackageCache.$rootOverrideForTesting.withValue(cacheDirectory) {
            ActrPackageTrustStore.$supportDirectoryOverrideForTesting.withValue(trustDirectory) {
                view.navigateCurrentTab(to: linkURL.absoluteString)
            }
            return ActrPackageCache.archiveURL(forDigest: frontDigest)
        }
        try await waitUntil { profile.waiting }
        XCTAssertEqual(joinAttempts, 0)
        profile.release()
        try await waitUntil { join.waiting }
        XCTAssertNil(view.pendingInviteRedeem)
        XCTAssertEqual(view.currentTab?.url?.absoluteString, "https://old.example/")
        XCTAssertNil(view.currentTab?.appId)
        XCTAssertEqual(downloads, 0)
        if withInvite {
            XCTAssertTrue(view.inviteRedemptionInFlight)
            view.navigateCurrentTab(to: linkURL.absoluteString)
            view.openURLInNewTab(linkURL)
            XCTAssertEqual(view.tabs.map(\.id), [tabId])
        }
        let other = view.addNewTab(focusAddressBar: false).id
        view.navigateCurrentTab(to: "https://active.example/")
        join.release()

        try await waitUntil { view.tabs.first { $0.id == tabId }?.appId != nil }
        let realmTab = try XCTUnwrap(view.tabs.first { $0.id == tabId })
        XCTAssertEqual(view.currentTabId, other)
        XCTAssertEqual(view.actriumMode, .focus(tabId: other))
        XCTAssertEqual(view.currentTab?.url?.absoluteString, "https://active.example/")
        XCTAssertEqual(savedSelection, other)
        XCTAssertFalse(savedAddresses.contains { ActrLinkInvite.invite(from: $0) != nil })
        XCTAssertEqual(realmTab.url?.absoluteString, realmURI)
        XCTAssertEqual(realmTab.appId, appId)
        XCTAssertEqual(realmTab.unifiedScript, fixture.script)
        // The console instance name comes from the signed Realm Profile, not
        // the Front package's application name ("echo-front").
        XCTAssertEqual(realmTab.title, "Echo")
        XCTAssertEqual(realmTab.verifiedAppName, "echo-front")
        XCTAssertEqual(joinAttempts, 1)
        XCTAssertEqual(downloads, 1)
        XCTAssertTrue(FileManager.default.fileExists(atPath: cachedArchive.path))
        XCTAssertEqual(apps.getByBundleId(appId)?.sourcePath, cachedArchive.standardizedFileURL.path)
        let pinnedKeys = try JSONDecoder().decode([String: [[String: String]]].self, from: Data(contentsOf: publisherStore))
        XCTAssertEqual(pinnedKeys["publisher.example"], [[
            "public_key": fixture.publicKey.base64EncodedString(), "key_id": fixture.keyId
        ]])

        view.updateTabURL(for: tabId, url: try XCTUnwrap(URL(string: "https://old.example/late")))
        XCTAssertEqual(view.tabs.first { $0.id == tabId }?.url?.absoluteString, realmURI)
    }

    @MainActor
    func testDistinctEmbeddedInvitesSurviveOverlappingDiscovery() async throws {
        try await assertOverlappingInvites(navigateAwayFromSecondTab: false)
    }

    @MainActor
    func testQueuedInviteUsesTheNewlyAcceptedProfileBeforeOpenAdmission() async throws {
        let preferences = try makePreferences()
        let realmURL = try XCTUnwrap(URL(string: realmURI))
        let blockerURL = try XCTUnwrap(URL(string: "actr://other.example/other"))
        let invite = try makeInviteText()
        let blockerInvite = try makeInviteText(for: "other@other.example", secretByte: 2)
        let inviteLink = try linkCarryingInvite(invite, to: realmURL)
        let blockerLink = try linkCarryingInvite(blockerInvite, to: blockerURL)
        let signer = Curve25519.Signing.PrivateKey()
        let original = try discoveryDocuments(membership: "open", signer: signer)
        let updated = try profile(membership: "open", signer: signer, sequence: 2).0
        let blockerDocuments = try discoveryDocuments(membership: "invite-only", for: blockerURL)
        let blocker = NavigationGate()
        let exchange = NavigationGate()
        defer {
            blocker.release()
            exchange.release()
        }
        let earlyOpenJoin = expectation(description: "The queued invite must retain priority through redemption")
        earlyOpenJoin.isInverted = true
        var profileFetches = 0
        var inviteSettled = false
        var joinOrder: [String] = []
        let view = BrowserViewModel(
            realmPreferences: preferences,
            realmTrustDirectory: realmTrustDirectory,
            pendingRealmInvites: PendingRealmInvites(),
            fetchRealmDocument: { url, _ in
                if url.host == "publisher.example" { throw CancellationError() }
                if url.path == "/realm/echo" {
                    profileFetches += 1
                    if profileFetches > 1 { return updated }
                }
                return try XCTUnwrap(original[url.absoluteString] ?? blockerDocuments[url.absoluteString])
            },
            joinOpenRealm: { admission in
                XCTAssertEqual(admission.info.realmName, self.realmName)
                XCTAssertEqual(admission.info.sequence, 2)
                if !inviteSettled { earlyOpenJoin.fulfill() }
                joinOrder.append("open")
            },
            inviteMembershipExchange: { profile, code, _, _ in
                if code == blockerInvite {
                    joinOrder.append("blocker")
                    await blocker.wait()
                    return #"{"member_id":"blocker","role":"viewer","key_id":"key"}"#
                }
                XCTAssertEqual(code, invite)
                XCTAssertEqual(profile.realmName, self.realmName)
                XCTAssertEqual(profile.sequence, 2)
                joinOrder.append("invite")
                await exchange.wait()
                inviteSettled = true
                return #"{"member_id":"editor","role":"editor","key_id":"key"}"#
            },
            confirmRealmMembership: { _ in false },
            saveBrowserSession: { _, _ in }
        )
        func acceptedSequence() -> UInt64? {
            try? RealmProfileStore.currentRealm(realmName, preferences: preferences)?.accepted.info.sequence
        }

        view.navigateCurrentTab(to: blockerLink.absoluteString)
        try await waitUntil { blocker.waiting }
        view.openURLInNewTab(inviteLink)
        let invitedTab = try XCTUnwrap(view.currentTabId)
        try await waitUntil { acceptedSequence() == 1 }
        view.openURLInNewTab(realmURL)
        let plainTab = try XCTUnwrap(view.currentTabId)
        try await waitUntil { acceptedSequence() == 2 }
        XCTAssertEqual(joinOrder, ["blocker"])

        blocker.release()
        try await waitUntil { exchange.waiting }
        await fulfillment(of: [earlyOpenJoin], timeout: 0.25)
        XCTAssertEqual(joinOrder, ["blocker", "invite"])

        exchange.release()
        try await waitUntil {
            view.currentTab?.url == realmURL && view.tabs.first(where: { $0.id == invitedTab })?.url == realmURL
        }
        XCTAssertEqual(joinOrder, ["blocker", "invite", "open"])
        XCTAssertEqual(view.currentTabId, plainTab)
    }

    @MainActor
    func testQueuedInviteDoesNotEnrollAfterNavigatingAway() async throws {
        try await assertOverlappingInvites(navigateAwayFromSecondTab: true)
    }

    @MainActor
    func testPlainOpenWaitsForInviteExchangeToTheSameRealm() async throws {
        try await assertPlainOpenWaitsForInvite(inAnotherWindow: false)
    }

    @MainActor
    func testPlainOpenReverifiesWhenAnotherTabAdvancesTheProfileWhileItWaits() async throws {
        let preferences = try makePreferences()
        let realmURL = try XCTUnwrap(URL(string: realmURI))
        let blockerURL = try XCTUnwrap(URL(string: "actr://other.example/other"))
        let invite = try makeInviteText()
        let blockerInvite = try makeInviteText(for: "other@other.example", secretByte: 2)
        let inviteLink = try linkCarryingInvite(invite, to: realmURL)
        let blockerLink = try linkCarryingInvite(blockerInvite, to: blockerURL)
        let signer = Curve25519.Signing.PrivateKey()
        let original = try discoveryDocuments(membership: "open", signer: signer)
        let updated = try profile(membership: "open", signer: signer, sequence: 2).0
        let blockerDocuments = try discoveryDocuments(membership: "invite-only", for: blockerURL)
        let blocker = NavigationGate()
        let exchange = NavigationGate()
        defer {
            blocker.release()
            exchange.release()
        }
        var profileFetches = 0
        var plainJoinSequences: [UInt64] = []
        let view = BrowserViewModel(
            realmPreferences: preferences,
            realmTrustDirectory: realmTrustDirectory,
            pendingRealmInvites: PendingRealmInvites(),
            fetchRealmDocument: { url, _ in
                if url.host == "publisher.example" { throw CancellationError() }
                if url.path == "/realm/echo" {
                    profileFetches += 1
                    // Fetches 1-2 are the invited tab's and the first plain
                    // tab's discovery (sequence 1). The second plain tab
                    // discovers the republished profile (sequence 2).
                    if profileFetches > 2 { return updated }
                }
                return try XCTUnwrap(original[url.absoluteString] ?? blockerDocuments[url.absoluteString])
            },
            joinOpenRealm: { admission in
                XCTAssertEqual(admission.info.realmName, self.realmName)
                plainJoinSequences.append(admission.info.sequence)
            },
            inviteMembershipExchange: { profile, code, _, _ in
                if code == blockerInvite {
                    await blocker.wait()
                    return #"{"member_id":"blocker","role":"viewer","key_id":"key"}"#
                }
                XCTAssertEqual(code, invite)
                await exchange.wait()
                return #"{"member_id":"editor","role":"editor","key_id":"key"}"#
            },
            confirmRealmMembership: { _ in false },
            saveBrowserSession: { _, _ in }
        )
        func acceptedSequence() -> UInt64? {
            try? RealmProfileStore.currentRealm(realmName, preferences: preferences)?.accepted.info.sequence
        }

        view.navigateCurrentTab(to: blockerLink.absoluteString)
        try await waitUntil("blocker exchange started") { blocker.waiting }
        view.openURLInNewTab(inviteLink)
        let invitedTab = try XCTUnwrap(view.currentTabId)
        try await waitUntil("invited discovery accepted sequence 1") { acceptedSequence() == 1 }
        view.openURLInNewTab(realmURL)
        let firstPlainTab = try XCTUnwrap(view.currentTabId)
        // The first plain link verified sequence 1 and is waiting for the invite.
        try await waitUntil("first plain discovery fetched the profile") { profileFetches == 2 }

        // While the invite is still pending, another tab discovers and accepts
        // the republished Profile (sequence 2).
        view.openURLInNewTab(realmURL)
        let secondPlainTab = try XCTUnwrap(view.currentTabId)
        try await waitUntil("another tab advanced the accepted profile") { acceptedSequence() == 2 }

        blocker.release()
        try await waitUntil("invited exchange started") { exchange.waiting }
        exchange.release()

        // The first plain link must not silently drop the navigation: it
        // should re-verify the latest Profile and join under it, just like
        // the second plain link that discovered sequence 2 itself.
        try await waitUntil("both plain links joined after re-verifying") { plainJoinSequences.count == 2 }
        XCTAssertEqual(plainJoinSequences, [2, 2])
        try await waitUntil("invited tab opened the Realm") {
            view.tabs.first(where: { $0.id == invitedTab })?.url == realmURL
        }
        try await waitUntil("first plain tab opened the Realm") {
            view.tabs.first(where: { $0.id == firstPlainTab })?.url == realmURL
        }
        try await waitUntil("second plain tab opened the Realm") {
            view.tabs.first(where: { $0.id == secondPlainTab })?.url == realmURL
        }
    }

    @MainActor
    func testPlainOpenInAnotherWindowWaitsForPendingInvite() async throws {
        try await assertPlainOpenWaitsForInvite(inAnotherWindow: true)
    }

    @MainActor
    func testReloadingRealmReleasesSupersededInviteReservation() async throws {
        let address = try XCTUnwrap(URL(string: realmURI))
        let inviteLink = try linkCarryingInvite(makeInviteText(), to: address)
        let documents = try discoveryDocuments(membership: "open")
        let profileGate = NavigationGate()
        defer { profileGate.release() }
        var profileFetches = 0
        var openAttempts = 0
        let view = BrowserViewModel(
            realmPreferences: try makePreferences(),
            realmTrustDirectory: realmTrustDirectory,
            pendingRealmInvites: PendingRealmInvites(),
            fetchRealmDocument: { url, _ in
                if url.path == "/realm/echo" {
                    profileFetches += 1
                    if profileFetches == 1 { await profileGate.wait() }
                }
                return try XCTUnwrap(documents[url.absoluteString])
            },
            joinOpenRealm: { _ in
                openAttempts += 1
                throw CancellationError()
            },
            inviteMembershipExchange: { _, _, _, _ in
                XCTFail("The superseded invite reached the Issuer")
                throw CancellationError()
            },
            confirmRealmMembership: { _ in false },
            saveBrowserSession: { _, _ in }
        )
        view.tabs[0].url = address
        view.tabs[0].systemTabType = nil
        view.navigateCurrentTab(to: inviteLink.absoluteString)
        try await waitUntil { profileGate.waiting }

        view.reloadCurrentTab()

        try await waitUntil { openAttempts == 1 }
        XCTAssertEqual(profileFetches, 2)
    }

    @MainActor
    private func assertPlainOpenWaitsForInvite(inAnotherWindow: Bool) async throws {
        let instanceURL = try XCTUnwrap(URL(string: realmURI))
        let invite = try makeInviteText()
        let inviteURL = try linkCarryingInvite(invite, to: instanceURL)
        let documents = try discoveryDocuments(membership: "open")
        let pendingInvites = PendingRealmInvites()
        let preferences = try makePreferences()
        let inviteProfile = NavigationGate()
        let inviteExchange = NavigationGate()
        defer {
            inviteProfile.release()
            inviteExchange.release()
        }
        var profileRequests = 0
        var joinOrder: [String] = []
        var inviteSettled = false
        let earlyOpenJoin = expectation(description: "Invite-free admission must wait for the invite")
        earlyOpenJoin.isInverted = true
        let fetch: @MainActor (URL, String) async throws -> Data = { url, _ in
            if url.host == "publisher.example" { throw CancellationError() }
            if url.path == "/realm/echo" {
                profileRequests += 1
                if profileRequests == 1 { await inviteProfile.wait() }
            }
            return try XCTUnwrap(documents[url.absoluteString])
        }
        let joinOpen: @MainActor (RealmJoinService.VerifiedProfile) async throws -> Void = { profile in
            XCTAssertEqual(profile.info.realmName, self.realmName)
            if !inviteSettled { earlyOpenJoin.fulfill() }
            joinOrder.append("open")
        }
        let invitedWindow = BrowserViewModel(
            realmPreferences: preferences,
            realmTrustDirectory: realmTrustDirectory,
            pendingRealmInvites: pendingInvites,
            fetchRealmDocument: fetch,
            joinOpenRealm: joinOpen,
            inviteMembershipExchange: { profile, code, _, _ in
                XCTAssertEqual(profile.realmName, self.realmName)
                XCTAssertEqual(code, invite)
                joinOrder.append("invite")
                await inviteExchange.wait()
                inviteSettled = true
                return #"{"member_id":"editor","role":"editor","key_id":"key"}"#
            },
            confirmRealmMembership: { _ in false },
            saveBrowserSession: { _, _ in }
        )
        let plainWindow = inAnotherWindow ? BrowserViewModel(
            realmPreferences: preferences,
            realmTrustDirectory: realmTrustDirectory,
            pendingRealmInvites: pendingInvites,
            fetchRealmDocument: fetch,
            joinOpenRealm: joinOpen,
            confirmRealmMembership: { _ in false },
            saveBrowserSession: { _, _ in }
        ) : invitedWindow

        invitedWindow.navigateCurrentTab(to: inviteURL.absoluteString)
        try await waitUntil { inviteProfile.waiting }
        if !inAnotherWindow {
            inviteProfile.release()
            try await waitUntil { inviteExchange.waiting }
        }
        plainWindow.openURLInNewTab(instanceURL)
        try await waitUntil { profileRequests == 2 }
        if inAnotherWindow {
            XCTAssertTrue(joinOrder.isEmpty)
            inviteProfile.release()
            try await waitUntil { inviteExchange.waiting }
            // Closing the invited window must preserve the reservation until
            // its already-started exchange completes.
            invitedWindow.invalidateRealmNavigationForWindowClose()
        }
        await fulfillment(of: [earlyOpenJoin], timeout: 0.25)
        XCTAssertEqual(joinOrder, ["invite"])

        inviteExchange.release()
        try await waitUntil { plainWindow.currentTab?.url == instanceURL }
        XCTAssertEqual(joinOrder, ["invite", "open"])
        XCTAssertNil(plainWindow.pendingInviteRedeem)
    }

    @MainActor
    private func assertOverlappingInvites(navigateAwayFromSecondTab: Bool) async throws {
        let firstURL = try XCTUnwrap(URL(string: realmURI))
        let secondURL = try XCTUnwrap(URL(string: "actr://other.example/other"))
        let secondRealm = "other@other.example"
        let firstInvite = try makeInviteText()
        let secondInvite = try makeInviteText(for: secondRealm, secretByte: 2)
        let firstLink = try linkCarryingInvite(firstInvite, to: firstURL)
        let secondLink = try linkCarryingInvite(secondInvite, to: secondURL)
        let firstDocuments = try discoveryDocuments(membership: "invite-only", for: firstURL)
        let secondDocuments = try discoveryDocuments(membership: "invite-only", for: secondURL)
        let firstProfile = NavigationGate()
        let secondProfile = NavigationGate()
        let firstJoin = NavigationGate()
        defer {
            firstProfile.release()
            secondProfile.release()
            firstJoin.release()
        }

        let earlySecondJoin = expectation(description: "The second exchange must wait for the first")
        earlySecondJoin.isInverted = true
        var firstJoinFinished = false
        var secondProfileReturned = false
        var joinAttempts: [String] = []
        let preferences = try makePreferences()
        let view = BrowserViewModel(
            realmPreferences: preferences,
            realmTrustDirectory: realmTrustDirectory,
            pendingRealmInvites: PendingRealmInvites(),
            fetchRealmDocument: { url, _ in
                if url.host == "publisher.example" { throw CancellationError() }
                if url.path == "/realm/echo" { await firstProfile.wait() }
                if url.path == "/realm/other" {
                    await secondProfile.wait()
                    secondProfileReturned = true
                }
                return try XCTUnwrap(firstDocuments[url.absoluteString] ?? secondDocuments[url.absoluteString])
            },
            inviteMembershipExchange: { profile, code, _, _ in
                joinAttempts.append(code)
                if code == firstInvite {
                    XCTAssertEqual(profile.realmName, self.realmName)
                    await firstJoin.wait()
                    firstJoinFinished = true
                    return #"{"member_id":"first","role":"editor","key_id":"key"}"#
                }
                XCTAssertEqual(code, secondInvite)
                XCTAssertEqual(profile.realmName, secondRealm)
                if !firstJoinFinished { earlySecondJoin.fulfill() }
                return #"{"member_id":"second","role":"editor","key_id":"key"}"#
            },
            confirmRealmMembership: { _ in false },
            saveBrowserSession: { _, _ in }
        )

        let firstTab = try XCTUnwrap(view.currentTabId)
        view.navigateCurrentTab(to: firstLink.absoluteString)
        try await waitUntil { firstProfile.waiting }
        view.openURLInNewTab(firstLink)
        XCTAssertEqual(view.tabs.map(\.id), [firstTab])
        view.openURLInNewTab(secondLink)
        let secondTab = try XCTUnwrap(view.currentTabId)
        XCTAssertNotEqual(firstTab, secondTab)
        try await waitUntil { secondProfile.waiting }

        firstProfile.release()
        try await waitUntil { firstJoin.waiting }
        secondProfile.release()
        try await waitUntil { secondProfileReturned }
        await fulfillment(of: [earlySecondJoin], timeout: 0.25)
        XCTAssertEqual(joinAttempts, [firstInvite])
        view.openURLInNewTab(secondLink)
        XCTAssertEqual(view.tabs.map(\.id), [firstTab, secondTab])
        if navigateAwayFromSecondTab { view.navigateCurrentTab(to: "https://active.example/") }

        firstJoin.release()
        try await waitUntil { !view.inviteRedemptionInFlight }
        if navigateAwayFromSecondTab {
            XCTAssertEqual(joinAttempts, [firstInvite])
            XCTAssertEqual(preferences.stringArray(forKey: "actr.redeemedRealms"), [realmName])
            XCTAssertEqual(view.currentTab?.url?.absoluteString, "https://active.example/")
        } else {
            try await waitUntil { view.currentTab?.url == secondURL }
            XCTAssertEqual(joinAttempts, [firstInvite, secondInvite])
            XCTAssertEqual(Set(preferences.stringArray(forKey: "actr.redeemedRealms") ?? []), [realmName, secondRealm])
        }
        try await waitUntil { view.tabs.first { $0.id == firstTab }?.url == firstURL }
        XCTAssertEqual(view.currentTabId, secondTab)
        XCTAssertNil(view.pendingInviteRedeem)
    }

    @MainActor
    private func loadWebLink(in holder: WebViewHolder) async {
        let pageReady = expectation(description: "The previous page finished loading")
        holder.onNavigationFinished = { _, _ in pageReady.fulfill() }
        holder.webView.loadHTMLString("""
            <style>body { margin: 0; } a { display: block; width: 160px; height: 80px; }</style>
            <a href="about:blank?next">Next</a>
            """, baseURL: nil)
        await fulfillment(of: [pageReady], timeout: 5)
        holder.onNavigationFinished = nil
    }

    @MainActor
    private func assertNoEnrollment(after supersede: @MainActor (BrowserViewModel) async throws -> Void) async throws {
        let documents = try discoveryDocuments(membership: "open")
        let profile = NavigationGate()
        let unwantedWork = expectation(description: "A superseded load must not enroll or download a Front")
        unwantedWork.isInverted = true
        let view = BrowserViewModel(
            realmPreferences: try makePreferences(),
            realmTrustDirectory: realmTrustDirectory,
            fetchRealmDocument: { url, _ in
                if url.path == "/realm/echo" { await profile.wait() }
                return try XCTUnwrap(documents[url.absoluteString])
            },
            joinOpenRealm: { _ in
                unwantedWork.fulfill()
                throw CancellationError()
            },
            confirmRealmMembership: { _ in false },
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
    private func waitUntil(_ label: String = "Realm navigation", _ condition: () -> Bool) async throws {
        let deadline = ContinuousClock.now + .seconds(3)
        while !condition() && ContinuousClock.now < deadline {
            try await Task.sleep(for: .milliseconds(10))
        }
        _ = try XCTUnwrap(condition() ? true : nil, "Timed out waiting for \(label)")
    }

    private func makePreferences() throws -> UserDefaults {
        let suite = "ActriumTests.RealmNavigation.\(UUID())"
        let preferences = try XCTUnwrap(UserDefaults(suiteName: suite))
        addTeardownBlock { UserDefaults(suiteName: suite)?.removePersistentDomain(forName: suite) }
        return preferences
    }

    private func discoveryDocuments(
        membership: String, frontDigest: String? = nil, for realmURL: URL? = nil,
        signer: Curve25519.Signing.PrivateKey = Curve25519.Signing.PrivateKey()
    ) throws -> [String: Data] {
        let address = try realmURL ?? XCTUnwrap(URL(string: realmURI))
        let domain = try XCTUnwrap(address.host)
        let realmId = address.lastPathComponent
        let (body, keys) = try profile(
            membership: membership, frontDigest: frontDigest, realmName: "\(realmId)@\(domain)", signer: signer
        )
        let descriptor = try JSONSerialization.data(withJSONObject: [
            "version": 1,
            "authority": "actr://\(domain)",
            "services": [[
                "rel": "realms", "href": "https://\(domain)/realm", "protocol": "actr-realm-profile/1"
            ]],
            "realm_signing_keys": try JSONSerialization.jsonObject(with: Data(keys.utf8))
        ])
        return [
            "https://\(domain)/.well-known/actr": descriptor,
            "https://\(domain)/realm/\(realmId)": body
        ]
    }

    private func publisherDescriptor(keyId: String, publicKey: Data) throws -> Data {
        let root = Curve25519.Signing.PrivateKey()
        let now = Date()
        let formatter = ISO8601DateFormatter()
        let signed = try sign([
            "schema": "actr.publisher/v1",
            "domain": "publisher.example",
            "sequence": 1,
            "profile_signing_keys": [[
                "id": "realm-key", "algorithm": "Ed25519", "status": "active",
                "public_key": root.publicKey.rawRepresentation.base64EncodedString()
            ]],
            "keys": [[
                "id": keyId, "algorithm": "Ed25519", "status": "active",
                "public_key": publicKey.base64EncodedString()
            ]],
            "profile_issued_at": formatter.string(from: now.addingTimeInterval(-60)),
            "profile_expires_at": formatter.string(from: now.addingTimeInterval(3600))
        ], with: root)
        return try JSONSerialization.data(withJSONObject: [
            "version": 1, "authority": "actr://publisher.example",
            "services": [["rel": "contracts", "href": "https://publisher.example/contracts"]],
            "publisher": try JSONSerialization.jsonObject(with: signed)
        ])
    }

    private func profile(
        membership: String, frontDigest: String? = nil, realmName: String? = nil,
        signer: Curve25519.Signing.PrivateKey, sequence: Int = 1
    ) throws -> (Data, String) {
        let publicKey = signer.publicKey.rawRepresentation.base64EncodedString()
        let keys = try sign([
            "sequence": 1,
            "keys": [["id": "realm-key", "algorithm": "Ed25519", "public_key": publicKey, "status": "active"]]
        ], with: signer)
        var front = archive("echo-front", digest: "b")
        if let frontDigest { front["sha256"] = frontDigest }
        let body = try sign([
            "schema": "actr.realm/v1",
            "sequence": sequence,
            "realm": ["name": realmName ?? self.realmName, "display_name": "Echo", "status": "active"],
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

    private func makeInviteText(for realm: String? = nil, secretByte: UInt8 = 1) throws -> String {
        let body = try JSONSerialization.data(withJSONObject: [
            "schema": "actr.realm-invite/v1",
            "realm": realm ?? realmName,
            "secret": base64URL(Data(repeating: secretByte, count: 32))
        ])
        return "ri_" + base64URL(body)
    }

    private func linkCarryingInvite(_ invite: String, to instanceURL: URL) throws -> URL {
        var link = try XCTUnwrap(URLComponents(url: instanceURL, resolvingAgainstBaseURL: false))
        link.queryItems = [URLQueryItem(name: "args", value: #"{"invite":"\#(invite)"}"#)]
        return try XCTUnwrap(link.url)
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
