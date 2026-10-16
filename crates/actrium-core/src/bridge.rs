use serde::{Deserialize, Serialize};

use crate::domain::TabStatus;

pub mod actr_bridge_contract {
    pub const MODULE_NAME: &str = "ActrBridge";
    pub const STREAM_EVENT_NAME: &str = "ActrStreamMessage";

    pub const STATUS_CONNECTED: &str = "connected";
    pub const STREAM_TYPE_TEXT: &str = "text";
    pub const STREAM_TYPE_VOICE: &str = "voice";
    pub const STREAM_TYPE_ATTACHMENT_RESULT: &str = "attachmentResult";
    pub const STREAM_STATUS_OK: i32 = 0;
    pub const STREAM_STATUS_ERROR: i32 = -1;
    pub const STREAM_METADATA_EOS_KEY: &str = "eos";
    pub const STREAM_METADATA_AOS_KEY: &str = "aos";
    pub const STREAM_METADATA_TERMINAL_VALUE: &str = "true";

    pub const ATTACHMENT_RESULT_TIMEOUT_MS: u32 = 30_000;
    pub const ATTACHMENT_CHUNK_SIZE_BYTES: u32 = 48 * 1024;
    pub const ATTACHMENT_CHECKSUM_ALGORITHM_MD5: &str = "md5";
    pub const ATTACHMENT_STATUS_CODE_OK: i32 = 0;
    pub const ATTACHMENT_TYPE_UNKNOWN: i32 = 0;
    pub const ATTACHMENT_TYPE_IMAGE: i32 = 1;
    pub const ATTACHMENT_TYPE_DOCUMENT: i32 = 2;
    pub const ATTACHMENT_TYPE_AUDIO: i32 = 3;
    pub const ATTACHMENT_TYPE_VIDEO: i32 = 4;
    pub const ATTACHMENT_TYPE_OTHER: i32 = 99;
    pub const ATTACHMENT_UPLOAD_STATUS_UNKNOWN: i32 = 0;
    pub const ATTACHMENT_UPLOAD_STATUS_COMPLETED: i32 = 1;
    pub const ATTACHMENT_UPLOAD_STATUS_FAILED: i32 = 2;
    pub const ASK_SERVICE_PROMPT_ROUTE_KEY: &str = "ask.AskService.Prompt";
    pub const ASK_SERVICE_ATTACH_ROUTE_KEY: &str = "ask.AskService.Attach";
    pub const ASK_SERVICE_PREPARE_ATTACHMENT_UPLOAD_ROUTE_KEY: &str =
        "ask.AskService.PrepareAttachmentUpload";
    pub const ASK_LOCAL_SERVICE_SEND_TEXT_ROUTE_KEY: &str = "actrium.AskLocalService.SendText";
    pub const ASK_LOCAL_SERVICE_START_SEND_VOICE_ROUTE_KEY: &str =
        "actrium.AskLocalService.StartSendVoice";
    pub const ASK_LOCAL_SERVICE_SEND_VOICE_CHUNK_ROUTE_KEY: &str =
        "actrium.AskLocalService.SendVoiceChunk";
    pub const IOS_ASK_SERVICE_PROMPT_TIMEOUT_MS: u32 = 20_000;
    pub const IOS_ASK_SERVICE_PREPARE_ATTACHMENT_UPLOAD_TIMEOUT_MS: u32 = 20_000;
    pub const ANDROID_ASK_SERVICE_PROMPT_TIMEOUT_MS: u32 = 300_000;
    pub const ANDROID_ASK_SERVICE_ATTACH_TIMEOUT_MS: u32 = 30_000;
    pub const ANDROID_ASK_LOCAL_SERVICE_RPC_TIMEOUT_MS: u32 = 30_000;

    pub const ERROR_INVALID_INPUT: &str = "INVALID_INPUT";
    pub const ERROR_UNKNOWN: &str = "UNKNOWN_ERROR";
    pub const ERROR_PROMPT_REQUEST: &str = "USR_PROMPT_REQUEST_ERROR";
    pub const ERROR_NOT_CONNECTED: &str = "NOT_CONNECTED";
    pub const ERROR_ACTR: &str = "ACTR_ERROR";
    pub const ERROR_GENERIC: &str = "ERROR";

    pub mod key {
        pub const TAB_ID: &str = "tabId";
        pub const ACTOR_TYPE: &str = "actorType";
        pub const PATH: &str = "path";
        pub const STATUS: &str = "status";
        pub const MESSAGE_ID: &str = "messageId";
        pub const SESSION_ID: &str = "sessionId";
        pub const TEXT: &str = "text";
        pub const MESSAGE: &str = "message";
        pub const STREAM_ID: &str = "streamId";
        pub const STREAM_ID_UPPER: &str = "streamID";
        pub const TEXT_RESPONSE_STREAM_ID: &str = "textResponseStreamId";
        pub const VOICE_RESPONSE_STREAM_ID: &str = "voiceResponseStreamId";
        pub const ATTACHMENT_IDS: &str = "attachmentIds";
        pub const LOCATION: &str = "location";
        pub const LATITUDE: &str = "latitude";
        pub const LONGITUDE: &str = "longitude";
        pub const ADDRESS: &str = "address";
        pub const PLACE_NAME: &str = "placeName";
        pub const FILE_DATA: &str = "fileData";
        pub const ATTACHMENT_ID: &str = "attachmentId";
        pub const CHECKSUM: &str = "checksum";
        pub const CHECKSUM_ALGORITHM: &str = "checksumAlgorithm";
        pub const SIZE_BYTES: &str = "sizeBytes";
        pub const RESULT_STREAM_ID: &str = "resultStreamId";
        pub const ID: &str = "id";
        pub const FILENAME: &str = "filename";
        pub const TYPE: &str = "type";
        pub const STATUS_CODE: &str = "statusCode";
        pub const ERROR_MESSAGE: &str = "errorMessage";
        pub const QUESTION_ID: &str = "questionId";
        pub const CHUNK_ID: &str = "chunkId";
        pub const METADATA: &str = "metadata";
        pub const METADATA_KEY: &str = "key";
        pub const METADATA_VALUE: &str = "value";
        pub const SEQUENCE: &str = "sequence";
        pub const TIMESTAMP_MS: &str = "timestampMs";
    }
}

pub mod email_bridge_contract {
    pub const MODULE_NAME: &str = "EmailModule";
    pub const METHOD_COMPOSE_EMAIL: &str = "composeEmail";
    pub const URI_SCHEME_MAILTO: &str = "mailto:";
    pub const ERROR_INVALID_RECIPIENTS: &str = "EMAIL_INVALID_RECIPIENTS";
    pub const ERROR_OPEN_FAILED: &str = "EMAIL_OPEN_FAILED";
}

pub mod user_info_bridge_contract {
    pub const MODULE_NAME: &str = "UserInfoModule";
    pub const METHOD_GET_CURRENT_USER_INFO: &str = "getCurrentUserInfo";
    pub const METHOD_LOGIN_WITH_TYPE: &str = "loginWithType";
    pub const EVENT_USER_INFO_CHANGED: &str = "onUserInfoChanged";
    pub const EVENT_LOGIN_STATUS_CHANGED: &str = "onLoginStatusChanged";
    pub const EVENT_USER_LOGOUT: &str = "onUserLogout";

    pub mod key {
        pub const ID: &str = "id";
        pub const LOGIN_API_URL: &str = "loginApiUrl";
        pub const USER_INFO: &str = "userInfo";
        pub const USER_IDENTITY: &str = "userIdentity";
        pub const SESSION_TOKEN: &str = "sessionToken";
        pub const CREATED_AT: &str = "createdAt";
        pub const UPDATED_AT: &str = "updatedAt";
        pub const REQUEST_CONTEXT: &str = "requestContext";
        pub const IS_LOGGED_IN: &str = "isLoggedIn";
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ActrActorType {
    pub manufacturer: String,
    pub name: String,
    #[serde(default)]
    pub version: String,
}

pub fn parse_actr_actor_type(raw: &str) -> Option<ActrActorType> {
    let trimmed = raw.trim();
    let mut parts = trimmed.split('+');
    let manufacturer = parts.next()?.trim();
    let name = parts.next()?.trim();

    if parts.next().is_some() || manufacturer.is_empty() || name.is_empty() {
        return None;
    }

    Some(ActrActorType {
        manufacturer: manufacturer.to_string(),
        name: name.to_string(),
        version: String::new(),
    })
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenTabRequest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tab_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub app_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub system_app_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub system_app_payload: Option<serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenTabResponse {
    pub tab_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TabSummary {
    pub tab_id: String,
    pub title: String,
    #[serde(default)]
    pub icon: String,
    #[serde(default)]
    pub app_url: String,
    #[serde(default)]
    pub status: TabStatus,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateTabInfoRequest {
    pub tab_id: String,
    pub title: String,
    #[serde(default)]
    pub icon: String,
    #[serde(default)]
    pub app_url: String,
    #[serde(default = "default_alive_status")]
    pub status: TabStatus,
}

fn default_alive_status() -> TabStatus {
    TabStatus::Alive
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TabFullscreenEvent {
    pub tab_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    pub timestamp_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TabUnreadStatusEvent {
    pub tab_id: String,
    pub unread: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TabBookmarkEvent {
    pub tab_id: String,
    pub is_bookmarked: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tab_summary_uses_existing_js_wire_names() {
        let summary = TabSummary {
            tab_id: "t1".to_string(),
            title: "Demo".to_string(),
            icon: "star".to_string(),
            app_url: "https://example.test".to_string(),
            status: TabStatus::Alive,
        };

        let json = serde_json::to_value(summary).unwrap();
        assert_eq!(json["tabId"], "t1");
        assert_eq!(json["appUrl"], "https://example.test");
        assert_eq!(json["status"], "alive");
    }

    #[test]
    fn update_request_defaults_to_alive_status() {
        let request: UpdateTabInfoRequest =
            serde_json::from_str(r#"{"tabId":"t1","title":"Demo"}"#).unwrap();
        assert_eq!(request.status, TabStatus::Alive);
        assert_eq!(request.icon, "");
    }

    #[test]
    fn open_tab_request_keeps_system_app_payload_shape() {
        let request: OpenTabRequest = serde_json::from_str(
            r#"{"systemAppType":"settings","systemAppPayload":{"page":"about"}}"#,
        )
        .unwrap();

        assert_eq!(request.system_app_type.as_deref(), Some("settings"));
        assert_eq!(request.system_app_payload.unwrap()["page"], "about");
    }

    #[test]
    fn actr_bridge_contract_uses_actrium_local_service_routes() {
        use actr_bridge_contract as contract;

        assert_eq!(contract::MODULE_NAME, "ActrBridge");
        assert_eq!(contract::STREAM_EVENT_NAME, "ActrStreamMessage");
        assert_eq!(contract::STREAM_TYPE_TEXT, "text");
        assert_eq!(contract::STREAM_TYPE_VOICE, "voice");
        assert_eq!(contract::STREAM_TYPE_ATTACHMENT_RESULT, "attachmentResult");
        assert_eq!(contract::STREAM_STATUS_OK, 0);
        assert_eq!(contract::STREAM_STATUS_ERROR, -1);
        assert_eq!(contract::STREAM_METADATA_EOS_KEY, "eos");
        assert_eq!(contract::STREAM_METADATA_AOS_KEY, "aos");
        assert_eq!(contract::STREAM_METADATA_TERMINAL_VALUE, "true");
        assert_eq!(contract::ATTACHMENT_RESULT_TIMEOUT_MS, 30_000);
        assert_eq!(contract::ATTACHMENT_CHUNK_SIZE_BYTES, 49_152);
        assert_eq!(contract::ATTACHMENT_CHECKSUM_ALGORITHM_MD5, "md5");
        assert_eq!(contract::ATTACHMENT_STATUS_CODE_OK, 0);
        assert_eq!(contract::ATTACHMENT_TYPE_UNKNOWN, 0);
        assert_eq!(contract::ATTACHMENT_TYPE_IMAGE, 1);
        assert_eq!(contract::ATTACHMENT_TYPE_DOCUMENT, 2);
        assert_eq!(contract::ATTACHMENT_TYPE_AUDIO, 3);
        assert_eq!(contract::ATTACHMENT_TYPE_VIDEO, 4);
        assert_eq!(contract::ATTACHMENT_TYPE_OTHER, 99);
        assert_eq!(contract::ATTACHMENT_UPLOAD_STATUS_UNKNOWN, 0);
        assert_eq!(contract::ATTACHMENT_UPLOAD_STATUS_COMPLETED, 1);
        assert_eq!(contract::ATTACHMENT_UPLOAD_STATUS_FAILED, 2);
        assert_eq!(
            contract::ASK_SERVICE_PROMPT_ROUTE_KEY,
            "ask.AskService.Prompt"
        );
        assert_eq!(
            contract::ASK_SERVICE_ATTACH_ROUTE_KEY,
            "ask.AskService.Attach"
        );
        assert_eq!(
            contract::ASK_SERVICE_PREPARE_ATTACHMENT_UPLOAD_ROUTE_KEY,
            "ask.AskService.PrepareAttachmentUpload"
        );
        assert_eq!(
            contract::ASK_LOCAL_SERVICE_SEND_TEXT_ROUTE_KEY,
            "actrium.AskLocalService.SendText"
        );
        assert_eq!(
            contract::ASK_LOCAL_SERVICE_START_SEND_VOICE_ROUTE_KEY,
            "actrium.AskLocalService.StartSendVoice"
        );
        assert_eq!(
            contract::ASK_LOCAL_SERVICE_SEND_VOICE_CHUNK_ROUTE_KEY,
            "actrium.AskLocalService.SendVoiceChunk"
        );
        assert_eq!(contract::IOS_ASK_SERVICE_PROMPT_TIMEOUT_MS, 20_000);
        assert_eq!(
            contract::IOS_ASK_SERVICE_PREPARE_ATTACHMENT_UPLOAD_TIMEOUT_MS,
            20_000
        );
        assert_eq!(contract::ANDROID_ASK_SERVICE_PROMPT_TIMEOUT_MS, 300_000);
        assert_eq!(contract::ANDROID_ASK_SERVICE_ATTACH_TIMEOUT_MS, 30_000);
        assert_eq!(contract::ANDROID_ASK_LOCAL_SERVICE_RPC_TIMEOUT_MS, 30_000);
        assert_eq!(contract::key::TAB_ID, "tabId");
        assert_eq!(contract::key::STREAM_ID, "streamId");
        assert_eq!(contract::key::STREAM_ID_UPPER, "streamID");
        assert_eq!(
            contract::key::TEXT_RESPONSE_STREAM_ID,
            "textResponseStreamId"
        );
        assert_eq!(
            contract::key::VOICE_RESPONSE_STREAM_ID,
            "voiceResponseStreamId"
        );
        assert_eq!(contract::key::ATTACHMENT_ID, "attachmentId");
        assert_eq!(contract::key::CHECKSUM, "checksum");
        assert_eq!(contract::key::CHECKSUM_ALGORITHM, "checksumAlgorithm");
        assert_eq!(contract::key::SIZE_BYTES, "sizeBytes");
        assert_eq!(contract::key::RESULT_STREAM_ID, "resultStreamId");
        assert_eq!(contract::ERROR_PROMPT_REQUEST, "USR_PROMPT_REQUEST_ERROR");
    }

    #[test]
    fn email_bridge_contract_keeps_existing_android_and_rn_wire_names() {
        use email_bridge_contract as contract;

        assert_eq!(contract::MODULE_NAME, "EmailModule");
        assert_eq!(contract::METHOD_COMPOSE_EMAIL, "composeEmail");
        assert_eq!(contract::URI_SCHEME_MAILTO, "mailto:");
        assert_eq!(
            contract::ERROR_INVALID_RECIPIENTS,
            "EMAIL_INVALID_RECIPIENTS"
        );
        assert_eq!(contract::ERROR_OPEN_FAILED, "EMAIL_OPEN_FAILED");
    }

    #[test]
    fn user_info_bridge_contract_keeps_existing_native_and_rn_wire_names() {
        use user_info_bridge_contract as contract;
        use user_info_bridge_contract::key;

        assert_eq!(contract::MODULE_NAME, "UserInfoModule");
        assert_eq!(contract::METHOD_GET_CURRENT_USER_INFO, "getCurrentUserInfo");
        assert_eq!(contract::METHOD_LOGIN_WITH_TYPE, "loginWithType");
        assert_eq!(contract::EVENT_USER_INFO_CHANGED, "onUserInfoChanged");
        assert_eq!(contract::EVENT_LOGIN_STATUS_CHANGED, "onLoginStatusChanged");
        assert_eq!(contract::EVENT_USER_LOGOUT, "onUserLogout");
        assert_eq!(key::ID, "id");
        assert_eq!(key::LOGIN_API_URL, "loginApiUrl");
        assert_eq!(key::USER_INFO, "userInfo");
        assert_eq!(key::USER_IDENTITY, "userIdentity");
        assert_eq!(key::SESSION_TOKEN, "sessionToken");
        assert_eq!(key::CREATED_AT, "createdAt");
        assert_eq!(key::UPDATED_AT, "updatedAt");
        assert_eq!(key::REQUEST_CONTEXT, "requestContext");
        assert_eq!(key::IS_LOGGED_IN, "isLoggedIn");
    }

    #[test]
    fn parse_actr_actor_type_splits_existing_wire_format() {
        let actor_type = parse_actr_actor_type(" actrium + app_server ").unwrap();
        assert_eq!(actor_type.manufacturer, "actrium");
        assert_eq!(actor_type.name, "app_server");
        assert_eq!(actor_type.version, "");
    }

    #[test]
    fn parse_actr_actor_type_rejects_ambiguous_or_empty_values() {
        assert!(parse_actr_actor_type("").is_none());
        assert!(parse_actr_actor_type("actrium").is_none());
        assert!(parse_actr_actor_type("+server").is_none());
        assert!(parse_actr_actor_type("actrium+").is_none());
        assert!(parse_actr_actor_type("actrium+server+extra").is_none());
    }
}
