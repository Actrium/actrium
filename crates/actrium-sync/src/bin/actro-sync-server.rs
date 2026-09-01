use actro_sync_server::{
    BrowserSyncHttpRequest, BrowserSyncHttpResponse, BrowserSyncServerDatabase, BrowserSyncService,
    BrowserSyncServiceConfig,
};
use std::env;
use std::io;
use std::process;
use tiny_http::{Header, Response, Server, StatusCode};

#[derive(Debug)]
struct ServerOptions {
    addr: String,
    database_path: Option<String>,
    registered_devices: Vec<RegisteredDevice>,
}

#[derive(Debug)]
struct RegisteredDevice {
    account_id: String,
    device_id: String,
    access_token: String,
}

fn main() {
    if let Err(error) = run() {
        eprintln!("actro-sync-server: {error}");
        process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let options = parse_args(env::args().skip(1))?;
    let database = match &options.database_path {
        Some(path) => BrowserSyncServerDatabase::open(path)?,
        None => BrowserSyncServerDatabase::open_in_memory()?,
    };
    let mut service =
        BrowserSyncService::from_database(database, BrowserSyncServiceConfig::default())?;
    let now = current_unix_seconds();
    for device in &options.registered_devices {
        service.register_device(
            &device.account_id,
            &device.device_id,
            &device.access_token,
            now,
        )?;
    }

    let server = Server::http(&options.addr)?;
    eprintln!(
        "actro-sync-server listening on http://{} with {} registered device(s)",
        options.addr,
        options.registered_devices.len()
    );

    for mut request in server.incoming_requests() {
        let http_request = match request_to_facade(&mut request) {
            Ok(request) => request,
            Err(error) => {
                let _ = request.respond(error_response(400, "invalidRequest", error.to_string()));
                continue;
            }
        };
        let response = service.handle_http_request(http_request);
        let _ = request.respond(response_from_facade(response));
    }

    Ok(())
}

fn parse_args<I>(args: I) -> Result<ServerOptions, String>
where
    I: IntoIterator<Item = String>,
{
    let mut addr = "127.0.0.1:8787".to_string();
    let mut database_path = None;
    let mut registered_devices = Vec::new();
    let mut args = args.into_iter();

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--addr" => {
                addr = args
                    .next()
                    .ok_or_else(|| "--addr requires host:port".to_string())?;
            }
            "--database" => {
                database_path = Some(
                    args.next()
                        .ok_or_else(|| "--database requires a path".to_string())?,
                );
            }
            "--register-device" => {
                let raw = args
                    .next()
                    .ok_or_else(|| "--register-device requires account:device:token".to_string())?;
                registered_devices.push(parse_registered_device(&raw)?);
            }
            "--help" | "-h" => {
                return Err(usage());
            }
            _ => {
                return Err(format!("unknown argument: {arg}\n\n{}", usage()));
            }
        }
    }

    Ok(ServerOptions {
        addr,
        database_path,
        registered_devices,
    })
}

fn parse_registered_device(raw: &str) -> Result<RegisteredDevice, String> {
    let mut parts = raw.splitn(3, ':');
    let account_id = parts.next().unwrap_or_default().trim();
    let device_id = parts.next().unwrap_or_default().trim();
    let access_token = parts.next().unwrap_or_default().trim();
    if account_id.is_empty() || device_id.is_empty() || access_token.is_empty() {
        return Err("--register-device must be account:device:token".to_string());
    }

    Ok(RegisteredDevice {
        account_id: account_id.to_string(),
        device_id: device_id.to_string(),
        access_token: access_token.to_string(),
    })
}

fn request_to_facade(request: &mut tiny_http::Request) -> io::Result<BrowserSyncHttpRequest> {
    let mut body = String::new();
    request.as_reader().read_to_string(&mut body)?;
    Ok(BrowserSyncHttpRequest {
        method: request.method().as_str().to_string(),
        path: request.url().to_string(),
        headers: request
            .headers()
            .iter()
            .map(|header| {
                (
                    header.field.as_str().to_string(),
                    header.value.as_str().to_string(),
                )
            })
            .collect(),
        body,
    })
}

fn response_from_facade(response: BrowserSyncHttpResponse) -> Response<io::Cursor<Vec<u8>>> {
    let mut tiny_response =
        Response::from_string(response.body).with_status_code(StatusCode(response.status_code));
    for (name, value) in response.headers {
        if let Ok(header) = Header::from_bytes(name.as_bytes(), value.as_bytes()) {
            tiny_response = tiny_response.with_header(header);
        }
    }
    tiny_response
}

fn error_response(status_code: u16, code: &str, message: String) -> Response<io::Cursor<Vec<u8>>> {
    let body = serde_json::json!({
        "error": {
            "code": code,
            "message": message,
        }
    })
    .to_string();
    Response::from_string(body)
        .with_status_code(StatusCode(status_code))
        .with_header(
            Header::from_bytes("Content-Type".as_bytes(), "application/json".as_bytes()).unwrap(),
        )
}

fn current_unix_seconds() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or_default()
}

fn usage() -> String {
    "Usage: actro-sync-server [--addr HOST:PORT] [--database PATH] [--register-device ACCOUNT:DEVICE:TOKEN]\n\n\
     Example:\n  actro-sync-server --addr 127.0.0.1:8787 --database /tmp/actro-sync.db --register-device profile-a:device-a:token-a-123"
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_server_options() {
        let options = parse_args([
            "--addr".to_string(),
            "127.0.0.1:9000".to_string(),
            "--database".to_string(),
            "/tmp/actro-sync.db".to_string(),
            "--register-device".to_string(),
            "profile-a:device-a:token-a-123".to_string(),
        ])
        .unwrap();

        assert_eq!(options.addr, "127.0.0.1:9000");
        assert_eq!(options.database_path.as_deref(), Some("/tmp/actro-sync.db"));
        assert_eq!(options.registered_devices.len(), 1);
        assert_eq!(options.registered_devices[0].account_id, "profile-a");
        assert_eq!(options.registered_devices[0].device_id, "device-a");
        assert_eq!(options.registered_devices[0].access_token, "token-a-123");
    }

    #[test]
    fn rejects_invalid_registered_device_argument() {
        assert!(parse_registered_device("profile-a:device-a").is_err());
        assert!(parse_registered_device(":device-a:token-a-123").is_err());
    }
}
