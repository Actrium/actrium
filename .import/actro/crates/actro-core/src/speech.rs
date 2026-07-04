const ZH_LANGUAGE_CODES: &[&str] = &["zh-CN", "zh-TW", "zh-Hans", "zh-Hant", "zh"];
const ASR_MODEL_BASE_DIR: &str = "model/asr";
const ASR_MODEL_READY_MARKER: &str = ".model_ready";
const QUESTION_INDICATORS: &[&str] = &[
    "吗",
    "呢",
    "吧",
    "怎么样",
    "如何",
    "什么",
    "为什么",
    "哪",
    "谁",
    "多少",
    "几",
    "怎么",
    "是不是",
    "有没有",
    "能不能",
    "会不会",
    "难道",
    "何时",
    "哪里",
];
const EXCLAMATION_INDICATORS: &[&str] = &["啊", "呀", "哇", "哎", "嘿", "哟"];
const COMMA_AFTER_NATURES: &[&str] = &["t", "nt", "ns", "nd", "nz", "ni"];
const COMMA_BEFORE_NATURES: &[&str] = &["c", "cc"];

pub mod asr_bridge_contract {
    pub const MODULE_NAME: &str = "AsrModule";
    pub const RESULT_EVENT_NAME: &str = "onAsrResult";
    pub const ERROR_EVENT_NAME: &str = "onAsrError";
    pub const ERROR_INIT: &str = "ASR_INIT_ERROR";
    pub const ERROR_FEED: &str = "ASR_FEED_ERROR";
    pub const ERROR_STOP: &str = "ASR_STOP_ERROR";

    pub mod key {
        pub const TEXT: &str = "text";
        pub const IS_FINAL: &str = "isFinal";
    }
}

pub mod tts_bridge_contract {
    pub const MODULE_NAME: &str = "TTSModule";
    pub const EVENT_START: &str = "onTTSStart";
    pub const EVENT_PAUSE: &str = "onTTSPause";
    pub const EVENT_RESUME: &str = "onTTSResume";
    pub const EVENT_FINISH: &str = "onTTSFinish";
    pub const EVENT_ERROR: &str = "onTTSError";

    pub mod key {
        pub const RATE: &str = "rate";
        pub const PITCH_MULTIPLIER: &str = "pitchMultiplier";
        pub const VOLUME: &str = "volume";
        pub const LANGUAGE: &str = "language";
        pub const VOICE_IDENTIFIER: &str = "voiceIdentifier";
        pub const CANCELLED: &str = "cancelled";
        pub const MESSAGE: &str = "message";
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AsrSegment {
    pub word: String,
    pub nature: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AsrModelPlan {
    pub language_dir: String,
    pub asset_path: String,
    pub storage_relative_path: String,
    pub ready_marker: String,
}

pub fn asr_model_language_dir(language: &str) -> &'static str {
    if ZH_LANGUAGE_CODES.contains(&language) {
        "zh"
    } else {
        "en"
    }
}

pub fn asr_model_plan(language: &str) -> AsrModelPlan {
    let language_dir = asr_model_language_dir(language);
    AsrModelPlan {
        language_dir: language_dir.to_string(),
        asset_path: format!("{ASR_MODEL_BASE_DIR}/{language_dir}"),
        storage_relative_path: format!("{ASR_MODEL_BASE_DIR}/{language_dir}"),
        ready_marker: ASR_MODEL_READY_MARKER.to_string(),
    }
}

pub fn parse_vosk_result_text(json: &str) -> String {
    parse_vosk_text_field(json, "text")
}

pub fn parse_vosk_partial_text(json: &str) -> String {
    parse_vosk_text_field(json, "partial")
}

pub fn post_process_zh_asr_segments(text: &str, segments: &[AsrSegment]) -> String {
    if text.trim().is_empty() || segments.is_empty() {
        return text.to_string();
    }

    let mut output = String::new();

    for (index, segment) in segments.iter().enumerate() {
        let word = segment.word.trim();
        let nature = segment.nature.as_deref();
        if word.is_empty() {
            continue;
        }

        if nature.is_some_and(|nature| COMMA_BEFORE_NATURES.contains(&nature)) {
            append_comma_if_needed(&mut output);
        }

        output.push_str(word);

        if index < segments.len() - 1
            && nature.is_some_and(|nature| COMMA_AFTER_NATURES.contains(&nature))
        {
            let next_nature = segments[index + 1].nature.as_deref();
            if !next_nature.is_some_and(|nature| COMMA_AFTER_NATURES.contains(&nature)) {
                append_comma_if_needed(&mut output);
            }
        }
    }

    append_end_punctuation(&mut output, segments);

    output
}

fn parse_vosk_text_field(json: &str, field: &str) -> String {
    serde_json::from_str::<serde_json::Value>(json)
        .ok()
        .and_then(|value| value.get(field).cloned())
        .map(json_opt_string)
        .unwrap_or_default()
}

fn json_opt_string(value: serde_json::Value) -> String {
    match value {
        serde_json::Value::Null => String::new(),
        serde_json::Value::String(value) => value,
        serde_json::Value::Bool(value) => value.to_string(),
        serde_json::Value::Number(value) => value.to_string(),
        other => other.to_string(),
    }
}

fn append_comma_if_needed(output: &mut String) {
    if !output.is_empty() && !ends_with_punctuation(output) {
        output.push('，');
    }
}

fn append_end_punctuation(output: &mut String, segments: &[AsrSegment]) {
    if output.is_empty() || ends_with_punctuation(output) {
        return;
    }

    let last_word = segments
        .last()
        .map(|segment| segment.word.trim())
        .unwrap_or_default();
    let has_question = segments
        .iter()
        .any(|segment| QUESTION_INDICATORS.contains(&segment.word.trim()));

    if EXCLAMATION_INDICATORS.contains(&last_word) {
        output.push('！');
    } else if has_question {
        output.push('？');
    } else {
        output.push('。');
    }
}

fn ends_with_punctuation(text: &str) -> bool {
    text.chars().last().is_some_and(|ch| {
        matches!(
            ch,
            '。' | '？' | '！' | '，' | '；' | '：' | '.' | '?' | '!' | ',' | ';' | ':'
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_asr_language_to_existing_model_dirs() {
        assert_eq!(asr_model_language_dir("zh-CN"), "zh");
        assert_eq!(asr_model_language_dir("zh-Hant"), "zh");
        assert_eq!(asr_model_language_dir("en-US"), "en");
        assert_eq!(asr_model_language_dir("fr-FR"), "en");
    }

    #[test]
    fn asr_bridge_contract_keeps_existing_android_wire_names() {
        use asr_bridge_contract as contract;

        assert_eq!(contract::MODULE_NAME, "AsrModule");
        assert_eq!(contract::RESULT_EVENT_NAME, "onAsrResult");
        assert_eq!(contract::ERROR_EVENT_NAME, "onAsrError");
        assert_eq!(contract::ERROR_INIT, "ASR_INIT_ERROR");
        assert_eq!(contract::ERROR_FEED, "ASR_FEED_ERROR");
        assert_eq!(contract::ERROR_STOP, "ASR_STOP_ERROR");
        assert_eq!(contract::key::TEXT, "text");
        assert_eq!(contract::key::IS_FINAL, "isFinal");
    }

    #[test]
    fn tts_bridge_contract_keeps_existing_ios_rn_wire_names() {
        use tts_bridge_contract as contract;

        assert_eq!(contract::MODULE_NAME, "TTSModule");
        assert_eq!(contract::EVENT_START, "onTTSStart");
        assert_eq!(contract::EVENT_PAUSE, "onTTSPause");
        assert_eq!(contract::EVENT_RESUME, "onTTSResume");
        assert_eq!(contract::EVENT_FINISH, "onTTSFinish");
        assert_eq!(contract::EVENT_ERROR, "onTTSError");
        assert_eq!(contract::key::RATE, "rate");
        assert_eq!(contract::key::PITCH_MULTIPLIER, "pitchMultiplier");
        assert_eq!(contract::key::VOLUME, "volume");
        assert_eq!(contract::key::LANGUAGE, "language");
        assert_eq!(contract::key::VOICE_IDENTIFIER, "voiceIdentifier");
        assert_eq!(contract::key::CANCELLED, "cancelled");
        assert_eq!(contract::key::MESSAGE, "message");
    }

    #[test]
    fn builds_asr_model_plan_from_language() {
        let zh = asr_model_plan("zh-CN");
        assert_eq!(zh.language_dir, "zh");
        assert_eq!(zh.asset_path, "model/asr/zh");
        assert_eq!(zh.storage_relative_path, "model/asr/zh");
        assert_eq!(zh.ready_marker, ".model_ready");

        let fallback = asr_model_plan("fr-FR");
        assert_eq!(fallback.language_dir, "en");
        assert_eq!(fallback.asset_path, "model/asr/en");
        assert_eq!(fallback.storage_relative_path, "model/asr/en");
    }

    #[test]
    fn parses_vosk_final_and_partial_text_fields() {
        assert_eq!(
            parse_vosk_result_text(r#"{"text":"hello world"}"#),
            "hello world"
        );
        assert_eq!(parse_vosk_partial_text(r#"{"partial":"hel"}"#), "hel");
    }

    #[test]
    fn vosk_json_parsing_matches_android_opt_string_fallbacks() {
        assert_eq!(parse_vosk_result_text(r#"{"text":123}"#), "123");
        assert_eq!(parse_vosk_result_text(r#"{"text":false}"#), "false");
        assert_eq!(parse_vosk_result_text(r#"{"other":"missing"}"#), "");
        assert_eq!(parse_vosk_result_text("not json"), "");
    }

    #[test]
    fn post_processes_zh_segments_with_android_punctuation_rules() {
        let segments = vec![
            AsrSegment {
                word: "今天".to_string(),
                nature: Some("t".to_string()),
            },
            AsrSegment {
                word: "天气".to_string(),
                nature: Some("n".to_string()),
            },
            AsrSegment {
                word: "很好".to_string(),
                nature: Some("a".to_string()),
            },
        ];

        assert_eq!(
            post_process_zh_asr_segments("今天 天气 很好", &segments),
            "今天，天气很好。"
        );
    }

    #[test]
    fn post_processes_zh_question_and_exclamation_endings() {
        let question = vec![
            AsrSegment {
                word: "你".to_string(),
                nature: Some("r".to_string()),
            },
            AsrSegment {
                word: "想".to_string(),
                nature: Some("v".to_string()),
            },
            AsrSegment {
                word: "什么".to_string(),
                nature: Some("r".to_string()),
            },
        ];
        assert_eq!(
            post_process_zh_asr_segments("你 想 什么", &question),
            "你想什么？"
        );

        let exclamation = vec![AsrSegment {
            word: "哇".to_string(),
            nature: Some("e".to_string()),
        }];
        assert_eq!(post_process_zh_asr_segments("哇", &exclamation), "哇！");
    }

    #[test]
    fn post_processes_zh_segments_preserves_fallback_edges() {
        assert_eq!(post_process_zh_asr_segments("  ", &[]), "  ");
        assert_eq!(post_process_zh_asr_segments("原始 文本", &[]), "原始 文本");

        let punctuated = vec![AsrSegment {
            word: "你好。".to_string(),
            nature: None,
        }];
        assert_eq!(
            post_process_zh_asr_segments("你好。", &punctuated),
            "你好。"
        );
    }
}
