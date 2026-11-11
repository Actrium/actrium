import Foundation
import ActriumCore

/// Native DTOs keep Rust FFI records out of the Tab and UI boundary.
struct TrustedProgramSource: Sendable {
    let href: String
    let expectedSha256: String?
    let expectedActorType: String?
}

struct VerifiedProgramResource: Sendable {
    let path: String
    let bytes: [UInt8]
}

struct VerifiedLocalProgram: Sendable {
    let publisher: String
    let program: String
    let version: String
    let digest: String
    let signingKeyId: String
    let title: String
    let description: String?
    let guestCode: String
    let serviceBytes: [UInt8]
    let resources: [VerifiedProgramResource]
    let metadataJson: String
}

enum ActriumTrustedProgramCoreBridge {
    static func load(_ source: TrustedProgramSource) async throws -> VerifiedLocalProgram {
        let result = try await AppleTrustedPackageLoader.shared.load(source: PackageSourceInfo(
            href: source.href, expectedSha256: source.expectedSha256,
            expectedActorType: source.expectedActorType))
        return VerifiedLocalProgram(
            publisher: result.publisher, program: result.program, version: result.version,
            digest: result.digest, signingKeyId: result.signingKeyId, title: result.title,
            description: result.description, guestCode: result.guestCode, serviceBytes: result.serviceBytes,
            resources: result.resources.map { VerifiedProgramResource(path: $0.path, bytes: $0.bytes) },
            metadataJson: result.metadataJson)
    }
}
