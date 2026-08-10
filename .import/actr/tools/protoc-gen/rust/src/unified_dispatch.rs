//! Unified `actr_dispatch.rs` generator.
//!
//! Emits a single workload-level dispatcher function (`actr_dispatch`) that
//! matches on `route_key` and either:
//!
//! - **Local route**: decodes the protobuf request, invokes the matching
//!   `ActrDispatchHandler` method, and encodes the response.
//! - **Remote route**: resolves the dependency alias via
//!   `Context::resolve_dependency`, discovers a route candidate, and calls
//!   `Context::call_raw` with the caller-supplied `timeout_ms` (the raw
//!   lane is fixed to `RpcReliable`).
//! - **Unknown route**: returns `ActrError::UnknownRoute`.
//!
//! The `ActrDispatchHandler` trait aggregates every local service method
//! into a single trait with uniquely-named methods
//! (`<service_snake>_<method_snake>`) so the user implements one trait for
//! the whole workload.

use crate::dispatch_spec::DispatchSpec;
use anyhow::{Result, anyhow};
use heck::ToSnakeCase;
use prost_types::MethodDescriptorProto;
use quote::{format_ident, quote};
use std::collections::HashMap;

/// Information needed to emit one local match arm in `actr_dispatch`.
struct LocalRouteInfo {
    route_key: String,
    /// Unique handler method name, e.g. `echo_service_echo`.
    handler_method: String,
    /// Fully-qualified Rust type path for the request message,
    /// e.g. `super::echo::EchoRequest`.
    input_type_path: String,
    /// Bare type name for error messages, e.g. `EchoRequest`.
    input_type_short: String,
    /// Fully-qualified Rust type path for the response message.
    output_type_path: String,
}

/// Information needed to emit one remote match arm.
struct RemoteRouteInfo {
    route_key: String,
    dependency_alias: String,
}

/// Type-owner map: fully-qualified proto type name -> declaring proto package.
///
/// Mirrors the `type_owner` map the per-service generator already builds, so
/// the unified dispatcher can qualify imported message types the same way.
pub struct UnifiedDispatcherGenerator<'a> {
    spec: &'a DispatchSpec,
    type_owners: &'a HashMap<String, String>,
}

impl<'a> UnifiedDispatcherGenerator<'a> {
    pub fn new(spec: &'a DispatchSpec, type_owners: &'a HashMap<String, String>) -> Self {
        Self { spec, type_owners }
    }

    /// Emit the full `actr_dispatch.rs` file content.
    pub fn generate(
        &self,
        methods_by_route: &HashMap<String, &MethodDescriptorProto>,
    ) -> Result<String> {
        let local_routes = self.collect_local_routes(methods_by_route)?;
        let remote_routes = self.collect_remote_routes(methods_by_route)?;

        let handler_trait = self.generate_handler_trait(&local_routes)?;
        let dispatch_fn = self.generate_dispatch_fn(&local_routes, &remote_routes)?;
        let workload_adapter = self.generate_workload_adapter(local_routes.is_empty());

        let imports = self.generate_imports(&local_routes)?;

        Ok(format!(
            "{imports}\n\n{handler_trait}\n\n{dispatch_fn}\n\n{workload_adapter}\n"
        ))
    }

    fn collect_local_routes(
        &self,
        methods_by_route: &HashMap<String, &MethodDescriptorProto>,
    ) -> Result<Vec<LocalRouteInfo>> {
        let mut routes: Vec<LocalRouteInfo> = Vec::new();
        for route in self.spec.local_routes() {
            // Verify the route has a matching method descriptor in the proto
            // files passed to protoc. We don't read the descriptor's contents
            // — the DispatchSpec already carries the input/output type names
            // — but we surface a clear error if the spec references a route
            // that isn't defined in any .proto file.
            if !methods_by_route.contains_key(&route.route_key) {
                return Err(anyhow!(
                    "dispatch spec references route `{}` but no matching method \
                     descriptor was passed to protoc",
                    route.route_key
                ));
            }

            let (service_name, method_name) = split_service_and_method(&route.route_key);
            let handler_method = format!(
                "{}_{}",
                service_name.to_snake_case(),
                method_name.to_snake_case()
            );

            let input_type_path = self.message_type_path(&route.input_type)?;
            let output_type_path = self.message_type_path(&route.output_type)?;
            let input_type_short = bare_type_name(&route.input_type);

            routes.push(LocalRouteInfo {
                route_key: route.route_key.clone(),
                handler_method,
                input_type_path,
                input_type_short,
                output_type_path,
            });
        }
        // Sort by route_key for deterministic output.
        routes.sort_by(|a, b| a.route_key.cmp(&b.route_key));
        Ok(routes)
    }

    fn collect_remote_routes(
        &self,
        methods_by_route: &HashMap<String, &MethodDescriptorProto>,
    ) -> Result<Vec<RemoteRouteInfo>> {
        let mut routes = Vec::new();
        for route in self.spec.remote_routes() {
            if !methods_by_route.contains_key(&route.route_key) {
                return Err(anyhow!(
                    "dispatch spec references remote route `{}` but no matching method descriptor was passed to protoc",
                    route.route_key
                ));
            }
            let alias = route
                .dependency_alias
                .as_deref()
                .filter(|alias| !alias.trim().is_empty())
                .ok_or_else(|| {
                    anyhow!(
                        "remote route `{}` has an empty dependency_alias",
                        route.route_key
                    )
                })?;
            routes.push(RemoteRouteInfo {
                route_key: route.route_key.clone(),
                dependency_alias: alias.to_string(),
            });
        }
        routes.sort_by(|a, b| a.route_key.cmp(&b.route_key));
        Ok(routes)
    }

    fn generate_imports(&self, _local_routes: &[LocalRouteInfo]) -> Result<String> {
        // Collect distinct declaring proto packages used by local routes.
        let mut packages: Vec<String> = Vec::new();
        for route in self.spec.local_routes() {
            for type_name in [&route.input_type, &route.output_type] {
                let cleaned = type_name.trim_start_matches('.');
                let owner = self.type_owners.get(cleaned).cloned().unwrap_or_else(|| {
                    // Fall back to the package implied by the type name.
                    if let Some(last_dot) = cleaned.rfind('.') {
                        cleaned[..last_dot].to_string()
                    } else {
                        String::new()
                    }
                });
                let module = owner.replace('.', "_");
                if !module.is_empty() && !packages.contains(&module) {
                    packages.push(module);
                }
            }
        }

        let module_paths = packages
            .iter()
            .map(|package| format!("//   super::{package}::<Message>"))
            .collect::<Vec<_>>()
            .join("\n");

        Ok(format!(
            r#"// Auto-generated code - DO NOT EDIT
// Generated by actr-cli's protoc-gen-actrframework plugin
// Unified workload-level dispatcher sourced from DispatchSpec v1.
#[allow(dead_code, unused_imports)]
use async_trait::async_trait;
use bytes::Bytes;
use prost::Message as ProstMessage;

#[allow(unused_imports)]
use actr_framework::{{Context, MessageDispatcher, Workload}};
#[allow(unused_imports)]
use actr_protocol::{{ActorResult, ActrError, PayloadType, RpcEnvelope}};

// Protobuf message types are referenced by declaring owner module.
{module_paths}
"#
        ))
    }

    fn generate_handler_trait(&self, local_routes: &[LocalRouteInfo]) -> Result<String> {
        if local_routes.is_empty() {
            // No local routes — the dispatcher doesn't need a handler trait.
            // Emit a marker trait so `actr_dispatch` can still take `&impl
            // ActrDispatchHandler` without requiring the user to implement
            // anything.
            return Ok(r#"/// Marker trait for the unified dispatcher.
///
/// No local routes exist in the DispatchSpec, so this workload has no
/// business-logic methods to implement. The dispatcher rejects every
/// route with `UnknownRoute` (or forwards to a remote alias).
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
pub trait ActrDispatchHandler: actr_framework::MaybeSendSync + 'static {}"#
                .to_string());
        }

        let mut method_sigs = Vec::new();
        for route in local_routes {
            let method_ident = format_ident!("{}", route.handler_method);
            let input_type = syn_str_to_tokens(&route.input_type_path, &route.handler_method)?;
            let output_type = syn_str_to_tokens(&route.output_type_path, &route.handler_method)?;

            method_sigs.push(quote! {
                /// RPC method: #handler_method
                async fn #method_ident<C: Context>(
                    &self,
                    req: #input_type,
                    ctx: &C,
                ) -> ActorResult<#output_type>;
            });
        }

        let trait_without_attr = quote! {
            /// Unified dispatcher handler trait — aggregates every local
            /// service method in this workload into one trait.
            ///
            /// Implement this trait on your workload struct and pass it to
            /// `actr_dispatch`. Method names are unique per route
            /// (`<service_snake>_<method_snake>`) so two services with the
            /// same proto method name do not collide.
            pub trait ActrDispatchHandler: actr_framework::MaybeSendSync + 'static {
                #(#method_sigs)*
            }
        };

        // Manually add #[async_trait] attribute to avoid quote! inserting
        // spaces; same ?Send on wasm32 rationale as the per-service handler.
        Ok(format!(
            "#[cfg_attr(not(target_arch = \"wasm32\"), async_trait)]\n\
             #[cfg_attr(target_arch = \"wasm32\", async_trait(?Send))]\n\
             {trait_without_attr}"
        ))
    }

    fn generate_dispatch_fn(
        &self,
        local_routes: &[LocalRouteInfo],
        remote_routes: &[RemoteRouteInfo],
    ) -> Result<String> {
        let mut arms: Vec<String> = Vec::new();

        for route in local_routes {
            let route_key = &route.route_key;
            let handler_method = &route.handler_method;
            let input_type = &route.input_type_path;
            let input_short = &route.input_type_short;

            let method_ident = format_ident!("{}", handler_method);
            let input_tokens = syn_str_to_tokens(input_type, &route.handler_method)?;

            let arm = quote! {
                #route_key => {
                    let req = #input_tokens::decode(payload)
                        .map_err(|e| ActrError::DecodeFailure(
                            format!("Failed to decode {}: {}", stringify!(#input_short), e)
                        ))?;
                    let resp = handler.#method_ident(req, ctx).await?;
                    Ok(Bytes::from(resp.encode_to_vec()))
                }
            };
            arms.push(arm.to_string());
        }

        for route in remote_routes {
            let route_key = &route.route_key;
            let alias = &route.dependency_alias;
            let arm = format!(
                r#"{route_key:?} => {{
                    let actr_type = ctx.resolve_dependency({alias:?}).await?;
                    let target = ctx.discover_route_candidate(&actr_type).await?;
                    ctx.call_raw(
                        &target,
                        route_key,
                        Bytes::copy_from_slice(payload),
                        timeout_ms,
                    ).await
                }}"#
            );
            arms.push(arm);
        }

        let arms_joined = arms.join(",\n            ");

        Ok(format!(
            r#"/// Unified workload-level dispatcher.
///
/// Routes `route_key` / `payload` to either a local `ActrDispatchHandler`
/// method (decode → invoke → encode) or a remote dependency (resolve alias
/// → discover → raw forwarding on the reliable lane). Unknown routes
/// return `ActrError::UnknownRoute`.
///
/// `timeout_ms` is supplied by the Rust workload adapter because the current
/// Rust raw-call API requires an explicit timeout. The generated dispatcher
/// never inlines a timeout constant.
pub async fn actr_dispatch<C: Context, H: ActrDispatchHandler>(
    ctx: &C,
    handler: &H,
    route_key: &str,
    payload: &[u8],
    timeout_ms: i64,
) -> ActorResult<Bytes> {{
    match route_key {{
            {arms_joined},
            _ => Err(ActrError::UnknownRoute(route_key.to_string())),
    }}
}}"#
        ))
    }

    fn generate_workload_adapter(&self, remote_only: bool) -> String {
        let unit_handler_impl = if remote_only {
            "impl ActrDispatchHandler for () {}\n\n"
        } else {
            ""
        };
        format!(
            r#"{unit_handler_impl}/// Workload wrapper for the unified DispatchSpec dispatcher.
pub struct ActrWorkload<H: ActrDispatchHandler>(pub H);

impl<H: ActrDispatchHandler> ActrWorkload<H> {{
    pub fn new(handler: H) -> Self {{
        Self(handler)
    }}
}}

/// MessageDispatcher adapter that routes every envelope through `actr_dispatch`.
pub struct ActrDispatcher<H: ActrDispatchHandler>(std::marker::PhantomData<H>);

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl<H: ActrDispatchHandler> MessageDispatcher for ActrDispatcher<H> {{
    type Workload = ActrWorkload<H>;

    async fn dispatch<C: Context>(
        workload: &Self::Workload,
        envelope: RpcEnvelope,
        ctx: &C,
    ) -> ActorResult<Bytes> {{
        let payload = envelope.payload.unwrap_or_default();
        actr_dispatch(
            ctx,
            &workload.0,
            envelope.route_key.as_str(),
            payload.as_ref(),
            envelope.timeout_ms,
        )
        .await
    }}
}}

impl<H: ActrDispatchHandler> Workload for ActrWorkload<H> {{
    type Dispatcher = ActrDispatcher<H>;
}}"#
        )
    }

    /// Resolve a fully-qualified proto type name to a Rust type path qualified
    /// by its declaring owner module (e.g. `super::echo::EchoRequest`),
    /// mirroring `ModernGenerator::message_type_path`.
    fn message_type_path(&self, type_name: &str) -> Result<String> {
        let cleaned = type_name.trim_start_matches('.');
        let message_path = if let Some(package) = self.type_owners.get(cleaned) {
            cleaned
                .strip_prefix(package)
                .and_then(|name| name.strip_prefix('.'))
                .unwrap_or(cleaned)
                .split('.')
                .map(str::to_string)
                .collect()
        } else {
            vec![bare_type_name(type_name)]
        };
        let Some((short_type, parents)) = message_path.split_last() else {
            return Ok(cleaned.to_string());
        };
        let module = self
            .resolve_declaring_package(type_name)
            .replace('.', "_")
            .to_snake_case();
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

    fn resolve_declaring_package(&self, type_name: &str) -> String {
        let cleaned = type_name.trim_start_matches('.');
        if let Some(package) = self.type_owners.get(cleaned) {
            return package.clone();
        }
        // Fall back to the package implied by the type name (everything
        // before the last dot).
        if let Some(last_dot) = cleaned.rfind('.') {
            cleaned[..last_dot].to_string()
        } else {
            String::new()
        }
    }
}

/// Parse a Rust type path string into a `TokenStream` for `quote!`.
fn syn_str_to_tokens(type_path: &str, context: &str) -> Result<proc_macro2::TokenStream> {
    type_path.parse().map_err(|err| {
        anyhow!("Invalid generated Rust type path '{type_path}' (context: {context}): {err}")
    })
}

/// Extract the bare type name (last path segment) from a fully-qualified
/// proto type name.
fn bare_type_name(type_name: &str) -> String {
    let cleaned = type_name.trim_start_matches('.');
    cleaned.rsplit('.').next().unwrap_or(cleaned).to_string()
}

/// Split a route key `pkg.Svc.Method` into `(Svc, Method)`.
fn split_service_and_method(route_key: &str) -> (&str, &str) {
    let mut parts = route_key.rsplitn(3, '.');
    let method = parts.next().unwrap_or("");
    let service = parts.next().unwrap_or("");
    (service, method)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dispatch_spec::DispatchRoute;
    use std::collections::HashMap;

    fn make_route(
        route_key: &str,
        proto_file: &str,
        proto_service: &str,
        method: &str,
        input_type: &str,
        output_type: &str,
        dependency_alias: Option<&str>,
    ) -> DispatchRoute {
        DispatchRoute {
            route_key: route_key.to_string(),
            proto_file: proto_file.to_string(),
            proto_service: proto_service.to_string(),
            method: method.to_string(),
            input_type: input_type.to_string(),
            output_type: output_type.to_string(),
            payload_type: "rpcReliable".to_string(),
            dependency_alias: dependency_alias.map(|s| s.to_string()),
        }
    }

    fn make_spec(routes: Vec<DispatchRoute>) -> DispatchSpec {
        DispatchSpec {
            schema_version: "v1".to_string(),
            routes,
            spec_hash: None,
        }
    }

    #[test]
    fn split_service_and_method_handles_package() {
        let (svc, method) = split_service_and_method("echo.EchoService.Echo");
        assert_eq!(svc, "EchoService");
        assert_eq!(method, "Echo");
    }

    #[test]
    fn bare_type_name_strips_package() {
        assert_eq!(bare_type_name(".echo.EchoRequest"), "EchoRequest");
        assert_eq!(bare_type_name("EchoRequest"), "EchoRequest");
    }

    #[test]
    fn empty_spec_generates_unknown_route_only() {
        let spec = make_spec(vec![]);
        let owners = HashMap::new();
        let methods = HashMap::new();
        let dispatcher_gen = UnifiedDispatcherGenerator::new(&spec, &owners);
        let code = dispatcher_gen.generate(&methods).unwrap();
        assert!(code.contains("ActrDispatchHandler"));
        assert!(code.contains("actr_dispatch"));
        assert!(code.contains("UnknownRoute"));
        assert!(code.contains("impl ActrDispatchHandler for ()"));
        assert!(code.contains("impl<H: ActrDispatchHandler> MessageDispatcher"));
        assert!(code.contains("impl<H: ActrDispatchHandler> Workload"));
    }

    #[test]
    fn local_route_generates_handler_method_and_arm() {
        let spec = make_spec(vec![make_route(
            "echo.EchoService.Echo",
            "local/echo.proto",
            "echo.EchoService",
            "Echo",
            "echo.EchoRequest",
            "echo.EchoResponse",
            None,
        )]);
        let mut owners = HashMap::new();
        owners.insert("echo.EchoRequest".to_string(), "echo".to_string());
        owners.insert("echo.EchoResponse".to_string(), "echo".to_string());
        // The dispatcher checks every local route has a matching method
        // descriptor in the proto files — populate one so the check passes.
        let method = MethodDescriptorProto {
            name: Some("Echo".to_string()),
            input_type: Some(".echo.EchoRequest".to_string()),
            output_type: Some(".echo.EchoResponse".to_string()),
            ..Default::default()
        };
        let mut methods: HashMap<String, &MethodDescriptorProto> = HashMap::new();
        methods.insert("echo.EchoService.Echo".to_string(), &method);
        let dispatcher_gen = UnifiedDispatcherGenerator::new(&spec, &owners);
        let code = dispatcher_gen.generate(&methods).unwrap();
        assert!(code.contains("fn echo_service_echo"));
        assert!(code.contains("\"echo.EchoService.Echo\""));
        // quote! emits `::` with surrounding spaces, so check for the
        // spaced form.
        assert!(
            code.contains("super :: echo :: EchoRequest"),
            "expected owner-qualified request type in:\n{code}"
        );
    }

    #[test]
    fn remote_route_uses_resolve_dependency_and_call_raw() {
        let spec = make_spec(vec![make_route(
            "other.OtherService.Ping",
            "remote/other/other.proto",
            "other.OtherService",
            "Ping",
            "other.PingRequest",
            "other.PingResponse",
            Some("other"),
        )]);
        let owners = HashMap::new();
        let method = MethodDescriptorProto {
            name: Some("Ping".to_string()),
            input_type: Some(".other.PingRequest".to_string()),
            output_type: Some(".other.PingResponse".to_string()),
            ..Default::default()
        };
        let mut methods: HashMap<String, &MethodDescriptorProto> = HashMap::new();
        methods.insert("other.OtherService.Ping".to_string(), &method);
        let dispatcher_gen = UnifiedDispatcherGenerator::new(&spec, &owners);
        let code = dispatcher_gen.generate(&methods).unwrap();
        assert!(code.contains("resolve_dependency"));
        assert!(code.contains("discover_route_candidate"));
        assert!(code.contains("call_raw"));
        assert!(code.contains("\"other.OtherService.Ping\""));
        assert!(code.contains("\"other\""));
        assert!(!code.contains("call_raw_with_timeout"));
        assert!(!code.contains("30_000"));
        assert!(!code.contains("30000"));
        assert!(!code.contains("DEFAULT_RPC_TIMEOUT_MS"));
    }

    #[test]
    fn no_remote_actr_type_string_emitted() {
        let spec = make_spec(vec![make_route(
            "other.OtherService.Ping",
            "remote/other/other.proto",
            "other.OtherService",
            "Ping",
            "other.PingRequest",
            "other.PingResponse",
            Some("other"),
        )]);
        let owners = HashMap::new();
        let method = MethodDescriptorProto {
            name: Some("Ping".to_string()),
            input_type: Some(".other.PingRequest".to_string()),
            output_type: Some(".other.PingResponse".to_string()),
            ..Default::default()
        };
        let mut methods: HashMap<String, &MethodDescriptorProto> = HashMap::new();
        methods.insert("other.OtherService.Ping".to_string(), &method);
        let dispatcher_gen = UnifiedDispatcherGenerator::new(&spec, &owners);
        let code = dispatcher_gen.generate(&methods).unwrap();
        assert!(
            !code.contains("manufacturer"),
            "generated actr_dispatch.rs must not embed ActrType fields; got:\n{code}"
        );
        assert!(
            !code.contains("ActrType {"),
            "generated actr_dispatch.rs must not construct ActrType; got:\n{code}"
        );
    }
}
