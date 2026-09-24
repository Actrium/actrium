//! Publisher registration proof payload helpers.
//!
//! These helpers define the stable bytes signed by a package launcher using the
//! package publisher's key. For unpublished package registration, Issuer
//! verifies this signature with the same Publisher key that signed the package
//! manifest.

use crate::ActrType;

pub const PUBLISHER_REGISTER_DOMAIN: &str = "ACTR-PUBLISHER-REGISTER-V1";

#[derive(Debug, Clone, Copy)]
pub struct PublisherRegisterPayload<'a> {
    /// Realm name `<realm_id>@<domain>`.
    pub realm: &'a str,
    pub actr_type: &'a ActrType,
    pub target: &'a str,
    pub manifest_sha256_hex: &'a str,
    pub publisher_auth_signed_at: u64,
    pub publisher_auth_nonce: &'a [u8],
}

pub fn build_publisher_register_payload(input: PublisherRegisterPayload<'_>) -> String {
    format!(
        "{domain}\n\
         auth_mode=package\n\
         realm={realm}\n\
         actr_type={actr_type}\n\
         target={target}\n\
         manifest_sha256={manifest_sha256}\n\
         publisher_auth_signed_at={publisher_auth_signed_at}\n\
         publisher_auth_nonce={publisher_auth_nonce}",
        domain = PUBLISHER_REGISTER_DOMAIN,
        realm = input.realm,
        actr_type = input.actr_type.to_string_repr(),
        target = input.target,
        manifest_sha256 = input.manifest_sha256_hex,
        publisher_auth_signed_at = input.publisher_auth_signed_at,
        publisher_auth_nonce = hex::encode(input.publisher_auth_nonce),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn publisher_register_payload_is_stable() {
        let actr_type = ActrType {
            publisher: "acme".to_string(),
            name: "echo".to_string(),
            version: "1.2.3".to_string(),
        };

        let payload = build_publisher_register_payload(PublisherRegisterPayload {
            realm: "k7m2q9xh@example.com",
            actr_type: &actr_type,
            target: "x86_64-unknown-linux-gnu",
            manifest_sha256_hex: "abc123",
            publisher_auth_signed_at: 1_782_200_000,
            publisher_auth_nonce: &[0xab, 0xcd],
        });

        assert_eq!(
            payload,
            "ACTR-PUBLISHER-REGISTER-V1\n\
             auth_mode=package\n\
             realm=k7m2q9xh@example.com\n\
             actr_type=acme/echo@1.2.3\n\
             target=x86_64-unknown-linux-gnu\n\
             manifest_sha256=abc123\n\
             publisher_auth_signed_at=1782200000\n\
             publisher_auth_nonce=abcd"
        );
    }
}
