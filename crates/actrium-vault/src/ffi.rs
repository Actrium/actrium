use crate::crypto;
pub use crate::model::VaultCoreError;
use crate::model::{VaultField, VaultItemCategory};
use crate::password;
pub use crate::password::PasswordStrengthLevel;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VaultFieldInfo {
    pub id: String,
    pub label: String,
    pub value: String,
    pub is_sensitive: bool,
}

impl From<VaultFieldInfo> for VaultField {
    fn from(field: VaultFieldInfo) -> Self {
        Self {
            id: field.id,
            label: field.label,
            value: field.value,
            is_sensitive: field.is_sensitive,
        }
    }
}

impl From<VaultField> for VaultFieldInfo {
    fn from(field: VaultField) -> Self {
        Self {
            id: field.id,
            label: field.label,
            value: field.value,
            is_sensitive: field.is_sensitive,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VaultFieldTemplateInfo {
    pub label: String,
    pub is_sensitive: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VaultCategoryInfo {
    pub kind: String,
    pub label: String,
    pub template: Vec<VaultFieldTemplateInfo>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PasswordStrengthInfo {
    pub level: PasswordStrengthLevel,
    pub meter_level: u32,
    pub score: u32,
    pub label: String,
}

pub fn vault_pbkdf2_iterations() -> u32 {
    crypto::PBKDF2_ITERATIONS
}

pub fn vault_key_byte_length() -> u32 {
    crypto::KEY_BYTE_LEN as u32
}

pub fn vault_salt_byte_length() -> u32 {
    crypto::SALT_BYTE_LEN as u32
}

pub fn vault_nonce_byte_length() -> u32 {
    crypto::NONCE_BYTE_LEN as u32
}

pub fn vault_verification_token() -> String {
    crypto::VERIFICATION_TOKEN.to_string()
}

pub fn generate_vault_salt_base64() -> String {
    crypto::generate_salt_base64()
}

pub fn derive_vault_key_base64(
    password: String,
    salt_base64: String,
) -> Result<String, VaultCoreError> {
    crypto::derive_key_base64(&password, &salt_base64)
}

pub fn create_vault_verification_hash(
    password: String,
    salt_base64: String,
) -> Result<String, VaultCoreError> {
    crypto::create_verification_hash(&password, &salt_base64)
}

pub fn verify_vault_password(
    password: String,
    salt_base64: String,
    verification_hash_base64: String,
) -> Result<bool, VaultCoreError> {
    crypto::verify_password(&password, &salt_base64, &verification_hash_base64)
}

pub fn encrypt_vault_string(
    key_base64: String,
    plaintext: String,
) -> Result<String, VaultCoreError> {
    let key = crypto::decode_key_base64(&key_base64)?;
    crypto::encrypt_base64(&key, &plaintext)
}

pub fn decrypt_vault_string(
    key_base64: String,
    ciphertext_base64: String,
) -> Result<String, VaultCoreError> {
    let key = crypto::decode_key_base64(&key_base64)?;
    crypto::decrypt_base64(&key, &ciphertext_base64)
}

pub fn encrypt_vault_fields(
    key_base64: String,
    fields: Vec<VaultFieldInfo>,
) -> Result<String, VaultCoreError> {
    let key = crypto::decode_key_base64(&key_base64)?;
    let fields: Vec<VaultField> = fields.into_iter().map(VaultField::from).collect();
    crypto::encrypt_fields_base64(&key, &fields)
}

pub fn decrypt_vault_fields(
    key_base64: String,
    ciphertext_base64: String,
) -> Result<Vec<VaultFieldInfo>, VaultCoreError> {
    let key = crypto::decode_key_base64(&key_base64)?;
    crypto::decrypt_fields_base64(&key, &ciphertext_base64)
        .map(|fields| fields.into_iter().map(VaultFieldInfo::from).collect())
}

pub fn vault_password_strength(password: String) -> PasswordStrengthInfo {
    let strength = password::evaluate_password_strength(&password);
    PasswordStrengthInfo {
        level: strength.level,
        meter_level: strength.meter_level,
        score: strength.score,
        label: strength.label,
    }
}

pub fn parse_vault_category(raw: String) -> Option<String> {
    VaultItemCategory::try_from(raw.as_str())
        .ok()
        .map(|category| category.as_str().to_string())
}

pub fn vault_category_info(raw: String) -> Result<VaultCategoryInfo, VaultCoreError> {
    let category = VaultItemCategory::try_from(raw.as_str())?;
    Ok(category_info(category))
}

pub fn vault_category_templates() -> Vec<VaultCategoryInfo> {
    VaultItemCategory::ALL
        .into_iter()
        .map(category_info)
        .collect()
}

fn category_info(category: VaultItemCategory) -> VaultCategoryInfo {
    VaultCategoryInfo {
        kind: category.as_str().to_string(),
        label: category.label().to_string(),
        template: category
            .template()
            .into_iter()
            .map(|field| VaultFieldTemplateInfo {
                label: field.label,
                is_sensitive: field.is_sensitive,
            })
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use base64::{Engine as _, engine::general_purpose::STANDARD};

    use super::*;
    use crate::crypto::{KEY_BYTE_LEN, SALT_BYTE_LEN};

    #[test]
    fn ffi_derive_key_returns_base64_32_byte_key() {
        let salt = STANDARD.encode([8_u8; SALT_BYTE_LEN]);
        let key = derive_vault_key_base64("secret".to_string(), salt).unwrap();
        assert_eq!(STANDARD.decode(key).unwrap().len(), KEY_BYTE_LEN);
    }

    #[test]
    fn ffi_encrypt_decrypt_string_roundtrips() {
        let key = STANDARD.encode([1_u8; KEY_BYTE_LEN]);
        let encrypted = encrypt_vault_string(key.clone(), "hello vault".to_string()).unwrap();
        assert_eq!(decrypt_vault_string(key, encrypted).unwrap(), "hello vault");
    }

    #[test]
    fn ffi_fields_roundtrip() {
        let key = STANDARD.encode([2_u8; KEY_BYTE_LEN]);
        let fields = vec![VaultFieldInfo {
            id: "field-1".to_string(),
            label: "Username".to_string(),
            value: "leo".to_string(),
            is_sensitive: false,
        }];

        let encrypted = encrypt_vault_fields(key.clone(), fields.clone()).unwrap();
        assert_eq!(decrypt_vault_fields(key, encrypted).unwrap(), fields);
    }

    #[test]
    fn ffi_category_templates_are_canonical() {
        let templates = vault_category_templates();
        assert_eq!(templates.len(), 4);
        assert_eq!(templates[1].kind, "sshKey");
        assert_eq!(templates[1].template[3].label, "Private Key");
    }
}
