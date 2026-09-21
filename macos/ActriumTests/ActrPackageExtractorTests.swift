import Foundation
import Testing
@testable import Actrium

@Suite
struct ActrPackageExtractorTests {
    @Test(arguments: ["../outside.js", "nested/../../outside.js", "/dev/zero", "nested\\outside.js", "bad\0path"])
    func rejectsUnsafeManifestPaths(path: String) {
        #expect(!ActrPackageExtractor.isSafeRelativePath(path))
    }

    @Test
    func confinesScriptReadsToRegularFilesInsideRoot() throws {
        let parent = try makeDirectory()
        defer { try? FileManager.default.removeItem(at: parent) }
        let root = parent.appendingPathComponent("package", isDirectory: true)
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: false)
        let script = root.appendingPathComponent("main.js")
        try Data("hello".utf8).write(to: script)
        let outside = parent.appendingPathComponent("outside.js")
        try Data("outside".utf8).write(to: outside)
        try FileManager.default.createSymbolicLink(at: root.appendingPathComponent("escape.js"), withDestinationURL: outside)

        #expect(ActrPackageExtractor.containedFile(in: root, relativePath: "main.js")?.path == script.resolvingSymlinksInPath().path)
        #expect(ActrPackageExtractor.containedFile(in: root, relativePath: "escape.js") == nil)
        #expect(ActrPackageExtractor.containedFile(in: root, relativePath: "../outside.js") == nil)
        #expect(ActrPackageExtractor.containedFile(in: root, relativePath: ".") == nil)
    }

    @Test
    func extractsRegularFiles() throws {
        let parent = try makeDirectory()
        defer { try? FileManager.default.removeItem(at: parent) }
        let archive = parent.appendingPathComponent("package.actr")
        try #require(Data(base64Encoded: Self.valid)).write(to: archive)
        let root = try ActrPackageExtractor.extract(archive: archive, temporaryDirectory: parent)
        #expect(try String(contentsOf: root.appendingPathComponent("nested/main.js"), encoding: .utf8) == "hello")
    }

    @Test(arguments: ["expanded", "underreportedSize", "manyEntries", "parentTraversal", "absolutePath", "symlink", "duplicate", "deepPath", "corrupt"])
    func rejectsUnsafeArchivesAndRemovesPartialOutput(kind: String) throws {
        let parent = try makeDirectory()
        defer { try? FileManager.default.removeItem(at: parent) }
        let archive = parent.appendingPathComponent("package.actr")
        let fixtures = [
            "expanded": Self.expanded,
            "underreportedSize": Self.underreportedSize,
            "manyEntries": Self.manyEntries,
            "parentTraversal": Self.parentTraversal,
            "absolutePath": Self.absolutePath,
            "symlink": Self.symlink,
            "duplicate": Self.duplicate,
            "deepPath": Self.deepPath,
            "corrupt": "aW52YWxpZCB6aXA=",
        ]
        try #require(Data(base64Encoded: try #require(fixtures[kind]))).write(to: archive)
        #expect(throws: (any Error).self) {
            _ = try ActrPackageExtractor.extract(
                archive: archive,
                maximumBytes: 1024,
                maximumEntries: kind == "manyEntries" || kind == "deepPath" ? 1 : 10,
                temporaryDirectory: parent
            )
        }
        #expect(try FileManager.default.contentsOfDirectory(atPath: parent.path) == ["package.actr"])
    }

    private func makeDirectory() throws -> URL {
        let parent = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        try FileManager.default.createDirectory(at: parent, withIntermediateDirectories: false)
        return parent
    }

    // Small deterministic ZIP fixtures (DEFLATE); no external archiver is required.
    private static let valid = "UEsDBBQAAAAIAAAAIVyGphA2BwAAAAUAAAAOAAAAbmVzdGVkL21haW4uanPLSM3JyQcAUEsBAhQDFAAAAAgAAAAhXIamEDYHAAAABQAAAA4AAAAAAAAAAAAAAICBAAAAAG5lc3RlZC9tYWluLmpzUEsFBgAAAAABAAEAPAAAADMAAAAAAA=="
    // Both local and central size fields lie about a 4 KiB DEFLATE body.
    private static let underreportedSize = "UEsDBBQAAAAIAAAAIVxANKb+FgAAAAEAAAAHAAAAbWFpbi5qc+3BAQ0AAADCoGzvX8oeDigAAADg3QBQSwECFAMUAAAACAAAACFcQDSm/hYAAAABAAAABwAAAAAAAAAAAAAAgIEAAAAAbWFpbi5qc1BLBQYAAAAAAQABADUAAAA7AAAAAAA="
    private static let expanded = "UEsDBBQAAAAIAAAAIVxANKb+FgAAAAAQAAAHAAAAbWFpbi5qc+3BAQ0AAADCoGzvX8oeDigAAADg3QBQSwECFAMUAAAACAAAACFcQDSm/hYAAAAAEAAABwAAAAAAAAAAAAAAgIEAAAAAbWFpbi5qc1BLBQYAAAAAAQABADUAAAA7AAAAAAA="
    private static let manyEntries = "UEsDBBQAAAAIAAAAIVwAAAAAAgAAAAAAAAABAAAAYQMAUEsDBBQAAAAIAAAAIVwAAAAAAgAAAAAAAAABAAAAYgMAUEsBAhQDFAAAAAgAAAAhXAAAAAACAAAAAAAAAAEAAAAAAAAAAAAAAICBAAAAAGFQSwECFAMUAAAACAAAACFcAAAAAAIAAAAAAAAAAQAAAAAAAAAAAAAAgIEhAAAAYlBLBQYAAAAAAgACAF4AAABCAAAAAAA="
    private static let parentTraversal = "UEsDBBQAAAAIAAAAIVz7OSuCBQAAAAMAAAAKAAAALi4vb3V0c2lkZUtKTAEAUEsBAhQDFAAAAAgAAAAhXPs5K4IFAAAAAwAAAAoAAAAAAAAAAAAAAICBAAAAAC4uL291dHNpZGVQSwUGAAAAAAEAAQA4AAAALQAAAAAA"
    private static let absolutePath = "UEsDBBQAAAAIAAAAIVz7OSuCBQAAAAMAAAAYAAAAL3RtcC9hY3RyLWFyY2hpdmUtZXNjYXBlS0pMAQBQSwECFAMUAAAACAAAACFc+zkrggUAAAADAAAAGAAAAAAAAAAAAAAAgIEAAAAAL3RtcC9hY3RyLWFyY2hpdmUtZXNjYXBlUEsFBgAAAAABAAEARgAAADsAAAAAAA=="
    private static let symlink = "UEsDBBQAAAAIAAAAIVxASv+xDwAAAA0AAAAEAAAAbGlua9PT09fT088vLSnOTEkFAFBLAQIUAxQAAAAIAAAAIVxASv+xDwAAAA0AAAAEAAAAAAAAAAAAAAD/oQAAAABsaW5rUEsFBgAAAAABAAEAMgAAADEAAAAAAA=="
    private static let duplicate = "UEsDBBQAAAAIAAAAIVzxhmx6BQAAAAMAAAABAAAAYcvPSwUAUEsDBBQAAAAIAAAAIVxmisoRBQAAAAMAAAADAAAALi9hKynPBwBQSwECFAMUAAAACAAAACFc8YZsegUAAAADAAAAAQAAAAAAAAAAAAAAgIEAAAAAYVBLAQIUAxQAAAAIAAAAIVxmisoRBQAAAAMAAAADAAAAAAAAAAAAAACAgSQAAAAuL2FQSwUGAAAAAAIAAgBgAAAASgAAAAAA"
    private static let deepPath = "UEsDBBQAAAAIAAAAIVyGphA2BwAAAAUAAAAHAAAAYS9iL2MvZMtIzcnJBwBQSwECFAMUAAAACAAAACFchqYQNgcAAAAFAAAABwAAAAAAAAAAAAAAgIEAAAAAYS9iL2MvZFBLBQYAAAAAAQABADUAAAAsAAAAAAA="
}
