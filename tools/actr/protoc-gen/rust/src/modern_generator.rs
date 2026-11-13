//! Modern code generator
//!
//! Generates code based on the actual actr-framework architecture:
//! - RpcRequest impls: type-safe Request -> Response association
//! - {Service}Handler trait: user-implemented business logic interface
//! - ContextExt client helpers (dependency side)
//!
//! The unified workload-level dispatch (`actr_dispatch.rs`) and the
//! aggregate `ActrHandler` adapter (`actr_handler.rs`) live in their own
//! generators; the former per-service `{Service}Workload` /
//! `{Service}Dispatcher` wrappers are no longer generated.

use actr_protocol::ReplyMode;
use anyhow::{Result, anyhow};
use heck::ToSnakeCase;
use proc_macro2::TokenStream;
use prost_types::MethodDescriptorProto;
use quote::{format_ident, quote};
use std::collections::HashMap;

use crate::payload_type_extractor::{
    ExplicitPayloadTypeMap, MethodKey, extract_payload_type_or_default,
};

/// Code generator role
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GeneratorRole {
    /// Generate export-side code for exports
    ServerSide,
    /// Generate dependency-side code for dependencies
    ClientSide,
}

/// Modern code generator
pub struct ModernGenerator {
    package_name: String,
    service_name: String,
    role: GeneratorRole,
    /// Fully-qualified proto message name (e.g. `ask.ContinuePromptResultStreamsRequest`)
    /// -> declaring proto package, built from `CodeGeneratorRequest.proto_file`.
    /// Used to import the declaring module for imported RPC message types so
    /// they resolve to their real owner instead of the current service's
    /// package.
    type_owner: HashMap<String, String>,
    /// `route_key -> ReplyMode` map, extracted from the proto `actr.reply_mode`
    /// method option via `prost-reflect` (see `option_extractor`). Methods not
    /// present default to `ReplyMode::Return`.
    reply_modes: HashMap<String, ReplyMode>,
    /// Explicitly declared `(actr.payload_type)` values recovered from the raw
    /// request bytes (prost drops the extension as an unknown field). Methods
    /// absent from the map fall back per `extract_payload_type_or_default`.
    explicit_payload_types: ExplicitPayloadTypeMap,
}

impl ModernGenerator {
    pub fn new(
        package_name: &str,
        service_name: &str,
        role: GeneratorRole,
        type_owner: HashMap<String, String>,
        reply_modes: HashMap<String, ReplyMode>,
    ) -> Self {
        Self::new_with_payload_types(
            package_name,
            service_name,
            role,
            type_owner,
            reply_modes,
            ExplicitPayloadTypeMap::new(),
        )
    }

    /// Create a generator with explicit protobuf `(actr.payload_type)` values.
    pub fn new_with_payload_types(
        package_name: &str,
        service_name: &str,
        role: GeneratorRole,
        type_owner: HashMap<String, String>,
        reply_modes: HashMap<String, ReplyMode>,
        explicit_payload_types: ExplicitPayloadTypeMap,
    ) -> Self {
        Self {
            package_name: package_name.to_string(),
            service_name: service_name.to_string(),
            role,
            type_owner,
            reply_modes,
            explicit_payload_types,
        }
    }

    /// Generate complete code
    pub fn generate(&self, methods: &[MethodDescriptorProto]) -> Result<String> {
        match self.role {
            GeneratorRole::ServerSide => self.generate_server_code(methods),
            GeneratorRole::ClientSide => self.generate_client_code(methods),
        }
    }

    /// Generate export-side code (exports)
    fn generate_server_code(&self, methods: &[MethodDescriptorProto]) -> Result<String> {
        let sections = [
            // 1. Generate imports
            self.generate_imports(methods),
            // 2. Generate RpcRequest trait impls (type-safe Request -> Response association)
            self.generate_message_impls(methods)?,
            // 3. Generate Handler trait (user-implemented interface)
            self.generate_handler_trait(methods)?,
            // 4. Generate usage docs
            self.generate_usage_docs(methods)?,
        ];

        Ok(sections.join("\n\n"))
    }

    /// Generate dependency-side code (dependencies)
    fn generate_client_code(&self, methods: &[MethodDescriptorProto]) -> Result<String> {
        let sections = [
            // 1. Generate imports
            self.generate_imports(methods),
            // 2. Generate RpcRequest trait impls (client also needs them for type-safe calls)
            self.generate_message_impls(methods)?,
            // 3. Generate Context extension methods
            self.generate_context_extensions(methods)?,
            // 4. Generate usage docs
            self.generate_client_usage_docs(methods)?,
        ];

        Ok(sections.join("\n\n"))
    }

    /// Whether *any* method on this service is `ReplyMode::Explicit`. Drives
    /// conditional emission of `Reply` / `ReplyMode` imports, the
    /// `reply_mode_for_route` override, and `fn reply_mode()` impls so services
    /// with no EXPLICIT methods stay byte-identical to pre-RFC-0323 codegen.
    fn has_explicit(&self, methods: &[MethodDescriptorProto]) -> bool {
        methods
            .iter()
            .any(|m| self.reply_mode_for(m) == ReplyMode::Explicit)
    }

    /// Generate import statements
    fn generate_imports(&self, methods: &[MethodDescriptorProto]) -> String {
        let module_paths = self
            .declaring_packages(methods)
            .into_iter()
            .filter(|package| !package.is_empty())
            .map(|package| format!("//   super::{package}::<Message>"))
            .collect::<Vec<_>>()
            .join("\n");

        // Only pull in `Reply` / `ReplyMode` when the service actually has an
        // EXPLICIT method, so all-RETURN services stay byte-identical to
        // pre-RFC-0323 codegen.
        let (framework_extra, protocol_extra) = if self.has_explicit(methods) {
            (", Reply", ", ReplyMode")
        } else {
            ("", "")
        };

        // The per-service output no longer emits dispatch code (the unified
        // `actr_dispatch.rs` owns it), so the imports are role-scoped: the
        // server side only needs the handler-trait surface, the client side
        // the ContextExt caller helpers.
        let (framework_use, protocol_use) = match self.role {
            GeneratorRole::ServerSide => (
                format!("use actr_framework::{{Context{framework_extra}}};"),
                format!(
                    "use actr_protocol::{{ActorResult, PayloadType, RpcRequest{protocol_extra}}};"
                ),
            ),
            GeneratorRole::ClientSide => (
                format!("use actr_framework::{{Context, Dest{framework_extra}}};"),
                format!(
                    "use actr_protocol::{{ActrId, ActorResult, RpcRequest, RpcEnvelope, PayloadType{protocol_extra}}};"
                ),
            ),
        };

        format!(
            r#"// Auto-generated code - DO NOT EDIT
// Generated by actr-cli's protoc-gen-actrframework plugin
#[allow(dead_code, unused_imports)]
use async_trait::async_trait;

#[allow(unused_imports)]
{framework_use}
#[allow(unused_imports)]
{protocol_use}

// Protobuf message types are referenced by declaring owner module.
{module_paths}
"#
        )
    }

    /// Distinct proto packages (as Rust module names) that declare the message
    /// types referenced by `methods`, resolved through the owner map. Types not
    /// found in the map (well-known types, external imports) fall back to the
    /// current service's package.
    fn declaring_packages(&self, methods: &[MethodDescriptorProto]) -> Vec<String> {
        let mut packages = Vec::new();
        for method in methods {
            for type_name in [method.input_type(), method.output_type()] {
                let module = crate::proto_module_path(&self.resolve_declaring_package(type_name));
                if !module.is_empty() && !packages.contains(&module) {
                    packages.push(module);
                }
            }
        }
        packages
    }

    fn resolve_declaring_package(&self, type_name: &str) -> String {
        let cleaned = type_name.trim_start_matches('.');
        if let Some(package) = self.type_owner.get(cleaned) {
            return package.clone();
        }
        self.package_name.clone()
    }

    /// The route_key the generator emits for `method`, mirroring the runtime
    /// dispatch match-arm key. Package-less protos omit the package segment
    /// rather than producing a leading dot.
    fn route_key_for(&self, method: &MethodDescriptorProto) -> String {
        if self.package_name.is_empty() {
            format!("{}.{}", self.service_name, method.name())
        } else {
            format!(
                "{}.{}.{}",
                self.package_name,
                self.service_name,
                method.name()
            )
        }
    }

    /// The `ReplyMode` declared for `method` via the `actr.reply_mode` proto
    /// option. Absent → `ReplyMode::Return`.
    fn reply_mode_for(&self, method: &MethodDescriptorProto) -> ReplyMode {
        self.reply_modes
            .get(&self.route_key_for(method))
            .copied()
            .unwrap_or(ReplyMode::Return)
    }

    /// Generate RpcRequest trait implementations
    ///
    /// Generates RpcRequest trait impls for each RPC method's Request type,
    /// associating it with the corresponding Response type. This enables
    /// type-safe API calls on the calling side:
    ///
    /// ```rust,ignore
    /// let response: EchoResponse = ctx.call(&target, request, 30_000).await?;
    /// //              ^^^^^^^^^^^^ inferred from EchoRequest::Response
    /// ```
    fn generate_message_impls(&self, methods: &[MethodDescriptorProto]) -> Result<String> {
        let mut impls = Vec::new();

        for method in methods {
            let input_type_path = self.message_type_path(method.input_type())?;
            let output_type_path = self.message_type_path(method.output_type())?;

            // Generate the same route key used for `(actr.reply_mode)` lookup
            // and by the unified dispatcher.
            let route_key = self.route_key_for(method);

            // Explicit `(actr.payload_type)` values win over the fallback.
            // The key is built from the same fields the extraction side
            // used, decoupled from the emitted route_key format.
            let key = MethodKey::new(&self.package_name, &self.service_name, method.name());
            let payload_type = self
                .explicit_payload_types
                .get(&key)
                .copied()
                .unwrap_or_else(|| extract_payload_type_or_default(method));

            // Generate PayloadType enum path (cannot use quote! as it adds quotes)
            let payload_type_code = payload_type.as_rust_variant();

            // EXPLICIT methods override reply_mode(); RETURN methods inherit the
            // trait default (Return) so all-RETURN codegen stays byte-identical.
            let reply_mode_arm = match self.reply_mode_for(method) {
                ReplyMode::Explicit => {
                    r#"
    fn reply_mode() -> ReplyMode {
        ReplyMode::Explicit
    }"#
                }
                ReplyMode::Return => "",
            };

            // Manually construct code string to avoid quote! adding quotes
            let impl_code = format!(
                r#"/// RpcRequest trait implementation - associates Request and Response types
///
/// This enables type-safe RPC calls with automatic response type inference:
/// ```rust,ignore
/// let response: {output_type_path} = ctx.call(&target, request, 30_000).await?;
/// ```
impl RpcRequest for {input_type_path} {{
    type Response = {output_type_path};

    fn route_key() -> &'static str {{
        "{route_key}"
    }}

    fn payload_type() -> PayloadType {{
        {payload_type_code}
    }}{reply_mode_arm}
}}"#
            );

            impls.push(impl_code);
        }

        Ok(impls.join("\n\n"))
    }

    /// Generate Handler trait
    fn generate_handler_trait(&self, methods: &[MethodDescriptorProto]) -> Result<String> {
        let handler_trait_name = format!("{}Handler", self.service_name);
        let handler_trait_ident = format_ident!("{}", handler_trait_name);

        let mut method_sigs = Vec::new();
        for method in methods {
            let method_name = method.name().to_snake_case();
            let method_ident = format_ident!("{}", method_name);
            let input_type = self.message_type_tokens(method.input_type())?;
            let output_type = self.message_type_tokens(method.output_type())?;

            // EXPLICIT methods take a `Reply<T>` handle and return `ActorResult<()>`;
            // the reply is delivered later via `reply.send()`. RETURN methods keep
            // the byte-identical `-> ActorResult<T>` signature.
            if self.reply_mode_for(method) == ReplyMode::Explicit {
                method_sigs.push(quote! {
                    /// RPC method (ReplyMode::Explicit): #method_name
                    ///
                    /// Reply is deferred — call `reply.send(Ok(resp))` or move the
                    /// handle into a spawned task. Dropping an un-`send` handle
                    /// sends an `Internal` error (first-write-wins).
                    async fn #method_ident<C: Context>(
                        &self,
                        req: #input_type,
                        ctx: &C,
                        reply: actr_framework::Reply<#output_type>,
                    ) -> ActorResult<()>;
                });
            } else {
                method_sigs.push(quote! {
                    /// RPC method: #method_name
                    async fn #method_ident<C: Context>(
                        &self,
                        req: #input_type,
                        ctx: &C,
                    ) -> ActorResult<#output_type>;
                });
            }
        }

        // Per Option U gamma-unified §4.5 / Phase 6b the handler trait keeps
        // its minimal bound (`MaybeSendSync + 'static`) so user structs do
        // not need an extra `Sized` clause.
        let handler_trait_without_attr = quote! {
            /// Service handler trait - users must implement this trait
            ///
            /// # Example
            ///
            /// ```rust,ignore
            /// pub struct MyService { /* ... */ }
            ///
            /// #[async_trait]
            /// impl #handler_trait_ident for MyService {
            ///     async fn method_name(&self, req: Request, ctx: &Context) -> ActorResult<Response> {
            ///         // Business logic
            ///         Ok(Response::default())
            ///     }
            /// }
            /// ```
            pub trait #handler_trait_ident: actr_framework::MaybeSendSync + 'static {
                #(#method_sigs)*
            }
        };

        // Manually add #[async_trait] attribute to avoid quote! inserting spaces.
        //
        // `?Send` on wasm32 so the generated impl matches `Context`'s
        // per-target auto-trait contract (gamma-unified §3.1). Native keeps the
        // default Send-future form so handler futures compose with tokio
        // multi-threaded executors.
        Ok(format!(
            "#[cfg_attr(not(target_arch = \"wasm32\"), async_trait)]\n\
             #[cfg_attr(target_arch = \"wasm32\", async_trait(?Send))]\n\
             {handler_trait_without_attr}"
        ))
    }

    /// Generate Context extension methods (client)
    fn generate_context_extensions(&self, methods: &[MethodDescriptorProto]) -> Result<String> {
        let client_struct_name = format!("{}Client", self.service_name);
        let client_ident = format_ident!("{}", client_struct_name);

        let mut client_methods = Vec::new();
        for method in methods {
            let method_name = method.name().to_snake_case();
            let method_ident = format_ident!("{}", method_name);
            let input_type = self.message_type_tokens(method.input_type())?;
            let output_type = self.message_type_tokens(method.output_type())?;

            client_methods.push(quote! {
                /// Call remote method: #method_name
                pub async fn #method_ident(
                    &self,
                    target: ActrId,
                    req: #input_type,
                    timeout_ms: i64,
                ) -> ActorResult<#output_type> {
                    self.ctx.call(&Dest::from(target), req, timeout_ms).await
                }
            });
        }

        // Generate Context extension
        let extension_method_name = self.service_name.to_snake_case();
        let extension_method_ident = format_ident!("{}", extension_method_name);

        Ok(quote! {
            /// Client interface
            ///
            /// Provides type-safe remote call methods
            pub struct #client_ident<'a, C: Context> {
                ctx: &'a C,
            }

            impl<'a, C: Context> #client_ident<'a, C> {
                #(#client_methods)*
            }

            /// Context extension trait
            ///
            /// Adds convenient caller methods to Context
            pub trait ContextExt {
                fn #extension_method_ident(&self) -> #client_ident<'_, Self> where Self: Sized + Context;
            }

            impl<T: Context> ContextExt for T {
                fn #extension_method_ident(&self) -> #client_ident<'_, Self> {
                    #client_ident { ctx: self }
                }
            }
        }
        .to_string())
    }

    /// Generate export-side usage docs
    fn generate_usage_docs(&self, methods: &[MethodDescriptorProto]) -> Result<String> {
        let handler_trait = format!("{}Handler", self.service_name);
        let first_method = methods.first();

        let example_method = if let Some(method) = first_method {
            let method_name = method.name().to_snake_case();
            let input_type = self.message_type_path(method.input_type())?;
            let output_type = self.message_type_path(method.output_type())?;
            format!(
                r#"
    async fn {method_name}(&self, req: {input_type}, ctx: &Context) -> ActorResult<{output_type}> {{
        // Implement business logic
        Ok({output_type}::default())
    }}"#
            )
        } else {
            "    // Implement methods...".to_string()
        };

        Ok(format!(
            r#"/*
## Usage Example

### 1. Implement Business Logic

```rust
use actr_framework::Context;
use actr_protocol::ActorResult;
use async_trait::async_trait;

pub struct MyService {{
    // Business state
}}

#[async_trait]
impl {handler_trait} for MyService {{
{example_method}
}}
```

### 2. Register the Entry Point

The workload entry composes every per-service implementation through the
generated `actr_handler::ActrHandler` adapter into the unified
`actr_dispatch::ActrWorkload`:

```rust
use generated::actr_dispatch::ActrWorkload;
use generated::actr_handler::ActrHandler;

actr_framework::entry!(
    ActrWorkload<ActrHandler<MyService>>,
    ActrWorkload::new(ActrHandler::new(MyService::new()))
);
```

## Architecture

- **{handler_trait}**: user-implemented business logic interface
- **actr_handler::ActrHandler**: generated adapter composing every per-service handler
- **actr_dispatch::ActrWorkload / ActrDispatcher**: unified static dispatch for the whole workload

Users only need to implement {handler_trait}; the framework auto-provides routing and workload capabilities.
*/
"#
        ))
    }

    /// Generate dependency-side usage docs
    fn generate_client_usage_docs(&self, methods: &[MethodDescriptorProto]) -> Result<String> {
        let service_name_snake = self.service_name.to_snake_case();
        let method_name_snake = methods
            .first()
            .map(|m| m.name().to_snake_case())
            .unwrap_or("unknown_method".to_string());

        Ok(format!(
            r#"/*
## Dependency Usage Example

```rust
use actr_framework::Context;
use actr_protocol::{{ActorResult, ActrId}};

async fn call_remote_service(ctx: &impl Context, target: ActrId) -> ActorResult<()> {{
    use super::ContextExt;

    // Type-safe remote call
    let response = ctx.{service_name_snake}()
        .{method_name_snake}(target, request, 30_000)
        .await?;

    Ok(())
}}
```

## Compile-time Routing

All remote calls determine the target service and method at compile time — no runtime lookup needed.
*/
"#
        ))
    }

    /// Extract message type name
    fn extract_message_type(&self, type_name: &str) -> Result<String> {
        let cleaned = type_name.trim_start_matches('.');
        if let Some(last_part) = cleaned.split('.').next_back() {
            Ok(last_part.to_string())
        } else {
            Ok(cleaned.to_string())
        }
    }

    fn message_type_path(&self, type_name: &str) -> Result<String> {
        let cleaned = type_name.trim_start_matches('.');
        let message_path = if let Some(package) = self.type_owner.get(cleaned) {
            cleaned
                .strip_prefix(package)
                .and_then(|name| name.strip_prefix('.'))
                .unwrap_or(cleaned)
                .split('.')
                .map(str::to_string)
                .collect()
        } else {
            vec![self.extract_message_type(type_name)?]
        };
        let Some((short_type, parents)) = message_path.split_last() else {
            return Ok(cleaned.to_string());
        };
        let module = crate::proto_module_path(&self.resolve_declaring_package(type_name));
        if module.is_empty() {
            Ok(short_type.clone())
        } else {
            let nested_modules = parents
                .iter()
                .map(|part| part.to_snake_case())
                .collect::<Vec<_>>()
                .join("::");
            if nested_modules.is_empty() {
                Ok(format!("super::{module}::{short_type}"))
            } else {
                Ok(format!("super::{module}::{nested_modules}::{short_type}"))
            }
        }
    }

    fn message_type_tokens(&self, type_name: &str) -> Result<TokenStream> {
        let type_path = self.message_type_path(type_name)?;
        type_path
            .parse()
            .map_err(|err| anyhow!("Invalid generated Rust type path '{}': {}", type_path, err))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use prost_types::MethodDescriptorProto;

    #[test]
    fn test_extract_message_type() {
        let generator = ModernGenerator::new(
            "test.v1",
            "TestService",
            GeneratorRole::ServerSide,
            HashMap::new(),
            HashMap::new(),
        );

        assert_eq!(
            generator
                .extract_message_type(".test.v1.EchoRequest")
                .unwrap(),
            "EchoRequest"
        );
        assert_eq!(
            generator
                .extract_message_type("test.v1.EchoResponse")
                .unwrap(),
            "EchoResponse"
        );
        assert_eq!(
            generator.extract_message_type("SimpleMessage").unwrap(),
            "SimpleMessage"
        );
    }

    #[test]
    fn test_generate_message_impls_includes_payload_type() {
        let generator = ModernGenerator::new(
            "test.v1",
            "TestService",
            GeneratorRole::ServerSide,
            HashMap::new(),
            HashMap::new(),
        );

        let methods = vec![MethodDescriptorProto {
            name: Some("Echo".to_string()),
            input_type: Some(".test.v1.EchoRequest".to_string()),
            output_type: Some(".test.v1.EchoResponse".to_string()),
            options: None,
            ..Default::default()
        }];

        let result = generator.generate_message_impls(&methods).unwrap();

        // Debug: print generated code
        eprintln!("Generated code:\n{result}");

        // Verify generated code contains payload_type() method
        assert!(
            result.contains("fn payload_type"),
            "Should contain 'fn payload_type'"
        );
        assert!(
            result.contains("PayloadType"),
            "Should contain 'PayloadType'"
        );
        // Verify default is RpcReliable
        assert!(
            result.contains("RpcReliable"),
            "Should contain 'RpcReliable'"
        );
    }

    #[test]
    fn test_generate_imports_includes_payload_type() {
        let generator = ModernGenerator::new(
            "test.v1",
            "TestService",
            GeneratorRole::ServerSide,
            HashMap::new(),
            HashMap::new(),
        );
        let imports = generator.generate_imports(&[]);

        // Verify PayloadType is imported (server-side imports are
        // handler-surface scoped; dispatch imports live in actr_dispatch.rs).
        assert!(imports.contains("PayloadType"));
        assert!(imports.contains("use actr_protocol::{ActorResult, PayloadType, RpcRequest}"));
        assert!(imports.contains("use actr_framework::{Context}"));
        assert!(!imports.contains("MessageDispatcher"));
        assert!(!imports.contains("bytes::Bytes"));
    }

    #[test]
    fn test_generate_client_code() {
        let generator = ModernGenerator::new(
            "test.v1",
            "TestService",
            GeneratorRole::ClientSide,
            HashMap::new(),
            HashMap::new(),
        );

        let methods = vec![MethodDescriptorProto {
            name: Some("Echo".to_string()),
            input_type: Some(".test.v1.EchoRequest".to_string()),
            output_type: Some(".test.v1.EchoResponse".to_string()),
            options: None,
            ..Default::default()
        }];

        let result = generator.generate(&methods);
        assert!(result.is_ok());

        let code = result.unwrap();
        // Client code should also contain RpcRequest impl
        assert!(code.contains("impl RpcRequest for super::test::v1::EchoRequest"));
        assert!(code.contains("fn payload_type() -> PayloadType"));
        assert!(code.contains("timeout_ms : i64"));
        assert!(code.contains("self . ctx . call (& Dest :: from (target) , req , timeout_ms)"));
        assert!(code.contains("use actr_framework::{Context, Dest}"));
        assert!(code.contains(
            "use actr_protocol::{ActrId, ActorResult, RpcRequest, RpcEnvelope, PayloadType}"
        ));
        assert!(!code.contains("MessageDispatcher"));
    }

    #[test]
    fn test_generate_server_code() {
        let generator = ModernGenerator::new(
            "test.v1",
            "TestService",
            GeneratorRole::ServerSide,
            HashMap::new(),
            HashMap::new(),
        );

        let methods = vec![MethodDescriptorProto {
            name: Some("Echo".to_string()),
            input_type: Some(".test.v1.EchoRequest".to_string()),
            output_type: Some(".test.v1.EchoResponse".to_string()),
            options: None,
            ..Default::default()
        }];

        let result = generator.generate(&methods);
        assert!(result.is_ok());

        let code = result.unwrap();
        // Verify Handler trait was generated
        assert!(code.contains("pub trait TestServiceHandler"));
        // Verify payload_type was generated
        assert!(code.contains("fn payload_type() -> PayloadType"));
        // Per-service dispatcher wrappers are no longer generated
        assert!(!code.contains("TestServiceDispatcher"));
    }

    #[test]
    fn legacy_wrappers_are_not_generated() {
        let generator = ModernGenerator::new(
            "test.v1",
            "TestService",
            GeneratorRole::ServerSide,
            HashMap::new(),
            HashMap::new(),
        );
        let methods = vec![MethodDescriptorProto {
            name: Some("Echo".to_string()),
            input_type: Some(".test.v1.EchoRequest".to_string()),
            output_type: Some(".test.v1.EchoResponse".to_string()),
            ..Default::default()
        }];

        let code = generator.generate(&methods).unwrap();

        // The per-service wrappers are gone entirely — the unified
        // actr_dispatch.rs / actr_handler.rs pair owns dispatch.
        assert!(
            !code.contains("TestServiceWorkload"),
            "legacy workload must not be emitted:\n{code}"
        );
        assert!(
            !code.contains("TestServiceDispatcher"),
            "legacy dispatcher must not be emitted:\n{code}"
        );
        assert!(
            !code.contains("impl < T : TestServiceHandler >"),
            "no wrapper impls may remain:\n{code}"
        );
        assert!(
            !code.contains("MessageDispatcher for"),
            "per-service dispatch impls must not be emitted:\n{code}"
        );
        // The business-facing surface is unaffected.
        assert!(code.contains("pub trait TestServiceHandler"));
        assert!(code.contains("impl RpcRequest for super::test::v1::EchoRequest"));
        assert!(code.contains("fn payload_type() -> PayloadType"));
        // The usage docs point at the unified adapter, never the legacy
        // per-service entry.
        assert!(
            code.contains("generated::actr_handler::ActrHandler"),
            "usage docs must point at the unified adapter:\n{code}"
        );
        assert!(
            !code.contains("entry! (TestServiceWorkload"),
            "usage docs must not promote the legacy workload entry:\n{code}"
        );
    }

    #[test]
    fn test_no_service_handler_witness_in_output() {
        let generator = ModernGenerator::new(
            "test.v1",
            "TestService",
            GeneratorRole::ServerSide,
            HashMap::new(),
            HashMap::new(),
        );

        let methods = vec![MethodDescriptorProto {
            name: Some("Echo".to_string()),
            input_type: Some(".test.v1.EchoRequest".to_string()),
            output_type: Some(".test.v1.EchoResponse".to_string()),
            options: None,
            ..Default::default()
        }];

        let code = generator.generate(&methods).unwrap();
        assert!(
            !code.contains("actr_framework::ServiceHandler"),
            "ServiceHandler witness must not be emitted"
        );
    }

    #[test]
    fn test_generate_server_code_with_no_local_methods() {
        let generator = ModernGenerator::new(
            "test.v1",
            "BridgeService",
            GeneratorRole::ServerSide,
            HashMap::new(),
            HashMap::new(),
        );

        let code = generator.generate(&[]).unwrap();

        assert!(code.contains("pub trait BridgeServiceHandler"));
        // Per-service workload/dispatcher wrappers are no longer generated —
        // the unified actr_dispatch.rs owns dispatch for the whole workload.
        assert!(!code.contains("BridgeServiceWorkload"));
        assert!(!code.contains("BridgeServiceDispatcher"));
    }

    #[test]
    fn server_code_has_no_remote_actr_type_embedding() {
        // Remote forwarding lives in the unified actr_dispatch.rs; the
        // per-service output must not embed ActrType strings or forwarding
        // arms anymore.
        let generator = ModernGenerator::new(
            "demo.app",
            "DemoClientApp",
            GeneratorRole::ServerSide,
            HashMap::new(),
            HashMap::new(),
        );

        let code = generator.generate(&[]).unwrap();

        assert!(code.contains("pub trait DemoClientAppHandler"));
        assert!(!code.contains("DemoClientAppWorkload"));
        assert!(!code.contains("publisher"));
        assert!(!code.contains("discover_route_candidate"));
        assert!(!code.contains("call_raw"));
    }

    #[test]
    fn test_imported_rpc_types_import_their_declaring_package() {
        // A `data_stream_app` service references `ask.*` message types; the
        // owner map lets the generator import the `ask` module so the bare
        // type names resolve to their real owner.
        let mut type_owner = HashMap::new();
        type_owner.insert(
            "ask.ContinuePromptResultStreamsRequest".to_string(),
            "ask".to_string(),
        );
        type_owner.insert(
            "ask.ContinuePromptResultStreamsResponse".to_string(),
            "ask".to_string(),
        );
        let generator = ModernGenerator::new(
            "data_stream_app",
            "DataStreamAppService",
            GeneratorRole::ServerSide,
            type_owner,
            HashMap::new(),
        );

        let methods = vec![MethodDescriptorProto {
            name: Some("ContinuePromptResultStreams".to_string()),
            input_type: Some(".ask.ContinuePromptResultStreamsRequest".to_string()),
            output_type: Some(".ask.ContinuePromptResultStreamsResponse".to_string()),
            options: None,
            ..Default::default()
        }];

        let code = generator.generate(&methods).unwrap();

        assert!(
            !code.contains("use super::ask::*;"),
            "expected no `ask` glob import, got:\n{code}"
        );
        assert!(
            !code.contains("use super::data_stream_app::*;"),
            "expected no current package glob import, got:\n{code}"
        );
        // Handler trait references the real declaring module.
        assert!(code.contains("super :: ask :: ContinuePromptResultStreamsRequest"));
    }

    #[test]
    fn test_imported_rpc_types_are_qualified_by_owner_module() {
        let mut type_owner = HashMap::new();
        type_owner.insert("ask.Request".to_string(), "ask".to_string());
        type_owner.insert("reply.Response".to_string(), "reply".to_string());
        let generator = ModernGenerator::new(
            "local",
            "LocalService",
            GeneratorRole::ServerSide,
            type_owner,
            HashMap::new(),
        );

        let methods = vec![MethodDescriptorProto {
            name: Some("Call".to_string()),
            input_type: Some(".ask.Request".to_string()),
            output_type: Some(".reply.Response".to_string()),
            options: None,
            ..Default::default()
        }];

        let code = generator.generate(&methods).unwrap();

        assert!(
            !code.contains("use super::ask::*;"),
            "generated code should not glob-import owner modules:\n{code}"
        );
        assert!(
            code.contains("impl RpcRequest for super::ask::Request"),
            "RpcRequest impl should qualify the request owner:\n{code}"
        );
        assert!(
            code.contains("type Response = super::reply::Response;"),
            "RpcRequest impl should qualify the response owner:\n{code}"
        );
        assert!(
            code.contains("req: super::ask::Request"),
            "handler signature should qualify the request owner:\n{code}"
        );
        assert!(
            code.contains("ActorResult < super::reply::Response >")
                || code.contains("ActorResult<super::reply::Response>"),
            "handler signature should qualify the response owner:\n{code}"
        );
        // Decode-side owner qualification lives in the unified
        // actr_dispatch.rs generator; the per-service output only carries
        // the handler signature and RpcRequest impl.
    }

    #[test]
    fn test_imported_nested_rpc_types_include_parent_modules() {
        let mut type_owner = HashMap::new();
        type_owner.insert(
            "remote.Outer.InnerRequest".to_string(),
            "remote".to_string(),
        );
        type_owner.insert(
            "remote.Outer.InnerResponse".to_string(),
            "remote".to_string(),
        );
        let generator = ModernGenerator::new(
            "local",
            "LocalService",
            GeneratorRole::ServerSide,
            type_owner,
            HashMap::new(),
        );

        let methods = vec![MethodDescriptorProto {
            name: Some("Call".to_string()),
            input_type: Some(".remote.Outer.InnerRequest".to_string()),
            output_type: Some(".remote.Outer.InnerResponse".to_string()),
            options: None,
            ..Default::default()
        }];

        let code = generator.generate(&methods).unwrap();

        assert!(
            code.contains("impl RpcRequest for super::remote::outer::InnerRequest"),
            "RpcRequest impl should include nested parent module:\n{code}"
        );
        assert!(
            code.contains("type Response = super::remote::outer::InnerResponse;"),
            "response type should include nested parent module:\n{code}"
        );
    }

    #[test]
    fn explicit_reply_mode_generates_explicit_dispatch_path() {
        use actr_protocol::ReplyMode;

        let mut reply_modes = HashMap::new();
        reply_modes.insert("test.v1.Svc.Transcode".to_string(), ReplyMode::Explicit);

        let generator = ModernGenerator::new(
            "test.v1",
            "Svc",
            GeneratorRole::ServerSide,
            HashMap::new(),
            reply_modes,
        );

        let methods = vec![
            MethodDescriptorProto {
                name: Some("Echo".to_string()),
                input_type: Some(".test.v1.Req".to_string()),
                output_type: Some(".test.v1.Resp".to_string()),
                ..Default::default()
            },
            MethodDescriptorProto {
                name: Some("Transcode".to_string()),
                input_type: Some(".test.v1.Req".to_string()),
                output_type: Some(".test.v1.Resp".to_string()),
                ..Default::default()
            },
        ];

        let code = generator
            .generate(&methods)
            .expect("EXPLICIT must generate, not reject");

        // EXPLICIT handler signature takes Reply<Resp> and returns ActorResult<()>.
        assert!(
            code.contains("reply : actr_framework :: Reply"),
            "EXPLICIT handler should take Reply<Resp>:\n{code}"
        );
        assert!(
            code.contains("ActorResult < () >"),
            "EXPLICIT handler should return ActorResult<()>:\n{code}"
        );
        // Per-service dispatch arms no longer exist here (the unified
        // actr_dispatch.rs builds the Reply and maps reply_mode_for_route);
        // the per-service output only carries the EXPLICIT semantics through
        // the handler signature and the RpcRequest impl.
        assert!(
            !code.contains("reply_deliver"),
            "per-service output must not build Replies:\n{code}"
        );
        assert!(
            !code.contains("reply_mode_for_route"),
            "reply_mode_for_route lives in the unified dispatcher:\n{code}"
        );
        // RpcRequest impl for EXPLICIT method overrides reply_mode() (raw string,
        // no quote! spacing).
        assert!(
            code.contains("fn reply_mode() -> ReplyMode"),
            "EXPLICIT RpcRequest impl should override reply_mode():\n{code}"
        );
        // Reply/ReplyMode imports present (service has an EXPLICIT method).
        assert!(
            code.contains("Context, Reply"),
            "Reply should be imported:\n{code}"
        );
    }

    #[test]
    fn explicit_reply_mode_matches_package_less_route_key() {
        use actr_protocol::ReplyMode;

        let mut reply_modes = HashMap::new();
        reply_modes.insert("Svc.Transcode".to_string(), ReplyMode::Explicit);

        let generator = ModernGenerator::new(
            "",
            "Svc",
            GeneratorRole::ServerSide,
            HashMap::new(),
            reply_modes,
        );
        let methods = vec![MethodDescriptorProto {
            name: Some("Transcode".to_string()),
            input_type: Some(".Req".to_string()),
            output_type: Some(".Resp".to_string()),
            ..Default::default()
        }];

        let code = generator
            .generate(&methods)
            .expect("package-less EXPLICIT route must generate");

        assert!(
            code.contains("\"Svc.Transcode\""),
            "package-less route key must not have a leading dot:\n{code}"
        );
        assert!(
            code.contains("reply : actr_framework :: Reply"),
            "package-less EXPLICIT route should keep the Reply handler signature:\n{code}"
        );
        assert!(
            code.contains("fn reply_mode() -> ReplyMode"),
            "package-less EXPLICIT route should override reply_mode:\n{code}"
        );
    }

    #[test]
    fn return_reply_mode_default_generates_without_error() {
        let generator = ModernGenerator::new(
            "test.v1",
            "Svc",
            GeneratorRole::ServerSide,
            HashMap::new(),
            HashMap::new(),
        );

        let methods = vec![MethodDescriptorProto {
            name: Some("Echo".to_string()),
            input_type: Some(".test.v1.Req".to_string()),
            output_type: Some(".test.v1.Resp".to_string()),
            ..Default::default()
        }];

        let code = generator
            .generate(&methods)
            .expect("RETURN (default) must generate cleanly");
        // RETURN impls rely on the RpcRequest trait default for reply_mode;
        // no `fn reply_mode` is emitted so RETURN codegen stays byte-identical
        // to pre-RFC-0323 output.
        assert!(!code.contains("fn reply_mode"));
        assert!(!code.contains("reply_mode_for_route"));
        assert!(!code.contains("reply_deliver"));
        assert!(code.contains("pub trait SvcHandler"));
    }
}
