import Testing
@testable import Actro

struct BrowserUserAgentIdentityTests {
    @Test func applicationNameUsesActroProductToken() {
        #expect(BrowserUserAgentIdentity.applicationName(version: "1.2.3") == "Actro/1.2.3")
    }

    @Test func applicationNameRemovesInvalidUserAgentCharacters() {
        #expect(BrowserUserAgentIdentity.applicationName(version: " 1.2 beta/测试 ") == "Actro/1.2beta")
    }

    @Test func applicationNameFallsBackWhenVersionIsMissing() {
        #expect(BrowserUserAgentIdentity.applicationName(version: nil) == "Actro/0")
        #expect(BrowserUserAgentIdentity.applicationName(version: "   ") == "Actro/0")
    }
}
