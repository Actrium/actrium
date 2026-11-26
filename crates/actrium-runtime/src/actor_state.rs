use std::fmt::Write;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Fingerprints measured by the Service Host for one completed workload start.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActorStateFingerprints {
    pub package: [u8; 32],
    pub instance_configuration: [u8; 32],
    pub effective_capabilities: [u8; 32],
}

impl ActorStateFingerprints {
    /// Hash the versioned state encoding documented in the Actor state profile.
    pub fn digest(&self, actor_id: &str, revision: u64) -> [u8; 32] {
        let fields = [
            "actrium.actor-state/1".to_owned(),
            actor_id.to_owned(),
            revision.to_string(),
            hex_digest(&self.package),
            hex_digest(&self.instance_configuration),
            hex_digest(&self.effective_capabilities),
        ];
        let encoded = serde_json::to_vec(&fields).expect("serializing JSON strings cannot fail");
        Sha256::digest(encoded).into()
    }
}

fn hex_digest(bytes: &[u8; 32]) -> String {
    let mut encoded = String::with_capacity(64);
    for byte in bytes {
        write!(encoded, "{byte:02x}").expect("writing to a String cannot fail");
    }
    encoded
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct ActorStateCommit {
    pub(crate) revision: u64,
    pub(crate) fingerprints: ActorStateFingerprints,
    pub(crate) digest: [u8; 32],
    pub(crate) running_instance: Option<String>,
}

impl ActorStateCommit {
    pub(crate) fn is_valid_for(&self, actor_id: &str) -> bool {
        self.revision != 0 && self.digest == self.fingerprints.digest(actor_id, self.revision)
    }
}
