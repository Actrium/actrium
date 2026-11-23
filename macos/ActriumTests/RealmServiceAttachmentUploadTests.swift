import ActrBindings
import ActriumCore
import ActriumIPC
import Foundation
import Testing

@testable import Actrium

struct RealmServiceAttachmentUploadTests {
    private let md5 = "900150983cd24fb0d6963f7d28e17f72"

    private func makePrepareCall(
        attachmentId: String = "attachment-1",
        filename: String = "document.pdf",
        type: Int32 = 2,
        streamId: String = "chunk-stream",
        checksum: String? = nil,
        checksumAlgorithm: String = "md5",
        sizeBytes: UInt64 = 3,
        resultStreamId: String = "result-stream"
    ) -> RealmServiceAttachmentPrepareCall {
        RealmServiceAttachmentPrepareCall(
            attachmentId: attachmentId,
            filename: filename,
            type: type,
            streamId: streamId,
            checksum: checksum ?? md5,
            checksumAlgorithm: checksumAlgorithm,
            sizeBytes: sizeBytes,
            resultStreamId: resultStreamId
        )
    }

    // MARK: - Capability policy

    @Test func requiresBothThePermissionAndTheDeclaredTransport() {
        let declared: Set<String> = ["prompt", "uploadAttachment"]
        #expect(RealmServiceAttachmentUploadPolicy.isPermitted(
            permissions: ["service.attachmentUpload"], declaredRoutes: declared))
        #expect(RealmServiceAttachmentUploadPolicy.isPermitted(
            permissions: ["host:service.uploadAttachment"], declaredRoutes: declared))
        // Module shorthands cover pre-existing methods, not this opt-in.
        #expect(!RealmServiceAttachmentUploadPolicy.isPermitted(
            permissions: ["service"], declaredRoutes: declared))
        #expect(!RealmServiceAttachmentUploadPolicy.isPermitted(
            permissions: ["host:service"], declaredRoutes: declared))
        #expect(!RealmServiceAttachmentUploadPolicy.isPermitted(
            permissions: [], declaredRoutes: declared))
        #expect(!RealmServiceAttachmentUploadPolicy.isPermitted(
            permissions: ["service.call"], declaredRoutes: declared))
        #expect(!RealmServiceAttachmentUploadPolicy.isPermitted(
            permissions: ["untrusted.service.attachmentUploadExtra"], declaredRoutes: declared))
        #expect(!RealmServiceAttachmentUploadPolicy.isPermitted(
            permissions: ["service.attachmentUpload"], declaredRoutes: ["prompt"]))
    }

    @Test func businessRoutesExcludeTheUploadAttachmentTransport() {
        #expect(RealmServiceAttachmentUploadPolicy.businessRoutes(["prompt", "uploadAttachment"]) == ["prompt"])
        #expect(RealmServiceAttachmentUploadPolicy.businessRoutes(["prompt"]) == ["prompt"])
    }

    // MARK: - Prepare validation

    @Test func validPreparePassesAndNormalizesChecksum() throws {
        let call = makePrepareCall(checksum: md5.uppercased())
        let normalized = try RealmServiceAttachmentUploadPolicy.validatePrepare(call)
        #expect(normalized == md5)
    }

    @Test func prepareRejectsInvalidFields() {
        let cases: [RealmServiceAttachmentPrepareCall] = [
            makePrepareCall(attachmentId: ""),
            makePrepareCall(attachmentId: String(repeating: "a", count: 129)),
            makePrepareCall(filename: ""),
            makePrepareCall(filename: String(repeating: "f", count: 257)),
            makePrepareCall(type: 0),
            makePrepareCall(type: 5),
            makePrepareCall(streamId: ""),
            makePrepareCall(resultStreamId: ""),
            makePrepareCall(streamId: "same", resultStreamId: "same"),
            makePrepareCall(checksum: String(md5.dropLast())),
            makePrepareCall(checksum: String(repeating: "z", count: 32)),
            makePrepareCall(checksumAlgorithm: "sha256"),
            makePrepareCall(sizeBytes: UInt64(RealmServiceAttachmentIPC.maxBytes) + 1),
        ]
        for call in cases {
            #expect(throws: (any Error).self) {
                try RealmServiceAttachmentUploadPolicy.validatePrepare(call)
            }
        }
        #expect(RealmServiceAttachmentIPC.maxBytes == 7 * 1024 * 1024)
    }

    // MARK: - Base64

    @Test func strictBase64AcceptsPaddedAndEmptyOnly() {
        #expect(RealmServiceAttachmentUploadPolicy.strictBase64Data("") == Data())
        #expect(RealmServiceAttachmentUploadPolicy.strictBase64Data("AQID") == Data([1, 2, 3]))
        #expect(RealmServiceAttachmentUploadPolicy.strictBase64Data("AQI=") == Data([1, 2]))
        #expect(RealmServiceAttachmentUploadPolicy.strictBase64Data("AQ==") == Data([1]))
        #expect(RealmServiceAttachmentUploadPolicy.strictBase64Data("AQIDBA") == nil)      // unpadded
        #expect(RealmServiceAttachmentUploadPolicy.strictBase64Data("AQI") == nil)         // unpadded
        #expect(RealmServiceAttachmentUploadPolicy.strictBase64Data("A=Q=") == nil)        // padding mid-string
        #expect(RealmServiceAttachmentUploadPolicy.strictBase64Data("====") == nil)        // too much padding
        #expect(RealmServiceAttachmentUploadPolicy.strictBase64Data("AQI=!") == nil)       // invalid character
        #expect(RealmServiceAttachmentUploadPolicy.strictBase64Data("AQID ") == nil)       // whitespace
    }

    // MARK: - Session store

    private let fixedTarget = ActrBindings.ActrId(realm: "realm@example.test", serialNumber: 1)

    private func makeStoreWithUploadingSession(
        sizeBytes: UInt64 = 3,
        attachmentId: String = "attachment-1"
    ) async throws -> (RealmServiceAttachmentSessionStore, RealmServiceAttachmentPrepareCall) {
        let store = RealmServiceAttachmentSessionStore()
        let call = makePrepareCall(attachmentId: attachmentId, sizeBytes: sizeBytes)
        let checksum = try RealmServiceAttachmentUploadPolicy.validatePrepare(call)
        try await store.registerPreparing(call, checksum: checksum)
        try await store.markUploading(
            attachmentId: attachmentId, streamId: call.streamId, target: fixedTarget
        )
        return (store, call)
    }

    @Test func prepareAcceptsTheMaximumAttachmentId() throws {
        let call = makePrepareCall(attachmentId: String(repeating: "a", count: 128))
        #expect(try RealmServiceAttachmentUploadPolicy.validatePrepare(call) == md5)
    }

    @Test func storeRejectsConflictingStreamsAndAttachmentIds() async throws {
        let store = RealmServiceAttachmentSessionStore()
        let first = makePrepareCall()
        try await store.registerPreparing(first, checksum: md5)
        await #expect(throws: (any Error).self) {
            try await store.registerPreparing(first, checksum: md5)
        }
        await #expect(throws: (any Error).self) {
            try await store.registerPreparing(
                makePrepareCall(attachmentId: "attachment-2", streamId: first.streamId),
                checksum: md5
            )
        }
        await #expect(throws: (any Error).self) {
            try await store.registerPreparing(
                makePrepareCall(attachmentId: "attachment-3", resultStreamId: first.resultStreamId),
                checksum: md5
            )
        }
    }

    @Test func chunkUsesTheFixedPrepareTarget() async throws {
        let (store, call) = try await makeStoreWithUploadingSession()
        let fixed = ActrBindings.ActrId(realm: "fixed@example.test", serialNumber: 2)
        try await store.markUploading(
            attachmentId: call.attachmentId, streamId: call.streamId, target: fixed
        )
        let planned = try await store.planChunk(
            streamId: call.streamId, sequence: 1, payloadBytes: 1, metadata: []
        )
        #expect(planned.plan.target == fixed)
        #expect(planned.plan.sequence == 1)
        #expect(!planned.plan.isEndOfStream)
    }

    @Test func aStaleAttemptCannotCancelOrRetargetARetry() async throws {
        let store = RealmServiceAttachmentSessionStore()
        let first = makePrepareCall(attachmentId: "same-id")
        let firstAttempt = try await store.registerPreparing(first, checksum: md5)
        // Cleanup scoped to another attempt is a no-op.
        let unrelated = await store.cancel(
            attachmentId: "same-id",
            streamId: "another-stream",
            attemptId: firstAttempt
        )
        #expect(unrelated == nil)
        #expect(await store.isCurrent(attachmentId: "same-id", streamId: first.streamId))
        let removed = await store.cancel(
            attachmentId: "same-id",
            streamId: first.streamId,
            attemptId: firstAttempt
        )
        #expect(removed?.chunkStreamId == first.streamId)
        if let removed {
            await store.completeTeardown(removed)
        }

        let retry = makePrepareCall(
            attachmentId: "same-id", streamId: "chunk-2", resultStreamId: "result-2"
        )
        try await store.registerPreparing(retry, checksum: md5)
        #expect(await store.isCurrent(attachmentId: "same-id", streamId: retry.streamId))
        let firstStillCurrent = await store.isCurrent(
            attachmentId: "same-id", streamId: first.streamId
        )
        #expect(!firstStillCurrent)

        // A late reply from the abandoned attempt must not move the retry
        // into uploading or clean up its streams.
        await #expect(throws: (any Error).self) {
            try await store.markUploading(
                attachmentId: "same-id", streamId: first.streamId, target: fixedTarget
            )
        }
        let stillCurrent = await store.isCurrent(
            attachmentId: "same-id", streamId: first.streamId
        )
        #expect(!stillCurrent)
        try await store.markUploading(
            attachmentId: "same-id", streamId: retry.streamId, target: fixedTarget
        )
        #expect(await store.isCurrent(attachmentId: "same-id", streamId: retry.streamId))
    }

    @Test func chunksRequireContiguousOneBasedSequences() async throws {
        let (store, call) = try await makeStoreWithUploadingSession(sizeBytes: 10)
        await #expect(throws: (any Error).self) {
            try await store.planChunk(streamId: call.streamId, sequence: 0, payloadBytes: 1, metadata: [])
        }
        _ = try await store.planChunk(streamId: call.streamId, sequence: 1, payloadBytes: 1, metadata: [])
        await #expect(throws: (any Error).self) {
            // Duplicate sequence.
            try await store.planChunk(streamId: call.streamId, sequence: 1, payloadBytes: 1, metadata: [])
        }
        await #expect(throws: (any Error).self) {
            // Skipped sequence.
            try await store.planChunk(streamId: call.streamId, sequence: 3, payloadBytes: 1, metadata: [])
        }
        let planned = try await store.planChunk(streamId: call.streamId, sequence: 2, payloadBytes: 1, metadata: [])
        #expect(planned.plan.sequence == 2)
    }

    @Test func chunkPayloadBoundaryIsInclusiveAt48KiB() async throws {
        let (store, call) = try await makeStoreWithUploadingSession(
            sizeBytes: UInt64(RealmServiceAttachmentIPC.chunkSizeBytes) + 1
        )
        let boundary = try await store.planChunk(
            streamId: call.streamId,
            sequence: 1,
            payloadBytes: RealmServiceAttachmentIPC.chunkSizeBytes,
            metadata: []
        )
        #expect(boundary.plan.sequence == 1)
        await #expect(throws: (any Error).self) {
            try await store.planChunk(
                streamId: call.streamId,
                sequence: 2,
                payloadBytes: RealmServiceAttachmentIPC.chunkSizeBytes + 1,
                metadata: []
            )
        }
    }

    @Test func finalDataChunkCarriesEosAndMustReachDeclaredSize() async throws {
        let (store, call) = try await makeStoreWithUploadingSession(sizeBytes: 4)
        let eos = [RealmServiceAttachmentUploadPolicy.endOfStreamMetadata]

        // EOS metadata before the declared size is reached.
        await #expect(throws: (any Error).self) {
            try await store.planChunk(streamId: call.streamId, sequence: 1, payloadBytes: 2, metadata: eos)
        }
        // Overshooting the declared size is rejected even without EOS.
        await #expect(throws: (any Error).self) {
            try await store.planChunk(streamId: call.streamId, sequence: 1, payloadBytes: 5, metadata: [])
        }
        // Landing exactly on the size without EOS metadata is rejected: the
        // final chunk itself must carry the marker.
        await #expect(throws: (any Error).self) {
            try await store.planChunk(streamId: call.streamId, sequence: 1, payloadBytes: 4, metadata: [])
        }
        // Metadata is only legal as the single eos=true entry.
        await #expect(throws: (any Error).self) {
            try await store.planChunk(
                streamId: call.streamId,
                sequence: 1,
                payloadBytes: 2,
                metadata: [RealmServiceAttachmentMetadata(key: "aos", value: "true")]
            )
        }
        _ = try await store.planChunk(streamId: call.streamId, sequence: 1, payloadBytes: 2, metadata: [])
        let final = try await store.planChunk(streamId: call.streamId, sequence: 2, payloadBytes: 2, metadata: eos)
        #expect(final.plan.isEndOfStream)
        // No writes after EOS.
        await #expect(throws: (any Error).self) {
            try await store.planChunk(streamId: call.streamId, sequence: 3, payloadBytes: 1, metadata: [])
        }
    }

    @Test func emptyFileIsOneEmptyEosChunk() async throws {
        let (store, call) = try await makeStoreWithUploadingSession(sizeBytes: 0)
        let eos = [RealmServiceAttachmentUploadPolicy.endOfStreamMetadata]
        let planned = try await store.planChunk(streamId: call.streamId, sequence: 1, payloadBytes: 0, metadata: eos)
        #expect(planned.plan.isEndOfStream)
        // A data-only empty chunk does not terminate the stream.
        let (store2, call2) = try await makeStoreWithUploadingSession(sizeBytes: 0, attachmentId: "attachment-2")
        await #expect(throws: (any Error).self) {
            try await store2.planChunk(streamId: call2.streamId, sequence: 1, payloadBytes: 0, metadata: [])
        }
    }

    @Test func chunksRejectUnknownAndResultStreams() async throws {
        let (store, call) = try await makeStoreWithUploadingSession()
        await #expect(throws: (any Error).self) {
            try await store.planChunk(streamId: "nope", sequence: 1, payloadBytes: 1, metadata: [])
        }
        await #expect(throws: (any Error).self) {
            try await store.planChunk(streamId: call.resultStreamId, sequence: 1, payloadBytes: 1, metadata: [])
        }
    }

    @Test func chunksRejectWhileStillPreparing() async throws {
        let store = RealmServiceAttachmentSessionStore()
        let call = makePrepareCall()
        try await store.registerPreparing(call, checksum: md5)
        await #expect(throws: (any Error).self) {
            try await store.planChunk(streamId: call.streamId, sequence: 1, payloadBytes: 1, metadata: [])
        }
    }

    @Test func unregisterReservesTheWholeSessionUntilTeardownCompletes() async throws {
        let (store, call) = try await makeStoreWithUploadingSession()
        let byChunk = await store.unregister(streamId: call.streamId)
        #expect(byChunk?.attachmentId == call.attachmentId)
        // Repeated cleanup cannot claim the same attempt twice.
        let again = await store.unregister(streamId: call.resultStreamId)
        #expect(again == nil)
        await #expect(throws: (any Error).self) {
            try await store.planChunk(streamId: call.streamId, sequence: 1, payloadBytes: 1, metadata: [])
        }
        if let byChunk {
            await store.completeTeardown(byChunk)
        }
        #expect(await store.session(forResultStreamId: call.resultStreamId) == nil)

        let (store2, call2) = try await makeStoreWithUploadingSession(attachmentId: "attachment-2")
        let byResult = await store2.unregister(streamId: call2.resultStreamId)
        #expect(byResult?.chunkStreamId == call2.streamId)
        if let byResult {
            await store2.completeTeardown(byResult)
        }
        #expect(await store2.session(forResultStreamId: call2.resultStreamId) == nil)
    }

    @Test func finishAndCancelAllDropSessionsAfterCleanup() async throws {
        let (store, call) = try await makeStoreWithUploadingSession()
        #expect(await store.session(forResultStreamId: call.resultStreamId)?.attachmentId == call.attachmentId)
        let finished = await store.finish(attachmentId: call.attachmentId)
        #expect(finished?.resultStreamId == call.resultStreamId)
        #expect(await store.finish(attachmentId: call.attachmentId) == nil)
        if let finished {
            await store.completeTeardown(finished)
        }

        let (store2, call2) = try await makeStoreWithUploadingSession(attachmentId: "attachment-2")
        let (store3, call3) = try await makeStoreWithUploadingSession(attachmentId: "attachment-3")
        // cancelAll spans independent store instances' sessions only within itself.
        let all = await store2.cancelAll()
        #expect(all.map(\.attachmentId) == [call2.attachmentId])
        for session in all {
            await store2.completeTeardown(session)
        }
        #expect(await store3.session(forResultStreamId: call3.resultStreamId)?.attachmentId == call3.attachmentId)
    }

    @Test func lateCallbackCannotClaimAReplacementWithTheSameResultStream() async throws {
        let store = RealmServiceAttachmentSessionStore()
        let call = makePrepareCall()
        let firstAttempt = try await store.registerPreparing(call, checksum: md5)
        if let first = await store.unregister(streamId: call.streamId) {
            await store.completeTeardown(first)
        }
        let secondAttempt = try await store.registerPreparing(call, checksum: md5)
        #expect(firstAttempt != secondAttempt)

        #expect(await store.claimResult(streamId: call.resultStreamId, attemptId: firstAttempt) == nil)
        #expect(await store.session(forResultStreamId: call.resultStreamId)?.attemptId == secondAttempt)
        let claimed = await store.claimResult(streamId: call.resultStreamId, attemptId: secondAttempt)
        #expect(claimed?.attemptId == secondAttempt)
        #expect(await store.session(forResultStreamId: call.resultStreamId)?.attemptId == secondAttempt)
        if let claimed {
            await store.completeTeardown(claimed)
        }
        #expect(await store.session(forResultStreamId: call.resultStreamId) == nil)
    }

    @Test func teardownHoldsReusableIDsUntilTransportCleanupCompletes() async throws {
        let store = RealmServiceAttachmentSessionStore()
        let call = makePrepareCall()
        let firstAttempt = try await store.registerPreparing(call, checksum: md5)
        let first = await store.claimResult(streamId: call.resultStreamId, attemptId: firstAttempt)
        #expect(first?.attemptId == firstAttempt)

        // The transport unregister is still in flight. The old attempt keeps
        // both public IDs reserved, so no replacement can be unregistered by
        // that old cleanup.
        let replacement = makePrepareCall(attachmentId: "replacement")
        await #expect(throws: (any Error).self) {
            try await store.registerPreparing(replacement, checksum: md5)
        }
        #expect(await store.claimResult(streamId: call.resultStreamId, attemptId: firstAttempt) == nil)
        if let first {
            await store.completeTeardown(first)
        }

        let secondAttempt = try await store.registerPreparing(replacement, checksum: md5)
        #expect(secondAttempt != firstAttempt)
        // Even a duplicated completion from the old owner is attempt-scoped.
        if let first {
            await store.completeTeardown(first)
        }
        #expect(await store.session(forResultStreamId: call.resultStreamId)?.attemptId == secondAttempt)
        let second = await store.claimResult(streamId: call.resultStreamId, attemptId: secondAttempt)
        if let second {
            await store.completeTeardown(second)
        }
        #expect(await store.session(forResultStreamId: call.resultStreamId) == nil)
    }

    @Test func lateChunkFailureCannotCancelAReregisteredAttempt() async throws {
        let store = RealmServiceAttachmentSessionStore()
        let call = makePrepareCall()
        let firstAttempt = try await store.registerPreparing(call, checksum: md5)
        try await store.markUploading(
            attachmentId: call.attachmentId,
            streamId: call.streamId,
            target: fixedTarget
        )
        let sending = try await store.planChunk(
            streamId: call.streamId,
            sequence: 1,
            payloadBytes: 1,
            metadata: []
        )
        #expect(sending.session.attemptId == firstAttempt)

        // The original attempt finishes teardown and a replacement reuses
        // every public ID while its chunk send is still suspended.
        if let original = await store.unregister(streamId: call.streamId) {
            await store.completeTeardown(original)
        }
        let secondAttempt = try await store.registerPreparing(call, checksum: md5)
        #expect(secondAttempt != firstAttempt)

        let lateCleanup = await store.cancel(
            attachmentId: sending.session.attachmentId,
            streamId: sending.session.chunkStreamId,
            attemptId: sending.session.attemptId
        )
        #expect(lateCleanup == nil)
        #expect(await store.session(forResultStreamId: call.resultStreamId)?.attemptId == secondAttempt)
        #expect(await store.isCurrent(attachmentId: call.attachmentId, streamId: call.streamId))

        let currentCleanup = await store.cancel(
            attachmentId: call.attachmentId,
            streamId: call.streamId,
            attemptId: secondAttempt
        )
        #expect(currentCleanup?.attemptId == secondAttempt)
        if let currentCleanup {
            await store.completeTeardown(currentCleanup)
        }
    }

    @Test func preparingCancellationWaitsForThePrepareOwnerToFinishCleanup() async throws {
        let store = RealmServiceAttachmentSessionStore()
        let call = makePrepareCall()
        let firstAttempt = try await store.registerPreparing(call, checksum: md5)
        let cancelled = await store.unregister(streamId: call.streamId)
        #expect(cancelled?.attemptId == firstAttempt)
        let stillCurrent = await store.isCurrent(
            attachmentId: call.attachmentId,
            streamId: call.streamId
        )
        #expect(!stillCurrent)
        await #expect(throws: (any Error).self) {
            try await store.registerPreparing(call, checksum: md5)
        }

        // The renderer-facing cancellation cannot finish transport cleanup
        // before a suspended registerStream call returns. The prepare owner
        // completes that cleanup and releases the IDs exactly once.
        let owned = await store.preparationCleanup(
            attachmentId: call.attachmentId,
            streamId: call.streamId
        )
        #expect(owned?.attemptId == firstAttempt)
        if let owned {
            await store.completeTeardown(owned)
        }
        let secondAttempt = try await store.registerPreparing(call, checksum: md5)
        #expect(secondAttempt != firstAttempt)
    }

    // MARK: - Binding wiring

    private func makeBinding(
        capabilitiesJSON: String,
        permissions: [String]
    ) async throws -> RealmServiceBinding {
        let profile = RealmProfileInfo(
            realmName: "realm@example.test",
            displayName: "Realm",
            status: "active",
            sequence: 1,
            primaryPackage: "pkg",
            frontPackage: "front",
            packages: [],
            packageHref: "https://example.test/pkg.actr",
            packageSha256: String(repeating: "0", count: 64),
            packageType: "pub/front@1.0.0",
            serviceHref: "https://example.test/svc.actr",
            serviceSha256: String(repeating: "1", count: 64),
            serviceType: "pub/svc@1.0.0",
            issuerUrl: "https://issuer.example.test",
            providerId: "provider-1"
        )
        let profileBody = Data(#"{"provider":{"signaling":[{"url":"wss://signaling.example.test/ws","priority":1}],"ice":[{"urls":["stun:stun.example.test"]}]}}"#.utf8)
        // The binding derives its routes from the verified Front session
        // metadata, so the fixture builds that metadata from the same
        // capability manifest the old initializer decoded.
        let object = try JSONSerialization.jsonObject(with: Data(capabilitiesJSON.utf8)) as? [String: Any]
        let hostModules = object?["hostModules"] as? [String: Any]
        let module = (hostModules?["service"] ?? hostModules?["host:service"]) as? [String: Any]
        let declaredRoutes = module?["rpc"] as? [String] ?? []
        let metadata = ActriumSessionMetadata(
            program: ProgramRelease(publisher: "pub", name: "svc", version: "1.0.0"),
            packageDigest: Data(repeating: 1, count: 32),
            guestDigest: Data(repeating: 2, count: 32),
            protocolFingerprint: Data(repeating: 3, count: 32),
            runtime: ActriumRuntime(actriumVersion: "1.0.0", frontHostApi: "1.0", keelApi: "1.0"),
            programCapabilities: ["service"],
            routes: declaredRoutes.map { ActriumRoute(route: $0, capabilities: ["service"]) }
        )
        return try await RealmServiceBinding(
            profile: profile,
            verifiedProfileBody: profileBody,
            front: Data([0x50, 0x4B, 0x03, 0x04]),
            metadata: metadata,
            trustedFrontKey: Data(),
            capabilitiesJSON: capabilitiesJSON,
            permissions: permissions
        )
    }

    @Test func bindingExposesUploadAttachmentOnlyThroughTheTransport() async throws {
        let capabilities = #"{"hostModules":{"service":{"rpc":["prompt","uploadAttachment"]}}}"#
        let binding = try await makeBinding(
            capabilitiesJSON: capabilities,
            permissions: ["service.attachmentUpload"]
        )
        let routes = await binding.routes
        let allowed = await binding.attachmentUploadAllowed
        #expect(routes == ["prompt"])
        #expect(allowed)
        await binding.stop()

        let denied = try await makeBinding(capabilitiesJSON: capabilities, permissions: [])
        let deniedAllowed = await denied.attachmentUploadAllowed
        #expect(!deniedAllowed)
        await denied.stop()

        let noTransport = try await makeBinding(
            capabilitiesJSON: #"{"hostModules":{"service":{"rpc":["prompt"]}}}"#,
            permissions: ["service.attachmentUpload"]
        )
        let noTransportRoutes = await noTransport.routes
        let noTransportAllowed = await noTransport.attachmentUploadAllowed
        #expect(noTransportRoutes == ["prompt"])
        #expect(!noTransportAllowed)
        await noTransport.stop()
    }

    @Test func aStoppedBindingRefusesToConnectAndStartARuntime() async throws {
        // Closing the tab may land while a prepare is suspended on the session
        // store hop. connect() must re-check the binding's own state before it
        // creates or reuses a startup task, so teardown cannot resurrect a
        // Realm node.
        let binding = try await makeBinding(
            capabilitiesJSON: #"{"hostModules":{"service":{"rpc":["uploadAttachment"]}}}"#,
            permissions: ["service.attachmentUpload"]
        )
        await binding.stop()
        await #expect(throws: CancellationError.self) {
            _ = try await binding.connect()
        }
    }

    @Test func prepareOnAStoppedBindingFailsWithoutStarting() async throws {
        let binding = try await makeBinding(
            capabilitiesJSON: #"{"hostModules":{"service":{"rpc":["uploadAttachment"]}}}"#,
            permissions: ["service.attachmentUpload"]
        )
        await binding.stop()
        let reply = await binding.prepareAttachmentUpload(makePrepareCall(), deliver: { _ in })
        #expect(reply.failure?.code == "cancelled")
    }

    // MARK: - Session recovery

    private struct SessionError: Error, CustomStringConvertible, Sendable {
        let description: String
    }

    private struct RecoveryConnection: Sendable, Equatable {
        let target: ActrBindings.ActrId
        let sessionEpoch: UInt64
    }

    private actor ResumeGate {
        private var released = false
        private var waiters: [CheckedContinuation<Void, Never>] = []

        func wait() async {
            if released { return }
            await withCheckedContinuation { waiters.append($0) }
        }

        func release() {
            released = true
            let waiters = self.waiters
            self.waiters.removeAll()
            waiters.forEach { $0.resume() }
        }
    }

    private actor AttachmentCounter {
        private(set) var count = 0

        func attach(_ target: ActrBindings.ActrId) -> ActrBindings.ActrId {
            count += 1
            return target
        }
    }

    @Test func downloadedFrontServiceBindingMatchesDeclaredCapabilities() throws {
        // A verified renderer-only Front has no capability manifest. It must
        // not derive Attach metadata or construct a Service binding.
        let binding: String? = try BrowserViewModel.serviceBindingForDeclaredCapabilities(nil) { _ in
            Issue.record("Attach metadata must not be derived without Service capabilities.")
            throw SessionError(description: "Unexpected Attach metadata derivation")
        }
        #expect(binding == nil)

        let serviceCapabilities = #"{"hostModules":{"service":{"rpc":["documents/read"]}}}"#
        let declared = try BrowserViewModel.serviceBindingForDeclaredCapabilities(serviceCapabilities) { $0 }
        #expect(declared == serviceCapabilities)

        let eventCapabilities = #"{"hostModules":{"service":{"events":["documents/changed"]}}}"#
        let eventBinding = try BrowserViewModel.serviceBindingForDeclaredCapabilities(eventCapabilities) { $0 }
        #expect(eventBinding == eventCapabilities)
    }

    @Test func localFrontSkipsServiceBindingForNonServiceCapabilities() throws {
        let localStorageOnly = #"{"hostModules":{"local-storage":{"rpc":["getItem","setItem"]}}}"#
        let rendererOnly: String? = try BrowserViewModel.serviceBindingForDeclaredCapabilities(
            localStorageOnly
        ) { _ in
            Issue.record("A local Front's non-Service capabilities must not derive Attach metadata.")
            throw SessionError(description: "Unexpected local Front Service binding")
        }
        #expect(rendererOnly == nil)
    }

    @Test func rpcRecoversAfterJoiningAResumeOnlyCaller() async throws {
        let oldTarget = ActrBindings.ActrId(realm: "realm@example.test", serialNumber: 1)
        let replacement = ActrBindings.ActrId(realm: "realm@example.test", serialNumber: 2)
        let gate = ResumeGate()

        // The resume-only caller is already in flight against offline A when
        // the RPC joins. Its transport error must trigger target-aware recovery.
        let resumeOnly = Task<ActrBindings.ActrId, Error> {
            await gate.wait()
            throw SessionError(description: "Unavailable: old Service is offline")
        }
        let rpc = Task {
            try await RealmServiceBinding.recoverCallAfterJoining(
                recovery: resumeOnly,
                recover: { _ in
                    Issue.record("A failed join must restart before target-aware recovery.")
                    throw SessionError(description: "Unexpected successful recovery")
                },
                restart: {
                    try await RealmServiceBinding.recoverDiscoveredConnection(
                        currentTarget: oldTarget,
                        discoveredTarget: replacement,
                        retarget: { replacement },
                        resume: {
                            Issue.record("The RPC must not retry Resume on the unreachable old peer.")
                            throw SessionError(description: "Unavailable: old Service is offline")
                        }
                    )
                }
            )
        }
        await gate.release()
        #expect(try await rpc.value == replacement)
    }

    @Test func joinedRecoveryKeepsFreshSameTargetSession() async throws {
        let target = ActrBindings.ActrId(realm: "realm@example.test", serialNumber: 1)
        let expired = RecoveryConnection(target: target, sessionEpoch: 1)
        let reattached = RecoveryConnection(target: target, sessionEpoch: 2)
        let resumeOnly = Task<RecoveryConnection, Error> { reattached }

        // Schedule the waiter before the original owner publishes startup. The
        // resume-only task has already re-Attached epoch 2 to the same Actor.
        let recovered = try await RealmServiceBinding.recoverCallAfterJoining(
            recovery: resumeOnly,
            recover: { joined in
                let connection = try await RealmServiceBinding.joinedRecoveryConnection(
                    joined: joined,
                    ownsRecoverySlot: true,
                    connect: {
                        Issue.record("The handoff must not read the expired startup session.")
                        return expired
                    }
                )
                return try await RealmServiceBinding.recoverDiscoveredConnection(
                    currentTarget: connection.target,
                    discoveredTarget: target,
                    retarget: { expired },
                    resume: { connection }
                )
            },
            restart: {
                Issue.record("A successful re-Attach must not restart recovery.")
                throw SessionError(description: "Unexpected restart")
            }
        )
        #expect(recovered == reattached)
        #expect(recovered.sessionEpoch == 2)
    }

    @Test func discoveryFailureKeepsJoinedSessionPublished() async throws {
        let target = ActrBindings.ActrId(realm: "realm@example.test", serialNumber: 1)
        let expired = RecoveryConnection(target: target, sessionEpoch: 1)
        let reattached = RecoveryConnection(target: target, sessionEpoch: 2)
        let resumeOnly = Task<RecoveryConnection, Error> { reattached }
        var published: RecoveryConnection?

        await #expect(throws: SessionError.self) {
            try await RealmServiceBinding.recoverCallAfterJoining(
                recovery: resumeOnly,
                recover: { joined in
                    RealmServiceBinding.publishJoinedRecovery(
                        joined: joined,
                        ownsRecoverySlot: true,
                        publish: { published = $0 }
                    )
                    _ = try await RealmServiceBinding.joinedRecoveryConnection(
                        joined: joined,
                        ownsRecoverySlot: true,
                        connect: {
                            Issue.record("The failed handoff must not read expired startup.")
                            return expired
                        }
                    )
                    throw SessionError(description: "TimedOut: Service discovery failed")
                },
                restart: {
                    Issue.record("A successful re-Attach must not restart after Discovery fails.")
                    throw SessionError(description: "Unexpected restart")
                }
            )
        }
        // Discovery failed after the new exclusive lease was acquired. The
        // next business call must see that lease rather than Attach it again.
        #expect(published == reattached)
    }

    @Test func concurrentRPCsShareOneReplacementAttach() async throws {
        let oldTarget = ActrBindings.ActrId(realm: "realm@example.test", serialNumber: 1)
        let replacement = ActrBindings.ActrId(realm: "realm@example.test", serialNumber: 2)
        let resumeGate = ResumeGate()
        let replacementGate = ResumeGate()
        let attachments = AttachmentCounter()
        let resumeOnly = Task<ActrBindings.ActrId, Error> {
            await resumeGate.wait()
            return oldTarget
        }

        // This is the single task published by recoverJoinedCall. Both RPCs
        // joining the successful Resume must await it rather than retargeting A
        // independently and contending for B's exclusive lease.
        let replacementRecovery = Task<ActrBindings.ActrId, Error> {
            await replacementGate.wait()
            return try await RealmServiceBinding.recoverDiscoveredConnection(
                currentTarget: oldTarget,
                discoveredTarget: replacement,
                retarget: { await attachments.attach(replacement) },
                resume: { oldTarget }
            )
        }
        func joinRPC() async throws -> ActrBindings.ActrId {
            try await RealmServiceBinding.recoverCallAfterJoining(
                recovery: resumeOnly,
                recover: { _ in try await replacementRecovery.value },
                restart: {
                    Issue.record("A successful shared Resume must not be restarted.")
                    throw SessionError(description: "Unexpected restart")
                }
            )
        }
        let first = Task { try await joinRPC() }
        let second = Task { try await joinRPC() }
        await resumeGate.release()
        await replacementGate.release()

        #expect(try await first.value == replacement)
        #expect(try await second.value == replacement)
        let attachCount = await attachments.count
        #expect(attachCount == 1)
    }

    @Test func replacementServiceIsAttachedWithoutResumingTheOldPeer() async throws {
        let oldTarget = ActrBindings.ActrId(realm: "realm@example.test", serialNumber: 1)
        let replacement = ActrBindings.ActrId(realm: "realm@example.test", serialNumber: 2)

        // A is unreachable while its old lease is still valid. Discovering B
        // must select a fresh Attach without first attempting to Resume A.
        let recovered = try await RealmServiceBinding.recoverDiscoveredConnection(
            currentTarget: oldTarget,
            discoveredTarget: replacement,
            retarget: { replacement },
            resume: {
                Issue.record("The unreachable old Service must not be resumed.")
                throw SessionError(description: "Unavailable: old Service is offline")
            }
        )
        #expect(recovered == replacement)

        // An unchanged target keeps the existing lease and normal Resume path.
        let resumed = try await RealmServiceBinding.recoverDiscoveredConnection(
            currentTarget: oldTarget,
            discoveredTarget: oldTarget,
            retarget: { replacement },
            resume: { oldTarget }
        )
        #expect(resumed == oldTarget)
    }

    @Test func onlyTerminalSessionErrorsTriggerAFreshAttach() {
        // A rebuilt channel generation or an expired lease cannot be repaired
        // by Resume alone; the Host must Attach a new session.
        for code in ["SessionExpired", "SessionEpochMismatch", "ChannelExpired"] {
            #expect(RealmServiceBinding.requiresReattach(
                SessionError(description: "actrium.session/v1:\(code):detail")))
        }
        // Transient transport failures must be surfaced; re-Attaching on every
        // RPC error would churn sessions and lose the tab's lease.
        for message in [
            "Unavailable: the peer is offline",
            "SessionOccupied: another tab owns this lease",
            "TimedOut: no response within the deadline",
            "CancellationError()",
        ] {
            #expect(!RealmServiceBinding.requiresReattach(SessionError(description: message)))
        }
        #expect(!RealmServiceBinding.requiresReattach(SessionError(description: "")))
        #expect(!RealmServiceBinding.requiresReattach(CancellationError()))
    }
}
