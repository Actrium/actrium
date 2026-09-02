pub const AUTH_ENVIRONMENT_DEV: &str = "dev";
pub const AUTH_ENVIRONMENT_BETA: &str = "beta";
pub const AUTH_ENVIRONMENT_PROD: &str = "prod";

pub const AUTH_ENVIRONMENT_DEV_HOST_SUFFIX: &str = ".develenv.com";
pub const AUTH_ENVIRONMENT_BETA_HOST_SUFFIX: &str = ".betaenv.com";
pub const AUTH_ENVIRONMENT_PROD_HOST_SUFFIX: &str = ".actrium.chat";

pub const AUTH_ENVIRONMENT_DEV_STORAGE_SUFFIX: &str = ".dev";
pub const AUTH_ENVIRONMENT_BETA_STORAGE_SUFFIX: &str = ".beta";
pub const AUTH_ENVIRONMENT_PROD_STORAGE_SUFFIX: &str = ".prod";

pub const AUTH_ACCOUNT_DEV_BASE_URL: &str = "http://account-actrium.develenv.com";
pub const AUTH_ACCOUNT_BETA_BASE_URL: &str = "https://account-actrium.betaenv.com";
pub const AUTH_ACCOUNT_PROD_BASE_URL: &str = "https://account.actrium.chat";

pub const AUTH_RESOURCE_UPLOADER_DEV_BASE_URL: &str = "https://resource-uploader.develenv.com";
pub const AUTH_RESOURCE_UPLOADER_BETA_BASE_URL: &str = "https://resource-uploader.betaenv.com";
pub const AUTH_RESOURCE_UPLOADER_PROD_BASE_URL: &str = "https://resource-uploader.actrium.chat";

pub const AUTH_CHECK_SESSION_PATH: &str = "/sso/session";
pub const AUTH_USER_INFO_PATH: &str = "/protocol/userinfo";
pub const AUTH_AUTHORIZE_PATH: &str = "/protocol/authorize";
pub const AUTH_DELETE_ACCOUNT_PATH: &str = "/sso/account";
pub const AUTH_WECHAT_SESSION_PATH: &str = "/sso/providers/wechat/session";
pub const AUTH_APPLE_SESSION_PATH: &str = "/sso/providers/apple/session";
pub const AUTH_GOOGLE_SESSION_PATH: &str = "/sso/providers/google/session";
pub const AUTH_ACCESS_CONTROL_PREFIX: &str = "/api/access_control";
pub const AUTH_CLIENT_APP_PREFIX: &str = "/api/client/app";
pub const AUTH_RESOURCE_UPLOAD_TOKEN_PATH: &str = "/api/get-token";

pub const AUTH_ACCESS_CONTROL_AUTHORIZED_CODE: i32 = 0;
pub const AUTH_ACCESS_CONTROL_STATUS_AUTHORIZED: &str = "authorized";
pub const AUTH_ACCESS_CONTROL_STATUS_NOT_AUTHORIZED: &str = "not_authorized";
pub const AUTH_ACCESS_CONTROL_STATUS_UNKNOWN: &str = "unknown";

pub const AUTH_API_RESPONSE_SUCCESS_CODE: i32 = 0;
pub const AUTH_API_RESPONSE_HTTP_SUCCESS_CODE: i32 = 200;
pub const AUTH_CLIENT_APP_NOT_FOUND_CODE: i32 = 10010;
pub const AUTH_CLIENT_APP_NOT_FOUND_MESSAGE_EN: &str = "app not found";
pub const AUTH_CLIENT_APP_NOT_FOUND_MESSAGE_ZH: &str = "\u{5e94}\u{7528}\u{4e0d}\u{5b58}\u{5728}";

pub fn auth_environment_from_url(raw_url: &str) -> &'static str {
    host_from_url(raw_url)
        .map(|host| auth_environment_from_host(&host))
        .unwrap_or(AUTH_ENVIRONMENT_PROD)
}

pub fn auth_environment_from_app_url(raw_url: &str) -> &'static str {
    if !has_valid_app_path(raw_url) {
        return AUTH_ENVIRONMENT_PROD;
    }
    auth_environment_from_url(raw_url)
}

pub fn auth_environment_storage_suffix(environment: &str) -> &'static str {
    match environment {
        AUTH_ENVIRONMENT_DEV => AUTH_ENVIRONMENT_DEV_STORAGE_SUFFIX,
        AUTH_ENVIRONMENT_BETA => AUTH_ENVIRONMENT_BETA_STORAGE_SUFFIX,
        AUTH_ENVIRONMENT_PROD => AUTH_ENVIRONMENT_PROD_STORAGE_SUFFIX,
        _ => AUTH_ENVIRONMENT_PROD_STORAGE_SUFFIX,
    }
}

pub fn auth_account_base_url(environment: &str) -> &'static str {
    match environment {
        AUTH_ENVIRONMENT_DEV => AUTH_ACCOUNT_DEV_BASE_URL,
        AUTH_ENVIRONMENT_BETA => AUTH_ACCOUNT_BETA_BASE_URL,
        AUTH_ENVIRONMENT_PROD => AUTH_ACCOUNT_PROD_BASE_URL,
        _ => AUTH_ACCOUNT_PROD_BASE_URL,
    }
}

pub fn auth_resource_uploader_base_url(environment: &str) -> &'static str {
    match environment {
        AUTH_ENVIRONMENT_DEV => AUTH_RESOURCE_UPLOADER_DEV_BASE_URL,
        AUTH_ENVIRONMENT_BETA => AUTH_RESOURCE_UPLOADER_BETA_BASE_URL,
        AUTH_ENVIRONMENT_PROD => AUTH_RESOURCE_UPLOADER_PROD_BASE_URL,
        _ => AUTH_RESOURCE_UPLOADER_PROD_BASE_URL,
    }
}

pub fn auth_access_control_path(app_id: &str, user_id: &str) -> String {
    format!("{AUTH_ACCESS_CONTROL_PREFIX}/{app_id}/{user_id}")
}

pub fn auth_client_app_path(app_id: &str) -> String {
    format!("{AUTH_CLIENT_APP_PREFIX}/{app_id}")
}

pub fn auth_access_control_url_from_app_url(
    app_url: &str,
    app_id: &str,
    user_id: &str,
) -> Option<String> {
    let base = base_url_from_url(app_url)?;
    Some(format!(
        "{base}{}",
        auth_access_control_path(app_id, user_id)
    ))
}

pub fn auth_is_authorized_code(code: i32) -> bool {
    code == AUTH_ACCESS_CONTROL_AUTHORIZED_CODE
}

pub fn auth_access_control_status(success: bool, code: Option<i32>) -> &'static str {
    if !success {
        return AUTH_ACCESS_CONTROL_STATUS_UNKNOWN;
    }

    match code {
        Some(code) if auth_is_authorized_code(code) => AUTH_ACCESS_CONTROL_STATUS_AUTHORIZED,
        Some(_) => AUTH_ACCESS_CONTROL_STATUS_NOT_AUTHORIZED,
        None => AUTH_ACCESS_CONTROL_STATUS_NOT_AUTHORIZED,
    }
}

pub fn auth_api_response_success(code: i32) -> bool {
    matches!(
        code,
        AUTH_API_RESPONSE_SUCCESS_CODE | AUTH_API_RESPONSE_HTTP_SUCCESS_CODE
    )
}

pub fn auth_is_client_app_not_found_code(code: i32) -> bool {
    code == AUTH_CLIENT_APP_NOT_FOUND_CODE
}

pub fn auth_is_client_app_not_found_message(message: &str) -> bool {
    let normalized = message.to_ascii_lowercase();
    normalized.contains(AUTH_CLIENT_APP_NOT_FOUND_MESSAGE_EN)
        || message.contains(AUTH_CLIENT_APP_NOT_FOUND_MESSAGE_ZH)
}

fn auth_environment_from_host(host: &str) -> &'static str {
    let normalized = host.to_ascii_lowercase();
    if normalized.ends_with(AUTH_ENVIRONMENT_DEV_HOST_SUFFIX) {
        AUTH_ENVIRONMENT_DEV
    } else if normalized.ends_with(AUTH_ENVIRONMENT_BETA_HOST_SUFFIX) {
        AUTH_ENVIRONMENT_BETA
    } else {
        AUTH_ENVIRONMENT_PROD
    }
}

fn base_url_from_url(raw_url: &str) -> Option<String> {
    let (scheme, _) = raw_url.trim().split_once("://")?;
    let scheme = scheme.trim().to_ascii_lowercase();
    if scheme.is_empty() {
        return None;
    }
    host_from_url(raw_url).map(|host| format!("{scheme}://{host}"))
}

fn host_from_url(raw_url: &str) -> Option<String> {
    let after_scheme = raw_url.trim().split_once("://")?.1;
    let authority = after_scheme
        .split(['/', '?', '#'])
        .next()
        .map(str::trim)
        .filter(|value| !value.is_empty())?;
    let host_port = authority.rsplit('@').next().unwrap_or(authority);
    let host = host_port
        .strip_prefix('[')
        .and_then(|value| value.split_once(']').map(|(host, _)| host))
        .unwrap_or_else(|| host_port.split(':').next().unwrap_or(host_port))
        .trim()
        .trim_end_matches('.');

    if host.is_empty() {
        None
    } else {
        Some(host.to_string())
    }
}

fn has_valid_app_path(raw_url: &str) -> bool {
    let after_scheme = match raw_url.trim().split_once("://") {
        Some((_, value)) => value,
        None => return false,
    };
    let after_authority = match after_scheme.find('/') {
        Some(index) => &after_scheme[index..],
        None => return false,
    };
    let path = after_authority
        .split(['?', '#'])
        .next()
        .unwrap_or_default()
        .trim_matches('/');

    path.rsplit('/').any(|segment| !segment.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_auth_environment_from_url_host_suffix() {
        assert_eq!(
            auth_environment_from_url("https://gate-actrium.develenv.com/with/app"),
            AUTH_ENVIRONMENT_DEV
        );
        assert_eq!(
            auth_environment_from_url("https://gate-actrium.betaenv.com/with/app"),
            AUTH_ENVIRONMENT_BETA
        );
        assert_eq!(
            auth_environment_from_url("https://gate.actrium.chat/with/app"),
            AUTH_ENVIRONMENT_PROD
        );
        assert_eq!(
            auth_environment_from_url("https://unknown.example/with/app"),
            AUTH_ENVIRONMENT_PROD
        );
    }

    #[test]
    fn app_url_mapping_preserves_legacy_app_path_requirement() {
        assert_eq!(
            auth_environment_from_app_url("https://gate-actrium.develenv.com/with/app"),
            AUTH_ENVIRONMENT_DEV
        );
        assert_eq!(
            auth_environment_from_app_url("https://gate-actrium.develenv.com"),
            AUTH_ENVIRONMENT_PROD
        );
        assert_eq!(
            auth_environment_from_app_url("not a url"),
            AUTH_ENVIRONMENT_PROD
        );
    }

    #[test]
    fn maps_auth_environment_storage_suffixes() {
        assert_eq!(
            auth_environment_storage_suffix(AUTH_ENVIRONMENT_DEV),
            AUTH_ENVIRONMENT_DEV_STORAGE_SUFFIX
        );
        assert_eq!(
            auth_environment_storage_suffix(AUTH_ENVIRONMENT_BETA),
            AUTH_ENVIRONMENT_BETA_STORAGE_SUFFIX
        );
        assert_eq!(
            auth_environment_storage_suffix(AUTH_ENVIRONMENT_PROD),
            AUTH_ENVIRONMENT_PROD_STORAGE_SUFFIX
        );
        assert_eq!(
            auth_environment_storage_suffix("unknown"),
            AUTH_ENVIRONMENT_PROD_STORAGE_SUFFIX
        );
    }

    #[test]
    fn maps_auth_service_base_urls() {
        assert_eq!(
            auth_account_base_url(AUTH_ENVIRONMENT_DEV),
            AUTH_ACCOUNT_DEV_BASE_URL
        );
        assert_eq!(
            auth_account_base_url(AUTH_ENVIRONMENT_BETA),
            AUTH_ACCOUNT_BETA_BASE_URL
        );
        assert_eq!(
            auth_account_base_url(AUTH_ENVIRONMENT_PROD),
            AUTH_ACCOUNT_PROD_BASE_URL
        );
        assert_eq!(auth_account_base_url("unknown"), AUTH_ACCOUNT_PROD_BASE_URL);
        assert_eq!(
            auth_resource_uploader_base_url(AUTH_ENVIRONMENT_BETA),
            AUTH_RESOURCE_UPLOADER_BETA_BASE_URL
        );
    }

    #[test]
    fn builds_auth_paths_without_changing_legacy_component_encoding() {
        assert_eq!(
            auth_access_control_path("app-123", "user.cn"),
            "/api/access_control/app-123/user.cn"
        );
        assert_eq!(auth_client_app_path("app-123"), "/api/client/app/app-123");
        assert_eq!(
            auth_access_control_path("app/with/slash", "user?raw=true"),
            "/api/access_control/app/with/slash/user?raw=true"
        );
    }

    #[test]
    fn builds_access_control_url_from_app_url_like_ios_login_manager() {
        assert_eq!(
            auth_access_control_url_from_app_url(
                "https://gate-actrium.betaenv.com:9443/with/app-123",
                "app-123",
                "user.cn"
            )
            .as_deref(),
            Some("https://gate-actrium.betaenv.com/api/access_control/app-123/user.cn")
        );
        assert_eq!(
            auth_access_control_url_from_app_url("not a url", "app-123", "user.cn"),
            None
        );
    }

    #[test]
    fn maps_access_control_response_status() {
        assert!(auth_is_authorized_code(AUTH_ACCESS_CONTROL_AUTHORIZED_CODE));
        assert!(!auth_is_authorized_code(10010));
        assert_eq!(
            auth_access_control_status(true, Some(AUTH_ACCESS_CONTROL_AUTHORIZED_CODE)),
            AUTH_ACCESS_CONTROL_STATUS_AUTHORIZED
        );
        assert_eq!(
            auth_access_control_status(true, Some(10010)),
            AUTH_ACCESS_CONTROL_STATUS_NOT_AUTHORIZED
        );
        assert_eq!(
            auth_access_control_status(true, None),
            AUTH_ACCESS_CONTROL_STATUS_NOT_AUTHORIZED
        );
        assert_eq!(
            auth_access_control_status(false, Some(AUTH_ACCESS_CONTROL_AUTHORIZED_CODE)),
            AUTH_ACCESS_CONTROL_STATUS_UNKNOWN
        );
    }

    #[test]
    fn maps_api_response_status_and_app_not_found_rules() {
        assert!(auth_api_response_success(AUTH_API_RESPONSE_SUCCESS_CODE));
        assert!(auth_api_response_success(
            AUTH_API_RESPONSE_HTTP_SUCCESS_CODE
        ));
        assert!(!auth_api_response_success(AUTH_CLIENT_APP_NOT_FOUND_CODE));

        assert!(auth_is_client_app_not_found_code(
            AUTH_CLIENT_APP_NOT_FOUND_CODE
        ));
        assert!(!auth_is_client_app_not_found_code(0));

        assert!(auth_is_client_app_not_found_message("App Not Found for id"));
        assert!(auth_is_client_app_not_found_message(
            "\u{5e94}\u{7528}\u{4e0d}\u{5b58}\u{5728}"
        ));
        assert!(!auth_is_client_app_not_found_message("network error"));
    }
}
