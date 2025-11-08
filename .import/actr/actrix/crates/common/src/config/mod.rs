//! 统一配置管理系统
//!
//! 本模块是 Actor-RTC 辅助服务配置的"单一真理之源"。
//! 所有配置项的定义、文档、默认值都在这里统一管理。

pub mod ais;
pub mod bind;
pub mod ks;
pub mod services;
pub mod signaling;
pub mod supervisor;
pub mod tracing;
pub mod turn;

pub use crate::config::ais::AisConfig;
pub use crate::config::bind::BindConfig;
pub use crate::config::services::ServicesConfig;
pub use crate::config::signaling::SignalingConfig;
pub use crate::config::supervisor::SupervisorConfig;
pub use crate::config::tracing::TracingConfig;
pub use crate::config::turn::TurnConfig;
use ::ks::storage::StorageBackend;
use serde::{Deserialize, Serialize};

/// Actor-RTC 辅助服务的主配置结构体
///
/// 这是系统的核心配置，包含了所有服务的配置信息。
/// 配置文件使用 TOML 格式，支持完整的类型安全加载。
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ActrixConfig {
    /// 服务启用标志位 (位掩码) - 已弃用
    ///
    /// 注意：此字段已弃用，现在使用 services.*.enabled 来控制各服务
    /// 保留此字段仅为了兼容旧的启动逻辑，建议尽快迁移
    ///
    /// 使用二进制位掩码控制各个服务的启用状态：
    /// - 位 0 (1): Signaling 信令服务
    /// - 位 1 (2): STUN 服务
    /// - 位 2 (4): TURN 服务
    /// - 位 3 (8): AIS 身份认证服务
    /// - 位 4 (16): KS 密钥服务
    ///
    /// 例如：enable = 31 表示启用所有服务 (1+2+4+8+16=31)
    #[serde(default = "default_enable")]
    pub enable: u8,

    /// 服务器实例名称
    ///
    /// 用于标识不同的服务器实例，在集群部署中用于区分节点。
    /// 建议使用有意义的命名规则，如：actrix-01, actrix-prod-east-1 等。
    pub name: String,

    /// 运行环境标识
    ///
    /// 指定当前运行环境，影响安全策略和默认行为：
    /// - "dev": 开发环境，允许 HTTP，证书检查较松
    /// - "prod": 生产环境，强制 HTTPS，严格的安全检查
    /// - "test": 测试环境，用于自动化测试
    pub env: String,

    /// 运行用户（可选）
    ///
    /// 指定服务运行的系统用户。服务会在绑定端口后切换到此用户运行，
    /// 以提高安全性。留空则保持当前用户。
    pub user: Option<String>,

    /// 运行用户组（可选）
    ///
    /// 指定服务运行的系统用户组。与 user 配置配合使用。
    pub group: Option<String>,

    /// PID 文件路径（可选）
    ///
    /// 用于存储进程 ID 的文件路径。系统管理工具可以使用此文件
    /// 来监控和管理服务进程。
    pub pid: Option<String>,

    /// 网络绑定配置
    ///
    /// 定义各种网络服务的绑定地址和端口配置。
    pub bind: BindConfig,

    /// TURN 服务特定配置
    ///
    /// TURN 中继服务的专用配置，包括公网地址、端口范围、认证域等。
    pub turn: TurnConfig,

    /// 位置标签
    ///
    /// 用于标识服务器的地理位置或逻辑分组，便于运维管理和监控。
    /// 例如：us-west-1, office-beijing, edge-node-01
    pub location_tag: String,

    /// Supervisor 平台集成配置（可选）
    ///
    /// 配置与 Supervisor 管理平台的集成，包括认证信息和连接地址。
    /// 如果不需要接入管理平台，可以省略此配置段。
    pub supervisor: Option<SupervisorConfig>,

    /// 服务配置集合
    ///
    /// 包含所有业务服务的配置，每个服务可以独立配置自己的参数和依赖。
    /// 采用服务级别的配置结构，实现高内聚低耦合。
    #[serde(default)]
    pub services: ServicesConfig,

    /// SQLite 数据库文件路径
    ///
    /// 指定用于存储持久化数据的 SQLite 数据库文件路径。
    /// 包括租户信息、访问控制列表、nonce 缓存等。
    pub sqlite: String,

    /// Actrix 内部服务通信共享密钥
    ///
    /// 用于 Actrix 各服务之间的内部通信认证，如 AIS 与 KS 之间的通信。
    /// 这是系统级的内部认证密钥，仅用于服务间通信，不应用于对外业务。
    ///
    /// 注意：
    /// - 此密钥仅限 Actrix 内部服务使用
    /// - 不应用于租户业务或外部 API 访问
    /// - 在生产环境中应使用强随机密钥
    /// - 字段名保留 actrix_shared_key 以保持向后兼容
    pub actrix_shared_key: String,

    /// 日志级别
    ///
    /// 控制系统日志的详细程度，可选值：
    /// - "trace": 最详细的调试信息
    /// - "debug": 调试信息
    /// - "info": 一般信息（推荐）
    /// - "warn": 警告信息
    /// - "error": 仅错误信息
    pub log_level: String,

    /// 日志输出目标
    ///
    /// 控制日志输出位置：
    /// - "console": 仅输出到控制台（默认）
    /// - "file": 输出到文件
    #[serde(default = "default_log_output")]
    pub log_output: String,

    /// 日志轮转开关
    ///
    /// 当 log_output = "file" 时有效：
    /// - true: 按天轮转日志文件
    /// - false: 追加到单个文件
    #[serde(default)]
    pub log_rotate: bool,

    /// 日志文件路径
    ///
    /// 当 log_output = "file" 时有效
    #[serde(default = "default_log_path")]
    pub log_path: String,

    /// OpenTelemetry 追踪配置（可选）
    ///
    /// 配置分布式追踪系统，支持导出到 Jaeger/Grafana Tempo 等 OTLP 后端。
    /// 需要编译时启用 `opentelemetry` feature。
    #[serde(default)]
    pub tracing: TracingConfig,
}

fn default_enable() -> u8 {
    6 // STUN + TURN (默认启用 ICE 服务)
}

fn default_log_output() -> String {
    "console".to_string()
}

fn default_log_path() -> String {
    "logs/".to_string()
}

impl Default for ActrixConfig {
    fn default() -> Self {
        Self {
            enable: default_enable(), // 默认启用 STUN + TURN
            name: "actrix-default".to_string(),
            env: "dev".to_string(),
            user: None,
            group: None,
            pid: Some("logs/actrix.pid".to_string()),
            bind: BindConfig::default(),
            turn: TurnConfig::default(),
            location_tag: "default-location".to_string(),
            supervisor: None,
            services: ServicesConfig::default(),
            sqlite: "database.db".to_string(),
            actrix_shared_key: "XDDYE8d+yMfdXcdWMrXprcUk2uzjnmoX6nCfFw1gGIg=".to_string(),
            log_level: "info".to_string(),
            log_output: default_log_output(),
            log_rotate: false,
            log_path: default_log_path(),
            tracing: TracingConfig::default(),
        }
    }
}

// 服务启用标志位常量
pub const ENABLE_SIGNALING: u8 = 0b00001;
pub const ENABLE_STUN: u8 = 0b00010;
pub const ENABLE_TURN: u8 = 0b00100;
pub const ENABLE_AIS: u8 = 0b01000;
pub const ENABLE_KS: u8 = 0b10000;

impl ActrixConfig {
    /// 检查是否启用了信令服务
    pub fn is_signaling_enabled(&self) -> bool {
        self.services
            .signaling
            .as_ref()
            .map(|sig| sig.enabled)
            .unwrap_or(false)
    }

    /// 检查是否启用了 STUN 服务
    pub fn is_stun_enabled(&self) -> bool {
        self.enable & ENABLE_STUN != 0
    }

    /// 检查是否启用了 TURN 服务
    pub fn is_turn_enabled(&self) -> bool {
        self.enable & ENABLE_TURN != 0
    }

    /// 检查是否启用了 AIS (AId Issue Service) 身份认证服务
    pub fn is_ais_enabled(&self) -> bool {
        self.services
            .ais
            .as_ref()
            .map(|ais| ais.enabled)
            .unwrap_or(false)
    }

    /// 检查是否启用了 KS (Key Server) 密钥服务
    pub fn is_ks_enabled(&self) -> bool {
        self.services
            .ks
            .as_ref()
            .map(|ks| ks.enabled)
            .unwrap_or(false)
    }

    /// 检查是否启用了 ICE 服务（STUN 或 TURN）
    pub fn is_ice_enabled(&self) -> bool {
        self.is_stun_enabled() || self.is_turn_enabled()
    }

    /// 检查是否启用了 Supervisor 客户端
    pub fn is_supervisor_enabled(&self) -> bool {
        self.supervisor.as_ref().is_some_and(|config| {
            !config.node_id.trim().is_empty() && !config.server_addr.trim().is_empty()
        })
    }

    /// 获取 PID 文件路径，如果没有配置则使用默认值
    pub fn get_pid_path(&self) -> Option<String> {
        self.pid.clone().or_else(|| {
            // 如果没有配置 pid，使用默认值 logs/actrix.pid
            Some("logs/actrix.pid".to_string())
        })
    }

    /// 获取 Actrix 内部服务通信共享密钥
    ///
    /// 此密钥用于 Actrix 系统内部服务间的认证通信，
    /// 如 AIS 与 KS 之间的服务调用。
    ///
    /// 注意：此密钥仅用于内部服务通信，不应用于对外业务
    pub fn get_actrix_shared_key(&self) -> &str {
        &self.actrix_shared_key
    }

    /// 获取追踪配置
    ///
    /// 返回 OpenTelemetry 追踪配置的引用
    pub fn tracing_config(&self) -> &TracingConfig {
        &self.tracing
    }

    /// 检查是否使用控制台日志输出
    pub fn is_console_logging(&self) -> bool {
        self.log_output == "console"
    }

    /// 检查是否应该轮转日志
    pub fn should_rotate_logs(&self) -> bool {
        self.log_output == "file" && self.log_rotate
    }

    /// 获取日志级别
    pub fn get_log_level(&self) -> &str {
        &self.log_level
    }

    /// 从文件加载配置
    pub fn from_file<P: AsRef<std::path::Path>>(
        path: P,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let path_ref = path.as_ref();

        // Check if file exists
        if !path_ref.exists() {
            return Err(format!("Configuration file does not exist: {path_ref:?}").into());
        }

        // Check if path is a file, not a directory
        if !path_ref.is_file() {
            return Err(format!("Path is not a valid file: {path_ref:?}").into());
        }

        // Read file content
        let content = std::fs::read_to_string(path_ref)?;

        // Parse TOML content
        let config: ActrixConfig = toml::from_str(&content)?;

        Ok(config)
    }

    /// 从 TOML 字符串加载配置
    pub fn from_toml(content: &str) -> Result<Self, toml::de::Error> {
        toml::from_str(content)
    }

    /// 将配置序列化为 TOML 字符串
    pub fn to_toml(&self) -> Result<String, toml::ser::Error> {
        toml::to_string(self)
    }

    /// 验证配置有效性
    ///
    /// 检查所有配置项的合法性，包括：
    /// - 必需字段是否存在
    /// - 数值范围是否合理
    /// - 文件路径是否有效
    /// - 服务配置是否一致
    pub fn validate(&self) -> Result<(), Vec<String>> {
        let mut errors = Vec::new();

        // 验证实例名称
        if self.name.trim().is_empty() {
            errors.push("Instance name cannot be empty".to_string());
        }

        // 验证环境
        if !["dev", "prod", "test"].contains(&self.env.as_str()) {
            errors.push(format!(
                "Invalid environment '{}', must be one of: dev, prod, test",
                self.env
            ));
        }

        // 验证日志级别
        if !["trace", "debug", "info", "warn", "error"].contains(&self.log_level.as_str()) {
            errors.push(format!(
                "Invalid log level '{}', must be one of: trace, debug, info, warn, error",
                self.log_level
            ));
        }

        // 验证日志输出
        if !["console", "file"].contains(&self.log_output.as_str()) {
            errors.push(format!(
                "Invalid log output '{}', must be 'console' or 'file'",
                self.log_output
            ));
        }

        // 验证 actrix_shared_key
        if self.actrix_shared_key.contains("default") || self.actrix_shared_key.contains("change") {
            errors.push("Security warning: actrix_shared_key appears to be a default value. Please change it!".to_string());
        }
        if self.actrix_shared_key.len() < 16 {
            errors.push("Security warning: actrix_shared_key is too short, recommend at least 16 characters".to_string());
        }

        // 验证 SQLite 路径
        if self.sqlite.trim().is_empty() {
            errors.push("SQLite database path cannot be empty".to_string());
        }

        // 验证追踪配置
        if let Err(e) = self.tracing.validate() {
            errors.push(format!("Tracing configuration error: {e}"));
        }

        // 验证 TURN 配置（如果启用）
        if self.is_turn_enabled() {
            if self.turn.advertised_ip.trim().is_empty() {
                errors.push("TURN advertised_ip is required when TURN is enabled".to_string());
            }
            if self.turn.realm.trim().is_empty() {
                errors.push("TURN realm is required when TURN is enabled".to_string());
            }
            // 验证 advertised_ip 格式
            if self.turn.advertised_ip.parse::<std::net::IpAddr>().is_err() {
                errors.push(format!(
                    "Invalid TURN advertised_ip '{}', must be a valid IP address",
                    self.turn.advertised_ip
                ));
            }
        }

        // 验证 KS 配置（如果启用）
        if let Some(ref ks) = self.services.ks {
            if ks.enabled {
                // 验证存储配置
                match ks.storage.backend {
                    StorageBackend::Sqlite => {
                        if let Some(ref sqlite_cfg) = ks.storage.sqlite {
                            if sqlite_cfg.path.trim().is_empty() {
                                errors.push(
                                    "KS SQLite storage is configured but path is empty".to_string(),
                                );
                            }
                        } else {
                            errors.push(
                                "KS is configured to use SQLite but sqlite config is missing"
                                    .to_string(),
                            );
                        }
                    }
                    StorageBackend::Redis => {
                        if ks.storage.redis.is_none() {
                            errors.push(
                                "KS is configured to use Redis but redis config is missing"
                                    .to_string(),
                            );
                        }
                    }
                    StorageBackend::Postgres => {
                        if ks.storage.postgres.is_none() {
                            errors.push(
                                "KS is configured to use PostgreSQL but postgres config is missing"
                                    .to_string(),
                            );
                        }
                    }
                }
            }
        }

        // 验证 AIS 配置（如果启用）
        if let Some(ref ais) = self.services.ais {
            if ais.enabled {
                // 检查是否能获取 KS 配置（显式配置或自动默认）
                if ais.get_ks_client_config(self).is_none() {
                    errors.push(
                        "AIS service is enabled but no KS available: \
                        either configure services.ais.dependencies.ks or enable local KS service"
                            .to_string(),
                    );
                }
            }
        }

        // 生产环境额外检查
        if self.env == "prod" {
            // 生产环境应使用 HTTPS
            if let Some(ref https) = self.bind.https {
                if https.port == 0 {
                    errors.push(
                        "Production environment should enable HTTPS with valid port".to_string(),
                    );
                }
            } else {
                errors.push("Production environment should enable HTTPS".to_string());
            }

            // 生产环境应使用文件日志
            if self.log_output == "console" {
                errors.push("Warning: Production environment should use file logging (log_output = \"file\")".to_string());
            }

            // 生产环境建议启用日志轮转
            if self.log_output == "file" && !self.log_rotate {
                errors.push("Warning: Production environment should enable log rotation (log_rotate = true)".to_string());
            }
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ::ks::KsServiceConfig;

    #[test]
    fn test_default_config() {
        let config = ActrixConfig::default();
        assert_eq!(config.enable, 6); // 默认启用 STUN + TURN
        assert_eq!(config.name, "actrix-default");
        assert_eq!(config.env, "dev");
        assert!(!config.is_signaling_enabled()); // Signaling 默认不启用
        assert!(config.is_stun_enabled());
        assert!(config.is_turn_enabled());
        assert!(!config.is_ais_enabled()); // AIS 默认不启用
        assert!(!config.is_ks_enabled()); // KS 默认不启用
    }

    #[test]
    fn test_toml_serialization() {
        let config = ActrixConfig::default();
        let toml_str = config.to_toml().unwrap();
        assert!(toml_str.contains("enable = 6")); // STUN + TURN
        assert!(toml_str.contains("name = \"actrix-default\""));
        assert!(
            toml_str
                .contains("actrix_shared_key = \"XDDYE8d+yMfdXcdWMrXprcUk2uzjnmoX6nCfFw1gGIg=\"")
        );

        let parsed_config = ActrixConfig::from_toml(&toml_str).unwrap();
        assert_eq!(parsed_config.enable, config.enable);
        assert_eq!(parsed_config.name, config.name);
        assert_eq!(parsed_config.actrix_shared_key, config.actrix_shared_key);
    }

    #[test]
    fn test_actrix_shared_key() {
        let config = ActrixConfig::default();
        assert_eq!(
            config.get_actrix_shared_key(),
            "XDDYE8d+yMfdXcdWMrXprcUk2uzjnmoX6nCfFw1gGIg="
        );

        // 测试自定义共享密钥
        let mut custom_config = config;
        custom_config.actrix_shared_key = "custom-shared-key".to_string();
        assert_eq!(custom_config.get_actrix_shared_key(), "custom-shared-key");
    }

    #[test]
    fn test_service_flags() {
        let mut config = ActrixConfig::default();

        // 测试启用 Signaling 服务
        config.services.signaling = Some(SignalingConfig {
            enabled: true,
            server: signaling::SignalingServerConfig::default(),
            dependencies: signaling::SignalingDependencies::default(),
        });
        assert!(config.is_signaling_enabled());

        // 测试禁用 Signaling 服务
        config.services.signaling = None;
        assert!(!config.is_signaling_enabled());

        // 测试启用 AIS 服务
        config.services.ais = Some(AisConfig {
            enabled: true,
            server: ais::AisServerConfig::default(),
            dependencies: ais::AisDependencies::default(),
        });
        assert!(config.is_ais_enabled());

        // 测试禁用 AIS 服务
        config.services.ais = Some(AisConfig {
            enabled: false,
            server: ais::AisServerConfig::default(),
            dependencies: ais::AisDependencies::default(),
        });
        assert!(!config.is_ais_enabled());

        // 测试启用 KS 服务
        config.services.ks = Some(KsServiceConfig {
            enabled: true,
            ..Default::default()
        });
        assert!(config.is_ks_enabled());

        // 测试禁用 KS 服务
        config.services.ks = Some(KsServiceConfig {
            enabled: false,
            ..Default::default()
        });
        assert!(!config.is_ks_enabled());

        // 测试 ICE 服务（STUN/TURN 使用 bitmask）
        config.enable = ENABLE_STUN;
        assert!(config.is_stun_enabled());
        assert!(config.is_ice_enabled());
    }

    #[test]
    fn test_ais_auto_ks_config() {
        let mut config = ActrixConfig::default();

        // 场景 1: 启用本地 KS，AIS 不配置 KS 客户端，应自动使用本地 KS
        config.services.ks = Some(KsServiceConfig {
            enabled: true,
            ..Default::default()
        });

        config.services.ais = Some(AisConfig {
            enabled: true,
            server: ais::AisServerConfig::default(),
            dependencies: ais::AisDependencies { ks: None }, // 未配置 KS
        });

        // 应该能获取到自动生成的 KS 配置
        let ks_config = config
            .services
            .ais
            .as_ref()
            .unwrap()
            .get_ks_client_config(&config);
        assert!(ks_config.is_some());
        let ks_config = ks_config.unwrap();
        // gRPC 默认端口 50052
        assert_eq!(ks_config.endpoint, "http://127.0.0.1:50052");

        // 场景 2: 显式配置 KS 客户端，应使用显式配置
        config.services.ais = Some(AisConfig {
            enabled: true,
            server: ais::AisServerConfig::default(),
            dependencies: ais::AisDependencies {
                ks: Some(crate::config::ks::KsClientConfig {
                    endpoint: "http://remote-ks:50052".to_string(),
                    #[allow(deprecated)]
                    psk: "custom".to_string(),
                    timeout_seconds: 10,
                    enable_tls: false,
                    tls_domain: None,
                    ca_cert: None,
                    client_cert: None,
                    client_key: None,
                }),
            },
        });

        let ks_config = config
            .services
            .ais
            .as_ref()
            .unwrap()
            .get_ks_client_config(&config);
        assert!(ks_config.is_some());
        let ks_config = ks_config.unwrap();
        assert_eq!(ks_config.endpoint, "http://remote-ks:50052"); // 使用显式配置

        // 场景 3: 没有本地 KS，也没有显式配置，应返回 None
        config.services.ks = None;
        config.services.ais = Some(AisConfig {
            enabled: true,
            server: ais::AisServerConfig::default(),
            dependencies: ais::AisDependencies { ks: None },
        });

        let ks_config = config
            .services
            .ais
            .as_ref()
            .unwrap()
            .get_ks_client_config(&config);
        assert!(ks_config.is_none());
    }

    #[test]
    fn test_signaling_auto_ks_config() {
        let mut config = ActrixConfig::default();

        // 启用本地 KS
        config.services.ks = Some(KsServiceConfig {
            enabled: true,
            ..Default::default()
        });

        config.services.signaling = Some(SignalingConfig {
            enabled: true,
            server: signaling::SignalingServerConfig::default(),
            dependencies: signaling::SignalingDependencies {
                ks: None,
                ais: None,
            },
        });

        // Signaling 应该能获取到自动生成的 KS 配置
        let ks_config = config
            .services
            .signaling
            .as_ref()
            .unwrap()
            .get_ks_client_config(&config);
        assert!(ks_config.is_some());
        let ks_config = ks_config.unwrap();
        // gRPC 默认端口 50052
        assert_eq!(ks_config.endpoint, "http://127.0.0.1:50052");
    }
}
