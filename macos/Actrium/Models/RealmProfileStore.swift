import ActriumCore
import Foundation

/// Persists accepted trust before a Front is acquired or started.
@MainActor
enum RealmProfileStore {
    static let didRetire = Notification.Name("ActriumRealmDidRetire")

    static func acceptRealm(
        body: String, realmName: String, signingKeysJSON: String, preferences: UserDefaults
    ) throws -> RealmProfileInfo {
        let key = realmKey(realmName)
        var baseline: RealmProfileBaseline?
        if let saved = preferences.dictionary(forKey: key) {
            guard let body = saved["body"] as? String, let signingKeys = saved["signingKeys"] as? String else {
                throw CoreError.RealmProfileInvalid(message: "The accepted Realm Profile could not be read.")
            }
            baseline = RealmProfileBaseline(body: body, signingKeysJson: signingKeys)
        }
        let active = try ActriumCore.acceptRealmProfileSuccessor(
            body: body, realmName: realmName, signingKeysJson: signingKeysJSON, baseline: baseline
        )
        var accepted = ["body": body, "signingKeys": signingKeysJSON]
        accepted["sequence"] = active.map { String($0.sequence) }
        preferences.set(accepted, forKey: key)

        guard let active else {
            NotificationCenter.default.post(name: didRetire, object: preferences, userInfo: ["realm": realmName])
            throw CoreError.RealmRetired(message: realmName)
        }
        return active
    }

    static func isCurrent(_ profile: RealmProfileInfo, preferences: UserDefaults) -> Bool {
        let saved = preferences.dictionary(forKey: realmKey(profile.realmName))
        return saved?["sequence"] as? String == String(profile.sequence)
    }

    static func acceptPublisher(
        body: String, domain: String, preferences: UserDefaults
    ) throws -> PublisherProfileInfo {
        let key = "actr.acceptedPublisherProfile.\(domain)"
        let accepted = try ActriumCore.acceptRealmPackagePublisher(
            body: body, domain: domain, previousPinsJson: preferences.string(forKey: key)
        )
        preferences.set(accepted.pinsJson, forKey: key)
        return accepted.profile
    }

    private static func realmKey(_ realmName: String) -> String {
        "actr.acceptedRealmProfile.\(realmName)"
    }
}

extension PublisherProfileInfo {
    func verificationKey(for localFront: LocalRealmFront) -> Data? {
        guard let key = packageSigningKeys.first(where: {
            $0.id == localFront.signingKeyId &&
                ($0.status == "active" || (!localFront.allowRebuild && $0.status == "retired"))
        }), let bytes = Data(base64Encoded: key.publicKeyBase64), bytes.count == 32 else {
            return nil
        }
        return bytes
    }
}
