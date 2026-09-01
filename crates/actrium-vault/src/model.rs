use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VaultField {
    pub id: String,
    pub label: String,
    pub value: String,
    pub is_sensitive: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VaultItemCategory {
    Login,
    SshKey,
    ApiKey,
    SecureNote,
}

impl VaultItemCategory {
    pub const ALL: [Self; 4] = [Self::Login, Self::SshKey, Self::ApiKey, Self::SecureNote];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Login => "login",
            Self::SshKey => "sshKey",
            Self::ApiKey => "apiKey",
            Self::SecureNote => "secureNote",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Login => "Login",
            Self::SshKey => "SSH Key",
            Self::ApiKey => "API Key",
            Self::SecureNote => "Secure Note",
        }
    }

    pub fn template(self) -> Vec<VaultFieldTemplate> {
        match self {
            Self::Login => vec![
                VaultFieldTemplate::new("URL", false),
                VaultFieldTemplate::new("Username", false),
                VaultFieldTemplate::new("Password", true),
            ],
            Self::SshKey => vec![
                VaultFieldTemplate::new("Host", false),
                VaultFieldTemplate::new("Port", false),
                VaultFieldTemplate::new("Username", false),
                VaultFieldTemplate::new("Private Key", true),
            ],
            Self::ApiKey => vec![
                VaultFieldTemplate::new("Service", false),
                VaultFieldTemplate::new("API Key", true),
                VaultFieldTemplate::new("API Secret", true),
            ],
            Self::SecureNote => vec![VaultFieldTemplate::new("Content", true)],
        }
    }
}

impl TryFrom<&str> for VaultItemCategory {
    type Error = VaultCoreError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "login" => Ok(Self::Login),
            "sshKey" => Ok(Self::SshKey),
            "apiKey" => Ok(Self::ApiKey),
            "secureNote" => Ok(Self::SecureNote),
            _ => Err(VaultCoreError::UnknownCategory),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VaultFieldTemplate {
    pub label: String,
    pub is_sensitive: bool,
}

impl VaultFieldTemplate {
    fn new(label: &str, is_sensitive: bool) -> Self {
        Self {
            label: label.to_string(),
            is_sensitive,
        }
    }
}

#[derive(Debug, Clone, Error)]
pub enum VaultCoreError {
    #[error("value is not valid base64")]
    InvalidBase64,
    #[error("vault key must be 32 bytes")]
    InvalidKey,
    #[error("vault salt must be 32 bytes")]
    InvalidSalt,
    #[error("AES-GCM nonce must be 12 bytes")]
    InvalidNonce,
    #[error("ciphertext does not match the Actro vault format")]
    InvalidCiphertext,
    #[error("encryption failed")]
    EncryptionFailed,
    #[error("decryption failed")]
    DecryptionFailed,
    #[error("decrypted value is not UTF-8 text")]
    InvalidUtf8,
    #[error("vault field JSON is invalid")]
    InvalidJson,
    #[error("unknown vault item category")]
    UnknownCategory,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn category_wire_values_match_current_swift() {
        assert_eq!(VaultItemCategory::Login.as_str(), "login");
        assert_eq!(VaultItemCategory::SshKey.as_str(), "sshKey");
        assert_eq!(VaultItemCategory::ApiKey.as_str(), "apiKey");
        assert_eq!(VaultItemCategory::SecureNote.as_str(), "secureNote");
    }

    #[test]
    fn category_templates_match_current_swift() {
        let login = VaultItemCategory::Login.template();
        assert_eq!(login[0].label, "URL");
        assert!(!login[0].is_sensitive);
        assert_eq!(login[2].label, "Password");
        assert!(login[2].is_sensitive);
    }
}
