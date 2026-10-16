#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BrowserSearchEngine {
    pub id: &'static str,
    pub display_name: &'static str,
    pub search_url_prefix: &'static str,
}

pub const DEFAULT_SEARCH_ENGINE_ID: &str = "google";
pub const DEFAULT_SEARCH_URL_PREFIX: &str = "https://www.google.com/search?q=";

pub const SEARCH_ENGINES: [BrowserSearchEngine; 3] = [
    BrowserSearchEngine {
        id: DEFAULT_SEARCH_ENGINE_ID,
        display_name: "Google",
        search_url_prefix: DEFAULT_SEARCH_URL_PREFIX,
    },
    BrowserSearchEngine {
        id: "bing",
        display_name: "Bing",
        search_url_prefix: "https://www.bing.com/search?q=",
    },
    BrowserSearchEngine {
        id: "duckduckgo",
        display_name: "DuckDuckGo",
        search_url_prefix: "https://duckduckgo.com/?q=",
    },
];

pub fn normalize_search_engine_id(engine_id: Option<&str>) -> &'static str {
    let normalized = engine_id.unwrap_or_default().trim().to_ascii_lowercase();
    SEARCH_ENGINES
        .iter()
        .find(|engine| engine.id == normalized)
        .map(|engine| engine.id)
        .unwrap_or(DEFAULT_SEARCH_ENGINE_ID)
}

pub fn search_engine_info(engine_id: Option<&str>) -> BrowserSearchEngine {
    let normalized = normalize_search_engine_id(engine_id);
    SEARCH_ENGINES
        .iter()
        .copied()
        .find(|engine| engine.id == normalized)
        .unwrap_or(SEARCH_ENGINES[0])
}

pub fn browser_search_url(query: &str, engine_id: Option<&str>) -> String {
    let engine = search_engine_info(engine_id);
    format!(
        "{}{}",
        engine.search_url_prefix,
        percent_encode_query(query)
    )
}

fn percent_encode_query(query: &str) -> String {
    let mut encoded = String::with_capacity(query.len());
    for byte in query.as_bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                encoded.push(*byte as char);
            }
            _ => {
                encoded.push('%');
                encoded.push(hex_digit(byte >> 4));
                encoded.push(hex_digit(byte & 0x0f));
            }
        }
    }
    encoded
}

fn hex_digit(value: u8) -> char {
    match value {
        0..=9 => (b'0' + value) as char,
        10..=15 => (b'A' + (value - 10)) as char,
        _ => unreachable!("hex digit nibble out of range"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_search_engine_ids() {
        assert_eq!(normalize_search_engine_id(None), "google");
        assert_eq!(normalize_search_engine_id(Some(" missing ")), "google");
        assert_eq!(
            normalize_search_engine_id(Some(" DuckDuckGo ")),
            "duckduckgo"
        );
    }

    #[test]
    fn builds_search_urls_with_shared_engine_catalog() {
        assert_eq!(
            browser_search_url("actrium browser", Some("duckduckgo")),
            "https://duckduckgo.com/?q=actrium%20browser"
        );
        assert_eq!(
            browser_search_url("a&b=c", Some("google")),
            "https://www.google.com/search?q=a%26b%3Dc"
        );
        assert_eq!(
            browser_search_url("\u{4f60}\u{597d}", Some("google")),
            "https://www.google.com/search?q=%E4%BD%A0%E5%A5%BD"
        );
    }
}
