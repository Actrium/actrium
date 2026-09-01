/**
 * Configuration types for Actor-RTC Web
 */

/**
 * Worker type
 */
export type WorkerType = 'service-worker' | 'web-worker';

/**
 * Service Worker runtime configuration
 *
 * Passed from main thread to Service Worker via DOM_PORT_INIT,
 * then forwarded to WASM register_client().
 */
export interface SwRuntimeConfig {
  /** AIS (Actor Identity Service) endpoint URL (e.g. http://host:port/ais) */
  ais_endpoint: string;

  /** Signaling WebSocket URL (e.g. wss://host:port/signaling/ws) */
  signaling_url: string;

  /** Deployment realm ID */
  realm_id: number;

  /** This actor's type (manufacturer:name:version) */
  client_actr_type: string;

  /** Target actor type for peer discovery (manufacturer:name:version) */
  /** Optional legacy default target. New workloads resolve dependency aliases at call time. */
  target_actr_type?: string;

  /** Service fingerprint for exact matching (optional) */
  service_fingerprint: string;

  /** ACL allow-list of actor types */
  acl_allow_types: string[];

  // ── Package loading (guest-bridge) ──

  /** URL of the signed .actr workload package (e.g. "/packages/echo.actr"). */
  package_url?: string;

  /** URL of the SW host WASM (wasm-pack output, e.g. "/packages/actr_sw_host_bg.wasm").
   *  Loaded independently of the workload; guest WASM and glue are authenticated
   *  resources inside the signed `.actr` package. */
  runtime_wasm_url?: string;

  // ── Package verification (Web verify_package) ──

  /** Trust anchors for verifying the .actr package signature. Array form
   *  mirrors the runtime `[[trust]]` config in `actr.toml`.
   *
   *  Verification is mandatory. The browser Service Worker currently honours
   *  only a usable `kind = "static"` anchor with `pubkey_b64`. A registry-only
   *  configuration, an empty array, or static entries without a resolved
   *  `pubkey_b64` cause initialization to fail; verification is never skipped.
   *
   *  A host may resolve `pubkey_file` before sending this config to the worker,
   *  but browser code cannot read that filesystem path itself. */
  trust?: TrustAnchor[];
}

/** Trust anchor config, matching `actr_config::TrustAnchor` in `actr_config`. */
export type TrustAnchor =
  | {
      /** Pre-shared Ed25519 public key; accepts any manufacturer. */
      kind: 'static';
      /** Base64 (standard) of the 32-byte Ed25519 public key. */
      pubkey_b64?: string;
      /** Path to a JSON file with a `public_key` field. This is not usable by
       *  the browser directly; a host integration must resolve it to
       *  `pubkey_b64` before initializing the Service Worker. */
      pubkey_file?: string;
    }
  | {
      /** Look up MFR public keys via AIS HTTP registry. Not yet implemented
       *  in the browser SW; a registry-only config fails closed. */
      kind: 'registry';
      endpoint: string;
    };

/**
 * Actor System configuration
 */
export interface ActorSystemConfig {
  /** Signaling server URL */
  signalingUrl: string;

  /** Realm name */
  realm: string;

  /** Actor identity (optional) */
  identity?: string;

  /** Auto-reconnect on disconnect */
  autoReconnect?: boolean;

  /** Connection timeout in milliseconds */
  connectionTimeout?: number;

  /** Worker type to use */
  workerType?: WorkerType;

  /** Service Worker file path (default: '/actor.sw.js') */
  serviceWorkerPath?: string;

  /** STUN/TURN server configuration */
  iceServers?: RTCIceServer[];

  /** ICE transport policy ('all' or 'relay' for force_relay mode) */
  iceTransportPolicy?: RTCIceTransportPolicy;

  /** Runtime config for the Service Worker WASM layer */
  runtimeConfig?: SwRuntimeConfig;
}

/**
 * Actor Client configuration
 */
export interface ActorClientConfig extends ActorSystemConfig {
  /** Enable debug logging */
  debug?: boolean;

  /** Retry configuration */
  retry?: RetryConfig;
}

/**
 * Retry configuration
 */
export interface RetryConfig {
  /** Maximum retry attempts */
  maxAttempts?: number;

  /** Initial retry delay in milliseconds */
  initialDelay?: number;

  /** Maximum retry delay in milliseconds */
  maxDelay?: number;

  /** Backoff multiplier */
  backoffMultiplier?: number;
}

/**
 * Default configuration values
 */
export const DEFAULT_CONFIG: Partial<ActorClientConfig> = {
  autoReconnect: true,
  connectionTimeout: 30000,
  workerType: 'service-worker',
  debug: false,
  retry: {
    maxAttempts: 3,
    initialDelay: 1000,
    maxDelay: 10000,
    backoffMultiplier: 2,
  },
};
