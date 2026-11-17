import Foundation
import Testing
@testable import Actrium

@MainActor
struct ActrPackageCacheEvictionTests {

    // MARK: - Last used

    @Test func lastUsed_touchRoundTrip() throws {
        try withTemporaryCacheRoot { _ in
            let digest = testDigest("a")
            try makeCacheEntry(digest: digest, archiveBytes: 1, extractedBytes: 0)
            let before = Int64(Date().timeIntervalSince1970)

            ActrPackageCache.touchLastUsed(forDigest: digest)

            let after = Int64(Date().timeIntervalSince1970)
            let stored = Int64(ActrPackageCache.lastUsed(forDigest: digest).timeIntervalSince1970)
            #expect(stored >= before)
            #expect(stored <= after)
        }
    }

    @Test func lastUsed_missingFileFallsBackToArchiveModificationDate() throws {
        try withTemporaryCacheRoot { _ in
            let digest = testDigest("a")
            try makeCacheEntry(digest: digest, archiveBytes: 1, extractedBytes: 0)
            let modified = Date(timeIntervalSince1970: 123_456)
            try FileManager.default.setAttributes(
                [.modificationDate: modified],
                ofItemAtPath: ActrPackageCache.archiveURL(forDigest: digest).path
            )

            #expect(ActrPackageCache.lastUsed(forDigest: digest) == modified)
        }
    }

    @Test func lastUsed_corruptFileWithoutArchiveFallsBackToDistantPast() throws {
        try withTemporaryCacheRoot { _ in
            let digest = testDigest("a")
            let packageDirectory = ActrPackageCache.archiveURL(forDigest: digest)
                .deletingLastPathComponent()
            try FileManager.default.createDirectory(
                at: packageDirectory,
                withIntermediateDirectories: true
            )
            try Data("not-a-timestamp".utf8).write(to: lastUsedURL(forDigest: digest))

            #expect(ActrPackageCache.lastUsed(forDigest: digest) == .distantPast)
        }
    }

    // MARK: - Size and eviction

    @Test func cacheSize_countsPackagesAndExtractions() throws {
        try withTemporaryCacheRoot { _ in
            try makeCacheEntry(digest: testDigest("a"), archiveBytes: 100, extractedBytes: 40)
            try makeCacheEntry(digest: testDigest("b"), archiveBytes: 7, extractedBytes: 5)

            #expect(ActrPackageCache.cacheSize() == 152)
        }
    }

    @Test func evict_withinLimitIsNoOp() throws {
        try withTemporaryCacheRoot { _ in
            let digest = testDigest("a")
            try makeCacheEntry(
                digest: digest,
                archiveBytes: 100,
                extractedBytes: 50,
                lastUsed: "100"
            )
            let size = ActrPackageCache.cacheSize()

            let freed = ActrPackageCache.evictIfNeeded(protectedDigests: [], maxBytes: size)

            #expect(freed == 0)
            #expect(ActrPackageCache.cacheSize() == size)
            #expect(fileExists(ActrPackageCache.archiveURL(forDigest: digest)))
        }
    }

    @Test func evict_removesOldestFirstUntilWithinLimit() throws {
        try withTemporaryCacheRoot { _ in
            let oldest = testDigest("a")
            let middle = testDigest("b")
            let newest = testDigest("c")
            try makeCacheEntry(digest: oldest, archiveBytes: 100, extractedBytes: 50, lastUsed: "100")
            try makeCacheEntry(digest: middle, archiveBytes: 100, extractedBytes: 50, lastUsed: "200")
            try makeCacheEntry(digest: newest, archiveBytes: 100, extractedBytes: 50, lastUsed: "300")

            let freed = ActrPackageCache.evictIfNeeded(protectedDigests: [], maxBytes: 200)

            #expect(freed == 306)
            #expect(!fileExists(ActrPackageCache.archiveURL(forDigest: oldest)))
            #expect(!fileExists(ActrPackageCache.extractedURL(forDigest: oldest)))
            #expect(!fileExists(ActrPackageCache.archiveURL(forDigest: middle)))
            #expect(!fileExists(ActrPackageCache.extractedURL(forDigest: middle)))
            #expect(fileExists(ActrPackageCache.archiveURL(forDigest: newest)))
            #expect(fileExists(ActrPackageCache.extractedURL(forDigest: newest)))
            #expect(ActrPackageCache.cacheSize() == 153)
        }
    }

    @Test func evict_preservesProtectedDigest() throws {
        try withTemporaryCacheRoot { _ in
            let protected = testDigest("a")
            let unprotected = testDigest("b")
            try makeCacheEntry(digest: protected, archiveBytes: 100, extractedBytes: 50, lastUsed: "100")
            try makeCacheEntry(digest: unprotected, archiveBytes: 100, extractedBytes: 50, lastUsed: "200")

            let freed = ActrPackageCache.evictIfNeeded(
                protectedDigests: [protected],
                maxBytes: 0
            )

            #expect(freed == 153)
            #expect(fileExists(ActrPackageCache.archiveURL(forDigest: protected)))
            #expect(fileExists(ActrPackageCache.extractedURL(forDigest: protected)))
            #expect(!fileExists(ActrPackageCache.archiveURL(forDigest: unprotected)))
            #expect(!fileExists(ActrPackageCache.extractedURL(forDigest: unprotected)))
        }
    }

    @Test func evict_removesTriadAndOnlyMatchingURLMapEntries() throws {
        try withTemporaryCacheRoot { root in
            let evicted = testDigest("a")
            let retained = testDigest("b")
            let evictedURL = try #require(URL(string: "https://example.com/evicted.actr"))
            let retainedURL = try #require(URL(string: "https://example.com/retained.actr"))
            try makeCacheEntry(digest: evicted, archiveBytes: 100, extractedBytes: 50, lastUsed: "100")
            try makeCacheEntry(digest: retained, archiveBytes: 100, extractedBytes: 50, lastUsed: "200")
            ActrPackageCache.rememberDownloadURL(evictedURL, digest: evicted)
            ActrPackageCache.rememberDownloadURL(retainedURL, digest: retained)

            _ = ActrPackageCache.evictIfNeeded(protectedDigests: [retained], maxBytes: 0)

            #expect(!fileExists(ActrPackageCache.archiveURL(forDigest: evicted)))
            #expect(!fileExists(ActrPackageCache.extractedURL(forDigest: evicted)))
            #expect(!fileExists(lastUsedURL(forDigest: evicted)))
            #expect(fileExists(ActrPackageCache.archiveURL(forDigest: retained)))
            #expect(fileExists(ActrPackageCache.extractedURL(forDigest: retained)))
            let map = try readURLMap(root: root)
            #expect(map[evictedURL.absoluteString] == nil)
            #expect(map[retainedURL.absoluteString]?["digest"] == retained)
        }
    }

    @Test func evict_skipsStagingAndInvalidDigestDirectories() throws {
        try withTemporaryCacheRoot { _ in
            let digest = testDigest("a")
            try makeCacheEntry(digest: digest, archiveBytes: 10, extractedBytes: 10, lastUsed: "100")
            let extractedRoot = ActrPackageCache.extractedURL(forDigest: digest)
                .deletingLastPathComponent()
            let packagesRoot = ActrPackageCache.archiveURL(forDigest: digest)
                .deletingLastPathComponent()
                .deletingLastPathComponent()
            let stagingDirectory = extractedRoot.appendingPathComponent(".staging-interrupted")
            let invalidDirectory = extractedRoot.appendingPathComponent("not-a-digest")
            let stagingFile = packagesRoot.appendingPathComponent(".staging-download")
            try FileManager.default.createDirectory(at: stagingDirectory, withIntermediateDirectories: true)
            try FileManager.default.createDirectory(at: invalidDirectory, withIntermediateDirectories: true)
            try Data(repeating: 1, count: 20).write(to: stagingDirectory.appendingPathComponent("payload"))
            try Data(repeating: 1, count: 20).write(to: invalidDirectory.appendingPathComponent("payload"))
            try Data(repeating: 1, count: 20).write(to: stagingFile)

            _ = ActrPackageCache.evictIfNeeded(protectedDigests: [], maxBytes: 0)

            #expect(!fileExists(ActrPackageCache.extractedURL(forDigest: digest)))
            #expect(fileExists(stagingDirectory))
            #expect(fileExists(invalidDirectory))
            #expect(fileExists(stagingFile))
        }
    }

    @Test func evict_toleratesCorruptLastUsedFile() throws {
        try withTemporaryCacheRoot { _ in
            let corrupt = testDigest("a")
            let valid = testDigest("b")
            try makeCacheEntry(digest: corrupt, archiveBytes: 50, extractedBytes: 50, lastUsed: "bad")
            try makeCacheEntry(digest: valid, archiveBytes: 50, extractedBytes: 50, lastUsed: "200")
            try FileManager.default.setAttributes(
                [.modificationDate: Date(timeIntervalSince1970: 100)],
                ofItemAtPath: ActrPackageCache.archiveURL(forDigest: corrupt).path
            )

            _ = ActrPackageCache.evictIfNeeded(protectedDigests: [], maxBytes: 103)

            #expect(!fileExists(ActrPackageCache.archiveURL(forDigest: corrupt)))
            #expect(fileExists(ActrPackageCache.archiveURL(forDigest: valid)))
        }
    }

    @Test func clearAll_removesEverythingExceptProtectedDigestsAndClearsStaging() throws {
        try withTemporaryCacheRoot { _ in
            let removed = testDigest("a")
            let protected = testDigest("b")
            let alsoRemoved = testDigest("c")
            try makeCacheEntry(digest: removed, archiveBytes: 10, extractedBytes: 10, lastUsed: "100")
            try makeCacheEntry(digest: protected, archiveBytes: 10, extractedBytes: 10, lastUsed: "200")
            try makeCacheEntry(digest: alsoRemoved, archiveBytes: 10, extractedBytes: 10, lastUsed: "300")
            let staging = ActrPackageCache.extractedURL(forDigest: protected)
                .deletingLastPathComponent()
                .appendingPathComponent(".staging-leftover")
            try FileManager.default.createDirectory(at: staging, withIntermediateDirectories: true)
            try Data(repeating: 1, count: 10).write(to: staging.appendingPathComponent("payload"))

            let freed = ActrPackageCache.clearAll(protectedDigests: [protected])

            #expect(freed > 0)
            #expect(!fileExists(ActrPackageCache.archiveURL(forDigest: removed)))
            #expect(!fileExists(ActrPackageCache.archiveURL(forDigest: alsoRemoved)))
            #expect(fileExists(ActrPackageCache.archiveURL(forDigest: protected)))
            #expect(fileExists(ActrPackageCache.extractedURL(forDigest: protected)))
            #expect(!fileExists(staging))
        }
    }

    @Test func digestFromExtractedPath_acceptsOnlyCurrentCacheRoot() throws {
        try withTemporaryCacheRoot { _ in
            let digest = testDigest("a")
            let extracted = ActrPackageCache.extractedURL(forDigest: digest)

            #expect(ActrPackageCache.digest(fromExtractedPath: extracted.path) == digest)
            #expect(ActrPackageCache.digest(fromExtractedPath: "/tmp/\(digest)") == nil)
            #expect(ActrPackageCache.digest(fromExtractedPath: "/tmp/not-a-digest") == nil)
        }
    }

    // MARK: - Extraction repair

    @Test func extract_corruptCachedDirectoryIsReplacedFromArchive() throws {
        try withTemporaryCacheRoot { _ in
            let digest = testDigest("a")
            let archive = try makeArchive()
            defer { try? FileManager.default.removeItem(at: archive) }
            let cachedArchive = try ActrPackageCache.installArchive(at: archive, digest: digest)
            let validate: (URL) throws -> Void = { root in
                let manifest = root.appendingPathComponent("manifest.toml")
                guard try String(contentsOf: manifest, encoding: .utf8) == "name = \"demo\"\n" else {
                    throw TestValidationFailure()
                }
            }
            let extracted = try ActrPackageCache.extractArchiveIfNeeded(
                cachedArchive,
                digest: digest,
                validate: validate
            )
            try "corrupt".write(
                to: extracted.appendingPathComponent("manifest.toml"),
                atomically: true,
                encoding: .utf8
            )

            let repaired = try ActrPackageCache.extractArchiveIfNeeded(
                cachedArchive,
                digest: digest,
                validate: validate
            )

            #expect(repaired == extracted)
            #expect(try String(
                contentsOf: repaired.appendingPathComponent("manifest.toml"),
                encoding: .utf8
            ) == "name = \"demo\"\n")
        }
    }

    // MARK: - Temporary download cleanup

    @Test func cleanupOrphanedTemporaryDownloads_deletesOnlyMatchingFiles() throws {
        let fm = FileManager.default
        let directory = fm.temporaryDirectory
            .appendingPathComponent("actr-download-cleanup-\(UUID().uuidString)", isDirectory: true)
        try fm.createDirectory(at: directory, withIntermediateDirectories: true)
        defer { try? fm.removeItem(at: directory) }

        let matching = directory.appendingPathComponent("actr-pkg-old.actr")
        let otherArchive = directory.appendingPathComponent("other.actr")
        let wrongExtension = directory.appendingPathComponent("actr-pkg-old.zip")
        let uppercaseExtension = directory.appendingPathComponent("actr-pkg-uppercase.ACTR")
        let wrongPrefix = directory.appendingPathComponent("prefix-actr-pkg-old.actr")
        let matchingDirectory = directory.appendingPathComponent("actr-pkg-directory.actr")
        for file in [matching, otherArchive, wrongExtension, uppercaseExtension, wrongPrefix] {
            try Data("download".utf8).write(to: file)
        }
        try fm.createDirectory(at: matchingDirectory, withIntermediateDirectories: true)

        ActrPackageDownloadService.cleanupOrphanedTemporaryDownloads(in: directory)

        #expect(!fileExists(matching))
        #expect(fileExists(otherArchive))
        #expect(fileExists(wrongExtension))
        #expect(fileExists(uppercaseExtension))
        #expect(fileExists(wrongPrefix))
        #expect(fileExists(matchingDirectory))
    }

    // MARK: - Helpers

    private func withTemporaryCacheRoot(_ body: (URL) throws -> Void) rethrows {
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent("actr-cache-eviction-\(UUID().uuidString)", isDirectory: true)
        let previousOverride = ActrPackageCache.rootOverrideForTesting
        ActrPackageCache.rootOverrideForTesting = root
        defer {
            ActrPackageCache.rootOverrideForTesting = previousOverride
            try? FileManager.default.removeItem(at: root)
        }
        try body(root)
    }

    private func makeCacheEntry(
        digest: String,
        archiveBytes: Int,
        extractedBytes: Int,
        lastUsed: String? = nil
    ) throws {
        let fm = FileManager.default
        let archive = ActrPackageCache.archiveURL(forDigest: digest)
        let extracted = ActrPackageCache.extractedURL(forDigest: digest)
        try fm.createDirectory(at: archive.deletingLastPathComponent(), withIntermediateDirectories: true)
        try fm.createDirectory(at: extracted, withIntermediateDirectories: true)
        try Data(repeating: 0x61, count: archiveBytes).write(to: archive)
        try Data(repeating: 0x62, count: extractedBytes)
            .write(to: extracted.appendingPathComponent("payload.bin"))
        if let lastUsed {
            try Data(lastUsed.utf8).write(to: lastUsedURL(forDigest: digest))
        }
    }

    private func lastUsedURL(forDigest digest: String) -> URL {
        ActrPackageCache.archiveURL(forDigest: digest)
            .deletingLastPathComponent()
            .appendingPathComponent("last-used")
    }

    private func readURLMap(root: URL) throws -> [String: [String: String]] {
        let data = try Data(contentsOf: root.appendingPathComponent("download-url-map.json"))
        let object = try JSONSerialization.jsonObject(with: data)
        return try #require(object as? [String: [String: String]])
    }

    private func makeArchive() throws -> URL {
        let fm = FileManager.default
        let source = fm.temporaryDirectory
            .appendingPathComponent("actr-cache-repair-\(UUID().uuidString)", isDirectory: true)
        try fm.createDirectory(at: source, withIntermediateDirectories: true)
        defer { try? fm.removeItem(at: source) }
        try "name = \"demo\"\n".write(
            to: source.appendingPathComponent("manifest.toml"),
            atomically: true,
            encoding: .utf8
        )
        try "console.log('demo')\n".write(
            to: source.appendingPathComponent("unified-app.js"),
            atomically: true,
            encoding: .utf8
        )

        let archive = fm.temporaryDirectory
            .appendingPathComponent("actr-cache-repair-\(UUID().uuidString).actr")
        let process = Process()
        process.executableURL = URL(fileURLWithPath: "/usr/bin/ditto")
        process.arguments = ["-c", "-k", "--sequesterRsrc", source.path, archive.path]
        try process.run()
        process.waitUntilExit()
        guard process.terminationStatus == 0 else {
            throw NSError(
                domain: "ActrPackageCacheEvictionTests",
                code: Int(process.terminationStatus)
            )
        }
        return archive
    }

    private func testDigest(_ character: String) -> String {
        String(repeating: character, count: 64)
    }

    private func fileExists(_ url: URL) -> Bool {
        FileManager.default.fileExists(atPath: url.path)
    }
}

private struct TestValidationFailure: Error {}
