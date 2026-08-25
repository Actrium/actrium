import Testing
@testable import Actro

struct BrowserUserAgentIdentityTests {
    @Test func applicationNameUsesActroProductToken() {
        #expect(
            BrowserUserAgentIdentity.applicationName(version: "1.2.3", safariVersion: "26.0")
                == "Version/26.0 Safari/605.1.15 Actro/1.2.3"
        )
    }

    @Test func applicationNameRemovesInvalidUserAgentCharacters() {
        #expect(
            BrowserUserAgentIdentity.applicationName(
                version: " 1.2 beta/测试 ",
                safariVersion: " 26 beta/测试 "
            ) == "Version/26beta Safari/605.1.15 Actro/1.2beta"
        )
    }

    @Test func applicationNameFallsBackWhenVersionIsMissing() {
        #expect(
            BrowserUserAgentIdentity.applicationName(version: nil, safariVersion: "26.0")
                == "Version/26.0 Safari/605.1.15 Actro/0"
        )
        #expect(
            BrowserUserAgentIdentity.applicationName(version: "   ", safariVersion: "26.0")
                == "Version/26.0 Safari/605.1.15 Actro/0"
        )
    }

    @Test func applicationNameAlwaysIncludesSafariCompatibilityTokens() {
        let value = BrowserUserAgentIdentity.applicationName(version: "1", safariVersion: "18.6")

        #expect(value.contains("Version/18.6"))
        #expect(value.contains("Safari/605.1.15"))
        #expect(value.hasSuffix("Actro/1"))
    }
}
