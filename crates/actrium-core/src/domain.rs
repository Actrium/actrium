use serde::{Deserialize, Serialize};

/// Built-in Actro app pages known by native shells and the shared RN runtime.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SystemAppType {
    History,
    Bookmarks,
    Settings,
    BookmarkTags,
    GlobalSearch,
    Login,
    Debug,
    StartPage,
}

impl SystemAppType {
    pub const ALL: [Self; 8] = [
        Self::History,
        Self::Bookmarks,
        Self::Settings,
        Self::BookmarkTags,
        Self::GlobalSearch,
        Self::Login,
        Self::Debug,
        Self::StartPage,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::History => "history",
            Self::Bookmarks => "bookmarks",
            Self::Settings => "settings",
            Self::BookmarkTags => "bookmarkTags",
            Self::GlobalSearch => "globalSearch",
            Self::Login => "login",
            Self::Debug => "debug",
            Self::StartPage => "startPage",
        }
    }

    pub fn title(self) -> &'static str {
        match self {
            Self::History => "History",
            Self::Bookmarks => "Bookmarks",
            Self::Settings => "Settings",
            Self::BookmarkTags => "Bookmark Tags",
            Self::GlobalSearch => "Search",
            Self::Login => "Login",
            Self::Debug => "Debug",
            Self::StartPage => "New Tab",
        }
    }

    pub fn about_url(self) -> &'static str {
        match self {
            Self::History => "about:history",
            Self::Bookmarks => "about:bookmarks",
            Self::Settings => "about:settings",
            Self::BookmarkTags => "about:bookmarkTags",
            Self::GlobalSearch => "about:globalSearch",
            Self::Login => "about:login",
            Self::Debug => "about:debug",
            Self::StartPage => "about:blank",
        }
    }
}

impl TryFrom<&str> for SystemAppType {
    type Error = ();

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "history" => Ok(Self::History),
            "bookmarks" => Ok(Self::Bookmarks),
            "settings" => Ok(Self::Settings),
            "bookmarkTags" => Ok(Self::BookmarkTags),
            "globalSearch" => Ok(Self::GlobalSearch),
            "login" => Ok(Self::Login),
            "debug" => Ok(Self::Debug),
            "startPage" | "blank" => Ok(Self::StartPage),
            _ => Err(()),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AppSourceType {
    Downloaded,
    LocalDev,
}

impl AppSourceType {
    pub const ALL: [Self; 2] = [Self::Downloaded, Self::LocalDev];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Downloaded => "downloaded",
            Self::LocalDev => "localDev",
        }
    }

    pub fn display_name(self) -> &'static str {
        match self {
            Self::Downloaded => "Downloaded",
            Self::LocalDev => "Local Development",
        }
    }
}

impl TryFrom<&str> for AppSourceType {
    type Error = ();

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "downloaded" => Ok(Self::Downloaded),
            "localDev" => Ok(Self::LocalDev),
            _ => Err(()),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TabStatus {
    Alive,
    Dead,
    Unknown,
}

impl Default for TabStatus {
    fn default() -> Self {
        Self::Unknown
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Contract {
    pub name: String,
    pub version: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TabRecord {
    pub id: String,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub app_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub system_app_type: Option<SystemAppType>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub guest_script: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub left_panel_script: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub right_panel_script: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unified_script: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub contract: Option<Contract>,
    #[serde(default)]
    pub permissions: Vec<String>,
    #[serde(default)]
    pub reload_nonce: u32,
    #[serde(default)]
    pub left_panel_default_visible: bool,
    #[serde(default)]
    pub right_panel_default_visible: bool,
    #[serde(default = "default_tab_background")]
    pub background_color: String,
    #[serde(default)]
    pub has_unread_message: bool,
    #[serde(default)]
    pub is_bookmarked: bool,
    #[serde(default)]
    pub status: TabStatus,
    #[serde(default)]
    pub sort_order: i32,
}

fn default_tab_background() -> String {
    "#1e1e1e".to_string()
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryItem {
    pub id: String,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub app_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub app_id: Option<String>,
    pub visit_time_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryGroupTab {
    pub id: String,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub app_url: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryGroup {
    pub id: String,
    pub name: String,
    pub color: TabGroupColor,
    pub tabs: Vec<HistoryGroupTab>,
    pub closed_at_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TabGroupColor {
    Gray,
    Blue,
    Red,
    Yellow,
    Green,
    Pink,
    Purple,
    Cyan,
    Orange,
}

impl Default for TabGroupColor {
    fn default() -> Self {
        Self::Gray
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TabGroup {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub color: TabGroupColor,
    #[serde(default)]
    pub is_collapsed: bool,
    pub created_at_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Tag {
    pub id: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    #[serde(default)]
    pub is_system: bool,
}

impl Tag {
    pub const DEFAULT_COLORS: [&'static str; 10] = [
        "#FF3B30", "#FF9500", "#FFCC00", "#34C759", "#00C7BE", "#007AFF", "#5856D6", "#AF52DE",
        "#FF2D55", "#8E8E93",
    ];
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Bookmark {
    pub id: String,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub app_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub app_id: Option<String>,
    pub created_at_ms: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_used_at_ms: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub manufacturer: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub favicon: Option<String>,
    #[serde(default)]
    pub tags: Vec<Tag>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn system_app_type_matches_runtime_strings() {
        assert_eq!(SystemAppType::BookmarkTags.as_str(), "bookmarkTags");
        assert_eq!(
            SystemAppType::try_from("globalSearch"),
            Ok(SystemAppType::GlobalSearch)
        );
        assert_eq!(SystemAppType::StartPage.about_url(), "about:blank");
    }

    #[test]
    fn app_source_type_matches_platform_strings() {
        assert_eq!(AppSourceType::Downloaded.as_str(), "downloaded");
        assert_eq!(AppSourceType::Downloaded.display_name(), "Downloaded");
        assert_eq!(AppSourceType::LocalDev.as_str(), "localDev");
        assert_eq!(AppSourceType::LocalDev.display_name(), "Local Development");
        assert_eq!(
            AppSourceType::try_from("localDev"),
            Ok(AppSourceType::LocalDev)
        );
        assert!(AppSourceType::try_from("local-dev").is_err());
    }

    #[test]
    fn tab_status_uses_native_wire_values() {
        assert_eq!(
            serde_json::to_string(&TabStatus::Alive).unwrap(),
            "\"alive\""
        );
        assert_eq!(serde_json::to_string(&TabStatus::Dead).unwrap(), "\"dead\"");
    }
}
