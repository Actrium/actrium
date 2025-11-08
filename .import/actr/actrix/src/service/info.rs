//\! 服务信息
//\!
//\! 定义了服务的基本信息结构
//! 服务信息管理模块

use actrix_common::status::services::ServiceStatus;
use serde::{Deserialize, Serialize};
use tracing::{error, info};
use url::Url;

use actrix_common::config::ActrixConfig;

use super::ServiceType;

/// 服务基本信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceInfo {
    /// 服务名称
    pub name: String,
    /// 服务类型, Turn 服务本身是 STUN 和 TURN 的集合
    pub service_type: ServiceType,
    pub domain_name: String,
    pub port_info: String,
    /// 服务状态
    pub status: ServiceStatus,
    /// 服务描述
    pub description: Option<String>,
}

impl ServiceInfo {
    pub fn new(
        name: impl Into<String>,
        service_type: ServiceType,
        description: Option<String>,
        config: &ActrixConfig,
    ) -> Self {
        let (port_info, domain_name) = match service_type {
            ServiceType::Signaling => {
                let (port_info, domain_name) = if config.env == "dev" {
                    // 开发环境优先使用 HTTP
                    if let Some(ref http_config) = config.bind.http {
                        (
                            http_config.port.to_string(),
                            format!("ws://{}", http_config.domain_name),
                        )
                    } else if let Some(ref https_config) = config.bind.https {
                        (
                            https_config.port.to_string(),
                            format!("wss://{}", https_config.domain_name),
                        )
                    } else {
                        ("0".to_string(), "ws://localhost".to_string())
                    }
                } else {
                    // 生产环境使用 HTTPS
                    if let Some(ref https_config) = config.bind.https {
                        (
                            https_config.port.to_string(),
                            format!("wss://{}", https_config.domain_name),
                        )
                    } else {
                        ("0".to_string(), "wss://localhost".to_string())
                    }
                };
                (port_info, domain_name)
            }
            ServiceType::Supervisor => {
                let (port_info, domain_name) = if config.env == "dev" {
                    // 开发环境优先使用 HTTP
                    if let Some(ref http_config) = config.bind.http {
                        (
                            http_config.port.to_string(),
                            format!("http://{}", http_config.domain_name),
                        )
                    } else if let Some(ref https_config) = config.bind.https {
                        (
                            https_config.port.to_string(),
                            format!("https://{}", https_config.domain_name),
                        )
                    } else {
                        ("0".to_string(), "http://localhost".to_string())
                    }
                } else {
                    // 生产环境使用 HTTPS
                    if let Some(ref https_config) = config.bind.https {
                        (
                            https_config.port.to_string(),
                            format!("https://{}", https_config.domain_name),
                        )
                    } else {
                        ("0".to_string(), "https://localhost".to_string())
                    }
                };
                (port_info, domain_name)
            }
            ServiceType::Turn => {
                let (port_info, domain_name) = (
                    config.bind.ice.port.to_string(),
                    format!("turn:{}", config.bind.ice.domain_name),
                );
                (port_info, domain_name)
            }
            ServiceType::Stun => {
                let (port_info, domain_name) = (
                    config.bind.ice.port.to_string(),
                    format!("stun:{}", config.bind.ice.domain_name),
                );
                (port_info, domain_name)
            }
            ServiceType::Ais => {
                let (port_info, domain_name) = if config.env == "dev" {
                    // 开发环境优先使用 HTTP
                    if let Some(ref http_config) = config.bind.http {
                        (
                            http_config.port.to_string(),
                            format!("http://{}", http_config.domain_name),
                        )
                    } else if let Some(ref https_config) = config.bind.https {
                        (
                            https_config.port.to_string(),
                            format!("https://{}", https_config.domain_name),
                        )
                    } else {
                        ("0".to_string(), "http://localhost".to_string())
                    }
                } else {
                    // 生产环境使用 HTTPS
                    if let Some(ref https_config) = config.bind.https {
                        (
                            https_config.port.to_string(),
                            format!("https://{}", https_config.domain_name),
                        )
                    } else {
                        ("0".to_string(), "https://localhost".to_string())
                    }
                };
                (port_info, domain_name)
            }
            ServiceType::Ks => {
                let (port_info, domain_name) = if config.env == "dev" {
                    // 开发环境优先使用 HTTP
                    if let Some(ref http_config) = config.bind.http {
                        (
                            http_config.port.to_string(),
                            format!("http://{}", http_config.domain_name),
                        )
                    } else if let Some(ref https_config) = config.bind.https {
                        (
                            https_config.port.to_string(),
                            format!("https://{}", https_config.domain_name),
                        )
                    } else {
                        ("0".to_string(), "http://localhost".to_string())
                    }
                } else {
                    // 生产环境使用 HTTPS
                    if let Some(ref https_config) = config.bind.https {
                        (
                            https_config.port.to_string(),
                            format!("https://{}", https_config.domain_name),
                        )
                    } else {
                        ("0".to_string(), "https://localhost".to_string())
                    }
                };
                (port_info, domain_name)
            }
        };
        Self {
            name: name.into(),
            service_type,
            port_info,
            domain_name,
            status: ServiceStatus::Unknown,
            description,
        }
    }

    /// 设置服务状态为运行中
    pub fn set_running(&mut self, url: Url) {
        self.status = ServiceStatus::Running(url.to_string());
        info!(
            "Service '{}' is now running at {}/{}",
            self.name,
            self.url(),
            self.domain_name
        );
    }

    /// 设置服务状态为错误
    pub fn set_error(&mut self, error: impl Into<String>) {
        let error_msg = error.into();
        self.status = ServiceStatus::Error(error_msg.clone());
        error!(
            "Service '{}' encountered error: {}/{}",
            self.name,
            self.url(),
            self.domain_name
        );
    }

    /// 检查服务是否正在运行
    pub fn is_running(&self) -> bool {
        matches!(self.status, ServiceStatus::Running(_))
    }

    /// 获取服务状态的 URL（如果是运行状态）
    pub fn url(&self) -> String {
        match &self.status {
            ServiceStatus::Running(url) => url.to_string(),
            _ => "N/A".to_string(),
        }
    }
}
