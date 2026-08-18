import Foundation
import Testing
@testable import Actro

@MainActor
struct URLParsingTests {
    private let vm = BrowserViewModel()

    // MARK: - Full URL Pass-through

    @Test func parseURL_httpURL_passesThrough() {
        let url = vm.parseURL("http://example.com")
        #expect(url?.absoluteString == "http://example.com")
    }

    @Test func parseURL_httpsURL_passesThrough() {
        let url = vm.parseURL("https://www.google.com/search?q=test")
        #expect(url?.absoluteString == "https://www.google.com/search?q=test")
    }

    @Test func parseURL_aboutURL_passesThrough() {
        let url = vm.parseURL("about:blank")
        #expect(url?.absoluteString == "about:blank")
    }

    @Test func parseURL_actroScheme_passesThrough() {
        let url = vm.parseURL("actro://settings")
        #expect(url?.absoluteString == "actro://settings")
    }

    @Test func actroArchiveURL_isNotMisclassifiedAsLocalPath() {
        #expect(vm.isLocalFilePath("actro:///tmp/example.actr") == false)
        #expect(vm.parseURL("actro:///tmp/example.actr")?.path == "/tmp/example.actr")
    }

    @Test func plainArchivePath_isStillRecognizedAsLocalPath() {
        #expect(vm.isLocalFilePath("/tmp/example.actr"))
        #expect(vm.isLocalFilePath("example.actr"))
    }

    // MARK: - Domain-like Input (auto-prefix https)

    @Test func parseURL_domainWithDot_addsHttps() {
        let url = vm.parseURL("example.com")
        #expect(url?.absoluteString == "https://example.com")
    }

    @Test func parseURL_domainWithSubdomain_addsHttps() {
        let url = vm.parseURL("www.google.com")
        #expect(url?.absoluteString == "https://www.google.com")
    }

    @Test func parseURL_domainWithPath_addsHttps() {
        let url = vm.parseURL("github.com/user/repo")
        #expect(url?.absoluteString == "https://github.com/user/repo")
    }

    @Test func parseURL_ipAddress_addsHttps() {
        let url = vm.parseURL("192.168.1.1")
        #expect(url?.absoluteString == "https://192.168.1.1")
    }

    // MARK: - Search Query Fallback

    @Test func parseURL_plainText_fallsToSearch() {
        let url = vm.parseURL("hello world")
        #expect(url != nil)
        let str = url!.absoluteString
        #expect(str.contains("google.com") || str.contains("bing.com") || str.contains("duckduckgo.com"))
    }

    @Test func parseURL_singleWord_fallsToSearch() {
        let url = vm.parseURL("weather")
        #expect(url != nil)
        let str = url!.absoluteString
        #expect(str.contains("google.com") || str.contains("bing.com") || str.contains("duckduckgo.com"))
    }

    @Test func parseURL_textWithSpaceAndDot_fallsToSearch() {
        // Contains both space and dot — should be treated as search
        let url = vm.parseURL("hello world. how are you")
        #expect(url != nil)
        let str = url!.absoluteString
        #expect(str.contains("google.com") || str.contains("bing.com") || str.contains("duckduckgo.com"))
    }
}
