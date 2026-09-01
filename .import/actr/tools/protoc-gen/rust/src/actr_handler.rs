//! Generated `actr_handler.rs` aggregate adapter generator.
//!
//! Emits `ActrHandler<...>` — one generic field per local proto Service,
//! each constrained by the corresponding `{Service}Handler` trait from that
//! service's `<file_stem>_actor` module. The adapter implements the sealed
//! `actr_dispatch::ActrDispatchHandler` trait and forwards every internal
//! `<service>_<method>` route method to the protobuf-named method on the
//! matching business handler.
//!
//! The generated type is *complete* runtime glue, not a user scaffold: it
//! never references, constructs, or assumes a concrete `{Service}Impl`.
//! Business code lives in per-service `src/<service>.rs` files and only
//! implements `{Service}Handler`; the workload entry composes the
//! implementations via `ActrHandler::new(...)`.
//!
//! The file is generated only when the DispatchSpec has at least one local
//! route; remote-only workloads use `()` as the unified handler.

use crate::dispatch_spec::DispatchSpec;
use actr_protocol::ReplyMode;
use anyhow::{Result, anyhow};
use heck::ToSnakeCase;
use quote::{format_ident, quote};
use std::collections::{BTreeSet, HashMap};

/// One local route reduced to what the adapter needs to forward it.
struct AdapterRoute {
    /// Internal sealed-trait method name, e.g. `echo_service_echo`.
    handler_method: String,
    /// Business-side proto method name in snake_case, e.g. `echo`.
    business_method: String,
    /// Field name on `ActrHandler`, e.g. `echo_service`.
    field_name: String,
    input_type_path: String,
    output_type_path: String,
    reply_mode: ReplyMode,
}

/// A local proto Service that contributes one generic field to the adapter.
struct AdapterService {
    /// Service name in snake_case — also the field name, e.g. `echo_service`.
    service_snake: String,
    /// Module declaring the `{Service}Handler` trait, e.g. `echo_actor`.
    handler_module: String,
    /// Handler trait name, e.g. `EchoServiceHandler`.
    handler_trait: String,
}

/// Generates `src/generated/actr_handler.rs` from the DispatchSpec.
pub struct ActrHandlerGenerator<'a> {
    spec: &'a DispatchSpec,
    type_owners: &'a HashMap<String, String>,
    /// `route_key -> ReplyMode` extracted from the raw
    /// `CodeGeneratorRequest` bytes (see `crate::option_extractor`).
    reply_modes: &'a HashMap<String, ReplyMode>,
}

impl<'a> ActrHandlerGenerator<'a> {
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

    /// Whether the spec has local routes; when false the CLI must not emit
    /// `actr_handler.rs` at all (remote-only workloads use `()`).
    pub fn has_local_services(&self) -> bool {
        !self.spec.local_routes().is_empty()
    }

    /// Emit the full `actr_handler.rs` file content.
    pub fn generate(&self) -> Result<String> {
        let services = self.collect_services()?;
        if services.is_empty() {
            return Err(anyhow!(
                "actr_handler generation requires at least one local service"
            ));
        }
        let routes = self.collect_routes()?;

        let imports = self.generate_imports(&routes);
        let adapter = self.generate_adapter(&services, &routes)?;

        Ok(format!("{imports}\n\n{adapter}\n"))
    }

    /// Distinct local services in deterministic (snake-name) order.
    fn collect_services(&self) -> Result<Vec<AdapterService>> {
        let mut seen: BTreeSet<String> = BTreeSet::new();
        let mut services: Vec<AdapterService> = Vec::new();
        for route in self.spec.local_routes() {
            let proto_service = route.proto_service.clone();
            if !seen.insert(proto_service.clone()) {
                continue;
            }
            let service_name = proto_service
                .rsplit('.')
                .next()
                .unwrap_or(proto_service.as_str());
            if service_name.is_empty() {
                return Err(anyhow!(
                    "dispatch spec route `{}` has an empty proto service name",
                    route.route_key
                ));
            }
            let file_stem = std::path::Path::new(&route.proto_file)
                .file_stem()
                .and_then(|stem| stem.to_str())
                .unwrap_or(service_name);
            services.push(AdapterService {
                service_snake: service_name.to_snake_case(),
                handler_module: format!("{}_actor", file_stem.to_snake_case()),
                handler_trait: format!("{service_name}Handler"),
            });
        }
        services.sort_by(|a, b| a.service_snake.cmp(&b.service_snake));

        // Two services whose names collapse to the same snake_case field name
        // (e.g. `EchoService` and `Echo_Service` in different packages)
        // would generate duplicate fields — reject with a clear error.
        let mut field_names = BTreeSet::new();
        for service in &services {
            if !field_names.insert(service.service_snake.clone()) {
                return Err(anyhow!(
                    "local services collapse to the same ActrHandler field `{}`; \
                     rename one proto service",
                    service.service_snake
                ));
            }
        }
        Ok(services)
    }

    fn collect_routes(&self) -> Result<Vec<AdapterRoute>> {
        let mut routes = Vec::new();
        for route in self.spec.local_routes() {
            let (service_name, method_name) = split_service_and_method(&route.route_key);
            let handler_method = format!(
                "{}_{}",
                service_name.to_snake_case(),
                method_name.to_snake_case()
            );
            routes.push(AdapterRoute {
                handler_method,
                business_method: method_name.to_snake_case(),
                field_name: service_name.to_snake_case(),
                input_type_path: self.message_type_path(&route.input_type),
                output_type_path: self.message_type_path(&route.output_type),
                reply_mode: self
                    .reply_modes
                    .get(&route.route_key)
                    .copied()
                    .unwrap_or(ReplyMode::Return),
            });
        }
        routes.sort_by(|a, b| a.handler_method.cmp(&b.handler_method));
        Ok(routes)
    }

    fn generate_imports(&self, routes: &[AdapterRoute]) -> String {
        let framework_extra = if routes
            .iter()
            .any(|route| route.reply_mode == ReplyMode::Explicit)
        {
            ", Reply"
        } else {
            ""
        };

        format!(
            r#"// Auto-generated code - DO NOT EDIT
// Generated by actr-cli's protoc-gen-actrframework plugin
// Aggregate workload adapter composing per-service business handlers.
#[allow(dead_code, unused_imports)]
use async_trait::async_trait;

#[allow(unused_imports)]
use actr_framework::{{Context{framework_extra}}};
#[allow(unused_imports)]
use actr_protocol::{{ActorResult}};

use super::actr_dispatch::ActrDispatchHandler;
"#
        )
    }

    fn generate_adapter(
        &self,
        services: &[AdapterService],
        routes: &[AdapterRoute],
    ) -> Result<String> {
        let generic_params: Vec<_> = services
            .iter()
            .map(|service| format_ident!("{}", service.handler_trait))
            .collect();
        let field_names: Vec<_> = services
            .iter()
            .map(|service| format_ident!("{}", service.service_snake))
            .collect();
        let handler_traits: Vec<_> = services
            .iter()
            .map(|service| {
                let module = format_ident!("{}", service.handler_module);
                let trait_name = format_ident!("{}", service.handler_trait);
                quote! { super::#module::#trait_name }
            })
            .collect();

        // Seal: the adapter is the only implementor of ActrDispatchHandler.
        let seal_impl = quote! {
            impl<#(#generic_params: #handler_traits),*> super::actr_dispatch::private::Sealed
                for ActrHandler<#(#generic_params),*>
            {
            }
        };

        let mut forwarders = Vec::new();
        for route in routes {
            let handler_method = format_ident!("{}", route.handler_method);
            let business_method = format_ident!("{}", route.business_method);
            let field = format_ident!("{}", route.field_name);
            let input_type = syn_str_to_tokens(&route.input_type_path, &route.handler_method)?;
            let output_type = syn_str_to_tokens(&route.output_type_path, &route.handler_method)?;

            if route.reply_mode == ReplyMode::Explicit {
                // Forward the exact Reply<T> the dispatcher built — never
                // re-wrap or copy it, so deferred replies, timeouts, the Drop
                // fallback, and dedup completion keep their semantics.
                forwarders.push(quote! {
                    async fn #handler_method<C: Context>(
                        &self,
                        req: #input_type,
                        ctx: &C,
                        reply: Reply<#output_type>,
                    ) -> ActorResult<()> {
                        self.#field.#business_method(req, ctx, reply).await
                    }
                });
            } else {
                forwarders.push(quote! {
                    async fn #handler_method<C: Context>(
                        &self,
                        req: #input_type,
                        ctx: &C,
                    ) -> ActorResult<#output_type> {
                        self.#field.#business_method(req, ctx).await
                    }
                });
            }
        }

        // `async_trait` may only decorate the trait impl — emit the struct,
        // inherent impl, and seal impl without it, and the dispatch impl with it.
        let struct_and_inherent = quote! {
            /// Aggregate workload adapter — fully generated, do not edit.
            ///
            /// Composes one business handler per local proto Service and
            /// implements the sealed `ActrDispatchHandler` trait by forwarding
            /// each internal `<service>_<method>` route to the protobuf-named
            /// method on the corresponding `{Service}Handler` implementation.
            ///
            /// Business code implements the per-service `{Service}Handler`
            /// traits (see the `<service>_actor` modules) and wires them
            /// together in the workload entry:
            ///
            /// ```rust,ignore
            /// entry!(
            ///     ActrWorkload<ActrHandler<FooServiceHandler>>,
            ///     ActrWorkload::new(ActrHandler::new(foo_service))
            /// );
            /// ```
            pub struct ActrHandler<#(#generic_params: #handler_traits),*> {
                #(
                    /// Business handler for one local proto Service.
                    pub #field_names: #generic_params,
                )*
            }

            impl<#(#generic_params: #handler_traits),*> ActrHandler<#(#generic_params),*> {
                /// Compose the per-service business handlers into the workload adapter.
                pub fn new(#(#field_names: #generic_params),*) -> Self {
                    Self { #(#field_names),* }
                }
            }

            #seal_impl
        };

        let dispatch_impl = quote! {
            impl<#(#generic_params: #handler_traits),*> ActrDispatchHandler
                for ActrHandler<#(#generic_params),*>
            {
                #(#forwarders)*
            }
        };

        // Manually add #[async_trait] attribute to avoid quote! inserting
        // spaces; same ?Send on wasm32 rationale as the dispatcher trait.
        Ok(format!(
            "{struct_and_inherent}\n\
             #[cfg_attr(not(target_arch = \"wasm32\"), async_trait)]\n\
             #[cfg_attr(target_arch = \"wasm32\", async_trait(?Send))]\n\
             {dispatch_impl}"
        ))
    }

    /// Resolve a fully-qualified proto type name to a Rust type path qualified
    /// by its declaring owner module, mirroring the unified dispatcher.
    fn message_type_path(&self, type_name: &str) -> String {
        let cleaned = type_name.trim_start_matches('.');
        let owner = self.type_owners.get(cleaned).cloned().unwrap_or_else(|| {
            if let Some(last_dot) = cleaned.rfind('.') {
                cleaned[..last_dot].to_string()
            } else {
                String::new()
            }
        });
        let relative = cleaned
            .strip_prefix(&owner)
            .and_then(|name| name.strip_prefix('.'))
            .unwrap_or(cleaned);
        let mut parts: Vec<&str> = relative.split('.').collect();
        let short_type = parts.pop().unwrap_or(cleaned);
        let module = owner.replace('.', "_").to_snake_case();
        if module.is_empty() {
            short_type.to_string()
        } else {
            let nested = parts
                .iter()
                .map(|part| part.to_snake_case())
                .collect::<Vec<_>>()
                .join("::");
            if nested.is_empty() {
                format!("super::{module}::{short_type}")
            } else {
                format!("super::{module}::{nested}::{short_type}")
            }
        }
    }
}

/// Parse a Rust type path string into a `TokenStream` for `quote!`.
fn syn_str_to_tokens(type_path: &str, context: &str) -> Result<proc_macro2::TokenStream> {
    type_path.parse().map_err(|err| {
        anyhow!("Invalid generated Rust type path '{type_path}' (context: {context}): {err}")
    })
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

    fn make_route(
        route_key: &str,
        proto_file: &str,
        proto_service: &str,
        method: &str,
        input_type: &str,
        output_type: &str,
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
            dependency_alias: None,
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
    fn single_service_generates_single_field_adapter() {
        let spec = make_spec(vec![make_route(
            "echo.EchoService.Echo",
            "local/echo.proto",
            "echo.EchoService",
            "Echo",
            "echo.EchoRequest",
            "echo.EchoResponse",
        )]);
        let reply_modes = HashMap::new();
        let owners = HashMap::new();
        let generator = ActrHandlerGenerator::new(&spec, &owners, &reply_modes);
        assert!(generator.has_local_services());
        let code = generator.generate().unwrap();

        assert!(
            code.contains("pub struct ActrHandler < EchoServiceHandler"),
            "single service should yield one generic param:\n{code}"
        );
        assert!(
            code.contains("EchoServiceHandler : super :: echo_actor :: EchoServiceHandler"),
            "generic param must be constrained by the per-service handler trait:\n{code}"
        );
        assert!(
            code.contains("pub echo_service : EchoServiceHandler"),
            "adapter stores one field per service:\n{code}"
        );
        assert!(
            code.contains("pub fn new (echo_service : EchoServiceHandler) -> Self"),
            "constructor takes one handler per service:\n{code}"
        );
        // Sealed trait forwarding to the protobuf-named business method.
        assert!(
            code.contains("super :: actr_dispatch :: private :: Sealed"),
            "adapter must implement the seal:\n{code}"
        );
        assert!(
            code.contains("fn echo_service_echo"),
            "sealed trait route method is prefixed:\n{code}"
        );
        assert!(
            code.contains("self . echo_service . echo (req , ctx) . await"),
            "route method forwards to the protobuf-named business method:\n{code}"
        );
        // No concrete Impl type may be referenced.
        assert!(
            !code.contains("Impl"),
            "adapter must not reference concrete Impl types:\n{code}"
        );
    }

    #[test]
    fn same_named_rpcs_across_services_forward_to_same_business_name() {
        let spec = make_spec(vec![
            make_route(
                "ask.AskService.Ask",
                "local/ask.proto",
                "ask.AskService",
                "Ask",
                "ask.AskRequest",
                "ask.AskResponse",
            ),
            make_route(
                "chat.ChatService.Ask",
                "local/chat.proto",
                "chat.ChatService",
                "Ask",
                "chat.AskRequest",
                "chat.AskResponse",
            ),
        ]);
        let reply_modes = HashMap::new();
        let code = ActrHandlerGenerator::new(&spec, &HashMap::new(), &reply_modes)
            .generate()
            .unwrap();

        // Two generic params, one per service.
        assert!(
            code.contains("pub struct ActrHandler < AskServiceHandler : super :: ask_actor :: AskServiceHandler , ChatServiceHandler : super :: chat_actor :: ChatServiceHandler >"),
            "two generic params, one per service:\n{code}"
        );
        assert!(
            code.contains("pub ask_service : AskServiceHandler"),
            "ask field:\n{code}"
        );
        assert!(
            code.contains("pub chat_service : ChatServiceHandler"),
            "chat field:\n{code}"
        );
        assert!(
            code.contains("pub fn new (ask_service : AskServiceHandler , chat_service : ChatServiceHandler) -> Self"),
            "constructor composes every service:\n{code}"
        );
        // Internal prefixed route names do not collide...
        assert!(code.contains("fn ask_service_ask"), "ask route:\n{code}");
        assert!(code.contains("fn chat_service_ask"), "chat route:\n{code}");
        // ...while both forward to the same protobuf-named business method.
        assert!(
            code.contains("self . ask_service . ask (req , ctx) . await"),
            "ask route forwards to AskService::ask:\n{code}"
        );
        assert!(
            code.contains("self . chat_service . ask (req , ctx) . await"),
            "chat route forwards to ChatService::ask:\n{code}"
        );
        // Owner-qualified types keep the two services' messages apart.
        assert!(
            code.contains("super :: ask :: AskRequest"),
            "ask request type qualified:\n{code}"
        );
        assert!(
            code.contains("super :: chat :: AskRequest"),
            "chat request type qualified:\n{code}"
        );
    }

    #[test]
    fn explicit_route_forwards_reply_handle_untouched() {
        let spec = make_spec(vec![
            make_route(
                "echo.EchoService.Echo",
                "local/echo.proto",
                "echo.EchoService",
                "Echo",
                "echo.EchoRequest",
                "echo.EchoResponse",
            ),
            make_route(
                "echo.EchoService.Transcode",
                "local/echo.proto",
                "echo.EchoService",
                "Transcode",
                "echo.EchoRequest",
                "echo.EchoResponse",
            ),
        ]);
        let reply_modes = HashMap::from([(
            "echo.EchoService.Transcode".to_string(),
            ReplyMode::Explicit,
        )]);
        let code = ActrHandlerGenerator::new(&spec, &HashMap::new(), &reply_modes)
            .generate()
            .unwrap();

        assert!(
            code.contains("reply : Reply"),
            "EXPLICIT forwarder takes the Reply<T> handle:\n{code}"
        );
        assert!(
            code.contains("self . echo_service . transcode (req , ctx , reply) . await"),
            "EXPLICIT forwarder passes the exact handle through:\n{code}"
        );
        assert!(
            code.contains("ActorResult < () >"),
            "EXPLICIT forwarder returns ActorResult<()>:\n{code}"
        );
        assert!(
            code.contains("Context, Reply"),
            "Reply import present:\n{code}"
        );
        // The RETURN forwarder keeps its plain signature.
        assert!(
            code.contains("self . echo_service . echo (req , ctx) . await"),
            "RETURN forwarder unaffected:\n{code}"
        );
    }

    #[test]
    fn no_local_routes_is_rejected() {
        let spec = make_spec(vec![]);
        let reply_modes = HashMap::new();
        let owners = HashMap::new();
        let generator = ActrHandlerGenerator::new(&spec, &owners, &reply_modes);
        assert!(!generator.has_local_services());
        assert!(generator.generate().is_err());
    }
}
