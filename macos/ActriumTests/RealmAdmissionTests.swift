import AppKit
import ActrBindings
import ActriumCore
import CryptoKit
import Foundation
import XCTest

@testable import Actrium

final class RealmAdmissionTests: XCTestCase {
    private let realmName = "echo@realm.example"
    private let realmTrustDirectory = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)

    deinit {
        try? FileManager.default.removeItem(at: realmTrustDirectory)
    }


    @MainActor
    func testSignedOpenPolicyAllowsInviteFreeAdmission() throws {
        let (body, keys) = try profile(membership: "open")
        let verified = try RealmJoinService.VerifiedProfile(
            body: body, realmName: realmName, signingKeysJSON: keys, preferences: try makePreferences()
        )
        XCTAssertTrue(verified.isOpen)
        XCTAssertEqual(verified.info.realmName, realmName)
    }

    @MainActor
    func testInviteOnlyPolicyCannotUseOpenJoin() async throws {
        let (body, keys) = try profile(membership: "invite-only")
        let verified = try RealmJoinService.VerifiedProfile(
            body: body, realmName: realmName, signingKeysJSON: keys, preferences: try makePreferences()
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
        let preferences = try makePreferences()
        XCTAssertThrowsError(try RealmJoinService.VerifiedProfile(
            body: changed, realmName: realmName, signingKeysJSON: keys, preferences: preferences
        ))
    }

    @MainActor
    func testRealmBaselineAdvancesBeforeFrontInstallation() throws {
        let preferences = try makePreferences()
        let signer = Curve25519.Signing.PrivateKey()
        let original = try profile(membership: "open", signer: signer)
        let first = try accept(original, preferences: preferences)
        let sameSequence = try profile(membership: "invite-only", signer: signer)
        XCTAssertThrowsError(try accept(sameSequence, preferences: preferences))
        XCTAssertTrue(RealmProfileStore.isCurrent(first, preferences: preferences))

        let update = try profile(membership: "invite-only", sequence: 2, signer: signer)
        let next = try accept(update, preferences: preferences)
        // A failed package load after acceptance leaves this newer baseline in place.
        XCTAssertEqual(next.info.sequence, 2)
        XCTAssertThrowsError(try accept(original, preferences: preferences))
        XCTAssertTrue(RealmProfileStore.isCurrent(next, preferences: preferences))
    }

    @MainActor
    func testFixedPackageDigestsSurviveReselectionAndCacheUpgrade() throws {
        let preferences = try makePreferences()
        let signer = Curve25519.Signing.PrivateKey()
        let original = try profile(membership: "open", signer: signer)
        preferences.set([
            "documents": [String(decoding: original.0, as: UTF8.self)], "signingKeys": original.1
        ], forKey: "actr.acceptedRealmProfile.\(realmName)")

        let localKeyId = try XCTUnwrap(packageKey(signer.publicKey.rawRepresentation, status: "active")["id"])
        let alternate = try profile(
            membership: "open", sequence: 2, signer: signer,
            localFrontKeyId: localKeyId, frontName: "echo-alternate"
        )
        let current = try accept(alternate, preferences: preferences)
        let changedOriginal = try profile(membership: "open", sequence: 3, signer: signer, frontDigest: "c")
        XCTAssertThrowsError(try accept(changedOriginal, preferences: preferences)) { error in
            guard case CoreError.RealmProfileInvalid(let message) = error else {
                return XCTFail("Expected a fixed package digest conflict")
            }
            XCTAssertTrue(message.contains("fixed digest"))
        }
        XCTAssertTrue(RealmProfileStore.isCurrent(current, preferences: preferences))

        let reselected = try profile(membership: "open", sequence: 3, signer: signer)
        XCTAssertEqual(try accept(reselected, preferences: preferences).info.sequence, 3)
        let fixedAlternate = try profile(
            membership: "open", sequence: 4, signer: signer, frontName: "echo-alternate", frontDigest: "d"
        )
        XCTAssertEqual(try accept(fixedAlternate, preferences: preferences).info.sequence, 4)
    }

    @MainActor
    func testRevokedRetirementFallsBackToTheLastTrustedProfile() throws {
        let preferences = try makePreferences()
        let removed = Curve25519.Signing.PrivateKey()
        let remaining = Curve25519.Signing.PrivateKey()
        let initial = try authorityDescriptor(
            sequence: 1, signer: removed, signerId: "removed",
            keys: ["removed": removed, "remaining": remaining], extensionValue: "signed metadata"
        )
        let updated = try authorityDescriptor(
            sequence: 2, signer: remaining, signerId: "remaining", keys: ["remaining": remaining]
        )

        func acceptAuthority(_ body: String, preferences: UserDefaults) throws -> AuthorityDiscoveryInfo {
            try RealmProfileStore.acceptAuthority(
                body: body, domain: "realm.example", realmName: realmName, preferences: preferences
            )
        }

        let firstKeys = try acceptAuthority(initial, preferences: preferences)
        let savedActive = try profile(membership: "open", sequence: 4, signer: remaining, keyId: "remaining")
        let removedSelection = try profile(
            membership: "open", sequence: 5, signer: removed, keyId: "removed",
            frontName: "echo-alternate", frontDigest: "c"
        )
        let retired = try profile(
            membership: "open", sequence: 90, status: "retired", signer: removed, keyId: "removed"
        )
        _ = try accept((savedActive.0, firstKeys.signingKeysJson), preferences: preferences)
        _ = try accept((removedSelection.0, firstKeys.signingKeysJson), preferences: preferences)
        XCTAssertThrowsError(try accept((retired.0, firstKeys.signingKeysJson), preferences: preferences)) { error in
            guard case CoreError.RealmRetired = error else { return XCTFail("Expected accepted retirement") }
        }

        let current = try acceptAuthority(updated, preferences: preferences)
        XCTAssertEqual(current.signingKeys.map(\.id), ["remaining"])
        XCTAssertEqual(
            try RealmProfileStore.fallbackRealm(realmName, preferences: preferences)?.accepted.info.sequence,
            4
        )
        XCTAssertThrowsError(try acceptAuthority(initial, preferences: preferences))
        // A Profile fetched against the earlier descriptor cannot reuse its revoked signer.
        XCTAssertThrowsError(try accept((retired.0, firstKeys.signingKeysJson), preferences: preferences))
        let rollback = try profile(membership: "open", sequence: 3, signer: remaining, keyId: "remaining")
        XCTAssertThrowsError(try accept((rollback.0, current.signingKeysJson), preferences: preferences))
        let replacement = try profile(
            membership: "open", sequence: 5, signer: remaining, keyId: "remaining",
            frontName: "echo-alternate", frontDigest: "d"
        )
        let restored = try accept((replacement.0, current.signingKeysJson), preferences: preferences)
        XCTAssertEqual(restored.info.sequence, 5)
        let changedTrusted = try profile(
            membership: "open", sequence: 6, signer: remaining, keyId: "remaining", frontDigest: "e"
        )
        XCTAssertThrowsError(try accept((changedTrusted.0, current.signingKeysJson), preferences: preferences))

        // Upgrade from the previous one-document cache, which held only the revoked retirement.
        let legacy = try makePreferences()
        legacy.set([
            "body": String(decoding: retired.0, as: UTF8.self), "signingKeys": firstKeys.signingKeysJson
        ], forKey: "actr.acceptedRealmProfile.\(realmName)")
        let migrated = try acceptAuthority(updated, preferences: legacy)
        XCTAssertEqual(
            try accept((rollback.0, migrated.signingKeysJson), preferences: legacy).info.sequence,
            3
        )
    }

    @MainActor
    func testFailedProfileFetchAfterRevocationStopsRunningFrontsAndUsesFallback() async throws {
        let preferences = try makePreferences()
        let removed = Curve25519.Signing.PrivateKey()
        let remaining = Curve25519.Signing.PrivateKey()
        let initial = try authorityDescriptor(
            sequence: 1, signer: removed, signerId: "removed",
            keys: ["removed": removed, "remaining": remaining]
        )
        let updated = try authorityDescriptor(
            sequence: 2, signer: remaining, signerId: "remaining", keys: ["remaining": remaining]
        )
        let authority = try RealmProfileStore.acceptAuthority(
            body: initial, domain: "realm.example", realmName: realmName, preferences: preferences
        )
        let running = try profile(membership: "open", sequence: 2, signer: removed, keyId: "removed")
        _ = try accept((running.0, authority.signingKeysJson), preferences: preferences)
        let fallback = try profile(membership: "invite-only", sequence: 3, signer: remaining, keyId: "remaining")
        _ = try accept((fallback.0, authority.signingKeysJson), preferences: preferences)

        let otherWindow = BrowserViewModel(realmPreferences: preferences, realmTrustDirectory: realmTrustDirectory, saveBrowserSession: { _, _ in })
        let trustedWindow = BrowserViewModel(realmPreferences: preferences, realmTrustDirectory: realmTrustDirectory, saveBrowserSession: { _, _ in })
        let invite = try inviteCode()
        var failedProfileFetches = 0
        var redeemedProfile: RealmProfileInfo?
        let refreshingWindow = BrowserViewModel(
            realmPreferences: preferences,
            realmTrustDirectory: realmTrustDirectory,
            fetchRealmDocument: { url, _ in
                if url.host == "publisher.example" { throw CancellationError() }
                if url.path == "/.well-known/actr" { return Data(updated.utf8) }
                XCTAssertEqual(url.path, "/realm/echo")
                XCTAssertNil(otherWindow.currentTab?.guestScript, "The other window must stop before the Profile fetch")
                XCTAssertEqual(trustedWindow.currentTab?.guestScript, "installed Front")
                failedProfileFetches += 1
                throw URLError(.resourceUnavailable)
            },
            inviteMembershipExchange: { profile, code, _, _ in
                XCTAssertEqual(code, invite)
                redeemedProfile = profile
                return #"{"member_id":"test-member","role":"viewer","key_id":"test-client"}"#
            },
            confirmRealmMembership: { _ in false },
            saveBrowserSession: { _, _ in }
        )
        let link = try XCTUnwrap(URL(string: "actr://realm.example/echo"))
        for (window, signer) in [(refreshingWindow, "removed"), (otherWindow, "removed"), (trustedWindow, "remaining")] {
            window.tabs[0].url = link
            window.tabs[0].systemTabType = nil
            window.tabs[0].appId = "echo-front"
            window.tabs[0].guestScript = "installed Front"
            window.tabs[0].serviceCallUrl = "actr://realm.example/service"
            window.tabs[0].realmProfileSigner = (realmName, signer)
        }

        let movedWindow = BrowserViewModel(
            realmPreferences: preferences,
            realmTrustDirectory: realmTrustDirectory,
            fetchRealmDocument: { url, _ in
                if url.path == "/.well-known/actr" { return Data(updated.utf8) }
                throw URLError(.resourceUnavailable)
            },
            confirmRealmMembership: { _ in false },
            saveBrowserSession: { _, _ in }
        )
        let transfer = Tab(
            title: "Echo", url: link, appId: "echo-front", serviceCallUrl: "actr://realm.example/service",
            guestScript: "installed Front", guestExtractedPath: "/transfer/cache"
        )
        refreshingWindow.reloadCurrentTab()
        let deadline = ContinuousClock.now + .seconds(2)
        while refreshingWindow.pendingInviteRedeem == nil && ContinuousClock.now < deadline {
            try await Task.sleep(for: .milliseconds(10))
        }
        XCTAssertGreaterThan(failedProfileFetches, 0)
        XCTAssertEqual(refreshingWindow.pendingInviteRedeem?.uri, link.absoluteString)

        movedWindow.insertTabFromCrossWindow(transfer, at: movedWindow.tabs.count, groupId: nil)
        XCTAssertNil(movedWindow.currentTab?.guestScript)
        XCTAssertNil(movedWindow.currentTab?.guestExtractedPath)
        let movedDeadline = ContinuousClock.now + .seconds(2)
        while movedWindow.pendingInviteRedeem == nil && ContinuousClock.now < movedDeadline {
            try await Task.sleep(for: .milliseconds(10))
        }
        XCTAssertEqual(movedWindow.pendingInviteRedeem?.uri, link.absoluteString)
        for window in [refreshingWindow, otherWindow, movedWindow] {
            XCTAssertNil(window.currentTab?.appId)
            XCTAssertNil(window.currentTab?.guestScript)
            XCTAssertNil(window.currentTab?.serviceCallUrl)
            XCTAssertNil(window.currentTab?.realmProfileSigner)
        }
        XCTAssertEqual(trustedWindow.currentTab?.realmProfileSigner?.keyId, "remaining")
        XCTAssertEqual(
            try RealmProfileStore.fallbackRealm(realmName, preferences: preferences)?.accepted.info.sequence,
            3
        )

        let fetchesBeforeRedeeming = failedProfileFetches
        refreshingWindow.confirmInviteRedeem(try XCTUnwrap(refreshingWindow.pendingInviteRedeem), code: invite)
        let redeemedDeadline = ContinuousClock.now + .seconds(2)
        func didRedeem() -> Bool {
            preferences.stringArray(forKey: "actr.redeemedRealms")?.contains(realmName) == true
        }
        while !didRedeem() && ContinuousClock.now < redeemedDeadline {
            try await Task.sleep(for: .milliseconds(10))
        }
        XCTAssertTrue(didRedeem())
        XCTAssertGreaterThan(failedProfileFetches, fetchesBeforeRedeeming)
        XCTAssertEqual(redeemedProfile?.realmName, realmName)
        XCTAssertEqual(redeemedProfile?.sequence, 3)
        refreshingWindow.invalidateRealmLoadsForWindowClose()
        movedWindow.invalidateRealmLoadsForWindowClose()

        let unexpectedExchange: RealmJoinService.InviteExchange = { _, _, _, _ in
            XCTFail("Retired or cancelled discovery must not redeem an invite")
            throw CancellationError()
        }
        do {
            _ = try await RealmJoinService.joinWithInvite(
                inviteText: invite, label: "Test", preferences: preferences,
                fetchDocument: { url in
                    if url.path == "/.well-known/actr" { return Data(updated.utf8) }
                    throw URLError(.cancelled)
                },
                exchange: unexpectedExchange
            )
            XCTFail("Expected cancellation")
        } catch let error as URLError {
            XCTAssertEqual(error.code, .cancelled)
        }

        let retired = try profile(
            membership: "invite-only", sequence: 4, status: "retired", signer: remaining, keyId: "remaining"
        )
        do {
            _ = try await RealmJoinService.joinWithInvite(
                inviteText: invite, label: "Test", preferences: preferences,
                fetchDocument: { url in
                    url.path == "/.well-known/actr" ? Data(updated.utf8) : retired.0
                },
                exchange: unexpectedExchange
            )
            XCTFail("Expected retirement")
        } catch CoreError.RealmRetired(let retiredRealm) {
            XCTAssertEqual(retiredRealm, realmName)
        }
    }

    @MainActor
    func testInviteLinkRedeemsOnceAndSavesTheCleanAddress() async throws {
        let preferences = try makePreferences()
        let signer = Curve25519.Signing.PrivateKey()
        let authority = try authorityDescriptor(
            sequence: 1, signer: signer, signerId: "realm-key", keys: ["realm-key": signer]
        )
        let (body, _) = try profile(membership: "invite-only", signer: signer)
        let invite = try inviteCode()
        let realmAddress = "actr://realm.example/echo"
        var link = try XCTUnwrap(URLComponents(string: realmAddress))
        let arguments = try JSONEncoder().encode(["invite": invite])
        link.queryItems = [URLQueryItem(name: "args", value: String(decoding: arguments, as: UTF8.self))]
        let linkURL = try XCTUnwrap(link.url)
        var exchanges = 0
        var savedAddresses: [String] = []
        let viewModel = BrowserViewModel(
            realmPreferences: preferences,
            realmTrustDirectory: realmTrustDirectory,
            fetchRealmDocument: { url, _ in
                if url.host == "publisher.example" { throw CancellationError() }
                return url.path == "/.well-known/actr" ? Data(authority.utf8) : body
            },
            inviteMembershipExchange: { profile, code, _, _ in
                XCTAssertEqual(profile.realmName, self.realmName)
                XCTAssertEqual(code, invite)
                exchanges += 1
                return #"{"member_id":"test-member","role":"viewer","key_id":"test-client"}"#
            },
            confirmRealmMembership: { _ in false },
            saveBrowserSession: { tabs, _ in
                savedAddresses.append(contentsOf: tabs.compactMap { $0.url?.absoluteString })
            }
        )
        defer { viewModel.invalidateRealmLoadsForWindowClose() }

        viewModel.navigateCurrentTab(to: linkURL.absoluteString)
        XCTAssertTrue(viewModel.inviteRedemptionInFlight)
        viewModel.navigateCurrentTab(to: linkURL.absoluteString)
        viewModel.openURLInNewTab(linkURL)

        let deadline = ContinuousClock.now + .seconds(2)
        while viewModel.currentTab?.url?.absoluteString != realmAddress && ContinuousClock.now < deadline {
            try await Task.sleep(for: .milliseconds(10))
        }
        XCTAssertEqual(exchanges, 1)
        XCTAssertFalse(viewModel.inviteRedemptionInFlight)
        XCTAssertNil(viewModel.pendingInviteRedeem)
        XCTAssertEqual(viewModel.tabs.count, 1)
        XCTAssertEqual(viewModel.currentTab?.url?.absoluteString, realmAddress)
        XCTAssertTrue(savedAddresses.contains(realmAddress))
        XCTAssertTrue(savedAddresses.allSatisfy { !$0.contains("args=") })
    }

    @MainActor
    func testCrossWindowMoveStopsTheSourceRealmBinding() async throws {
        let preferences = try makePreferences()
        let source = BrowserViewModel(realmPreferences: preferences, realmTrustDirectory: realmTrustDirectory, saveBrowserSession: { _, _ in })
        let destination = BrowserViewModel(realmPreferences: preferences, realmTrustDirectory: realmTrustDirectory, saveBrowserSession: { _, _ in })
        let sourceId = try XCTUnwrap(source.currentTabId)
        let link = try XCTUnwrap(URL(string: "actr://realm.example/echo"))
        source.tabs[0].url = link
        source.tabs[0].guestScript = "installed Front"
        let document = try profile(membership: "invite-only")
        let accepted = try accept(document, preferences: preferences)
        let binding = try RealmServiceBinding(
            profile: accepted.info,
            verifiedProfileBody: document.0,
            front: Data(),
            serviceType: ActrBindings.ActrType(publisher: "publisher.example", name: "echo", version: "1.0.0"),
            trustedFrontKey: Data(),
            capabilitiesJSON: #"{"hostModules":{"service":{"rpc":["uploadAttachment"]}}}"#,
            permissions: ["service.attachmentUpload"]
        )
        RealmServiceBindings.shared.attach(binding, to: sourceId)
        defer { RealmServiceBindings.shared.remove(sourceId) }
        let sourceWasBound = await RealmServiceBindings.shared.attachmentUploadAvailable(sourceId)
        XCTAssertTrue(sourceWasBound)

        let transfer = Tab(title: "Echo", url: link, guestScript: source.tabs[0].guestScript)
        destination.insertTabFromCrossWindow(transfer, at: destination.tabs.count, groupId: nil, select: false)
        source.removeTabById(sourceId)
        XCTAssertFalse(source.tabs.contains { $0.id == sourceId })
        XCTAssertNil(destination.tabs.last?.guestScript)
        let sourceStillBound = await RealmServiceBindings.shared.attachmentUploadAvailable(sourceId)
        XCTAssertFalse(sourceStillBound)

        let deadline = ContinuousClock.now + .seconds(2)
        while ContinuousClock.now < deadline {
            if !(await binding.attachmentUploadAvailable) { return }
            try await Task.sleep(for: .milliseconds(10))
        }
        XCTFail("The source Realm binding was not stopped")
    }

    @MainActor
    func testTransferredRealmGroupOpensEachTabOnSelection() async throws {
        let preferences = try makePreferences()
        let signer = Curve25519.Signing.PrivateKey()
        let descriptor = try authorityDescriptor(
            sequence: 1, signer: signer, signerId: "realm-key", keys: ["realm-key": signer]
        )
        let secondProfile = try profile(membership: "invite-only", signer: signer, realm: "echo-second@realm.example")
        let profiles = [
            "/realm/echo": try profile(membership: "invite-only", signer: signer).0,
            "/realm/echo-second": secondProfile.0
        ]
        var profileRequests: [String] = []
        let destination = BrowserViewModel(
            realmPreferences: preferences,
            realmTrustDirectory: realmTrustDirectory,
            fetchRealmDocument: { url, _ in
                if url.path == "/.well-known/actr" { return Data(descriptor.utf8) }
                profileRequests.append(url.path)
                return try XCTUnwrap(profiles[url.path])
            },
            confirmRealmMembership: { _ in false },
            saveBrowserSession: { _, _ in }
        )
        let first = Tab(title: "Echo", url: URL(string: "actr://realm.example/echo"), guestScript: "copied Front")
        let second = Tab(
            title: "Echo Second", url: URL(string: "actr://realm.example/echo-second"), guestScript: "copied Front"
        )
        for tab in [first, second] {
            destination.insertTabFromCrossWindow(tab, at: destination.tabs.count, groupId: nil, select: false)
        }
        destination.createGroupFromTabs([first.id, second.id], name: "Echo")
        XCTAssertTrue(destination.tabs.suffix(2).allSatisfy { $0.guestScript == nil })

        @MainActor func waitForInvite() async throws {
            let deadline = ContinuousClock.now + .seconds(2)
            while destination.pendingInviteRedeem == nil && ContinuousClock.now < deadline {
                try await Task.sleep(for: .milliseconds(10))
            }
        }
        destination.currentTabId = first.id
        try await waitForInvite()
        XCTAssertEqual(destination.pendingInviteRedeem?.tabId, first.id)
        XCTAssertEqual(profileRequests, ["/realm/echo"])

        destination.switchToTab(id: second.id)
        try await waitForInvite()
        XCTAssertEqual(destination.pendingInviteRedeem?.tabId, second.id)
        XCTAssertEqual(profileRequests, ["/realm/echo", "/realm/echo-second"])
        destination.cancelInviteRedeem(try XCTUnwrap(destination.pendingInviteRedeem))

        destination.switchToTab(id: first.id)
        try await waitForInvite()
        XCTAssertEqual(destination.pendingInviteRedeem?.tabId, first.id)
        XCTAssertEqual(profileRequests, ["/realm/echo", "/realm/echo-second", "/realm/echo"])
        destination.cancelInviteRedeem(try XCTUnwrap(destination.pendingInviteRedeem))
    }

    @MainActor
    func testRestoredBackgroundLocalFrontRequestsAccessOnSelection() async throws {
        let preferences = try makePreferences()
        preferences.set([realmName], forKey: "actr.redeemedRealms")
        let signer = Curve25519.Signing.PrivateKey()
        let descriptor = try authorityDescriptor(
            sequence: 1, signer: signer, signerId: "realm-key", keys: ["realm-key": signer]
        )
        let publisherRoot = Curve25519.Signing.PrivateKey()
        let frontKey = packageKey(Curve25519.Signing.PrivateKey().publicKey.rawRepresentation, status: "active")
        let keyId = try XCTUnwrap(frontKey["id"])
        let publisher = try publisherDescriptor(sequence: 1, root: publisherRoot, keys: [frontKey])
        let (body, _) = try profile(membership: "invite-only", signer: signer, localFrontKeyId: keyId)
        let documents = [
            "realm.example/.well-known/actr": Data(descriptor.utf8),
            "realm.example/realm/echo": body,
            "publisher.example/.well-known/actr": Data(publisher.utf8)
        ]

        var requests: [String] = []
        var prompts = 0
        let viewModel = BrowserViewModel(
            chooseLocalFrontFile: { panel in
                prompts += 1
                XCTAssertTrue(panel.message.contains("front.actr"))
                XCTAssertTrue(panel.message.contains(keyId))
                return nil
            },
            realmPreferences: preferences,
            realmTrustDirectory: realmTrustDirectory,
            fetchRealmDocument: { url, _ in
                let key = (url.host ?? "") + url.path
                requests.append(key)
                return try XCTUnwrap(documents[key])
            },
            confirmRealmMembership: { _ in true },
            saveBrowserSession: { _, _ in }
        )
        let webAddress = "https://web.example/"
        let realmAddress = "actr://realm.example/echo"
        viewModel.restoreSessionIfNeeded {
            BrowserDataRepository.RestoredSession(tabs: [
                .init(id: UUID(), title: "Web", address: webAddress),
                .init(id: UUID(), title: "Echo", address: realmAddress)
            ], activeTabIndex: 0)
        }
        defer { viewModel.invalidateRealmLoadsForWindowClose() }
        XCTAssertEqual(viewModel.currentTab?.url?.absoluteString, webAddress)
        let frontTab = try XCTUnwrap(viewModel.tabs.first { $0.url?.absoluteString == realmAddress })
        await Task.yield()
        XCTAssertTrue(requests.isEmpty)
        XCTAssertEqual(prompts, 0)

        viewModel.switchToTab(id: frontTab.id)
        let deadline = ContinuousClock.now + .seconds(2)
        while prompts == 0 && ContinuousClock.now < deadline {
            try await Task.sleep(for: .milliseconds(10))
        }
        XCTAssertEqual(requests, [
            "realm.example/.well-known/actr", "realm.example/realm/echo", "publisher.example/.well-known/actr"
        ])
        XCTAssertEqual(prompts, 1)
        XCTAssertNil(viewModel.pendingInviteRedeem)
        XCTAssertNil(viewModel.currentTab?.guestExtractedPath)
    }

    @MainActor
    func testOnlyAcceptedRetirementClearsRunningFronts() async throws {
        let preferences = try makePreferences()
        let signer = Curve25519.Signing.PrivateKey()
        let active = try profile(membership: "open", sequence: 2, signer: signer)
        _ = try accept(active, preferences: preferences)
        let windows = (0..<2).map { _ in
            BrowserViewModel(realmPreferences: preferences, realmTrustDirectory: realmTrustDirectory, saveBrowserSession: { _, _ in })
        }
        for window in windows {
            window.tabs[0].url = URL(string: "actr://realm.example/echo")
            window.tabs[0].appId = "echo-front"
            window.tabs[0].guestScript = "active guest"
        }
        let tabId = try XCTUnwrap(windows[0].currentTabId)
        let coordinatorManager = ViewBridgeCoordinatorManager.shared
        let coordinator = coordinatorManager.coordinator(for: tabId)
        defer { coordinatorManager.removeCoordinator(for: tabId) }
        let pool = RendererProcessPool.shared
        let renderer = try pool.allocateRenderer(for: tabId, fingerprint: "system.realm-admission")
        coordinator.tabId = tabId
        coordinator.allocatedServiceId = renderer

        let stale = try profile(membership: "open", status: "retired", signer: signer)
        XCTAssertThrowsError(try accept(stale, preferences: preferences))
        XCTAssertTrue(windows.allSatisfy { $0.currentTab?.guestScript == "active guest" })
        XCTAssertEqual(pool.rendererIdentifier(for: tabId), renderer)

        let retired = try profile(membership: "open", sequence: 3, status: "retired", signer: signer)
        XCTAssertThrowsError(try accept(retired, preferences: preferences)) { error in
            guard case CoreError.RealmRetired(let name) = error else {
                return XCTFail("Expected accepted retirement")
            }
            XCTAssertEqual(name, self.realmName)
        }
        XCTAssertTrue(windows.allSatisfy { $0.currentTab?.appId == nil && $0.currentTab?.guestScript == nil })
        XCTAssertEqual(windows[0].currentTabId, tabId)
        XCTAssertNil(pool.rendererIdentifier(for: tabId))
        XCTAssertNil(coordinator.allocatedServiceId)
        XCTAssertFalse(coordinatorManager.coordinator(for: tabId) === coordinator)
        let reopened = try profile(membership: "open", sequence: 4, signer: signer)
        XCTAssertThrowsError(try accept(reopened, preferences: preferences))
    }

    @MainActor
    func testPublisherRotationKeepsDigestPinnedFrontsAndRejectsRollback() throws {
        let preferences = try makePreferences()
        let root = Curve25519.Signing.PrivateKey()
        let firstKey = Curve25519.Signing.PrivateKey().publicKey.rawRepresentation
        let nextKey = Curve25519.Signing.PrivateKey().publicKey.rawRepresentation
        let previous = packageKey(firstKey, status: "active")
        let next = packageKey(nextKey, status: "active")
        let first = try publisherDescriptor(sequence: 1, root: root, keys: [previous])
        let pinned = LocalRealmFront(signingKeyId: try XCTUnwrap(previous["id"]), allowRebuild: false)
        let rebuildable = LocalRealmFront(signingKeyId: pinned.signingKeyId, allowRebuild: true)

        func acceptPublisher(_ body: String) throws -> PublisherProfileInfo {
            try RealmProfileStore.acceptPublisher(body: body, domain: "publisher.example", preferences: preferences)
        }

        XCTAssertEqual(try acceptPublisher(first).verificationKey(for: rebuildable), firstKey)
        let rotated = try publisherDescriptor(
            sequence: 2, root: root, keys: [packageKey(firstKey, status: "retired"), next]
        )
        let accepted = try acceptPublisher(rotated)
        XCTAssertEqual(accepted.profileSequence, 2)
        XCTAssertEqual(accepted.verificationKey(for: pinned), firstKey)
        XCTAssertNil(accepted.verificationKey(for: rebuildable))
        XCTAssertThrowsError(try acceptPublisher(first))

        let removed = try publisherDescriptor(sequence: 3, root: root, keys: [next])
        let latest = try acceptPublisher(removed)
        XCTAssertNil(latest.verificationKey(for: pinned))
        XCTAssertNil(latest.verificationKey(for: rebuildable))
    }

    @MainActor
    func testInFlightFrontInstallUsesLatestPublisherTrust() async throws {
        let preferences = try makePreferences()
        let domain = "publisher.example"
        let root = Curve25519.Signing.PrivateKey()
        let publicKey = Curve25519.Signing.PrivateKey().publicKey.rawRepresentation
        let replacement = packageKey(Curve25519.Signing.PrivateKey().publicKey.rawRepresentation, status: "active")
        let initial = try publisherDescriptor(sequence: 1, root: root, keys: [packageKey(publicKey, status: "active")])
        let snapshot = try RealmProfileStore.acceptPublisher(body: initial, domain: domain, preferences: preferences)
        let keyId = try XCTUnwrap(snapshot.packageSigningKeys.first?.id)
        let rebuildable = LocalRealmFront(signingKeyId: keyId, allowRebuild: true)
        let pinned = LocalRealmFront(signingKeyId: keyId, allowRebuild: false)
        let verifiedKey = try XCTUnwrap(snapshot.verificationKey(for: rebuildable))
        let (started, signalStarted) = AsyncStream<Void>.makeStream()
        let (resume, release) = AsyncStream<Void>.makeStream()
        defer { release.finish() }
        let install = Task { @MainActor in
            signalStarted.yield(())
            signalStarted.finish()
            for await _ in resume { break }
            try RealmProfileStore.requirePublisherKey(
                verifiedKey, domain: domain, localFront: rebuildable, preferences: preferences
            )
        }
        for await _ in started { break }
        let retired = try publisherDescriptor(
            sequence: 2, root: root, keys: [packageKey(publicKey, status: "retired"), replacement]
        )
        _ = try RealmProfileStore.acceptPublisher(body: retired, domain: domain, preferences: preferences)
        release.yield(())
        release.finish()
        guard case .failure = await install.result else {
            return XCTFail("A pending rebuild used a retired key")
        }
        XCTAssertNoThrow(try RealmProfileStore.requirePublisherKey(
            verifiedKey, domain: domain, localFront: pinned, preferences: preferences
        ))

        let removed = try publisherDescriptor(sequence: 3, root: root, keys: [replacement])
        _ = try RealmProfileStore.acceptPublisher(body: removed, domain: domain, preferences: preferences)
        XCTAssertThrowsError(try RealmProfileStore.requirePublisherKey(
            verifiedKey, domain: domain, localFront: pinned, preferences: preferences
        ))
    }

    @MainActor
    private func accept(_ document: (Data, String), preferences: UserDefaults) throws -> RealmProfileStore.AcceptedRealm {
        try RealmProfileStore.acceptRealm(
            body: String(decoding: document.0, as: UTF8.self), realmName: realmName,
            signingKeysJSON: document.1, preferences: preferences
        )
    }

    private func authorityDescriptor(
        sequence: Int, signer: Curve25519.Signing.PrivateKey, signerId: String,
        keys: [String: Curve25519.Signing.PrivateKey], extensionValue: String? = nil
    ) throws -> String {
        var member: [String: Any] = [
            "sequence": sequence,
            "keys": keys.sorted { $0.key < $1.key }.map { id, key in
                [
                    "id": id, "algorithm": "Ed25519", "status": "active",
                    "public_key": key.publicKey.rawRepresentation.base64EncodedString()
                ]
            }
        ]
        member["x-extension"] = extensionValue
        let signedKeys = try sign(member, with: signer, keyId: signerId)
        let descriptor = try JSONSerialization.data(withJSONObject: [
            "version": 1, "authority": "actr://realm.example",
            "services": [[
                "rel": "realms", "href": "https://realm.example/realm", "protocol": "actr-realm-profile/1"
            ]],
            "realm_signing_keys": try JSONSerialization.jsonObject(with: signedKeys)
        ])
        return String(decoding: descriptor, as: UTF8.self)
    }

    private func makePreferences() throws -> UserDefaults {
        let suite = "ActriumTests.RealmAdmission.\(UUID())"
        let preferences = try XCTUnwrap(UserDefaults(suiteName: suite))
        addTeardownBlock { UserDefaults(suiteName: suite)?.removePersistentDomain(forName: suite) }
        return preferences
    }

    private func inviteCode() throws -> String {
        func base64URL(_ bytes: Data) -> String {
            bytes.base64EncodedString()
                .replacingOccurrences(of: "+", with: "-")
                .replacingOccurrences(of: "/", with: "_")
                .replacingOccurrences(of: "=", with: "")
        }
        let invite = try JSONSerialization.data(withJSONObject: [
            "schema": "actr.realm-invite/v1", "realm": realmName,
            "secret": base64URL(Data(repeating: 1, count: 32))
        ])
        return "ri_" + base64URL(invite)
    }

    private func packageKey(_ publicKey: Data, status: String) -> [String: String] {
        let digest = SHA256.hash(data: publicKey).prefix(8).map { String(format: "%02x", $0) }.joined()
        return [
            "id": "actr-pub-" + digest, "status": status, "algorithm": "Ed25519",
            "public_key": publicKey.base64EncodedString()
        ]
    }

    private func publisherDescriptor(
        sequence: Int, root: Curve25519.Signing.PrivateKey, keys: [[String: String]]
    ) throws -> String {
        let now = Date()
        let formatter = ISO8601DateFormatter()
        let profile = try sign([
            "schema": "actr.publisher/v1",
            "domain": "publisher.example",
            "sequence": sequence,
            "profile_signing_keys": [[
                "id": "realm-key", "algorithm": "Ed25519", "status": "active",
                "public_key": root.publicKey.rawRepresentation.base64EncodedString()
            ]],
            "keys": keys,
            "profile_issued_at": formatter.string(from: now.addingTimeInterval(-60)),
            "profile_expires_at": formatter.string(from: now.addingTimeInterval(3600))
        ], with: root)
        let descriptor = try JSONSerialization.data(withJSONObject: [
            "version": 1, "authority": "actr://publisher.example",
            "services": [["rel": "contracts", "href": "https://publisher.example/contracts"]],
            "publisher": try JSONSerialization.jsonObject(with: profile)
        ])
        return String(decoding: descriptor, as: UTF8.self)
    }

    private func profile(
        membership: String, sequence: Int = 1, status: String = "active",
        signer: Curve25519.Signing.PrivateKey = Curve25519.Signing.PrivateKey(),
        keyId: String = "realm-key", realm: String? = nil, localFrontKeyId: String? = nil,
        frontName: String = "echo-front", frontDigest: String = "b"
    ) throws -> (Data, String) {
        let publicKey = signer.publicKey.rawRepresentation.base64EncodedString()
        let keys = try sign([
            "sequence": 1,
            "keys": [["id": keyId, "algorithm": "Ed25519", "public_key": publicKey, "status": "active"]]
        ], with: signer, keyId: keyId)
        var front: [String: Any] = archive(frontName, digest: frontDigest)
        if let localFrontKeyId {
            front["href"] = "front.actr"
            front["sha256"] = nil
            front["dependencies"] = ["service": "service"]
            front["local_path"] = ["signing_key_id": localFrontKeyId, "allow_rebuild": true] as [String: Any]
        }
        let body = try sign([
            "schema": "actr.realm/v1",
            "sequence": sequence,
            "realm": ["name": realm ?? realmName, "display_name": "Echo", "status": status],
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
        ], with: signer, keyId: keyId)
        return (body, String(decoding: keys, as: UTF8.self))
    }

    private func archive(_ name: String, digest: String) -> [String: String] {
        [
            "href": "https://publisher.example/\(name).actr",
            "type": "publisher.example/\(name)@1.0.0",
            "sha256": String(repeating: digest, count: 64)
        ]
    }

    private func sign(
        _ payload: [String: Any], with key: Curve25519.Signing.PrivateKey, keyId: String = "realm-key"
    ) throws -> Data {
        let body = try JSONSerialization.data(withJSONObject: payload, options: [.sortedKeys, .withoutEscapingSlashes])
        let signature = try key.signature(for: body)
        var document = payload
        document["signature"] = [
            "key_id": keyId, "algorithm": "Ed25519", "value": signature.base64EncodedString()
        ]
        return try JSONSerialization.data(withJSONObject: document)
    }
}
