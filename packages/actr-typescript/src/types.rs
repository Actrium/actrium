use napi::bindgen_prelude::*;
use napi_derive::napi;

// Realm
#[napi(object)]
pub struct Realm {
    #[napi(js_name = "realmId")]
    pub realm_id: u32,
}

impl From<actr_protocol::Realm> for Realm {
    fn from(realm: actr_protocol::Realm) -> Self {
        Self {
            realm_id: realm.realm_id,
        }
    }
}

impl From<Realm> for actr_protocol::Realm {
    fn from(realm: Realm) -> Self {
        Self {
            realm_id: realm.realm_id,
        }
    }
}

// ActrType
#[napi(object)]
pub struct ActrType {
    pub manufacturer: String,
    pub name: String,
    pub version: String,
}

impl From<actr_protocol::ActrType> for ActrType {
    fn from(t: actr_protocol::ActrType) -> Self {
        Self {
            manufacturer: t.manufacturer,
            name: t.name,
            version: t.version,
        }
    }
}

impl From<ActrType> for actr_protocol::ActrType {
    fn from(t: ActrType) -> Self {
        Self {
            manufacturer: t.manufacturer,
            name: t.name,
            version: t.version,
        }
    }
}

// ActrId
#[napi(object)]
pub struct ActrId {
    pub realm: Realm,
    #[napi(js_name = "serialNumber")]
    pub serial_number: BigInt,
    pub r#type: ActrType,
}

impl From<actr_protocol::ActrId> for ActrId {
    fn from(id: actr_protocol::ActrId) -> Self {
        Self {
            realm: id.realm.into(),
            serial_number: id.serial_number.into(),
            r#type: id.r#type.into(),
        }
    }
}

impl TryFrom<ActrId> for actr_protocol::ActrId {
    type Error = Error;

    fn try_from(id: ActrId) -> Result<Self> {
        let (negative, serial_number, lossless) = id.serial_number.get_u64();
        if negative || !lossless {
            return Err(Error::new(
                Status::InvalidArg,
                "serialNumber must be an unsigned 64-bit bigint",
            ));
        }
        Ok(Self {
            realm: id.realm.into(),
            serial_number,
            r#type: id.r#type.into(),
        })
    }
}

// PayloadType
#[napi]
pub enum PayloadType {
    RpcReliable,
    RpcSignal,
    StreamReliable,
    StreamLatencyFirst,
    MediaRtp,
}

impl From<actr_protocol::PayloadType> for PayloadType {
    fn from(t: actr_protocol::PayloadType) -> Self {
        match t {
            actr_protocol::PayloadType::RpcReliable => PayloadType::RpcReliable,
            actr_protocol::PayloadType::RpcSignal => PayloadType::RpcSignal,
            actr_protocol::PayloadType::StreamReliable => PayloadType::StreamReliable,
            actr_protocol::PayloadType::StreamLatencyFirst => PayloadType::StreamLatencyFirst,
            actr_protocol::PayloadType::MediaRtp => PayloadType::MediaRtp,
        }
    }
}

impl From<PayloadType> for actr_protocol::PayloadType {
    fn from(t: PayloadType) -> Self {
        match t {
            PayloadType::RpcReliable => actr_protocol::PayloadType::RpcReliable,
            PayloadType::RpcSignal => actr_protocol::PayloadType::RpcSignal,
            PayloadType::StreamReliable => actr_protocol::PayloadType::StreamReliable,
            PayloadType::StreamLatencyFirst => actr_protocol::PayloadType::StreamLatencyFirst,
            PayloadType::MediaRtp => actr_protocol::PayloadType::MediaRtp,
        }
    }
}

// MetadataEntry
#[napi(object)]
pub struct MetadataEntry {
    pub key: String,
    pub value: String,
}

// DataChunk
#[napi(object)]
pub struct DataChunk {
    #[napi(js_name = "streamId")]
    pub stream_id: String,
    pub sequence: i64,
    pub payload: Buffer,
    pub metadata: Vec<MetadataEntry>,
    #[napi(js_name = "timestampMs")]
    pub timestamp_ms: Option<i64>,
}

#[napi(object)]
pub struct StreamSignal {
    pub chunk: DataChunk,
    pub sender: ActrId,
}

impl From<actr_protocol::DataChunk> for DataChunk {
    fn from(stream: actr_protocol::DataChunk) -> Self {
        Self {
            stream_id: stream.stream_id,
            sequence: stream.sequence as i64,
            payload: stream.payload.to_vec().into(),
            metadata: stream
                .metadata
                .into_iter()
                .map(|e| MetadataEntry {
                    key: e.key,
                    value: e.value,
                })
                .collect(),
            timestamp_ms: stream.timestamp_ms,
        }
    }
}

impl From<DataChunk> for actr_protocol::DataChunk {
    fn from(stream: DataChunk) -> Self {
        Self {
            stream_id: stream.stream_id,
            sequence: stream.sequence as u64,
            payload: bytes::Bytes::from(stream.payload.to_vec()),
            metadata: stream
                .metadata
                .into_iter()
                .map(|e| actr_protocol::MetadataEntry {
                    key: e.key,
                    value: e.value,
                })
                .collect(),
            timestamp_ms: stream.timestamp_ms,
        }
    }
}

// RpcEnvelopeBridge
#[napi(object)]
pub struct RpcEnvelopeBridge {
    #[napi(js_name = "routeKey")]
    pub route_key: String,
    pub payload: Buffer,
    #[napi(js_name = "requestId")]
    pub request_id: String,
    #[napi(js_name = "timeoutMs")]
    pub timeout_ms: i64,
}

impl From<actr_protocol::RpcEnvelope> for RpcEnvelopeBridge {
    fn from(envelope: actr_protocol::RpcEnvelope) -> Self {
        Self {
            route_key: envelope.route_key,
            payload: envelope
                .payload
                .map(|p| p.to_vec().into())
                .unwrap_or_else(|| Buffer::from(vec![])),
            request_id: envelope.request_id,
            timeout_ms: envelope.timeout_ms,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn actor_id(serial_number: u64) -> actr_protocol::ActrId {
        actr_protocol::ActrId {
            realm: actr_protocol::Realm { realm_id: 1 },
            serial_number,
            r#type: actr_protocol::ActrType {
                manufacturer: "test".into(),
                name: "Binding".into(),
                version: "1.0.0".into(),
            },
        }
    }

    #[test]
    fn actor_ids_round_trip_without_number_precision_loss() {
        for serial in [0, (1_u64 << 53) + 1, i64::MAX as u64 + 1, u64::MAX] {
            let original = actor_id(serial);
            let binding: ActrId = original.clone().into();
            let restored = actr_protocol::ActrId::try_from(binding).unwrap();
            assert_eq!(restored, original);
        }
    }

    #[test]
    fn rejects_negative_and_overflowing_actor_ids() {
        for serial in [BigInt::from(-1_i64), BigInt::from(1_u128 << 64)] {
            let mut binding: ActrId = actor_id(0).into();
            binding.serial_number = serial;
            let error = actr_protocol::ActrId::try_from(binding).unwrap_err();
            assert_eq!(error.status, Status::InvalidArg);
        }
    }
}
