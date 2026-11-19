import Foundation
import Testing
@testable import Actrium

struct ActrLinkInviteTests {
    private let baseLink = "actr://realm.develenv.com/actr-test-0-0-5"
    private let invite = "ri_test-invite"

    /// Percent-encoded `args` value, as shipped in invite links.
    private func encodedArgs(_ json: String) -> String {
        json.addingPercentEncoding(withAllowedCharacters: .alphanumerics) ?? ""
    }

    /// Build a link whose `args` is the percent-encoded form shipped in invites.
    private func link(argsJSON: String, extraQuery: String = "") -> String {
        "\(baseLink)?args=\(encodedArgs(argsJSON))\(extraQuery)"
    }

    private func argsLink(invite: String) -> String {
        link(argsJSON: #"{"invite":"\#(invite)"}"#)
    }

    // MARK: - Extraction

    @Test func inviteIsExtractedFromPercentEncodedArgs() {
        let uri = argsLink(invite: invite)
        #expect(ActrLinkInvite.invite(from: uri) == invite)
    }

    @Test func linkWithoutArgsKeepsTheSheet() {
        #expect(ActrLinkInvite.invite(from: baseLink) == nil)
        #expect(ActrLinkInvite.invite(from: "\(baseLink)?tab=notes") == nil)
    }

    @Test func malformedArgsKeepTheSheet() {
        #expect(ActrLinkInvite.invite(from: "\(baseLink)?args=not-json") == nil)
        #expect(ActrLinkInvite.invite(from: link(argsJSON: #"["ri_x"]"#)) == nil)
    }

    @Test func argsWithoutAUsableInviteKeepTheSheet() {
        #expect(ActrLinkInvite.invite(from: link(argsJSON: #"{"other":"ri_x"}"#)) == nil)
        #expect(ActrLinkInvite.invite(from: link(argsJSON: #"{"invite":5}"#)) == nil)
        #expect(ActrLinkInvite.invite(from: argsLink(invite: "   ")) == nil)
    }

    @Test func firstArgsItemWins() {
        let uri = "\(baseLink)?args=\(encodedArgs(#"{"invite":"ri_first"}"#))"
            + "&args=\(encodedArgs(#"{"invite":"ri_second"}"#))"
        #expect(ActrLinkInvite.invite(from: uri) == "ri_first")
    }

    // MARK: - Sanitizing

    @Test func strippingRemovesArgsAndKeepsOtherParameters() throws {
        let uri = argsLink(invite: invite) + "&tab=notes"
        let stripped = try #require(ActrLinkInvite.strippingInviteArgs(from: uri))
        let components = try #require(URLComponents(string: stripped))
        #expect(components.queryItems?.contains { $0.name == "args" } == false)
        #expect(components.queryItems?.first { $0.name == "tab" }?.value == "notes")
        #expect(components.path == "/actr-test-0-0-5")
        #expect(ActrLinkInvite.invite(from: stripped) == nil)
    }

    @Test func strippingDropsTheQueryWhenArgsWasTheOnlyParameter() throws {
        let stripped = try #require(ActrLinkInvite.strippingInviteArgs(from: argsLink(invite: invite)))
        #expect(stripped == baseLink)
        #expect(try #require(URLComponents(string: stripped)).queryItems == nil)
    }

    @Test func strippingLeavesLinksWithoutArgsUnchanged() {
        #expect(ActrLinkInvite.strippingInviteArgs(from: baseLink) == baseLink)
        #expect(
            ActrLinkInvite.strippingInviteArgs(from: "\(baseLink)?tab=notes")
                == "\(baseLink)?tab=notes"
        )
    }

    @Test func malformedAuthorityCannotReturnAnInviteBearingAddress() {
        let malformed = "actr://[invalid/echo?args=\(encodedArgs(#"{"invite":"ri_test"}"#))"
        #expect(ActrLinkInvite.strippingInviteArgs(from: malformed)?.contains("args=") != true)
    }
}
