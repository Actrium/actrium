#[cfg(not(target_arch = "wasm32"))]
use std::path::{Path, PathBuf};
#[cfg(not(target_arch = "wasm32"))]
use std::sync::Arc;
#[cfg(not(target_arch = "wasm32"))]
use std::time::Duration;

#[cfg(not(target_arch = "wasm32"))]
use crate::error::HyperError;
#[cfg(not(target_arch = "wasm32"))]
use crate::verify::TrustProvider;

/// Hyper initialization configuration.
#[cfg(not(target_arch = "wasm32"))]
#[derive(Clone)]
pub struct HyperConfig {
    /// Root data directory (instance id, non-actor-storage state).
    pub data_dir: PathBuf,

    /// Filesystem root under which per-actor storage lives.
    ///
    /// Actor storage is placed at a **fixed** layout derived from the validated
    /// actor identity hash — `{storage_root}/actors/v1/<sha256-hex>/store.sqlite3`.
    /// The actor identity never contributes a path component, so no template or
    /// manifest string can influence the location (CWE-22 / CWE-73).
    ///
    /// Defaults to `data_dir.join("storage")`. Only used by the direct-filesystem
    /// storage branch; `Hyper::with_platform()` providers own their own root.
    pub storage_root: PathBuf,

    /// Pluggable package-signature verifier. Replaces the old `TrustMode` enum.
    ///
    /// Construct via [`crate::verify::StaticTrust`], [`crate::verify::RegistryTrust`],
    /// or [`crate::verify::ChainTrust`] (or bring your own).
    pub trust_provider: Arc<dyn TrustProvider>,

    /// How far in advance of expiry the framework should fire the
    /// `on_credential_expiring` hook.
    ///
    /// Default: 5 minutes.
    pub credential_expiry_warning: Duration,

    /// Queue-length trip point for the `on_mailbox_backpressure` hook.
    ///
    /// When `Some(threshold)`, the hook fires once per incident as soon as
    /// the mailbox queued-message count crosses `threshold`, and re-arms
    /// once the queue falls back below. When `None`, a built-in default of
    /// [`DEFAULT_MAILBOX_BACKPRESSURE_THRESHOLD`] messages is used. The
    /// mailbox trait currently exposes `status()` which reports
    /// queued_messages, so the polling-based implementation in
    /// `lifecycle::node` works against any mailbox backend that supports
    /// the base trait.
    pub mailbox_backpressure_threshold: Option<usize>,

    /// Conflict-key dispatch concurrency (B2). `None` resolves to
    /// [`DispatchConcurrency::default`], whose gate is now **on** (strategy A).
    /// Default-on is free for a keyless actor: with no declared conflict key the
    /// node keeps it bit-for-bit serial and never spawns a scheduler, so the
    /// resident concurrency machinery only engages once a method declares a
    /// conflict key on a workload that can multiplex (native `Linked` /
    /// V2 `Wasm`). Pass `Some(DispatchConcurrency { enabled: false, .. })` (or
    /// set `ACTR_DISPATCH_SERIAL=1`) to force the fully-serial B1 runner. See
    /// [`DispatchConcurrency`].
    pub dispatch_concurrency: Option<DispatchConcurrency>,

    /// WASM runtime resource limits (issue #346). `None` resolves to
    /// [`WasmRuntimeLimits::default`] (safe finite defaults — hardening on by
    /// default). See [`WasmRuntimeLimits`].
    pub wasm_limits: Option<WasmRuntimeLimits>,
}

/// Conflict-key dispatch concurrency knobs (design doc §4.2, §7).
///
/// The gate defaults **on** (strategy A). Two orthogonal safety nets keep that
/// free and safe:
///
/// * **keyless zero-overhead** — an actor that declares no conflict key is kept
///   fully serial by the node, which does not even spawn a dispatch scheduler
///   for it (it stays on the M4 per-dispatch `run_loop`). Default-on therefore
///   costs a keyless actor nothing.
/// * **undeclared = global barrier** — when a scheduler *is* running (some
///   method declared a key), every *undeclared* method still projects to the
///   global [`crate::dispatch::ConflictKey::Serial`] barrier, so an unkeyed
///   route can never interleave. These two nets are independent of the gate's
///   default value.
#[cfg(not(target_arch = "wasm32"))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DispatchConcurrency {
    /// Master switch. Default `true` (strategy A default-on). A keyless actor is
    /// unaffected (kept serial with no scheduler); set `false` (or export
    /// `ACTR_DISPATCH_SERIAL=1`) to force the fully-serial B1 runner everywhere.
    pub enabled: bool,
    /// `C` — maximum number of dispatches in flight at once. Default `8`.
    pub budget: usize,
    /// `M` — total in-queue + in-flight bound; a full queue applies
    /// back-pressure up to the node entry loop. Default `256`.
    pub queue_cap: usize,
    /// Per-dispatch deadline. `None` (default) = no deadline, exactly the M4
    /// behaviour. When set, the V2 wasm runner arms an external deadline for
    /// every resident dispatch. On expiry it first drops the whole Store (the
    /// only hard cancellation boundary for Wasmtime concurrent calls), then
    /// resolves the triggering caller as `TimedOut`, fails co-resident calls
    /// retryably, and rebuilds before accepting more guest work. The native
    /// linked runner applies the same fail-closed rule but terminates because
    /// it has no generic way to reconstruct an arbitrary actor instance.
    ///
    /// Only in effect on an interleaved runner — a keyed actor with the gate
    /// on. A keyless (serial) actor runs each dispatch to completion on the
    /// pre-B2 loop and does not consult this field.
    pub dispatch_timeout: Option<std::time::Duration>,
}

#[cfg(not(target_arch = "wasm32"))]
impl Default for DispatchConcurrency {
    fn default() -> Self {
        DispatchConcurrency {
            // Default-on (strategy A): the gate defaults *on*, but an actor that
            // declares no conflict key stays bit-for-bit serial (the node never
            // spawns a scheduler for it — see `lifecycle::node`). So turning the
            // gate on by default costs a keyless actor nothing; concurrency only
            // materializes once a method declares a conflict key.
            enabled: true,
            budget: 8,
            queue_cap: 256,
            dispatch_timeout: None,
        }
    }
}

/// WASM runtime resource limits (issue #346: untrusted-workload DoS hardening).
///
/// Bounds a single wasm actor's CPU, stack, memory, and aggregate footprint so
/// a malicious or buggy guest cannot monopolize the host: fuel + epoch
/// interrupt runaway compute, a store-local resource limiter caps linear memory /
/// tables / instances, and the aggregate knobs (compile/instantiate/store
/// semaphores) bound process-wide resource use. `None` on [`HyperConfig`]
/// resolves to [`WasmRuntimeLimits::default`], whose values are safe finite
/// defaults (not unbounded) — the hardening is on by default.
///
/// This complements [`DispatchConcurrency::dispatch_timeout`]: on the V2
/// concurrent path a deadline discards the entire Store, because dropping an
/// individual Wasmtime `call_concurrent` future does not cancel its guest task.
/// Fuel and epoch interruption remain necessary so non-yielding pure compute
/// reaches a compiled check point and cannot monopolize an executor thread.
#[cfg(not(target_arch = "wasm32"))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WasmRuntimeLimits {
    /// Fuel granted to a single guest entry (instantiation or one
    /// lifecycle/hook/dispatch/data-chunk call). Serial Stores reset this
    /// before every entry. Concurrent V2 calls share Store-level accounting:
    /// a busy cohort earns another slice only when it reaches a new concurrency
    /// high-water mark; the high-water mark resets once the Store is quiescent.
    /// This prevents short-call admission churn from repeatedly refilling a
    /// long-lived sibling while retaining same-instance interleaving.
    /// Default `1_000_000` (a few ms of Cranelift-compiled compute).
    pub fuel_per_invocation: u64,
    /// Period at which a background task calls `Engine::increment_epoch`; each
    /// entry sets a deadline of a few ticks. Default `50ms`.
    pub epoch_tick: Duration,
    /// Elapsed-time deadline wrapping each guest entry (`tokio::time::timeout`,
    /// which runs on the monotonic clock and is unaffected by wall-clock
    /// adjustments). Default `5s`. Distinct from fuel/epoch: a guest that
    /// awaits a host import indefinitely is interrupted here.
    pub invocation_timeout: Duration,
    /// Max linear memory bytes per store. Default 64 MiB.
    pub max_linear_memory: usize,
    /// Max table elements per store. Default 10_000.
    pub max_table_elements: u32,
    /// Max memories per store. Default 256 (component adapters use several).
    pub max_memories: u32,
    /// Max tables per store. Default 256.
    pub max_tables: u32,
    /// Max internal core instances per store. Default 256.
    pub max_instances: u32,
    /// Max wasm stack size. Default 2 MiB.
    pub max_wasm_stack: usize,
    /// Async support stack size (wasmtime fibers). Default 3 MiB.
    pub async_stack_size: usize,
    /// If set, wasmtime cooperatively yields the guest back to the host every
    /// `n` fuel instead of trapping on exhaustion — enabling fair interleaving
    /// under `run_concurrent`. `None` (default) = trap on exhaustion; the
    /// instance is rebuilt on the next entry (`WasmWorkloadV2::ensure_instance`).
    pub fuel_async_yield_interval: Option<u64>,
    /// Max `.actr` component byte size accepted by the loader. Package
    /// verification also uses this as both the per-entry and cumulative
    /// decompressed budget across the binary/resources/protos/lock file.
    /// Default 64 MiB.
    pub max_component_bytes: usize,
    /// Max concurrent component compilations process-wide. Default 4.
    pub max_concurrent_compiles: usize,
    /// Max concurrent instantiations process-wide. Default 16.
    pub max_concurrent_instantiates: usize,
    /// Max live wasm stores process-wide. Default 256.
    pub max_active_stores: usize,
    /// Max aggregate configured linear-memory budget across live Stores.
    /// Default 1 GiB.
    pub max_total_linear_memory: usize,
    /// Max guest invocations concurrently admitted process-wide. Default 1024.
    pub max_outstanding_invocations: usize,
    /// Whether a `memory.grow`/`table.grow` over the limit traps (true) or
    /// returns an error the guest can handle (false). Default true (trap).
    pub trap_on_grow_failure: bool,
}

#[cfg(not(target_arch = "wasm32"))]
impl Default for WasmRuntimeLimits {
    fn default() -> Self {
        WasmRuntimeLimits {
            fuel_per_invocation: 1_000_000,
            epoch_tick: Duration::from_millis(50),
            invocation_timeout: Duration::from_secs(5),
            max_linear_memory: 64 * 1024 * 1024,
            max_table_elements: 10_000,
            max_memories: 256,
            max_tables: 256,
            max_instances: 256,
            max_wasm_stack: 2 * 1024 * 1024,
            async_stack_size: 3 * 1024 * 1024,
            fuel_async_yield_interval: None,
            max_component_bytes: 64 * 1024 * 1024,
            max_concurrent_compiles: 4,
            max_concurrent_instantiates: 16,
            max_active_stores: 256,
            max_total_linear_memory: 1024 * 1024 * 1024,
            max_outstanding_invocations: 1024,
            trap_on_grow_failure: true,
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl WasmRuntimeLimits {
    /// Validate that every security boundary is finite and internally
    /// consistent before loading an untrusted workload.
    pub fn validate(&self) -> Result<(), HyperError> {
        let nonzero = [
            (self.fuel_per_invocation > 0, "fuel_per_invocation"),
            (!self.epoch_tick.is_zero(), "epoch_tick"),
            (!self.invocation_timeout.is_zero(), "invocation_timeout"),
            (self.max_linear_memory > 0, "max_linear_memory"),
            (self.max_table_elements > 0, "max_table_elements"),
            (self.max_memories > 0, "max_memories"),
            (self.max_tables > 0, "max_tables"),
            (self.max_instances > 0, "max_instances"),
            (self.max_wasm_stack > 0, "max_wasm_stack"),
            (self.async_stack_size > 0, "async_stack_size"),
            (self.max_component_bytes > 0, "max_component_bytes"),
            (self.max_concurrent_compiles > 0, "max_concurrent_compiles"),
            (
                self.max_concurrent_instantiates > 0,
                "max_concurrent_instantiates",
            ),
            (self.max_active_stores > 0, "max_active_stores"),
            (self.max_total_linear_memory > 0, "max_total_linear_memory"),
            (
                self.max_outstanding_invocations > 0,
                "max_outstanding_invocations",
            ),
        ];
        if let Some((_, field)) = nonzero.into_iter().find(|(valid, _)| !valid) {
            return Err(HyperError::Config(format!(
                "WASM runtime limit `{field}` must be greater than zero"
            )));
        }
        if self.epoch_tick > self.invocation_timeout {
            return Err(HyperError::Config(
                "WASM epoch_tick must not exceed invocation_timeout".to_string(),
            ));
        }
        if self.max_linear_memory > self.max_total_linear_memory {
            return Err(HyperError::Config(
                "WASM max_linear_memory must not exceed max_total_linear_memory".to_string(),
            ));
        }
        if matches!(self.fuel_async_yield_interval, Some(0)) {
            return Err(HyperError::Config(
                "WASM fuel_async_yield_interval must be greater than zero when set".to_string(),
            ));
        }
        Ok(())
    }
}

/// Env escape hatch: `ACTR_DISPATCH_SERIAL=1` (or `true`) forces dispatch fully
/// serial regardless of config. Pure function of the raw env string so it can
/// be unit-tested without touching process env.
#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn dispatch_serial_env_override(raw: Option<&str>) -> bool {
    matches!(
        raw.map(|s| s.trim()),
        Some("1") | Some("true") | Some("TRUE") | Some("yes")
    )
}

/// Default mailbox backpressure threshold (queued-message count).
///
/// Chosen conservatively — most Actor-RTC workloads are below this in
/// steady state, so a warning at this level means queue growth needs
/// attention. Tune per-actor via
/// [`HyperConfig::mailbox_backpressure_threshold`].
#[cfg(not(target_arch = "wasm32"))]
pub(crate) const DEFAULT_MAILBOX_BACKPRESSURE_THRESHOLD: usize = 1024;

/// Default credential-expiry warning lead time.
#[cfg(not(target_arch = "wasm32"))]
pub(crate) const DEFAULT_CREDENTIAL_EXPIRY_WARNING: Duration = Duration::from_secs(5 * 60);

#[cfg(not(target_arch = "wasm32"))]
impl std::fmt::Debug for HyperConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HyperConfig")
            .field("data_dir", &self.data_dir)
            .field("storage_root", &self.storage_root)
            .field("trust_provider", &self.trust_provider)
            .field("credential_expiry_warning", &self.credential_expiry_warning)
            .field(
                "mailbox_backpressure_threshold",
                &self.mailbox_backpressure_threshold,
            )
            .field("dispatch_concurrency", &self.dispatch_concurrency)
            .field("wasm_limits", &self.wasm_limits)
            .finish()
    }
}

/// Raw TOML shape for the optional `[hyper]` section of `actr.toml`.
///
/// All fields optional; each controls one knob of [`HyperConfig`]. The
/// trust anchor lives inside `[hyper.trust]` (singular) and uses the
/// same tag-dispatched schema as the top-level `[[trust]]` array so users
/// can pick whichever style they prefer.
#[cfg(not(target_arch = "wasm32"))]
#[derive(Debug, Clone, Default, serde::Deserialize)]
pub(crate) struct HyperSection {
    /// Root data directory.
    #[serde(default)]
    pub data_dir: Option<std::path::PathBuf>,

    /// Filesystem root for per-actor storage. Relative paths resolve against the
    /// directory containing the config file.
    #[serde(default)]
    pub storage_root: Option<std::path::PathBuf>,

    /// Tombstone: the old `storage_path_template` key was removed. Kept only to
    /// produce an explicit migration error instead of letting Serde silently
    /// ignore the unknown field (this struct has no `deny_unknown_fields`).
    #[serde(default, rename = "storage_path_template")]
    pub removed_storage_path_template: Option<String>,

    /// Single trust anchor; for chain composition use the top-level
    /// `[[trust]]` array instead.
    #[serde(default)]
    pub trust: Option<HyperTrustAnchor>,

    /// Enable on-demand credential recovery for signaling authentication
    /// failures. Kept in the private runtime wrapper so adding the flag does not
    /// add a required field to the public `HyperConfig` struct.
    #[serde(default)]
    pub membership_controller_enabled: Option<bool>,
}

/// Trust anchor config for `[hyper.trust]`. Superset of
/// `actr_config::TrustAnchor` with an extra `dev_only` kind that lets
/// tests and examples opt in to [`crate::verify::StaticTrust::dev_only`]
/// explicitly.
#[cfg(not(target_arch = "wasm32"))]
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub(crate) enum HyperTrustAnchor {
    /// Accept any package — for tests and local development **only**.
    ///
    /// When selected by `Node::from_config_file`, emits a prominent
    /// `tracing::warn!` at load time.
    DevOnly,
    /// Pre-shared Ed25519 public key. See
    /// [`actr_config::TrustAnchor::Static`] for field semantics.
    Static {
        #[serde(default)]
        pubkey_file: Option<std::path::PathBuf>,
        #[serde(default)]
        pubkey_b64: Option<String>,
    },
    /// Issuer HTTP registry endpoint. See
    /// [`actr_config::TrustAnchor::Registry`].
    Registry { endpoint: String },
}

/// Top-level wrapper used when parsing `actr.toml` for the `[hyper]`
/// section in isolation. Ignores every other field.
#[cfg(not(target_arch = "wasm32"))]
#[derive(Debug, Clone, Default, serde::Deserialize)]
pub(crate) struct HyperSectionWrapper {
    #[serde(default)]
    pub hyper: HyperSection,
}

/// Load an `actr.toml`-style file and return a [`crate::Node`] in the
/// `Init` state. This function handles the full pipeline:
///
/// 1. Parse the runtime section via [`actr_config::ConfigParser`].
/// 2. Parse the optional `[hyper]` section for data-dir / template /
///    trust overrides.
/// 3. Resolve trust anchors (preferring `[hyper.trust]`, falling back to
///    top-level `[[trust]]`, finally producing an `Err` unless the
///    effective config explicitly opts into dev-only trust).
/// 4. Build a [`crate::Hyper`] and wrap it in `Node<Init>`.
#[cfg(not(target_arch = "wasm32"))]
pub(crate) async fn node_from_config_file(
    path: &Path,
) -> crate::error::HyperResult<crate::Node<crate::Init>> {
    node_from_config_file_with_package(path, None).await
}

/// Same as [`node_from_config_file`] but lets the caller supply the
/// [`actr_config::PackageInfo`] that becomes the node's registered
/// `actr_type`. Used by the language bindings, which already parsed
/// `manifest.toml` and don't want their identity collapsed to the
/// placeholder.
#[cfg(not(target_arch = "wasm32"))]
pub(crate) async fn node_from_config_file_with_package(
    path: &Path,
    package_info: Option<actr_config::PackageInfo>,
) -> crate::error::HyperResult<crate::Node<crate::Init>> {
    use crate::error::HyperError;
    use crate::verify::{ChainTrust, RegistryTrust, StaticTrust, TrustProvider};

    // Load both the RuntimeConfig and the raw TOML so we can pick up
    // [hyper.*] overrides that `ConfigParser` doesn't surface.
    let raw_text = std::fs::read_to_string(path).map_err(|e| {
        HyperError::Config(format!(
            "failed to read runtime config `{}`: {e}",
            path.display()
        ))
    })?;

    let raw_runtime: actr_config::RuntimeRawConfig = raw_text.parse().map_err(|e| {
        HyperError::Config(format!(
            "failed to parse runtime config `{}`: {e}",
            path.display()
        ))
    })?;
    // `RuntimeConfig` requires a PackageInfo. When the caller has one
    // already (bindings come in here with the manifest's `[package]`),
    // honour it so the node registers under the real actr_type. Without
    // an explicit one, fall back to the historical `local/Client@0.0.0`
    // placeholder so callers without a sibling manifest still work.
    let package_info = package_info.unwrap_or_else(|| actr_config::PackageInfo {
        name: "client".to_string(),
        actr_type: actr_protocol::ActrType {
            publisher: "local".to_string(),
            name: "Client".to_string(),
            version: "0.0.0".to_string(),
        },
        description: None,
        authors: vec![],
        license: None,
    });
    let runtime_config = actr_config::ConfigParser::parse_runtime(raw_runtime, path, package_info)
        .map_err(|e| HyperError::Config(format!("failed to parse runtime config: {e}")))?;

    // Parse the optional [hyper] section.
    let hyper_section: HyperSectionWrapper = toml::from_str(&raw_text).map_err(|e| {
        HyperError::Config(format!(
            "failed to parse [hyper] section of `{}`: {e}",
            path.display()
        ))
    })?;
    let hyper_section = hyper_section.hyper;

    // `storage_path_template` was removed in favour of the fixed hashed layout.
    // Error explicitly rather than letting Serde silently drop the key.
    if hyper_section.removed_storage_path_template.is_some() {
        return Err(HyperError::Config(
            "`storage_path_template` was removed; actor storage now uses a fixed \
             hashed layout. Configure `[hyper].storage_root` instead."
                .to_string(),
        ));
    }

    let membership_controller_enabled =
        hyper_section.membership_controller_enabled.unwrap_or(false);

    // Resolve the data_dir: [hyper].data_dir > CLI user-config default.
    let data_dir = if let Some(dir) = hyper_section.data_dir.clone() {
        expand_tilde_path(&dir)
    } else {
        actr_config::user_config::resolve_hyper_data_dir().map_err(|e| {
            HyperError::Config(format!(
                "failed to resolve default hyper data_dir (set `[hyper].data_dir` explicitly): {e}"
            ))
        })?
    };

    // Resolve trust: prefer [hyper.trust], fall back to top-level [[trust]].
    // Absolutize the config file's directory so a relative `storage_root` (and
    // `pubkey_file`) resolves against the config location, not `data_dir`.
    let base_dir = absolutize(path.parent().unwrap_or_else(|| Path::new(".")));
    let trust: Arc<dyn TrustProvider> = if let Some(anchor) = hyper_section.trust.clone() {
        match anchor {
            HyperTrustAnchor::DevOnly => {
                tracing::warn!(
                    "[hyper.trust] kind = \"dev_only\" selected; accepting any package — \
                     NEVER use in production"
                );
                Arc::new(StaticTrust::dev_only())
            }
            HyperTrustAnchor::Static {
                pubkey_file,
                pubkey_b64,
            } => {
                let key_bytes = load_static_pubkey_bytes(
                    pubkey_file.as_deref().map(|p| resolve_path(&base_dir, p)),
                    pubkey_b64,
                )?;
                Arc::new(StaticTrust::new(key_bytes)?)
            }
            HyperTrustAnchor::Registry { endpoint } => {
                let base = endpoint.trim_end_matches("/issuer").to_string();
                Arc::new(RegistryTrust::new(base))
            }
        }
    } else if !runtime_config.trust.is_empty() {
        // Chain fallback from the top-level [[trust]] anchors.
        let mut providers: Vec<Arc<dyn TrustProvider>> =
            Vec::with_capacity(runtime_config.trust.len());
        for anchor in &runtime_config.trust {
            let provider: Arc<dyn TrustProvider> = match anchor {
                actr_config::TrustAnchor::Static {
                    pubkey_file,
                    pubkey_b64,
                } => {
                    let key_bytes =
                        load_static_pubkey_bytes(pubkey_file.clone(), pubkey_b64.clone())?;
                    Arc::new(StaticTrust::new(key_bytes)?)
                }
                actr_config::TrustAnchor::Registry { endpoint } => {
                    let base = endpoint.trim_end_matches("/issuer").to_string();
                    Arc::new(RegistryTrust::new(base))
                }
            };
            providers.push(provider);
        }
        if providers.len() == 1 {
            providers.into_iter().next().unwrap()
        } else {
            Arc::new(ChainTrust::new(providers))
        }
    } else {
        return Err(HyperError::Config(
            "no `[hyper.trust]` or `[[trust]]` anchor configured. \
             Every runtime must declare a package-signature trust policy. \
             For dev / tests set `[hyper.trust] kind = \"dev_only\"`; \
             for production use `kind = \"static\"` with a `pubkey_file` \
             or `kind = \"registry\"` with an Issuer endpoint."
                .to_string(),
        ));
    };

    // Build HyperConfig with the resolved values.
    let mut hyper_config = HyperConfig::new(&data_dir, trust);
    if let Some(root) = &hyper_section.storage_root {
        // A relative `storage_root` resolves against the config file's directory,
        // never the process CWD at open time. `base_dir` is absolute (and any
        // leading `~` is expanded), so `resolved` is absolute and
        // `with_storage_root` keeps it as-is instead of rebasing onto `data_dir`.
        let root = expand_tilde_path(root);
        let resolved = resolve_path(&base_dir, &root);
        hyper_config = hyper_config.with_storage_root(resolved);
    }

    // Build Hyper and return Node<Init>. Observability bring-up is left
    // to the caller — bindings and the CLI both want control over when
    // the tracing subscriber gets installed (they may want to layer in
    // their own filters first).
    let hyper = crate::Hyper::new(hyper_config)
        .await?
        .with_membership_controller_enabled(membership_controller_enabled);
    let _ = &base_dir;
    Ok(crate::Node::from_hyper(hyper, runtime_config))
}

#[cfg(not(target_arch = "wasm32"))]
fn resolve_path(base_dir: &Path, path: impl AsRef<Path>) -> std::path::PathBuf {
    let p = path.as_ref();
    if p.is_absolute() {
        p.to_path_buf()
    } else {
        base_dir.join(p)
    }
}

/// Make `path` absolute by joining it onto the process CWD when relative.
///
/// Applied to the config file's directory so that a relative `[hyper].storage_root`
/// resolves against the (now-absolute) config location. Without this, a relative
/// config path makes `resolve_path` return a relative `storage_root`, which
/// [`HyperConfig::with_storage_root`] would then rebase a second time against
/// `data_dir` (e.g. `configs/actr.toml` + `storage_root = "store"` wrongly became
/// `<data_dir>/configs/store`).
#[cfg(not(target_arch = "wasm32"))]
fn absolutize(path: &Path) -> std::path::PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .map(|cwd| cwd.join(path))
            .unwrap_or_else(|_| path.to_path_buf())
    }
}

/// Expand a leading `~/` via the CLI's home-dir rule.
///
/// `PathBuf` does not expand `~`, so an explicit `~` in `[hyper].data_dir` or
/// `[hyper].storage_root` would otherwise create a literal `~` directory. This
/// makes an explicit tilde behave like the resolved default data_dir.
#[cfg(not(target_arch = "wasm32"))]
fn expand_tilde_path(path: &Path) -> std::path::PathBuf {
    actr_config::user_config::expand_tilde(path.to_string_lossy().into_owned())
}

#[cfg(not(target_arch = "wasm32"))]
fn load_static_pubkey_bytes(
    pubkey_file: Option<std::path::PathBuf>,
    pubkey_b64: Option<String>,
) -> crate::error::HyperResult<Vec<u8>> {
    use crate::error::HyperError;
    use base64::Engine;

    if let Some(b64) = pubkey_b64 {
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(&b64)
            .map_err(|e| HyperError::Config(format!("invalid pubkey_b64: {e}")))?;
        if bytes.len() != 32 {
            return Err(HyperError::Config(format!(
                "pubkey_b64 must decode to 32 bytes, got {}",
                bytes.len()
            )));
        }
        return Ok(bytes);
    }
    let path = pubkey_file.ok_or_else(|| {
        HyperError::Config("static trust anchor requires `pubkey_file` or `pubkey_b64`".to_string())
    })?;
    let text = std::fs::read_to_string(&path).map_err(|e| {
        HyperError::Config(format!(
            "failed to read pubkey_file `{}`: {e}",
            path.display()
        ))
    })?;
    let value: serde_json::Value = serde_json::from_str(&text).map_err(|e| {
        HyperError::Config(format!(
            "pubkey_file `{}` is not valid JSON: {e}",
            path.display()
        ))
    })?;
    let b64 = value
        .get("public_key")
        .and_then(|v| v.as_str())
        .ok_or_else(|| {
            HyperError::Config(format!(
                "pubkey_file `{}` is missing the `public_key` field",
                path.display()
            ))
        })?;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(b64)
        .map_err(|e| {
            HyperError::Config(format!(
                "pubkey_file `{}` has invalid base64: {e}",
                path.display()
            ))
        })?;
    if bytes.len() != 32 {
        return Err(HyperError::Config(format!(
            "pubkey_file `{}` must contain a 32-byte key, got {}",
            path.display(),
            bytes.len()
        )));
    }
    Ok(bytes)
}

#[cfg(not(target_arch = "wasm32"))]
impl HyperConfig {
    /// Build a new HyperConfig with the given `data_dir` and package trust provider.
    ///
    /// There is no default provider — you must explicitly decide how packages
    /// are authenticated (see [`crate::verify::StaticTrust`] /
    /// [`crate::verify::RegistryTrust`] / [`crate::verify::ChainTrust`]).
    pub fn new(data_dir: impl AsRef<Path>, trust_provider: Arc<dyn TrustProvider>) -> Self {
        let data_dir = data_dir.as_ref().to_path_buf();
        Self {
            storage_root: data_dir.join("storage"),
            data_dir,
            trust_provider,
            credential_expiry_warning: DEFAULT_CREDENTIAL_EXPIRY_WARNING,
            mailbox_backpressure_threshold: None,
            dispatch_concurrency: None,
            wasm_limits: None,
        }
    }

    /// Override the filesystem root for per-actor storage (default
    /// `data_dir.join("storage")`). Relative paths are resolved against
    /// `data_dir` at Hyper init, never against the process CWD.
    pub fn with_storage_root(mut self, storage_root: impl AsRef<Path>) -> Self {
        let root = storage_root.as_ref();
        self.storage_root = if root.is_absolute() {
            root.to_path_buf()
        } else {
            self.data_dir.join(root)
        };
        self
    }

    /// The filesystem root under which per-actor storage is placed.
    pub fn storage_root(&self) -> &Path {
        &self.storage_root
    }

    pub fn with_trust_provider(mut self, trust_provider: Arc<dyn TrustProvider>) -> Self {
        self.trust_provider = trust_provider;
        self
    }

    /// Override the credential-expiry warning lead time.
    pub fn with_credential_expiry_warning(mut self, window: Duration) -> Self {
        self.credential_expiry_warning = window;
        self
    }

    /// Set the mailbox backpressure threshold.
    ///
    /// See [`HyperConfig::mailbox_backpressure_threshold`] for semantics.
    pub fn with_mailbox_backpressure_threshold(mut self, threshold: Option<usize>) -> Self {
        self.mailbox_backpressure_threshold = threshold;
        self
    }

    /// Resolve the active mailbox backpressure threshold — explicit
    /// override or the built-in [`DEFAULT_MAILBOX_BACKPRESSURE_THRESHOLD`].
    pub fn resolved_mailbox_backpressure_threshold(&self) -> usize {
        self.mailbox_backpressure_threshold
            .unwrap_or(DEFAULT_MAILBOX_BACKPRESSURE_THRESHOLD)
    }

    /// Set the conflict-key dispatch concurrency config (B2). `None` falls back
    /// to [`DispatchConcurrency::default`] (gate on, strategy A); pass
    /// `Some(DispatchConcurrency { enabled: false, .. })` to force serial.
    pub fn with_dispatch_concurrency(mut self, cfg: Option<DispatchConcurrency>) -> Self {
        self.dispatch_concurrency = cfg;
        self
    }

    /// Resolve the effective dispatch concurrency, applying the
    /// `ACTR_DISPATCH_SERIAL` escape hatch (which forces `enabled = false`).
    pub fn resolved_dispatch_concurrency(&self) -> DispatchConcurrency {
        let mut cfg = self.dispatch_concurrency.unwrap_or_default();
        let raw = std::env::var("ACTR_DISPATCH_SERIAL").ok();
        if dispatch_serial_env_override(raw.as_deref()) {
            if cfg.enabled {
                tracing::warn!(
                    "ACTR_DISPATCH_SERIAL is set; forcing fully-serial dispatch \
                     (dispatch concurrency disabled)"
                );
            }
            cfg.enabled = false;
        }
        cfg
    }

    /// Set the WASM runtime resource limits (issue #346). `None` falls back to
    /// [`WasmRuntimeLimits::default`] (safe finite defaults).
    pub fn with_wasm_limits(mut self, cfg: Option<WasmRuntimeLimits>) -> Self {
        self.wasm_limits = cfg;
        self
    }

    /// Resolve the effective WASM runtime limits. `None` resolves to
    /// [`WasmRuntimeLimits::default`]; no env escape hatch yet (fuel/epoch are
    /// safety-critical, not a tuning surface).
    pub fn resolved_wasm_limits(&self) -> WasmRuntimeLimits {
        self.wasm_limits.unwrap_or_default()
    }
}

#[cfg(test)]
#[path = "config_tests.rs"]
mod tests;
