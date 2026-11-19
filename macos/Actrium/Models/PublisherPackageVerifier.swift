import ActriumCore
import Foundation

/// Archive verification is CPU work; it must not hold up the SQLite queue.
@MainActor
enum PublisherPackageVerifier {
    private static let queue = DispatchQueue(label: "com.actrium.package-verification", qos: .userInitiated)

    static func verify(archive: Data, profile: RealmProfileInfo, publisher: PublisherProfileInfo) async throws -> Data {
        try await withCheckedThrowingContinuation { continuation in
            queue.async {
                continuation.resume(with: Result { try verifiedSigner(archive: archive, profile: profile, publisher: publisher) })
            }
        }
    }

    private nonisolated static func verifiedSigner(
        archive: Data, profile: RealmProfileInfo, publisher: PublisherProfileInfo
    ) throws -> Data {
        var failure: Error = GuestLocalStorageError.InvalidPublisher(message: "No active package signing key is available.")
        for key in publisher.packageSigningKeys where key.status == "active" {
            guard let bytes = Data(base64Encoded: key.publicKeyBase64), bytes.count == 32 else { continue }
            do {
                _ = try ActriumCore.verifyRealmFrontPackage(archive: archive, trustedPublicKey: bytes, profile: profile)
                return bytes
            } catch { failure = error }
        }
        throw failure
    }
}
