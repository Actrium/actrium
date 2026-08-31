//! Unified `actr_dispatch.rs` generator.
//!
//! Emits a single workload-level dispatcher function (`actr_dispatch`) that
//! matches on `route_key` and either:
//!
//! - **Local route (Return)**: decodes the protobuf request, invokes the
//!   matching `ActrDispatchHandler` method, and encodes the response.
//! - **Local route (Explicit)**: keeps the full `RpcEnvelope`, builds one
//!   `Reply<T>` from its `request_id` plus the runtime context's deliver /
//!   deadline / expects-response state, hands the untouched handle to the
//!   handler method, and returns empty response bytes — the reply, dedup
//!   completion, and mailbox ACK are deferred to `reply.send()`.
//! - **Remote route**: resolves the dependency alias via
//!   `Context::resolve_dependency`, discovers a route candidate, and calls
//!   `Context::call_raw_with_payload_type` with the caller-supplied
//!   `timeout_ms` and the route's declared RPC lane.
//! - **Unknown route**: returns `ActrError::UnknownRoute`.
//!
//! The `ActrDispatchHandler` trait aggregates every local service method
//! into a single trait with uniquely-named methods
//! (`<service_snake>_<method_snake>`). It is *sealed*: only the generated
//! `actr_handler::ActrHandler` adapter may implement it, so business code
//! implements the per-service `{Service}Handler` traits from the
//! `<service>_actor` modules instead.
//!
//! When at least one local route is `ReplyMode::Explicit` the dispatcher
//! also overrides `MessageDispatcher::reply_mode_for_route` so the runtime
//! knows those routes complete through a `Reply<T>` handle.

use crate::dispatch_spec::DispatchSpec;
use actr_protocol::ReplyMode;
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
    /// How the callee delivers the response (RFC-0323).
    reply_mode: ReplyMode,
}

/// Information needed to emit one remote match arm.
struct RemoteRouteInfo {
    route_key: String,
    dependency_alias: String,
    payload_type: &'static str,
}

/// Type-owner map: fully-qualified proto type name -> declaring proto package.
///
/// Mirrors the `type_owner` map the per-service generator already builds, so
/// the unified dispatcher can qualify imported message types the same way.
pub struct UnifiedDispatcherGenerator<'a> {
    spec: &'a DispatchSpec,
    type_owners: &'a HashMap<String, String>,
    /// `route_key -> ReplyMode` map, extracted from the raw
    /// `CodeGeneratorRequest` bytes by `crate::option_extractor`. Routes
    /// absent from the map default to `ReplyMode::Return`.
    reply_modes: &'a HashMap<String, ReplyMode>,
}

impl<'a> UnifiedDispatcherGenerator<'a> {
    pub fn new(
        spec: &'a DispatchSpec,
        type_owners: &'a HashMap<String, String>,
        reply_modes: &'a HashMap<String, ReplyMode>,
    ) -> Self {
        Self {
            spec,
            type_owners,
            reply_modes,
        }
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
        let workload_adapter =
            self.generate_workload_adapter(&local_routes, local_routes.is_empty());

        let imports = self.generate_imports(&local_routes);

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
                reply_mode: self.reply_mode_for(&route.route_key),
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
                payload_type: rust_rpc_payload_type_variant(&route.payload_type)?,
            });
        }
        routes.sort_by(|a, b| a.route_key.cmp(&b.route_key));
        Ok(routes)
    }

    /// The `ReplyMode` declared for `route_key` via the `actr.reply_mode`
    /// proto option. Absent → `ReplyMode::Return`.
    fn reply_mode_for(&self, route_key: &str) -> ReplyMode {
        self.reply_modes
            .get(route_key)
            .copied()
            .unwrap_or(ReplyMode::Return)
    }

    /// Whether *any* local route is `ReplyMode::Explicit`. Drives the
    /// `Reply` / `ReplyMode` imports and the `reply_mode_for_route`
    /// override so all-RETURN workloads stay byte-identical to pre-RFC-0323
    /// unified codegen.
    fn has_explicit(local_routes: &[LocalRouteInfo]) -> bool {
        local_routes
            .iter()
            .any(|route| route.reply_mode == ReplyMode::Explicit)
    }

    fn generate_imports(&self, local_routes: &[LocalRouteInfo]) -> String {
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

        // Only pull in `Reply` / `ReplyMode` when a local route is
        // EXPLICIT, so all-RETURN codegen stays byte-identical.
        let (framework_extra, protocol_extra) = if Self::has_explicit(local_routes) {
            (", Reply", ", ReplyMode")
        } else {
            ("", "")
        };

        format!(
            r#"// Auto-generated code - DO NOT EDIT
// Generated by actr-cli's protoc-gen-actrframework plugin
// Unified workload-level dispatcher sourced from DispatchSpec v1.
#[allow(dead_code, unused_imports)]
use async_trait::async_trait;
use bytes::Bytes;
use prost::Message as ProstMessage;

#[allow(unused_imports)]
use actr_framework::{{Context, MessageDispatcher, Workload{framework_extra}}};
#[allow(unused_imports)]
use actr_protocol::{{ActorResult, ActrError, PayloadType, RpcEnvelope{protocol_extra}}};

// Protobuf message types are referenced by declaring owner module.
{module_paths}
"#
        )
    }

    fn generate_handler_trait(&self, local_routes: &[LocalRouteInfo]) -> Result<String> {
        if local_routes.is_empty() {
            // No local routes — the dispatcher doesn't need handler methods.
            // Emit a sealed marker trait so `actr_dispatch` can still take
            // `&impl ActrDispatchHandler`; the module-private impl for `()`
            // below is the only implementor.
            return Ok(r#"pub(super) mod private {
    /// Seal: only the generated `actr_handler::ActrHandler` adapter
    /// implements `ActrDispatchHandler`. The module is `pub(super)` so the
    /// sibling adapter module inside `generated/` can name the trait while
    /// business code outside `generated/` cannot — user code must implement
    /// the per-service `{Service}Handler` traits instead.
    pub trait Sealed {}
}

/// Sealed marker trait for the unified dispatcher.
///
/// No local routes exist in the DispatchSpec, so this workload has no
/// business-logic methods to implement. The dispatcher rejects every
/// route with `UnknownRoute` (or forwards to a remote alias).
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
#[allow(private_bounds)]
pub trait ActrDispatchHandler: private::Sealed + actr_framework::MaybeSendSync + 'static {}"#
                .to_string());
        }

        let mut method_sigs = Vec::new();
        for route in local_routes {
            let method_ident = format_ident!("{}", route.handler_method);
            let input_type = syn_str_to_tokens(&route.input_type_path, &route.handler_method)?;
            let output_type = syn_str_to_tokens(&route.output_type_path, &route.handler_method)?;

            if route.reply_mode == ReplyMode::Explicit {
                // EXPLICIT route: the handler receives the exact `Reply<T>`
                // the dispatcher built from the envelope — never re-wrapped —
                // and completes it (or moves it) later.
                method_sigs.push(quote! {
                    /// RPC method (ReplyMode::Explicit): #handler_method
                    ///
                    /// Reply is deferred — call `reply.send(Ok(resp))` or move
                    /// the handle into a spawned task. Dropping an un-`send`
                    /// handle sends an `Internal` error (first-write-wins).
                    async fn #method_ident<C: Context>(
                        &self,
                        req: #input_type,
                        ctx: &C,
                        reply: Reply<#output_type>,
                    ) -> ActorResult<()>;
                });
            } else {
                method_sigs.push(quote! {
                    /// RPC method: #handler_method
                    async fn #method_ident<C: Context>(
                        &self,
                        req: #input_type,
                        ctx: &C,
                    ) -> ActorResult<#output_type>;
                });
            }
        }

        // The seal module is plain Rust; the `async_trait` attribute may only
        // decorate the trait itself, so emit them separately.
        let seal_module = quote! {
            pub(super) mod private {
                /// Seal: only the generated `actr_handler::ActrHandler`
                /// adapter implements `ActrDispatchHandler`. The module is
                /// `pub(super)` so the sibling adapter module inside
                /// `generated/` can name the trait while business code
                /// outside `generated/` cannot — user code must implement
                /// the per-service `{Service}Handler` traits instead.
                pub trait Sealed {}
            }
        };

        let trait_without_attr = quote! {
            /// Sealed unified dispatcher handler trait — aggregates every local
            /// service method in this workload into one trait.
            ///
            /// Only the generated `actr_handler::ActrHandler` adapter
            /// implements this trait. Method names are unique per route
            /// (`<service_snake>_<method_snake>`) so two services with the
            /// same proto method name do not collide; the adapter forwards each
            /// one to the protobuf-named method on the matching
            /// `{Service}Handler` implementation.
            #[allow(private_bounds)]
            pub trait ActrDispatchHandler: private::Sealed + actr_framework::MaybeSendSync + 'static {
                #(#method_sigs)*
            }
        };

        // Manually add #[async_trait] attribute to avoid quote! inserting
        // spaces; same ?Send on wasm32 rationale as the per-service handler.
        Ok(format!(
            "{seal_module}\n\
             #[cfg_attr(not(target_arch = \"wasm32\"), async_trait)]\n\
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

            if route.reply_mode == ReplyMode::Explicit {
                // EXPLICIT dispatch arm: the dispatcher received the full
                // `RpcEnvelope`, so build exactly one `Reply<T>` from its
                // `request_id` plus the runtime context's deliver / deadline
                // / expects-response state and pass it through untouched —
                // re-wrapping would fork the first-write-wins completion
                // state. Return empty bytes: the reply, dedup completion, and
                // mailbox ACK are deferred to `reply.send()`.
                let arm = quote! {
                    #route_key => {
                        let req = #input_tokens::decode(payload)
                            .map_err(|e| ActrError::DecodeFailure(
                                format!("Failed to decode {}: {}", stringify!(#input_short), e)
                            ))?;
                        let deliver = ctx.reply_deliver().ok_or_else(|| ActrError::Internal(
                            "EXPLICIT dispatch requires a reply deliver on the runtime context".to_string()
                        ))?;
                        if !ctx.claim_reply() {
                            return Err(ActrError::Internal(
                                "EXPLICIT dispatch requires a claimable reply context".to_string()
                            ));
                        }
                        let reply = Reply::new(
                            request_id.to_string(),
                            ctx.reply_deadline(),
                            deliver,
                            ctx.expects_response(),
                        );
                        let reply_guard = reply.dispatch_guard();
                        match handler.#method_ident(req, ctx, reply).await {
                            Ok(()) => {
                                drop(reply_guard);
                                Ok(Bytes::new())
                            }
                            Err(error) => {
                                reply_guard.complete_error(error.clone());
                                Err(error)
                            }
                        }
                    }
                };
                arms.push(arm.to_string());
            } else {
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
        }

        for route in remote_routes {
            let route_key = &route.route_key;
            let alias = &route.dependency_alias;
            let payload_type = route.payload_type;
            let arm = format!(
                r#"{route_key:?} => {{
                    let actr_type = ctx.resolve_dependency({alias:?}).await?;
                    let target = ctx.discover_route_candidate(&actr_type).await?;
                    ctx.call_raw_with_payload_type(
                        &target,
                        route_key,
                        PayloadType::{payload_type},
                        Bytes::copy_from_slice(payload),
                        timeout_ms,
                    ).await
                }}"#
            );
            arms.push(arm);
        }

        let arms_joined = arms.join(",\n            ");
        // An empty DispatchSpec must not leave a dangling match comma: the
        // dispatcher is all-UnknownRoute in that case.
        let arms_separator = if arms_joined.is_empty() {
            String::new()
        } else {
            format!("{arms_joined},")
        };

        // The dispatcher needs the envelope's `request_id` when any local
        // route is EXPLICIT (it correlates the deferred `Reply<T>` with the
        // request). All-RETURN workloads never read it.
        let request_id_param = if Self::has_explicit(local_routes) {
            "request_id: &str,"
        } else {
            "_request_id: &str,"
        };

        // Remote forwarding arms consume \`timeout_ms\`; local-only workloads
        // never read it, so silence the unused-variable lint there.
        let timeout_param = if remote_routes.is_empty() {
            "_timeout_ms: i64,"
        } else {
            "timeout_ms: i64,"
        };

        Ok(format!(
            r#"/// Unified workload-level dispatcher.
///
/// Routes `route_key` / `payload` to either a local `ActrDispatchHandler`
/// method (decode → invoke → encode), a local EXPLICIT method (decode → build
/// one `Reply<T>` from the envelope's `request_id` and the context's
/// deliver/deadline/expects-response → hand it through untouched → return
/// empty bytes), or a remote dependency (resolve alias → discover → raw
/// forwarding on the route's declared RPC lane). Unknown routes return
/// `ActrError::UnknownRoute`.
///
/// `timeout_ms` is supplied by the Rust workload adapter because the current
/// Rust raw-call API requires an explicit timeout. The generated dispatcher
/// never inlines a timeout constant.
pub async fn actr_dispatch<C: Context, H: ActrDispatchHandler>(
    ctx: &C,
    handler: &H,
    route_key: &str,
    payload: &[u8],
    {request_id_param}
    {timeout_param}
) -> ActorResult<Bytes> {{
    match route_key {{
            {arms_separator}
            _ => Err(ActrError::UnknownRoute(route_key.to_string())),
    }}
}}"#
        ))
    }

    fn generate_workload_adapter(
        &self,
        local_routes: &[LocalRouteInfo],
        remote_only: bool,
    ) -> String {
        let unit_handler_impl = if remote_only {
            "impl private::Sealed for () {}\nimpl ActrDispatchHandler for () {}\n\n"
        } else {
            ""
        };

        // `reply_mode_for_route` override: only emit when a local route is
        // EXPLICIT, so all-RETURN workloads inherit the trait default (Return)
        // and stay byte-identical.
        let reply_mode_for_route = if Self::has_explicit(local_routes) {
            let arms = local_routes
                .iter()
                .filter(|route| route.reply_mode == ReplyMode::Explicit)
                .map(|route| format!("{:?} => ReplyMode::Explicit,", route.route_key))
                .collect::<Vec<_>>()
                .join("\n                ");
            format!(
                "
    fn reply_mode_for_route(route_key: &str) -> ReplyMode {{
        match route_key {{
                {arms}
            _ => ReplyMode::Return,
        }}
    }}"
            )
        } else {
            String::new()
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
        // Capture the request id before consuming the payload so EXPLICIT
        // routes can correlate their deferred Reply<T> with this envelope.
        let request_id = envelope.request_id.clone();
        let payload = envelope.payload.unwrap_or_default();
        actr_dispatch(
            ctx,
            &workload.0,
            envelope.route_key.as_str(),
            payload.as_ref(),
            &request_id,
            envelope.timeout_ms,
        )
        .await
    }}{reply_mode_for_route}
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

fn rust_rpc_payload_type_variant(value: &str) -> Result<&'static str> {
    match value.to_ascii_lowercase().as_str() {
        "rpcreliable" => Ok("RpcReliable"),
        "rpcsignal" => Ok("RpcSignal"),
        _ => Err(anyhow!(
            "unsupported remote RPC payload_type `{value}` in DispatchSpec"
        )),
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
            reply_mode: "return".to_string(),
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

    fn make_method(name: &str, input: &str, output: &str) -> MethodDescriptorProto {
        MethodDescriptorProto {
            name: Some(name.to_string()),
            input_type: Some(input.to_string()),
            output_type: Some(output.to_string()),
            ..Default::default()
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
        let reply_modes = HashMap::new();
        let dispatcher_gen = UnifiedDispatcherGenerator::new(&spec, &owners, &reply_modes);
        let code = dispatcher_gen.generate(&methods).unwrap();
        assert!(code.contains("ActrDispatchHandler"));
        assert!(code.contains("actr_dispatch"));
        assert!(code.contains("UnknownRoute"));
        assert!(code.contains("impl ActrDispatchHandler for ()"));
        assert!(code.contains("impl private::Sealed for ()"));
        // An empty DispatchSpec must not emit a dangling match comma (the
        // generated code has to stay syntactically valid).
        assert!(
            !code.contains("match route_key {\n            ,"),
            "empty dispatcher must not leave a dangling match comma:\n{code}"
        );
        assert!(code.contains("_ => Err(ActrError::UnknownRoute"));
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
        let method = make_method("Echo", ".echo.EchoRequest", ".echo.EchoResponse");
        let mut methods: HashMap<String, &MethodDescriptorProto> = HashMap::new();
        methods.insert("echo.EchoService.Echo".to_string(), &method);
        let reply_modes = HashMap::new();
        let dispatcher_gen = UnifiedDispatcherGenerator::new(&spec, &owners, &reply_modes);
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
    fn handler_trait_is_sealed_and_documented_for_local_routes() {
        let spec = make_spec(vec![make_route(
            "echo.EchoService.Echo",
            "local/echo.proto",
            "echo.EchoService",
            "Echo",
            "echo.EchoRequest",
            "echo.EchoResponse",
            None,
        )]);
        let method = make_method("Echo", ".echo.EchoRequest", ".echo.EchoResponse");
        let methods = HashMap::from([("echo.EchoService.Echo".to_string(), &method)]);
        let reply_modes = HashMap::new();
        let code = UnifiedDispatcherGenerator::new(&spec, &HashMap::new(), &reply_modes)
            .generate(&methods)
            .unwrap();

        assert!(
            code.contains("pub trait ActrDispatchHandler : private :: Sealed"),
            "ActrDispatchHandler must be sealed:\n{code}"
        );
        // The seal module must be `pub(super)`: visible to the sibling
        // generated adapter module, unnameable from business code outside
        // `generated/`. Plain `pub mod private` would let user code impl the
        // seal itself and bypass the adapter.
        // quote! renders `pub(super)` with spaces between tokens.
        assert!(
            code.contains("pub (super) mod private"),
            "seal module must be pub(super):\n{code}"
        );
        assert!(
            !code.contains("pub mod private"),
            "seal module must not be fully pub:\n{code}"
        );
        assert!(
            !code.contains("impl ActrDispatchHandler for ()"),
            "workloads with local routes must not impl the handler for ():\n{code}"
        );
    }

    #[test]
    fn remote_only_seal_module_is_also_pub_super() {
        let spec = make_spec(vec![]);
        let methods = HashMap::new();
        let reply_modes = HashMap::new();
        let code = UnifiedDispatcherGenerator::new(&spec, &HashMap::new(), &reply_modes)
            .generate(&methods)
            .unwrap();
        assert!(
            code.contains("pub(super) mod private"),
            "remote-only seal module must be pub(super):\n{code}"
        );
        assert!(
            !code.contains("pub mod private"),
            "remote-only seal module must not be fully pub:\n{code}"
        );
        assert!(code.contains("impl private::Sealed for ()"));
    }

    #[test]
    fn explicit_local_route_builds_reply_and_maps_reply_mode() {
        let spec = make_spec(vec![
            make_route(
                "echo.EchoService.Echo",
                "local/echo.proto",
                "echo.EchoService",
                "Echo",
                "echo.EchoRequest",
                "echo.EchoResponse",
                None,
            ),
            make_route(
                "echo.EchoService.Transcode",
                "local/echo.proto",
                "echo.EchoService",
                "Transcode",
                "echo.EchoRequest",
                "echo.EchoResponse",
                None,
            ),
        ]);
        let echo = make_method("Echo", ".echo.EchoRequest", ".echo.EchoResponse");
        let transcode = make_method("Transcode", ".echo.EchoRequest", ".echo.EchoResponse");
        let methods = HashMap::from([
            ("echo.EchoService.Echo".to_string(), &echo),
            ("echo.EchoService.Transcode".to_string(), &transcode),
        ]);
        let reply_modes = HashMap::from([(
            "echo.EchoService.Transcode".to_string(),
            ReplyMode::Explicit,
        )]);

        let code = UnifiedDispatcherGenerator::new(&spec, &HashMap::new(), &reply_modes)
            .generate(&methods)
            .expect("EXPLICIT routes must generate, not reject");

        // Sealed trait method takes Reply<Resp> and returns ActorResult<()>.
        assert!(
            code.contains("reply : Reply"),
            "EXPLICIT handler method should take a Reply<T>:\n{code}"
        );
        assert!(
            code.contains("ActorResult < () >"),
            "EXPLICIT handler method should return ActorResult<()>:\n{code}"
        );
        // Dispatch arm builds exactly one Reply from the envelope + context.
        assert!(
            code.contains("ctx . reply_deliver ()"),
            "EXPLICIT arm should pull the deliver from the context:\n{code}"
        );
        assert!(
            code.contains("ctx . claim_reply ()"),
            "EXPLICIT arm should claim a deferred reply before handing it to the handler:\n{code}"
        );
        assert!(
            code.contains("request_id . to_string ()"),
            "EXPLICIT arm should correlate the Reply with the envelope request_id:\n{code}"
        );
        assert!(
            code.contains("ctx . reply_deadline ()"),
            "EXPLICIT arm should reuse the context deadline:\n{code}"
        );
        assert!(
            code.contains("ctx . expects_response ()"),
            "EXPLICIT arm should honor expects-response:\n{code}"
        );
        assert!(
            code.contains("reply . dispatch_guard ()"),
            "EXPLICIT arm should retain a dispatcher completion guard:\n{code}"
        );
        assert!(
            code.contains("reply_guard . complete_error (error . clone ())"),
            "EXPLICIT handler errors should beat the generic Drop fallback:\n{code}"
        );
        assert!(
            code.contains("Ok (Bytes :: new ())"),
            "EXPLICIT arm should return empty response bytes:\n{code}"
        );
        // The dispatcher receives the envelope's request id so EXPLICIT
        // routes can correlate their deferred Reply<T>.
        assert!(
            code.contains("request_id: &str,"),
            "actr_dispatch should take the request id when EXPLICIT routes exist:\n{code}"
        );
        // reply_mode_for_route override maps only the EXPLICIT route.
        assert!(
            code.contains("fn reply_mode_for_route(route_key: &str) -> ReplyMode"),
            "should override reply_mode_for_route:\n{code}"
        );
        assert!(
            code.contains("\"echo.EchoService.Transcode\" => ReplyMode::Explicit,"),
            "EXPLICIT route should map to Explicit:\n{code}"
        );
        // Reply / ReplyMode imports present.
        assert!(
            code.contains("Workload, Reply"),
            "Reply import missing:\n{code}"
        );
        assert!(
            code.contains("RpcEnvelope, ReplyMode"),
            "ReplyMode import missing:\n{code}"
        );
    }

    #[test]
    fn return_only_routes_stay_byte_compatible() {
        let spec = make_spec(vec![make_route(
            "echo.EchoService.Echo",
            "local/echo.proto",
            "echo.EchoService",
            "Echo",
            "echo.EchoRequest",
            "echo.EchoResponse",
            None,
        )]);
        let method = make_method("Echo", ".echo.EchoRequest", ".echo.EchoResponse");
        let methods = HashMap::from([("echo.EchoService.Echo".to_string(), &method)]);
        let reply_modes = HashMap::new();

        let code = UnifiedDispatcherGenerator::new(&spec, &HashMap::new(), &reply_modes)
            .generate(&methods)
            .unwrap();

        assert!(!code.contains("reply_mode_for_route"));
        assert!(!code.contains("reply_deliver"));
        assert!(!code.contains("ReplyMode"));
        assert!(
            code.contains("_request_id: &str,"),
            "all-RETURN dispatch should not read the request id:\n{code}"
        );
    }

    #[test]
    fn remote_route_preserves_payload_type_in_raw_call() {
        let mut route = make_route(
            "other.OtherService.Ping",
            "remote/other/other.proto",
            "other.OtherService",
            "Ping",
            "other.PingRequest",
            "other.PingResponse",
            Some("other"),
        );
        route.payload_type = "rpcSignal".to_string();
        let spec = make_spec(vec![route]);
        let owners = HashMap::new();
        let method = make_method("Ping", ".other.PingRequest", ".other.PingResponse");
        let mut methods: HashMap<String, &MethodDescriptorProto> = HashMap::new();
        methods.insert("other.OtherService.Ping".to_string(), &method);
        let reply_modes = HashMap::new();
        let dispatcher_gen = UnifiedDispatcherGenerator::new(&spec, &owners, &reply_modes);
        let code = dispatcher_gen.generate(&methods).unwrap();
        assert!(code.contains("resolve_dependency"));
        assert!(code.contains("discover_route_candidate"));
        assert!(code.contains("call_raw_with_payload_type"));
        assert!(code.contains("PayloadType::RpcSignal"));
        assert!(code.contains("\"other.OtherService.Ping\""));
        assert!(code.contains("\"other\""));
        assert!(!code.contains("call_raw_with_timeout"));
        assert!(!code.contains("30_000"));
        assert!(!code.contains("30000"));
        assert!(!code.contains("DEFAULT_RPC_TIMEOUT_MS"));
    }

    #[test]
    fn remote_route_rejects_non_rpc_payload_type() {
        let mut route = make_route(
            "other.OtherService.Ping",
            "remote/other/other.proto",
            "other.OtherService",
            "Ping",
            "other.PingRequest",
            "other.PingResponse",
            Some("other"),
        );
        route.payload_type = "streamReliable".to_string();
        let spec = make_spec(vec![route]);
        let method = make_method("Ping", ".other.PingRequest", ".other.PingResponse");
        let methods = HashMap::from([("other.OtherService.Ping".to_string(), &method)]);
        let reply_modes = HashMap::new();

        let error = UnifiedDispatcherGenerator::new(&spec, &HashMap::new(), &reply_modes)
            .generate(&methods)
            .unwrap_err();

        assert!(error.to_string().contains("streamReliable"));
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
        let method = make_method("Ping", ".other.PingRequest", ".other.PingResponse");
        let mut methods: HashMap<String, &MethodDescriptorProto> = HashMap::new();
        methods.insert("other.OtherService.Ping".to_string(), &method);
        let reply_modes = HashMap::new();
        let dispatcher_gen = UnifiedDispatcherGenerator::new(&spec, &owners, &reply_modes);
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
