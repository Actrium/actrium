# 服务管理抽象架构（细粒度控制版）

本模块提供了一个通用的服务管理抽象，用于细粒度地管理不同类型的服务。与之前的粗粒度设计不同，现在可以独立控制每个具体的服务：

- **ICE服务**: `STUN`、`TURN`
- **HTTP路由服务**: `Admin`、`Authority`、`Signaling`、`Status`

## 核心设计

### 1. HttpRouterService Trait

HTTP路由服务为axum提供路由器，多个HTTP服务共享同一个HTTP服务器：

```rust
#[async_trait]
pub trait HttpRouterService: Send + Sync + Debug {
    fn info(&self) -> &ServiceInfo;
    fn info_mut(&mut self) -> &mut ServiceInfo;
    async fn build_router(&mut self) -> Result<Router>;
    fn route_prefix(&self) -> &str; // 如 "/admin", "/status" 等
}
```

### 2. IceService Trait

ICE服务独立运行UDP服务器：

```rust
#[async_trait]
pub trait IceService: Send + Sync + Debug {
    fn info(&self) -> &ServiceInfo;
    fn info_mut(&mut self) -> &mut ServiceInfo;
    async fn start(&mut self, shutdown_rx: Receiver<()>) -> Result<()>;
    async fn stop(&mut self) -> Result<()>;
}
```

### 3. ServiceContainer 枚举

分为两大类服务容器：

```rust
pub enum ServiceContainer {
    HttpRouter(Box<dyn HttpRouterService>), // HTTP路由服务
    Ice(Box<dyn IceService>),               // ICE服务
}
```

### 4. ServiceManager

智能管理不同类型的服务：

- **HTTP服务**: 合并所有HTTP路由服务到单个axum服务器
- **ICE服务**: 每个ICE服务独立运行

## 具体服务实现

### ICE 服务（ice.rs）

- **StunService**: 独立的STUN服务器
- **TurnService**: TURN服务器（包含内置STUN支持）

### HTTP 路由服务（http.rs）

- **AdminService**: 管理员API (`/admin`)
- **AuthorityService**: 认证授权服务 (`/authority`)  
- **SignalingService**: WebRTC信令服务 (`/signaling`)
- **StatusService**: 状态监控API (`/status`)

## 使用示例

```rust
use service::{
    ServiceManager, ServiceContainer,
    AdminService, StatusService, SignalingService, AuthorityService,
    StunService, TurnService
};

async fn main() -> Result<()> {
    let config = Config::from_file("config.toml")?;
    let mut service_manager = ServiceManager::new(config);
    
    // 添加ICE服务 - 细粒度控制
    if config.is_turn_enabled() {
        let turn_service = TurnService::new(config.clone());
        service_manager.add_service(ServiceContainer::turn(turn_service));
    } else if config.is_stun_enabled() {
        let stun_service = StunService::new(config.clone());
        service_manager.add_service(ServiceContainer::stun(stun_service));
    }
    
    // 添加HTTP路由服务 - 每个服务独立控制
    let admin_service = AdminService::new(config.clone());
    service_manager.add_service(ServiceContainer::admin(admin_service));
    
    let status_service = StatusService::new(config.clone());
    service_manager.add_service(ServiceContainer::status(status_service));
    
    // 可选添加其他服务
    if config.is_signaling_enabled() {
        let signaling_service = SignalingService::new(config.clone());
        service_manager.add_service(ServiceContainer::signaling(signaling_service));
    }
    
    // 启动所有服务
    service_manager.start_all().await?;
    
    // 等待关闭信号
    service_manager.wait_for_shutdown().await;
    
    Ok(())
}
```

## 运行效果

启动后会看到类似的输出：

```
🚀 启动 WebRTC 辅助服务器集群
📊 计划启动的服务:
  - TURN Server (UDP, 包含内置 STUN 支持)
  - Admin API Service (/admin)
  - Status API Service (/status)
  - Authority Service (/authority)
  - Signaling WebSocket Service (/signaling)
✅ 所有服务已启动
📡 HTTP服务器监听在: https://0.0.0.0:8443
🔧 可用的API端点:
  - https://0.0.0.0:8443/admin/health
  - https://0.0.0.0:8443/status/health
  - https://0.0.0.0:8443/authority/health
  - https://0.0.0.0:8443/signaling/health
```

## 架构优势

### 1. **细粒度控制**
- 每个服务独立配置和管理
- 可以选择性启用/禁用具体服务
- 便于调试和测试单个服务

### 2. **资源优化**
- HTTP服务共享单个axum服务器
- ICE服务独立运行，互不干扰
- 统一的关闭信号管理

### 3. **扩展性**
- 添加新的HTTP路由服务：实现`HttpRouterService` trait
- 添加新的ICE服务：实现`IceService` trait
- 在`ServiceContainer`中添加对应的构造函数

### 4. **类型安全**
- 使用枚举而不是trait object
- 编译时确保服务类型正确
- 明确的服务分类和管理

## 实际集成

当前的HTTP服务实现使用简化的路由器，实际项目中需要：

1. **Admin服务**: 调用`admin` crate的路由器构建函数
2. **Authority服务**: 调用`authority` crate的路由器构建函数
3. **Signaling服务**: 调用`signaling` crate的路由器构建函数
4. **Status服务**: 集成系统监控和健康检查功能

例如：

```rust
// 在AdminService::build_router()中
async fn build_router(&mut self) -> Result<Router> {
    let admin_config = admin::AdminConfig {
        secret_key: self.config.admin.secret_key.clone(),
        private_key_path: self.config.admin.private_key_path.clone(),
        token_expire_seconds: self.config.admin.token_expire_seconds as i64,
    };
    Ok(admin::create_admin_router(admin_config))
}
```

这种设计完美满足了你要求的细粒度服务控制，同时保持了代码的清晰性和可维护性。 