import ActriumCore
import Foundation

enum ActrOpenFailure: String, Error {
    case invalidLink = "ACTR_LINK_INVALID"
    case realmRetired = "ACTR_REALM_RETIRED"
    case instanceNotFound = "ACTR_INSTANCE_NOT_FOUND"
    case realmUnverified = "ACTR_REALM_UNVERIFIED"
    case packageNotFound = "PACKAGE_NOT_FOUND"
    case invalidPackage = "PACKAGE_INVALID"
    case downloadFailed = "PACKAGE_DOWNLOAD_FAILED"
    case packageTooLarge = "PACKAGE_TOO_LARGE"
    case signatureInvalid = "PACKAGE_SIGNATURE_INVALID"
    case digestMismatch = "PACKAGE_DIGEST_MISMATCH"
    case verificationFailed = "FRONT_VERIFICATION_FAILED"
    case publisherIdentityChanged = "PUBLISHER_IDENTITY_CHANGED"
    case publisherUnverified = "PUBLISHER_UNVERIFIED"
    case storageFailed = "ACTR_STORAGE_FAILED"
    case unknown = "ACTR_OPEN_FAILED"

    init(_ error: Error, fallback: Self = .unknown) {
        switch error {
        case let failure as Self: self = failure
        case is AppleActrPackageSignatureError: self = .signatureInvalid
        case is ActrPackageVerificationError: self = .verificationFailed
        case ActrPackageDownloadError.sizeExceeded: self = .packageTooLarge
        case is URLError, is ActrPackageDownloadError: self = .downloadFailed
        case CoreError.UriSchemeInvalid, CoreError.UriDomainInvalid, CoreError.UriLabelInvalid,
             CoreError.UriLegacyForm, CoreError.UriLocalRealmForeign:
            self = .invalidLink
        case CoreError.AuthorityDescriptorInvalid, CoreError.RealmProfileInvalid:
            self = .realmUnverified
        case CoreError.RealmRetired: self = .realmRetired
        case GuestLocalStorageError.IdentityChanged: self = .publisherIdentityChanged
        case GuestLocalStorageError.InvalidPublisher: self = .publisherUnverified
        case GuestLocalStorageError.DatabaseFailure, GuestStorageDatabaseError.busy: self = .storageFailed
        default: self = fallback
        }
    }
}
