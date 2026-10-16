# Rust Core Boundary

Actrium targets macOS, iOS, Android, and Windows. Native non-UI logic should move
into Rust when it is shared, stateful, security-sensitive, protocol-oriented, or
likely to diverge across platform rewrites. It should also move when leaving it
in platform code creates repeated cross-layer hops for deterministic non-UI
work, even before a second platform consumes it.

## Anti-Fragmentation Guardrail

Rust is the shared core, not a dumping ground for every repeated literal or
small helper. A candidate should move to Rust only when the resulting ownership
graph is simpler than the platform-local version.

Prefer Rust when it creates a single source of truth for durable protocol,
schema, parser, persistence, crypto, or deterministic data transformation
rules. Defer or avoid Rust when the behavior is mostly platform shell workflow,
UI routing, OS SDK orchestration, local notification plumbing, or a thin
one-platform convenience wrapper.

Before moving another boundary, check these costs:

- Does the call path become platform -> Rust -> same platform for a tiny local
  decision?
- Does the platform now need extra generated bindings, facade wrappers, and
  adapter structs for a rule that was already local and obvious?
- Does the extraction split one user-facing workflow across too many files or
  languages?
- Is the rule still changing with platform UX decisions?
- Would a future Windows adapter actually reuse the same non-UI rule, or would
  it need a different shell policy anyway?

If the answer to any of these is yes, keep the platform workflow local and only
share stable wire names, schema values, or parser/normalization rules. Existing
Rust-owned surfaces should be periodically reviewed with the same test: if a
helper only adds a cross-language hop without reducing drift or complexity, it
should be collapsed back into the platform adapter or capped at constants.

When a platform consumes many Rust-owned constants in one workflow, group them
locally behind a small platform contract/cache instead of scattering raw FFI
helper calls through UI code. This preserves Rust as the source of truth while
keeping Swift/Kotlin workflow code readable and avoiding repeated tiny FFI hops.

## Current Adapter Audit

- TypeScript runtime code consumes Rust-generated contracts through
  `packages/actrium/src/runtime-host/coreContract.ts`; direct imports from the
  generated contract file are blocked by `npm run check:generated`.
- Android runtime bridge keys, event names, payload builders, and `openTab`
  classification are grouped under `RuntimeBridgeParams`. Rust-generated
  runtime DTOs are mapped there into local Kotlin payload types before reaching
  `MainViewModel`, `TabletViewModel`, or `ActiveModel`. Phone/tablet view
  models still own routing, focus, fullscreen, and collection mutation.
- Android user-info/login-result bridge constants are grouped under
  `UserInfoBridgeParams`; `UserInfoModule` still owns WeChat/Google SDK
  orchestration, coroutine collection, active React instance checks, and RN event
  dispatch.
- Apple apps group the heaviest runtime key usage under local Swift adapters
  such as `RuntimeBridgeContract` and `RuntimeOpenTabContract`. Remaining direct
  `ActriumCore` imports should stay near native module, repository, or domain-model
  boundaries; new SwiftUI/UIKit view code should prefer those adapters instead
  of calling generated FFI helpers directly.
- iOS RN tab and user-info modules now consume local Swift contracts; their Rust
  helper calls live in nearby model/manager boundary files. Keep following that
  pattern when another RN module gains a real native manager boundary.
- Apple ACTR package manifest reading uses the shared
  `AppleActrPackageManifestReader` helper in the `ActriumCore` Swift package.
  Target-local `ActrPackageManifestReader` adapters only map that shared result
  into platform-local manifest info for `BrowserViewModel`.
- TTS, Email, and ASR native modules may keep direct Rust bridge-contract imports
  while the module itself is the real platform boundary. Add a local adapter only
  when another manager/view layer starts consuming the same Rust values.
- Windows C# code consumes the Rust core through a single adapter,
  `windows/src/ActriumCore.cs`, backed by the minimal C ABI in
  `crates/actrium-core/include/actrium_core.h`. UI files must not add their own
  `DllImport` calls; `npm run check:rust-boundary` guards that.
- Windows WebView2 page/host messaging is split between
  `windows/src/HostBridgeProtocol.cs` and
  `windows/src/HostMessageBridge.cs`. The protocol dispatcher may
  call the local `ActriumCore` adapter for stable metadata or other durable non-UI
  payloads. Its tab and navigation methods delegate back to `MainForm`; WebView
  lifecycle, page routing, tab activation, and high-volume UI events stay out of
  Rust.
- Windows WebView2 profile setup lives in
  `windows/src/BrowserProfilePolicy.cs`,
  `windows/src/WindowsAppPaths.cs`, and `MainForm.cs`. User data
  folder selection, cookies/cache/local storage placement, private profile
  folder creation/cleanup, and shared `CoreWebView2Environment` reuse are
  browser-shell lifecycle concerns. Keep them local unless a future
  cross-platform browser profile schema appears.
- Windows browsing-data clearing lives in `MainForm.cs` and
  `windows/src/BrowserBrowsingDataPolicy.cs`. WebView2 profile data
  deletion, native confirmation prompts, cache/cookie/site-data selection, and
  status text are desktop browser-shell UX. Rust should not own this unless a
  future shared privacy policy introduces durable non-UI rules.
- Windows content blocking lives in `MainForm.cs`,
  `windows/src/BrowserContentBlockingPolicy.cs`, and
  `windows/src/BrowserContentBlockingStore.cs`. The current rule set,
  WebView2 `WebResourceRequested` interception, blocked-response generation,
  menu toggle, and local setting persistence are Windows shell behavior. Rust
  should wait for a real shared rule-list parser, privacy schema, or
  cross-platform blocking model before taking ownership.
- Browser history/bookmark/session snapshot schema now has a shared Rust
  contract in `crates/actrium-core/src/browser_data.rs`. Windows
  `BrowserHistoryStore.cs`, `BrowserBookmarkStore.cs`, and
  `BrowserSessionStore.cs` delegate whole-snapshot normalization (canonical
  addresses, titles, dedupe, ordering, caps, stable session tab ids) to
  ActriumCore through `ActriumCore.cs` and persist the Rust output verbatim; the
  local C# rules survive only as a degraded fallback when the native DLL is
  absent. `NavigationPolicy.TryNormalizeAddress` remains address-bar input
  parsing and is no longer reapplied on the persistence path. Apple
  (`AppleBrowserStorageDatabase.saveSession`) and Android
  (`BrowserStorageDatabase.saveSession`) likewise route session snapshots
  through `browser_session_snapshot_normalize_json` before writing rows.
- Browser history/bookmark/session storage schema also has a Rust-owned v6
  contract in `crates/actrium-core/src/browser_data.rs`. It defines the
  `actro_browser.db` table set: `browser_history_pages`,
  `browser_history_visits`, `browser_bookmark_folders`, `browser_bookmarks`,
  `browser_sessions`, `browser_session_tabs`, and `browser_sync_cursors`, plus
  baseline repository SQL for importing/querying history, bookmarks, and
  session tabs. Version 2 added stable `browser_session_tabs.tab_id` values so
  browser-page sessions can be restored and later merged by tab identity rather
  than order alone. Version 3 adds sync metadata (`sync_state`,
  `sync_revision`, `origin_device_id`, `last_synced_at`) and `deleted_at`
  tombstones to browser storage rows, and switches history/bookmark deletes to
  local-equivalent tombstone updates so later sync can observe removals.
  Version 4 adds `browser_sync_cursors` and a separate Rust-owned
  `BrowserStorageSyncSql` contract for dirty-row selection, remote apply
  upserts, mark-synced updates, and per-scope collection cursors. Version 5
  adds bookmark `ownerKind` / `ownerId` metadata so durable user/native
  bookmarks and tab-derived bookmark state no longer overwrite one another.
  Version 6 adds session `ownerKind` / `ownerId` plus `surfaceKind` /
  `surfaceId` metadata so saved browser sessions can be distinguished by
  device and platform surface. Rust also owns the first typed
  `BrowserStorageSyncBatch` JSON contract and normalizer.
  Remote batches carry per-collection cursor handoff values, and cycle
  responses merge those pull cursors into `ackBatch.cursors` so every platform
  advances cursors through the same `upsertSyncCursor` path. Rust also owns
  dirty-row-to-batch/push converters, ack/cursor normalizers, push/pull/cycle
  transport envelope helpers, `browserStorageSyncClientCyclePlanJson`,
  `browserStorageSyncClientAckBatchAfterRemoteApplyJson`,
  `browserStorageSyncTransportContractJson`,
  `browserStorageSyncCycleTransportResultJson`,
  `browserStorageSyncStatusPolicyJson`,
  `browserStorageSyncScheduleStateJson`,
  `browserStorageSyncAckPlanJson`,
  `browserStorageSyncRemoteApplyPlanJson`,
  `browserStorageSyncRemoteApplyResultJson`,
  `browserStorageSyncServerMergePlanJson`, and
  `browserStorageSyncServerMergeResultJson` helpers. The dirty-row converters
  let native SQLite adapters query Rust-provided `selectDirty*` SQL and pass
  one JSON row array back to Rust, which then owns the typed record, push
  payload, pull-cursor, and client cycle-plan shape without per-row FFI. The
  transport helpers keep device/scope/cursor payload shape and status
  classification in Rust before platform retry scheduling. The status-policy
  helper maps those classifications to stable scheduler/client/credential/user
  action names plus deterministic retry delay, ceiling, jitter, telemetry level,
  and retry-reset metadata. The schedule-state helper turns that policy plus an
  observed timestamp into `nextRetryAttempt`, `nextRunDelaySeconds`,
  `nextRunAtUnixSeconds`, waiting-for-credentials, user-action, and reset flags
  that platform schedulers can consume without reimplementing backoff math. The
  post-remote ack
  helper keeps pull cursors in the ack path after successful remote apply or
  local-dirty conflicts, but strips those cursors after hard remote apply
  failures so a client can still ack accepted local push records without
  advancing past unapplied remote rows. The ack helper turns reference/cloud ack
  payloads into native-executable `mark*Synced` and
  `upsertSyncCursor` operations; the remote apply helper turns normalized
  remote batches into ordered `applyRemote*` operations with tombstone params.
  `browserStorageSyncConflictPolicyJson` exposes the Rust-owned
  `skipLocalDirtyOrNewerRevision` policy, `localDirtyOrNewerRevision` reason,
  SQL guard semantics, affected record types, and required client action.
  `browserStorageSyncMergePolicyJson` exposes collection-specific data merge
  policy for history pages, history visits, bookmark folders, bookmarks,
  sessions, and session tabs, including identity keys, apply ordering,
  tombstone behavior, reference/cloud merge requirements, and the UI responsibilities
  that stay platform-owned. Remote apply operations carry the relevant
  `mergePolicy` name while native SQLite adapters still execute statement
  keys and params. `browserStorageAddressCanonicalizationPolicyJson` documents
  the first Rust-owned persisted/sync address rules, and
  `browserStorageCanonicalizeAddressesJson` canonicalizes address batches for
  storage export/import, returns `canonicalizationSummary` diagnostics, and
  avoids making address-bar navigation, search expansion, or WebView lifecycle
  a per-address FFI path.
  `browserStorageBookmarkFolderTreeIntegrityPolicyJson` documents the first
  Rust-owned bookmark folder graph rules, and
  `browserStorageBookmarkFolderTreeIntegrityPlanJson` validates folders,
  current/candidate move batches, or sync-record batches in one call. It
  normalizes self-parent references, ignores deleted folders and missing
  partial-batch parents in the active graph, returns `cycleDetected`
  rejection issues, and adds `integrityAction` / `integrityReason` diagnostics
  on each normalized folder plus `integritySummary` counts and id groups before
  native SQLite apply without creating per-move FFI chatter.
  `browserStorageBookmarkOwnershipPolicyJson` documents the first
  Rust-owned bookmark ownership/source-resolution rules, and
  `browserStorageBookmarkOwnershipPlanJson` resolves bookmark arrays,
  current/candidate batches, or sync-record batches in one call. It retains
  active user-owned bookmarks ahead of tab-derived bookmarks for the same
  canonical address, returns `retentionReason` / `suppressedBookmarkIds` /
  `ownershipSummary` diagnostics for same-address source decisions, preserves
  tab unstar/delete tombstones without hiding the user-owned bookmark, and keeps
  v7 multi-source schema, source labels, native SQLite writes, and conflict UI
  platform/product-owned.
  `browserStorageHistoryVisitAggregationPolicyJson` documents the
  first Rust-owned history visit aggregation rules, and
  `browserStorageHistoryVisitAggregationPlanJson` turns visit arrays,
  current/candidate batches, or sync-record batches into canonical
  history-page aggregates in one call. It deduplicates by visit id, excludes
  deleted visits from page aggregates, derives visit count and first/latest
  timestamps by canonical address, returns `aggregationAction` /
  `aggregationReason` / `aggregationSummary` diagnostics for active included
  visits, duplicate-id overlays, multi-visit pages, and preserved tombstones,
  and keeps native SQLite apply execution platform-owned.
  `browserStorageRetentionPruningPolicyJson` documents the first
  Rust-owned history/session retention pruning rules, and
  `browserStorageRetentionPruningPlanJson` turns history pages, history visits,
  session tabs, current/candidate batches, or sync-record batches into a
  native-executable prune-candidate plan in one call. It keeps the newest active
  history pages globally, supports `maxHistoryPagesPerOriginDevice` windows from
  normalized history-page `originDeviceId` metadata, prunes visits for overflow
  pages, reuses session-tab ordering before applying per-session tab limits, and
  supports `maxSessionTabsPerOriginDevice` windows after tab ordering. It
  preserves tombstones, returns `retentionReason` diagnostics for keep/prune/
  tombstone decisions plus `retentionSummary` counts and kept/pruned/tombstone
  id groups, and keeps SQLite execution/provider IO/product UX
  platform-owned. The remote apply result helper turns native
  rows-affected/error reports into applied/skipped/failed counts plus conflict
  and failure records. The reference merge helpers turn canonical client push
  requests into ordered `mergePushed*` operations, then summarize reference
  rows-affected/error reports into accepted/skipped/failed counts,
  rejected records, failures, accepted per-record `syncRevision` values,
  cursor updates, and the `ackBatch` that clients can feed back into the
  existing ack path. Rust also owns the first reference storage SQL contract for
  these `mergePushed*` operations: a server revision allocation statement,
  SQLite-compatible reference history/bookmark/session tables, device cursor
  persistence, and `selectRemote*` pull queries that emit the same row aliases
  consumed by the batch normalizer. The reference merge binding rule is explicit:
  execute `allocateServerRevision`, then prepend the returned server revision
  to the operation params before running the matching `mergePushed*` SQL.
  `crates/actrium-sync` consumes those contracts as a SQLite reference
  executor: it initializes the reference schema, applies pushed merge
  plans, returns accepted revisions/cursors, builds remote pull batches with
  cursor handoff values, and can run a first local push-plus-pull sync cycle.
  It is not part of the
  client FFI surface. The same reference crate now exposes `BrowserSyncService`,
  an in-process endpoint boundary that registers devices, checks token-backed
  request contexts, enforces account/scope and device matching, applies push
  record limits, and caps pull batch size before delegating to the executor.
  It also exposes a transport-neutral HTTP facade:
  `BrowserSyncHttpRequest`, `BrowserSyncHttpResponse`, and
  `BrowserSyncService::handle_http_request` map the Rust-owned endpoint paths
  and headers to `push`, `pull`, `cycle`, and cursor service calls with stable
  JSON error responses. The crate also includes a minimal `tiny_http` binary
  wrapper for local end-to-end probing. That binary adapts sockets into the
  facade and supports local device bootstrap. This crate is a reference
  executor and local probe, not the product sync backend. Product browser sync
  should use user-owned cloud storage through provider adapters; see
  `docs/architecture/browser-sync-cloud-drive.md`.
  Rust also owns the first provider connection/session status contract for that
  cloud-drive path: `browserStorageSyncCloudProviderConnectionContractJson` and
  `browserStorageSyncCloudProviderConnectionStatusPolicyJson` accept a coarse
  native provider snapshot and classify whether the selected provider is ready
  to sync, needs sign-in/reconnect, needs provider storage setup, is over quota,
  is temporarily unavailable, has an unsupported adapter, or needs local device
  membership documents written. The output carries
  `shouldAttemptSync`, `shouldAttemptMembership`, scheduler/provider/user
  actions, retry timing, and diagnostics while leaving OAuth, secure provider
  credential storage, provider SDK calls, and UI wording platform-owned.
  Rust also owns the first batch-only deterministic storage policies that should
  stay identical across cloud-drive providers and native SQLite adapters:
  `browserStorageAddressCanonicalizationPolicyJson` /
  `browserStorageCanonicalizeAddressesJson`,
  `browserStorageBookmarkFolderTreeIntegrityPolicyJson` /
  `browserStorageBookmarkFolderTreeIntegrityPlanJson`,
  `browserStorageBookmarkRestorePlacementPolicyJson` /
  `browserStorageBookmarkRestorePlacementPlanJson`,
  `browserStorageHistoryVisitAggregationPolicyJson` /
  `browserStorageHistoryVisitAggregationPlanJson`, and
  `browserStorageSessionTabOrderingPolicyJson` /
  `browserStorageSessionTabOrderingPlanJson`, and
  `browserStorageSessionActiveTabConsistencyPolicyJson` /
  `browserStorageSessionActiveTabConsistencyPlanJson`, and
  `browserStorageRetentionPruningPolicyJson` /
  `browserStorageRetentionPruningPlanJson`. These helpers operate on
  whole imported/exported batches so native code does not bounce per tab, visit,
  bookmark, folder, address, or session through Rust. The bookmark
  restore-placement helper resolves same-id delete/restore conflicts by state
  timestamp, returns `stateConflicts` and `stateConflictSummary` diagnostics from
  the same batched plan, and adds `placementReason` / `stateConflictReason`
  diagnostics to normalized bookmarks while still using persisted folder ids
  only; bookmark list UI state remains platform/product-owned. The address canonicalizer strips username/password
  userinfo before persistence/sync and reports `credentialStripped` /
  `canonicalizationSummary` diagnostics without taking over native address-bar
  input or navigation. The session-tab
  ordering helper returns `orderingAction` / `orderingReason` diagnostics for
  already-contiguous, renumbered, or tombstone rows plus `orderingSummary`
  counts and ordered/renumbered/tombstone id groups. The active-tab consistency
  helper clamps persisted restore metadata, returns
  `consistencyReason` diagnostics for within-range, clamped, empty-session, or
  tombstone decisions plus `consistencySummary` counts and session/active-tab id
  groups, and still leaves live foreground focus platform-owned.
  Client request assembly for the HTTP reference path is also a Rust-owned
  protocol boundary:
  `browserStorageSyncCycleRequestPlanJson` turns a push request and current
  local cursors into the canonical cycle URL, headers, and `bodyJson` while
  naming the response classifier and apply action. Apple, Android, and Windows
  storage adapters expose this through `exportSyncCycleRequestPlan` /
  `ExportSyncCycleRequestPlan`. Apple, Android, and Windows also have thin
  native sync-cycle clients above those adapters:
  `AppleBrowserStorageSyncClient`, Android `BrowserStorageSyncClient`, and
  Windows `BrowserStorageSyncClient`. They send one Rust-planned cycle request
  through URLSession/HttpURLConnection/HttpClient, classify the transport result
  through Rust, and invoke the platform `applySyncCycleResponse` /
  `ApplySyncCycleResponse` entry only on success. Apple, Android, and Windows
  also have native sync coordinators above those clients. The coordinators use
  injectable credential providers, skip network work with a structured
  `credentialsUnavailable` result when `baseUrl` / `accessToken` / `scope` /
  `deviceId` are missing, include the Rust-owned `statusPolicy` payload in
  cycle results, include the Rust-owned `scheduleState` payload for the next
  scheduler decision, and delegate actual transport/apply work to the existing
  sync clients. Apple, Android, and Windows also have first native scheduler
  adapters that consume `scheduleState`, keep retry attempts native, debounce
  local browser-data changes, and schedule foreground/in-process follow-up
  work. iOS now reads sync credentials from
  `UserInfoManager`, Android mirrors `AuthRepository` token saves/clears into a
  native credential store through `BrowserDataModule`, macOS has a local
  `UserDefaults`-backed credential provider, and Windows has a JSON-backed
  credential store. These endpoint-token paths are transitional development
  hooks for the HTTP reference cycle. Platform code still owns cloud provider
  sign-in, OS background scheduler execution, telemetry delivery,
  product-visible sync status, provider membership UX, and user recovery. Rust
  owns the cloud manifest/device/log/snapshot document contracts through
  `browser_storage_sync_cloud_data_model_json` and companion normalize helpers.
  Rust also owns `browserStorageSyncCloudStoreCapabilityPreflightJson` as the
  provider capability gate before cloud-file IO starts: provider adapters pass
  their `BrowserStorageSyncCloudStoreCapabilities`, and Rust returns
  ready/degraded/block classifications for root-path setup, optimistic revision
  safety, object-size limits, and adapter-emulation warnings while provider SDK
  calls and account UI remain native.
  Rust now also owns the provider adapter object-space contract:
  `browserStorageSyncCloudProviderAdapterContractJson` and
  `browserStorageSyncCloudProviderAdapterPlanJson` turn a selected provider
  kind, adapter family, root path, capabilities, or capability preflight into
  the canonical `Actro/BrowserSync` layout, required objects/prefixes, coarse
  operation groups, production-readiness flags, root setup actions, and
  adapter-emulation decisions. This is intentionally a batched plan above
  provider IO; concrete SDK calls still run in native/OpenDAL/Apple adapters, so
  product code does not bounce every cloud object through FFI.
  Rust also owns `browserStorageSyncCloudCompatibilityStatusJson` as the
  cloud-file compatibility preflight gate: provider adapters pass
  manifest/device/log/snapshot bytes before normalization or read/write
  continuation, and Rust classifies them as compatible, client-upgrade-required,
  cloud-data-migration-required, or invalid/recoverable. Rust also owns
  `browserStorageSyncCloudCompatibilityPreflightJson` for batched provider-read
  preflight, so adapters can pass a whole cloud file batch once and receive the
  aggregate continue/block/upgrade/migrate/recover decision without per-file FFI
  calls or platform-specific priority rules.
  Rust also owns `browserStorageSyncCloudRecoverySnapshotPlanJson` for the
  explicit migration/repair recovery snapshot batch. It consumes the cloud schema
  migration playbook plus the same compatibility preflight and returns ordered
  `writeRecoveryCopy` operations under `Actro/BrowserSync/recovery/...`; provider
  adapters execute the read/write copies through their existing object-store
  layer instead of adding per-file copy FFI or independently choosing backup
  paths. `browserStorageSyncCloudRecoverySnapshotResultPlanJson` consumes the
  whole provider receipt/error batch for those copies and returns the shared
  complete/reread/retry/provider-recovery decision before cloud migration or
  repair may proceed.
  `browserStorageSyncCloudLocalFolderRecoverySnapshotRoundJson` is the coarse
	  local-folder execution entry: native tests or a local-folder provider pass the
	  root, migration playbook, and compatibility preflight once, and Rust copies
	  real source objects to recovery paths through `BrowserStorageSyncCloudStore`
	  before running the same result classifier. Recovery snapshot rounds include a
	  Rust-owned `roundSummary` for read/write counts, planned snapshot counts,
	  migration/repair document counts, snapshot mode/root, source and snapshot
	  paths, successful snapshot paths, reasons, and issue diagnostics. This keeps
	  the native boundary at a whole recovery-snapshot round rather than one FFI
	  call per file. Apple,
  Android, and Windows local-folder sync clients consume that round on
  compatibility-blocked cycles and return the rollout/playbook/recovery bundle
  to product UI or scheduler layers; compatible cycles leave those recovery
  fields null and continue through normal cycle/compaction/cleanup.
  `browserStorageSyncCloudLocalFolderCompatibilityPreflightJson` is the
  coarse local-folder version of the same gate: native clients pass only a root
  directory and observation time, and Rust lists/reads real manifest/device/log/
  snapshot files through `BrowserStorageSyncLocalFolderCloudStore` before
  returning the shared compatibility-preflight plan.
  Rust also owns the first cloud device membership write planner:
  `browserStorageSyncCloudMembershipPlanJson` consumes provider-read
  manifest/device bytes, provider revision tokens, actor/member device identity,
  and a join/revoke action, then returns ordered `writeDevice` and
  `writeManifest` operations with `writeReason` diagnostics plus the next
  manifest/device JSON. Provider adapters still own provider session UI, SDK
  calls, and retry loops, but they should not independently mutate cloud
  membership rows or revive revoked devices.
  `browserStorageSyncCloudMembershipResultPlanJson` is the matching receipt
  handoff, so partial writes, conflicts, provider auth recovery, and quota
  recovery are classified consistently across platforms.
  The Apple, Android, and Windows local-folder membership clients also wrap
  those membership rounds with Rust-owned provider diagnostics and cloud
  observability events, so connect/revoke status surfaces do not invent separate
  priority or export semantics from sync cycles. `browserStorageSyncCloudMembershipStatusPolicyJson`
  now classifies each whole membership round once, returning the shared
  `statusPolicy` payload for connect/revoke completion, retry/reread, and
  provider/user recovery actions without moving provider SDK calls into Rust.
  `browserStorageSyncCloudMembershipScheduleStateJson` turns that membership
  status policy into the shared `scheduleState` shape, including retry timing,
  provider waiting flags, user-action flags, and pending cloud reread state, so
  connect/revoke diagnostics and scheduler decisions do not diverge from the
  cloud-cycle path.
  Rust also owns the first cloud-drive push write planner:
  `browserStorageSyncCloudPushPlanJson` consumes provider-read manifest/device
  document bytes, provider revision tokens, and a push request, then returns
  ordered `writeDevice`, `writeLog`, and `writeManifest` operations with
  `writeReason` diagnostics, a Rust-owned `writeSummary`, and the next manifest
  JSON. The summary counts operation types, required writes, optimistic-revision
  writes, touched collections, write reasons, write paths, and log paths so
  platform/provider adapters do not parse planned writes differently before
  executing them. Platform/provider adapters still execute those writes and pass
  the resulting write
  receipts/errors back to
  `browserStorageSyncCloudPushResultPlanJson`. Rust then decides whether local
  dirty rows can be acknowledged through the existing ack plan path, whether
  manifest/device state must be reread before rebuilding the push plan, or
  whether provider credential/quota/error recovery is required. Push result
  issues preserve `writeReason`, and successful results expose `ackAction` /
  `ackReason`, so platform diagnostics do not reconstruct device/log/manifest
  write roles or local-ack motivation.
  Rust now also owns the cloud-drive pull read planner:
  `browserStorageSyncCloudPullPlanJson` consumes provider-read manifest bytes
  plus local cursor JSON and returns collection-level `readSnapshot` /
  `listLogs` operations with `readReason` diagnostics and a Rust-owned
  `readSummary`. The summary counts required reads, active collections, snapshot
  reads, log listings, direct log/object reads, cloud-state rereads, page
  cursors, read reasons, read paths, and read prefixes so provider adapters do
  not parse planned reads differently before executing them. Platform/provider
  adapters still own object bytes IO, but Rust also owns
  `browserStorageSyncCloudPullContinuationPlanJson`, which
  turns provider `listLogs` results with `nextCursor` into follow-up list
  operations carrying `pageCursor` and `readReason` plus a Rust-owned
  `continuationSummary` for read-result counts, continuable list-result counts,
  collections, prefixes, page cursors, read reasons, and source operation
  indices. That keeps continuation cursor shape and read selection reasons
  shared; Rust-backed store runners now execute those continuation pages in the
  pull round, while concrete external provider adapters keep their SDK calls
  behind the same store boundary.
  Rust-executed planned read results preserve `readReason`, including
  `readListedCloudLogObject` for log-object reads expanded from a provider log
  listing. Rust also owns
  `browserStorageSyncCloudPullApplyPlanJson`, which turns those provider-read
  snapshot/log objects into a normalized `remoteBatchJson` and native-executable
  remote apply plan, keeping cloud log ordering, self-device log skipping, and
  cursor merging shared. The same plan also exposes `skippedSnapshots` and
  `skippedLogs` diagnostics with stable `skipReason` / `action` values for
  incremental snapshot skips, same-device logs, and
  already-current log skips. `applyAction` / `applyReason` identify whether
  native SQLite should run the remote apply plan or finish as no changes,
  keeping that decision out of platform count checks. `applySummary` adds
  imported collections, snapshot/log imports, skipped snapshot/log reasons and
  paths, remote record types, cursor collections, remote apply statement keys,
  and the final apply decision so platform diagnostics do not parse
  `remoteBatchJson` differently. Rust also owns
  `browserStorageSyncCloudCompactionPlanJson`, which consumes manifest,
  snapshot, and log objects and returns provider-executable `writeSnapshot`,
  `deleteLog`, and `writeManifest` operations when a contiguous log range
  crosses the retention threshold. Planned write operations include
  `writeReason` diagnostics, and planned delete operations include
  `deleteReason` diagnostics, so provider adapters do not infer write or
  log-removal causes. Provider adapters still execute object IO, but compaction
  thresholds, log-gap safety, retained-log windows, and manifest snapshot heads
  stay in Rust.
  Provider adapters pass those write/delete
  receipts back to `browserStorageSyncCloudCompactionResultPlanJson`, so Rust
  also owns whether compaction committed, whether manifest/snapshot/log state
  must be reread before retry, and whether failed old-log deletion is only a
  cleanup retry. Compaction result issues preserve planned `writeReason` /
  `deleteReason` diagnostics. Rust also owns
  `browserStorageSyncCloudLocalFolderCompactionRoundJson`, which runs one
  real-file local-folder compaction attempt by preflighting compatibility,
  reading manifest/snapshot/log files, writing snapshot/manifest objects, and
  deleting old logs only after the manifest commit succeeds. This keeps the
  compaction runner coarse-grained instead of exposing per-file provider FFI.
  Rust also owns the first cloud high-watermark/retention status
  planner through `browserStorageSyncCloudHighWatermarkPolicyJson` and
  `browserStorageSyncCloudHighWatermarkPlanJson`: provider adapters pass
  manifest, snapshot, log, and local cursor state in one batch, and Rust
  classifies collection currency, missing required logs, compaction pressure,
  and non-required old-log cleanup candidates with `deleteReason` diagnostics
  without moving provider IO or native SQLite apply execution into Rust. The follow-up
  `browserStorageSyncCloudCleanupResultPlanJson` classifies those candidate
  delete receipts, treats missing logs as idempotent cleanup success, requires
  retention-state rereads after conflicts, and returns only Rust-selected log
  cleanup retries for transient failures. Cleanup result issues preserve
  planned `deleteReason` diagnostics. Rust also owns
  `browserStorageSyncCloudLocalFolderCleanupRoundJson`, which runs one
  real-file local-folder high-watermark cleanup attempt by preflighting
  compatibility, reading manifest/snapshot/log files, deleting only Rust-planned
  non-required `candidateDeleteLog` objects, and classifying delete receipts
  through the same cleanup result handoff. Cleanup rounds include a Rust-owned
  `roundSummary` for read/delete counts, planned cleanup delete counts,
  retained-log and collection currency counts, cleanup completion/retry state,
  reasons, deleted paths, and issue diagnostics. This keeps cleanup runner
  sequencing coarse-grained instead of exposing per-file provider FFI. Rust also includes
  `BrowserStorageSyncMemoryCloudStore`,
  `BrowserStorageSyncLocalFolderCloudStore`, and planned write/delete executors
  for provider-operation tests. The local-folder store is a coarse file-backed
  fixture that maps cloud object paths under a local Actrium root and uses content
  hashes as optimistic revisions; it is not exposed as per-operation native FFI.
  That keeps revision conflict handling, content hash metadata, receipt/error
  mapping, real filesystem IO coverage, and compaction/push write execution
  semantics in Rust fixtures while concrete provider adapters still own SDK
  calls and OS scheduling. `browser_storage_sync_cloud_local_folder_cycle_round_json`
  exposes the local-folder store as one native-callable cycle round over a root
  directory, optional push request, cursors, and device identity, returning the
  same cycle/client-plan JSON without exposing list/read/write/delete as tiny FFI
  calls. Windows, the Apple shared package, and Android now consume that helper through
  `BrowserStorageSyncClient.RunLocalFolderSyncCycleAsync`,
  `BrowserStorageDatabase.ApplyCloudCycleClientPlan`,
  `AppleBrowserStorageSyncClient.runLocalFolderSyncCycle`, and
  `AppleBrowserStorageDatabase.applyCloudCycleClientPlan`, plus Android
  `BrowserStorageSyncClient.runLocalFolderSyncCycle` and
  `BrowserStorageDatabase.applyCloudCycleClientPlan`: Rust owns the local-folder
  cloud-file compatibility preflight, provider round, cloud-cycle status,
  schedule state, provider diagnostics, observability event, attempt plan, loop
  plan, and client plan. Those same clients now call
  `browserStorageSyncCloudLocalFolderCompactionRoundJson` and
  `browserStorageSyncCloudLocalFolderCleanupRoundJson` after native ack/apply
  with the latest local cursor state, returning the Rust maintenance outputs as
  `compactionRound` and `cleanupRound`, while native
  C#/Swift/Kotlin adapters own SQLite ack/apply execution and future
  UI/scheduling around provider status. The cloud side also
  has coarse `browser_storage_sync_cloud_run_push_round()`,
  `browser_storage_sync_cloud_run_membership_round()`,
  `browser_storage_sync_cloud_run_compaction_round()`,
  `browser_storage_sync_cloud_run_pull_round()`, and
  `browser_storage_sync_cloud_run_cycle_round()` Rust helpers that perform one
  injected-store round from provider reads through write/read execution and
	  result or remote-apply planning. Compaction rounds include a Rust-owned
	  `roundSummary` for read/write/delete counts, planned compaction write/delete
	  counts, compacted collection/log counts, manifest commit state, cleanup
	  completion/retry state, reasons, paths, and issue diagnostics. Cleanup rounds
	  include a Rust-owned `roundSummary` for read/delete counts, planned cleanup
	  delete counts, retained-log and collection currency counts, cleanup
	  completion/retry state, reasons, deleted paths, and issue diagnostics.
	  Recovery snapshot rounds include a Rust-owned `roundSummary` for read/write
	  counts, planned snapshot counts, migration/repair document counts, snapshot
	  mode/root, source and snapshot paths, successful snapshot paths, reasons, and
	  issue diagnostics. Push rounds include a Rust-owned
  `roundSummary` for read/write success and failure counts, planned write type
  counts, touched collections, read/write reasons, successful paths, failed
  provider error kinds, issue reasons, and ack/reread/retry action state. Pull
  rounds include a Rust-owned
  `roundSummary` for read success/failure counts, provider pagination
  continuations, imported/skipped cloud inputs, active collections, read
  reasons, failed provider error kinds, and the pull action/reason so native
  adapters do not rebuild those diagnostics separately.
  `browser_storage_sync_cloud_local_folder_membership_round_json()` exposes the
  local-folder store as one native-callable membership round over a root
  directory and join/revoke request, returning the same membership-round result
  JSON without exposing provider manifest/device reads and writes as tiny FFI
  calls. Membership rounds include a Rust-owned `roundSummary` for read/write
  counts, planned membership write counts, compatibility-blocked state,
  read/write reasons, successful paths, failed provider error kinds, issue
  reasons, and connect/revoke action state. Windows
  `BrowserStorageSyncClient.RunLocalFolderMembershipRoundAsync`,
  Apple `AppleBrowserStorageSyncClient.runLocalFolderMembershipRound`, and
  Android `BrowserStorageSyncClient.runLocalFolderMembershipRound` now consume
  that entry with the Rust local-folder capability preflight, so native
  connect/disconnect flows do not duplicate manifest/device membership
  sequencing. That Rust membership round now also executes the cloud-file
  compatibility preflight before manifest/device writes and returns the
  `compatibilityPreflight` payload with the membership result, preventing
  provider connection/disconnect flows from bypassing client-upgrade,
  migration-required, or cloud-repair gates. On blocked membership rounds, the
  Apple, Android, and Windows clients now also return the Rust
  `schemaRolloutPlan`, `schemaMigrationPlaybook`, and `recoverySnapshotRound`
  bundle so connect/revoke flows use the same recovery handoff as normal
  local-folder sync cycles. They also return the Rust membership `statusPolicy`
  from `browser_storage_sync_cloud_membership_status_policy_json`, so the
  `classification`, `completed`, `shouldReread`, `shouldRetry`, and
  `clientAction` fields are no longer rederived separately in Swift, Kotlin, or
  C#. The pull round executes Rust-planned
  `continueProviderLogListing` pages up to the shared safety cap and reports
  `paginationLimitExceeded` if provider pagination never settles, with
  `roundSummary` carrying the same pagination and remote-apply outcome in the
  pull-round result. The cycle round combines one optional push round with one
  pull round and owns the push-failure short circuit, keeping native code out
  of cloud document sequencing without taking over provider SDK calls, native
  SQLite apply execution, or scheduler retries. Cycle results carry
  `BrowserStorageSyncCloudCycleRoundSummary` for push/pull presence,
  client-step categories, failures, conflicts, imports/skips, affected
  collections, and diagnostic reasons. They also carry
  `BrowserStorageSyncCloudCycleClientPlan`, which flattens local ack, remote
  apply, reread, and retry steps;
  `browser_storage_sync_cloud_cycle_client_plan_json` exposes that plan from
  cycle round JSON so platform adapters do not reinterpret nested push/pull
  results independently. Local ack steps carry `stepReason` from push result
  `ackReason`, and remote apply steps carry `stepReason` from pull apply
  `applyReason`, keeping local-ack and remote-apply motivation in Rust.
  `browser_storage_sync_cloud_cycle_status_policy_json` also classifies the round
  for native schedulers and provider recovery UX with cloud/provider terms such
  as `cloudConflict`, `providerUnavailable`, `providerAuthRequired`,
  `providerQuotaExceeded`, and `cloudDocumentInvalid`, avoiding server-oriented
  labels in the cloud-drive product path.
  `browser_storage_sync_cloud_cycle_schedule_state_json` converts that policy and
  an observed timestamp into native scheduler state, including retry timing,
  provider-waiting flags, user-action flags, and pending reread state.
  `browser_storage_sync_cloud_provider_error_policy_json` and
  `browser_storage_sync_cloud_provider_error_classification_json` now give
  Apple, Android, Windows, and future providers one Rust-owned provider error
  classification contract. Native adapters pass coarse SDK/OpenDAL/native error
  batches or failed operation results; Rust normalizes status/code/kind/message
  data into shared provider error kinds, cloud-reread/retry/recovery/user-action
  classifications, scheduler/provider actions, retry timing, and telemetry
  level. This keeps conflict/reread, transient retry, auth/quota, unsupported,
  and provider-recovery priority in Rust without moving provider SDK calls,
  object IO, secure credentials, or UI wording out of native code.
  The classification entry also accepts nested round/client result shapes:
  adapters can pass a whole `cycleRound`, `pushRound`, `pullRound`,
  `membershipRound`, `compactionRound`, `cleanupRound`,
  `recoverySnapshotRound`, or `resultPlan` payload, and Rust extracts the
  failed provider read/write/delete operation results plus `resultPlan.issues`
  in one call. The boundary stays the same: Rust classifies provider failure
  semantics, while platform adapters still execute provider SDK/file IO and
  pass the resulting rounds back coarsely; the Apple, Android, and Windows
  local-folder clients feed their real rounds in as a
  `localFolderProviderRoundErrors` source for provider diagnostics.
  `browser_storage_sync_cloud_provider_diagnostics_json` now consumes that
  provider error classification as a first-class optional source, preserving the
  raw `providerErrorClassification` while mapping it into the shared provider
  diagnostics vocabulary for UI, scheduler, and observability handoff.
  `browser_storage_sync_cloud_membership_attempt_plan_json` now composes the
  membership round, membership status policy, and membership schedule state into
  one provider-neutral connect/revoke attempt plan. Apple, Android, and Windows
  local-folder membership clients return it as `attemptPlan` and feed it into
  the cloud observability event, keeping finish/reread/retry/recovery decisions
  in Rust while provider SDK calls, OS schedulers, and UI copy stay native.
  `browser_storage_sync_cloud_membership_loop_plan_json` consumes already
  produced membership rounds and adds the bounded connect/revoke loop decision:
  run another provider round, wait for retry backoff, stop for provider or user
  recovery, or stop after max attempts. It also returns
  `BrowserStorageSyncCloudMembershipLoopSummary` as membership `loopSummary`,
  grouping attempt counts, finish/reread/retry categories, provider-operation
  counts, recovery stops, retry backoff, final action/classification, and
  diagnostics. The platform clients return it as `loopPlan` and pass it into
  observability without moving cloud provider APIs into Rust.
  `browser_storage_sync_cloud_cycle_attempt_plan_json` composes the client plan,
  status policy, and schedule state into one provider-neutral next-attempt plan,
  including local ack, remote apply, reread, retry scheduling, stop/recovery, and
  provider/user action flags. That keeps multi-round cloud-cycle orchestration
  semantics in Rust while concrete provider SDK calls, OS schedulers, and native
  SQLite apply execution stay platform-owned.
  `browser_storage_sync_cloud_cycle_loop_plan_json` then consumes a batch of
  already-produced cycle rounds and adds the bounded loop decision above those
  attempts: continue, run another provider round, stop for native SQLite
  ack/remote-apply work, hand retry back to the scheduler, or stop for provider/
  user recovery. It also returns `BrowserStorageSyncCloudCycleLoopSummary` as
  `loopSummary`, grouping attempt counts, ack/apply/reread/retry categories,
  native apply barriers, retry backoff, final action/classification, and
  diagnostics so platform status surfaces do not rescan the attempt list.
  Apple, Android, and Windows storage
  adapters now execute those remote apply plans in native SQLite transactions
  while Rust still owns statement keys, parameter order, local-dirty overwrite
  guards, and result interpretation. The same adapters also execute Rust ack
  plans for `mark*Synced` and `upsertSyncCursor` operations. The ack SQL writes
  the accepted revision back with `sync_revision = MAX(sync_revision, ?)` while
  still guarding against stale local rows, so accepted revisions can become the
  comparable cross-device revision. The same adapters expose one higher-level
  cycle apply entry (`applySyncCycleResponse` / `ApplySyncCycleResponse`) that
  applies the Rust remote plan, requests Rust for the safe post-remote ack batch,
  then applies ack records/cursors and reports whether cursor ack was deferred.
  The adapters can export local dirty rows as Rust-normalized sync batches or
  push requests, keeping the batched sync-cycle
  shape intact instead of allowing each platform to invent its own wire payload,
  cursor update, accepted-revision writeback, or stale-revision behavior.
  Windows consumes this contract through `BrowserStorageDatabase.cs` with
  `Microsoft.Data.Sqlite` and migrates legacy JSON files into the shared SQLite
  schema. Windows, Apple, and Android tests also exercise real SQLite v1/v2/v3
  browser storage fixtures migrating to v6 sync columns, stable `tab_id`, bookmark
  owner fields, session owner/surface fields, cursor tables, dirty export, and tombstone writes. Android currently validates this
  through instrumentation APK compilation locally, with the same test body
  runnable on a connected device or emulator. Apple now has a shared package adapter,
  `packages/actrium-platform/apple/ActriumCore/Sources/ActriumCore/AppleBrowserStorageDatabase.swift`,
  which consumes the same Rust schema/SQL through `AppleSQLiteDatabase` for
  future macOS/iOS browser history, bookmark, and session persistence, including
  `browser_schema_meta` helpers used by platform one-time import flags. Android
  now has the matching app-local
  `android/android/app/src/main/java/io/actrium/app/data/database/BrowserStorageDatabase.kt`
  adapter, which consumes the same Rust schema/SQL through Android
  `SQLiteDatabase` without merging it into the existing Room active-list
  database. The existing macOS/iOS app `HistoryRepository` /
  `BookmarkRepository` remain separate app-history/app-bookmark repositories,
  while Apple product view models now merge browser-page rows read back from
  the shared browser database. Platform database handles, Room/SQLite
  execution, migration timing, and UI workflows remain platform-owned.
- macOS now has a first product call path into that browser storage contract:
  `macos/Actrium/Database/BrowserDataRepository.swift` writes
  successful http/https page visits, web bookmarks, and browser-page session
  snapshots through `AppleBrowserStorageDatabase`. `HistoryViewModel` and
  `BookmarkViewModel` also merge shared browser history/bookmark rows back into
  the existing macOS UI, deduping by browser address so old local rows remain
  the richer source for tags, notes, app IDs, and app history. On first launch
  after the migration, the view models also backfill existing local http/https
  history/bookmarks into `actro_browser.db` with idempotent upserts and
  platform-scoped meta flags. Full native tab state still serves system tabs,
  ACTR tabs, and tab groups. Do not store `actro://`, ACTR package, or system
  tabs in the browser page schema.
- iOS now has the matching first product call path through
  `ios/Actrium/Database/BrowserDataRepository.swift`. It records
  http/https `HistoryItem` visits from `HistoryViewModel`, writes browser-page
  session snapshots from `BrowserViewModel.saveTabs()`, projects the current
  native tab `isBookmarked` snapshot into browser bookmarks, and merges shared
  browser history rows back into `HistoryViewModel`. iOS also backfills existing
  local http/https history rows into `actro_browser.db` once, tracked through a
  platform-scoped meta flag. When `UserDefaults.persistedTabs` is absent or
  empty, iOS can restore http/https tabs from the shared browser session as a
  fallback and immediately persist that fallback back into the local tab store.
  Shared session `tab_id` is reused as the local `Tab.id` when it is
  UUID-compatible, with a deterministic local fallback for older rows.
  This is still a browser-page projection of the iOS tab host state, not a
  replacement for `UserDefaults.persistedTabs` or any richer RN-side bookmark
  model.
- Android now has the matching product projection in
  `android/android/app/src/main/java/io/actrium/app/data/repository/BrowserDataRepository.kt`.
  Phone `MainViewModel` and tablet `TabletViewModel` keep Room-backed
  `ActiveModel` as the complete native tab source, while the repository records
  http/https tab updates as browser history and snapshots the current
  `ActiveModel` browser-page subset into shared browser bookmarks/session. The
  Android `BrowserDataModule` native bridge exposes shared browser history and
  bookmarks to RN, and `packages/actrium/src/system-apps/HistoryApp.tsx`
  can load shared browser history through that bridge when native history props
  are absent. When the Room active-tab list is empty, Android phone and tablet
  startup can restore http/https tabs from the shared browser session and write
  the restored projection back to Room so the richer local source remains in
  charge after first recovery. Shared session `tab_id` maps to `ActiveModel.id`.
  The current Android integration is still a
  browser-page projection; fully wiring Android navigation to a first-class
  history surface and deciding how browser bookmarks should coexist with ACTR
  app bookmarks are follow-ups.
- Windows browser history and bookmark UI workflow remains local. Recording
  successful non-private external navigations, rendering generated
  `actro://history` / `actro://bookmarks` documents, filtering/removing entries
  from those local pages, host-bridge dispatch, clearing local history, honoring
  private-window rules, and active-tab bookmark menu state are WebView2 shell
  behavior. Rust owns the durable snapshot schema, not the browser event timing
  or generated page UX.
- Windows browser chrome and navigation state live in
  `windows/src/BrowserToolbar.cs` and `MainForm.cs`. Address
  focus shortcuts, address normalization, address bar search fallback,
  back/forward/reload/home commands, title updates, link-hover status text, and
  WebView2 process-failure status are platform workflow, not Rust-core work.
- Windows address bar search fallback lives in
  `windows/src/BrowserAddressInputPolicy.cs`. It is intentionally
  toolbar-input-only: startup arguments, host-bridge navigation, session
  restore, and page-initiated navigation keep using strict browser-safe URL
  flows. The engine catalog, engine-id normalization, and search URL construction
  are shared Rust helpers; deciding when typed text becomes a search request is
  product shell behavior.
- Windows search engine settings live in
  `windows/src/BrowserSearchEnginePolicy.cs`,
  `windows/src/BrowserSearchSettingsStore.cs`, and `MainForm.cs`.
  `BrowserSearchEnginePolicy.cs` adapts ActriumCore's shared browser search
  contract to Windows with a local fallback when the native DLL is absent.
  Engine menu routing and selected-engine persistence stay Windows-owned until a
  product-wide preferences schema exists.
- Windows startup restore settings live in
  `windows/src/BrowserStartupSettingsStore.cs` and `MainForm.cs`.
  The setting controls whether a normal launch consumes the local session
  snapshot or opens home. It is shell startup UX and should stay Windows-owned
  until there is a real cross-platform preferences contract.
- Windows navigation failure pages live in
  `windows/src/BrowserNavigationErrorPolicy.cs` and `MainForm.cs`.
  WebView2 error events, generated local error documents, and reload retry
  routing are browser-shell UX. Keep them local unless a future shared
  browser-domain contract needs durable non-UI error classification.
- Windows native home-page document generation lives in
  `windows/src/BrowserHomeDocumentPolicy.cs` and `MainForm.cs`.
  Generated `actro://home` HTML, safe status text rendering, and the decision to
  show the document are WebView2 shell UX. Rust only supplies stable core
  metadata through the local `ActriumCore.cs` adapter.
- Windows WebView2 process-failure handling lives in
  `windows/src/BrowserProcessFailurePolicy.cs` and `MainForm.cs`.
  Status text, renderer/frame reload classification, and tab recovery attempts
  are browser-engine lifecycle concerns. Keep them Windows-owned; Rust has no
  useful role unless a future cross-platform crash-report schema appears.
- Windows desktop menu commands live in
  `windows/src/BrowserMenuCommand.cs`,
  `windows/src/BrowserMenuModel.cs` and
  `windows/src/BrowserAppMenu.cs`, and execute through the single
  `MainForm.cs` command router. Tab/window commands, local file opening, page
  address copy/default-browser opening, save-as-PDF, print, navigation
  accelerators, stop/reload, history, address focus, clear browsing data, and
  developer tools are native shell orchestration; moving them to Rust would
  split one UI workflow across languages without creating a durable shared
  rule.
- Windows current-page address actions live in
  `windows/src/BrowserPageActionPolicy.cs` and `MainForm.cs`.
  Clipboard writes, default-browser `ProcessStartInfo` handoff, and deciding
  which addresses are shell-safe are local desktop shell behavior. Keep them out
  of Rust unless a future shared external-open policy needs durable
  cross-platform semantics.
- Windows downloads live in `windows/src/BrowserDownloadManager.cs`
  and `windows/src/BrowserDownloadPolicy.cs`. WebView2
  `DownloadStarting`, save dialogs, destination paths, open-downloads-folder
  shell handoff, last-completed-download open/reveal actions, and progress
  status are platform shell behavior. The file-name and default-folder policy is
  Windows-local because invalid path characters, user profile folders, Explorer
  reveal arguments, and dialog expectations are OS-specific; it should stay out
  of Rust until a broader cross-platform downloads model appears.
- Windows save-as-PDF lives in `MainForm.cs` and
  `windows/src/BrowserPdfPolicy.cs`. WebView2 `PrintToPdfAsync`,
  save-file dialog ownership, PDF destination paths, and status text are desktop
  browser-shell workflow. Keep this local unless a future cross-platform export
  contract needs durable non-UI rules.
- Windows page find lives in `windows/src/BrowserFindBar.cs`,
  `windows/src/BrowserFindPolicy.cs`, and `MainForm.cs`. The native
  find bar, `Ctrl+F` plus repeated-find keyboard routing, `window.find` script
  execution, and match status are WebView shell workflow. Keep this local unless
  a future shared browser engine introduces a durable cross-platform find
  contract.
- Windows browser permissions live in
  `windows/src/BrowserPermissionManager.cs` and
  `windows/src/BrowserPermissionPolicy.cs`. WebView2
  `PermissionRequested`, native confirmation dialogs, and Allow/Deny state
  assignment are platform shell UX. Rust should not own these prompts unless a
  future product-wide permission policy needs a shared non-UI schema.
- Windows page zoom lives in `windows/src/BrowserZoomPolicy.cs` and
  `MainForm.cs`. WebView2 `ZoomFactor`, Windows keyboard accelerators, menu
  availability, and status text are desktop shell workflow. Keep this local
  unless a future shared browser-domain model needs cross-platform zoom state.
- Windows new-window handling lives in `MainForm.cs`. WebView2
  `NewWindowRequested`, target-blank/window-open routing, and whether a request
  becomes a tab are browser shell policy. It should stay platform-owned; Rust may
  provide durable URL or domain rules later, but not WebView lifecycle routing.
- Windows full-screen handling lives in `MainForm.cs`. WebView2
  `ContainsFullScreenElementChanged`, F11 browser full-screen, WinForms
  border/window-state changes, and browser chrome hide/restore are native shell
  orchestration. Rust has no useful ownership there.
- Windows URL normalization and navigation scheme allowlisting live in
  `windows/src/NavigationPolicy.cs`. Extracting this from
  `MainForm` makes the Windows shell rule testable and reusable by startup URL,
  toolbar, local file opening, host bridge, and future menu commands, but it
  remains a Windows browser policy rather than a Rust contract.
- Windows page-initiated navigation interception also stays in `MainForm.cs`.
  WebView2 `NavigationStarting` is canceled when the target is not an
  already-absolute browser-safe URL. This is shell security policy; Rust can own
  durable URL parsing only if a cross-platform browser-domain contract emerges.
- Windows external protocol handoff lives in
  `windows/src/BrowserExternalProtocolManager.cs` and
  `windows/src/BrowserExternalProtocolPolicy.cs`. Prompting and
  opening `mailto:`, `tel:`, or other non-browser schemes through Windows default
  handlers are OS integration concerns; unsafe/browser-internal schemes remain
  local shell policy.
- Windows tab lifecycle lives in `windows/src/BrowserTab.cs`,
  `windows/src/BrowserTabSidebar.cs`, and `MainForm.cs`. Per-tab
  WebView2 ownership, active-tab switching, tab cycling/direct-selection
  shortcuts, duplicate-tab creation, `Ctrl+Shift+T` closed-tab reopening,
  close-last-tab behavior, and tab title/loading rendering are platform workflow
  and should stay out of Rust. The page bridge methods `openTab`, `closeTab`,
  `selectTab`, `navigate`,
  `getTabs`, and `getActiveTab` are only a Windows shell API surface over that
  same local workflow.
- Windows multi-window lifecycle lives in
  `windows/src/BrowserApplicationContext.cs` and `MainForm.cs`.
  `Ctrl+N`/File > New Window create a fresh secondary shell window,
  `Ctrl+Shift+N`/File > New Private Window create a secondary window with an
  isolated WebView2 profile, `--private`/`--private-window`/`--incognito` start
  the first window in that private mode, repeated `--url` values and positional
  URL/file arguments open initial tabs, the first normal window owns session
  restore/persist plus primary-window geometry restore/persist, and the process
  exits when the last window closes. This is native app/window and profile
  orchestration and should stay out of Rust.
- Windows launch-time tab restore lives in
  `windows/src/BrowserSessionStore.cs`, with the local startup toggle
  in `windows/src/BrowserStartupSettingsStore.cs`. These are shell
  startup state, not the shared history/bookmark/session database contract. Keep
  them Windows-owned until a durable cross-platform persistence surface appears.
- Windows primary-window geometry restore lives in
  `windows/src/BrowserWindowStateStore.cs` and `MainForm.cs`. Bounds,
  maximized state, visible-screen adjustment, and deciding which windows write
  the snapshot are desktop shell behavior. Keep them local; moving this to Rust
  would split a WinForms window-placement workflow without creating a stable
  shared algorithm.
- `npm run check:rust-boundary` protects the RN/UI boundary files that already
  moved behind local adapters. It is intentionally narrow; it should not become a
  blanket ban on `ActriumCore` imports at real parser, persistence, model, ACTR,
  ASR, TTS, Email, or package-loading boundaries.
- Swift adapters should return platform-local DTOs when their callers do not need
  Rust-generated types. Otherwise the generated type leaks across the boundary
  and the caller still has to import `ActriumCore` just to read fields.
- Existing Rust-owned helpers should continue to be judged by whether they reduce
  drift across platform protocols or simplify a durable non-UI rule. If a helper
  only turns a small local UI decision into a platform -> Rust -> platform hop,
  cap it at constants or keep it platform-local.

## Clearly Rust-Owned

1. **ACTR package and manifest core**
   - Manifest schema, contract gating, `systemAppType`, `appId` extraction,
     path normalization, asset classification, integrity planning, and package
     metadata normalization.
   - Initial home: `crates/actrium-core/src/actr.rs`.
   - Native UniFFI exposes integrity planning and normalization helpers so
     Swift/Kotlin/Windows adapters can share manifest integrity rules while
     keeping file IO/hash execution platform-owned.
   - Remaining platform compatibility code:
     - Legacy Apple `layout.leftPanel/rightPanel` fallback in
       `BrowserViewModel`.
     - RN archive download/unzip, entry loading, and asset copy/rewrite in
       `packages/actrium/src/runtime-host/utils`.

2. **Cross-platform browser/client domain models**
   - `TabRecord`, `SystemAppType`, `TabStatus`, history, bookmarks, tags, and
     tab groups.
   - Initial home: `crates/actrium-core/src/domain.rs`.
   - Platform shells may map these into Swift/Kotlin/UI-native view models, but
     they should not invent incompatible field names or status values.

3. **Native bridge payload contracts**
   - Stable wire names, request/response field shapes, tab summaries, and simple
     tab events when they prevent cross-platform protocol drift.
   - Initial home: `crates/actrium-core/src/bridge.rs`.
   - Swift/Kotlin/Windows bridge modules should become thin adapters around this
     contract only for durable protocol data. Platform route policy, UI
     navigation, event delivery mechanics, and OS/RN bridge lifecycles stay
     platform-owned.
   - Email native module name, compose method name, system mail URI scheme, and
     promise error codes are Rust-owned bridge protocol values.
   - UserInfo native module name, RN method names, user-info event names, and
     user-info/login-status payload keys are Rust-owned bridge protocol values.

4. **Auth environment and login-routing rules**
   - Environment names, host suffix mapping, iOS app-URL path requirements, and
     per-environment user-info storage suffixes are Rust-owned.
   - Account/resource service base URLs, auth API path constants,
     access-control path construction, client-app path construction, and iOS
     app-URL to access-control URL construction are Rust-owned protocol rules.
     Resource upload token path naming is also Rust-owned.
   - Access-control response semantics are Rust-owned: the authorized response
     code is `0`; successful non-zero or missing-code responses normalize to
     `not_authorized`; failed transport/API responses normalize to `unknown`.
   - Shared auth/account API response envelope rules are Rust-owned: business
     success codes are `0` and `200`, and client-app not-found detection uses
     code `10010` plus the legacy English/Chinese message fragments.
   - Initial home: `crates/actrium-core/src/auth.rs`.
   - iOS `LoginManager` and `UserInfoModule` consume the UniFFI helpers. The RN
     runtime consumes `AUTH_ENVIRONMENT_CONTRACT` and `AUTH_ENDPOINT_CONTRACT`
     for account/resource endpoints and auth path builders, plus
     `AUTH_ACCESS_CONTROL_RESPONSE_CONTRACT` for authorization status checks.
   - Vendor SDK calls, login sessions, and OS permission/UI flows remain
     platform-owned.

5. **ACTR client config generation**
   - `Actr.toml` and `Actr.lock.toml` template defaults, service URL
     normalization, client `actr_type`, dependency metadata, and lock-file
     fingerprints should be Rust-owned.
   - Initial home: `crates/actrium-core/src/actr_config.rs`.
   - The RN runtime consumes Rust-generated TypeScript helpers so it can keep
     writing config files without synchronous native FFI. Swift/Kotlin/Windows
     native adapters can expose direct UniFFI wrappers once they need native-side
     config generation.

6. **Vault cryptography and migration core**
   - Key derivation, encryption/decryption, vault item serialization, schema
     migration, and password verification should become Rust-owned before adding
     mobile/Windows vaults.
   - Initial home: `crates/actrium-vault`.
   - Current core surface preserves the macOS format: PBKDF2-HMAC-SHA256 with
     100,000 iterations, 32-byte salt, 32-byte key, AES-256-GCM, and base64
     `nonce(12) + ciphertext + tag(16)` payloads.
   - Keychain, biometric prompts, Android Keystore, and Windows Credential
     Locker remain platform-owned.

9. **Shared persistence schema and migrations**
   - History/bookmarks/tags/tab session schema and migration rules should move
     under Rust once the first domain contracts are consumed by platforms.
   - The platform layer may keep database handles if needed, but schema/version
     decisions should be single-source.
   - Initial home: `crates/actrium-core/src/persistence.rs`.
   - RN runtime SQLite schema/version/migration SQL is now Rust-owned in
     `crates/actrium-core/src/runtime_persistence.rs` and projected into the
     generated TypeScript runtime contract.
   - RN runtime repository SQL, app status constants, message type constants, and
     message progress values are also Rust-owned in `runtime_persistence.rs`.
     Platform code still owns database lifecycle and error behavior. Apple
     targets share sqlite3 handle/statement glue in the `ActriumCore` Swift
     package; Rust can own selected typed parameter vectors when their ordering
     and normalization are part of the shared persistence contract.

10. **Speech contracts**
   - Language-code normalization, bundled model asset/storage path planning,
     ready-marker naming, Vosk result JSON text extraction, and deterministic
     final-result text post-processing should be Rust-owned.
   - Initial home: `crates/actrium-core/src/speech.rs`.
   - Android ASR native module/event names, result payload keys, and promise
     error codes are also Rust-owned because they are a Kotlin/RN bridge
     protocol rather than UI behavior.
   - TTS native module/event names, speech option keys, and event payload keys
     are Rust-owned for the same reason: they are RN/native bridge protocol, not
     platform speech synthesis behavior.
   - Platform shells still own Vosk/OS speech engine lifetimes, model file IO,
     audio buffering, microphone permissions, TTS engine/audio-session control,
     and segmentation adapters such as Android HanLP.

## Platform-Owned

- SwiftUI/UIKit/Android/Windows UI and layout.
- RN root view creation, WebView creation, XPC process orchestration, and native
  view managers.
- Platform shell routing such as `openTab` display policy, phone/tablet
  navigation, focus/fullscreen transitions, and NotificationCenter/EventBus
  delivery. Rust may expose stable keys, payload constructors, or lightweight
  classifiers, but it should not own view-model navigation.
- Vendor login SDK calls and OS permission dialogs.
- Keychain/Keystore/Credential Locker access and biometric UX.
- Platform-specific shell integration such as windows, menus, share sheets,
  notifications, and app lifecycle.
- ASR engine bindings, model asset copying, audio capture/buffering, and
  language-specific tokenizer/segmenter adapters.

## Migration Order

1. Build `actrium-core` as the canonical Rust contract crate.
2. Bind `actrium-core` through UniFFI for native shells; keep JSON/TypeScript
   adapters as generated consumers of the same Rust contract rather than a
   separate source of truth.
3. Replace TS/Swift/Kotlin manifest and bridge validators with calls into
   `actrium-core`.
4. Move shared persistence and vault logic once domain types are consumed.
5. Keep vault cryptography in its own Rust crate because it has a
   security-sensitive dependency and release surface.

## Current Actrium Core FFI Surface

`actrium-core` now exports a small UniFFI API that is intentionally focused on
high-confidence shared logic:

- `parse_actr_manifest` and `validate_actr_manifest`
- `evaluate_contract`
- `extract_app_id_from_url`
- `normalize_path` and `relative_path_from`
- `is_asset_path`
- `normalize_integrity_algorithm`, `normalize_integrity_path`, and
  `normalize_sha256_hex`
- `create_actr_integrity_plan`
- `parse_system_app_type` and `system_app_info`
- `parse_app_source_type`, `app_source_type_info`,
  `app_source_type_downloaded`, and `app_source_type_local_dev`
- `supported_contract_name` and `supported_contract_version`
- `core_schema_version`, `core_schema_statements`, and
  `core_schema_table_names`
- `default_tags`, `default_system_tags`, and `default_tag_colors`
- `core_history_repository_sql` and `core_tag_repository_sql`
- `core_history_item_from_row`, `core_history_search_title_param`, and
  `core_tag_from_row`
- `core_history_insert_visit_params`, `core_tag_insert_params`, and
  `core_tag_update_params`
- `core_bookmark_repository_sql` and `core_app_repository_sql`
- `core_bookmark_from_row`, `core_bookmark_search_params`,
  `core_bookmark_insert_params`, and `core_bookmark_update_params`
- `core_installed_app_from_row`, `core_app_search_params`,
  `core_installed_app_insert_params`, and `core_installed_app_update_params`
- `CoreSqlValueInfo`, the typed SQL bind value contract used by native
  repository adapters
- `runtime_app_status_alive`, `runtime_app_status_dead`,
  `default_runtime_app_status`, and `normalize_runtime_app_status`
- `active_tab_from_row`, `active_tab_to_row`, and
  `active_tab_rows_from_tabs`
- `runtime_tab_update_from_fields` and
  `active_tab_apply_runtime_update`
- `browser_data_contract`, browser snapshot normalization helpers,
  `browser_storage_schema`, `browser_storage_schema_statements`,
  `browser_storage_table_names`, and `browser_storage_repository_sql`
- `browser_storage_schema_migration_plan_json`, the ordered incremental
  migration plan (legacy `ALTER TABLE … ADD COLUMN` additions plus idempotent
  index/cursor-table/version-stamp statements) that platform adapters replay
  verbatim; native code keeps only `PRAGMA table_info` introspection and
  statement execution
- `runtime_bridge_*_key`, `runtime_tab_unread_from_fields`,
  `runtime_tab_bookmark_status_from_fields`, `runtime_tab_bookmark_event`,
  `runtime_tab_fullscreen_event`, `runtime_extension_panel_visible_event`, and
  `runtime_container_size_change_event`
- `runtime_*_event_name` helpers for shared native-to-RN events: tab fullscreen
  enter/exit, tab bookmark changes, extension-panel visibility, container-size
  changes, and global-search input changes
- `runtime_auth_context_*_key`, the canonical RN/native auth context field
  names for `clientId`, `appId`, `tabId`, and `appUrl`
- `auth_environment_*` helpers, covering canonical `dev`/`beta`/`prod`
  environment names, URL host-suffix mapping, iOS app-URL path validation, and
  per-environment user-info storage suffixes
- `auth_account_base_url`, `auth_resource_uploader_base_url`,
  `auth_access_control_path`, `auth_client_app_path`, and
  `auth_access_control_url_from_app_url`, covering shared auth/resource service
  base URLs plus auth-related protocol path construction
- `auth_is_authorized_code` and `auth_access_control_status`, covering shared
  access-control response normalization
- `auth_api_response_success`, `auth_is_client_app_not_found_code`, and
  `auth_is_client_app_not_found_message`, covering shared auth/account API
  envelope and client-app not-found semantics
- `runtime_login_result_*` key/event helpers plus `runtime_login_type_*`,
  the canonical RN/native login result event contract
- `runtime_tab_props_*_key` and `runtime_root_props_*_key`, the canonical
  React Native tab/root props field names used by native adapters
- `runtime_tab_from_active_tab`, `runtime_root_props_from_active_tabs`,
  `runtime_root_props_from_runtime_tabs`, and `runtime_system_root_props`
- ACTR client config constants and helpers in `actr_config`, covering
  `Actr.toml` / `Actr.lock.toml` template values plus signaling/STUN/TURN URL
  normalization
- `actr_bridge_contract`, the canonical ActrBridge native module name,
  stream-event name, payload keys, stream type values, stream metadata terminal
  markers, stream status literals, attachment upload/result metadata, attachment
  chunk/checksum/status/type values, prompt/attachment/local-service RPC route
  keys, platform-specific RPC timeout values, and promise error codes used by
  iOS native, Android native, and RN TypeScript bridge adapters
- `parse_actr_actor_type`, the canonical `publisher/program@version` Actr actor type
  parser used by iOS and Android before constructing their platform SDK
  `ActrType` values
- `email_bridge_contract` / `EMAIL_BRIDGE_CONTRACT` for the Email native module
  name, compose method name, mail URI scheme, and promise error codes used by
  Android native and RN TypeScript bridge adapters
- `user_info_bridge_contract` / `USER_INFO_BRIDGE_CONTRACT` for the UserInfo
  native module name, RN method names, user-info event names, and user-info /
  login-status payload keys used by iOS native, Android native, and RN
  TypeScript bridge adapters
- `asr_model_language_dir`, `AsrModelPlanInfo`, `asr_model_plan`, Vosk JSON text
  extraction helpers, `AsrSegmentInfo`, and `post_process_zh_asr_segments` for
  ASR model planning plus segmented Chinese final-result punctuation
- `asr_bridge_contract` / `ASR_BRIDGE_CONTRACT` for Android ASR native module
  name, result/error event names, result payload keys, and promise error codes
- `tts_bridge_contract` / `TTS_BRIDGE_CONTRACT` for TTS native module name,
  start/pause/resume/finish/error event names, speech option keys, and
  finish/error payload keys

This is enough for macOS, iOS, Android, and Windows shells to stop duplicating
manifest validation, native bridge/auth contract constants, and database
schema creation rules before larger repository/vault migrations begin.

## Current TypeScript Runtime Contract

The React Native runtime consumes Rust-owned TypeScript contracts through one
stable local adapter:

- `packages/actrium/src/runtime-host/coreContract.ts`

That adapter re-exports the generated source artifact:

- `packages/actrium/src/runtime-host/generated/actriumCoreContract.ts`

The generator lives in `crates/actrium-core/src/bin/generate-ts-contracts.rs` and
uses `actrium-core` constants/types for:

- supported ACTR contract name/version and gate messages
- `SystemAppType` values and built-in app metadata
- ACTR manifest shape/defaults
- app id extraction from `/with/{appId}` URLs
- path normalization, relative path extraction, and asset extension checks
- manifest integrity planning: algorithm normalization, safe file paths, and
  SHA-256 hex normalization
- RN runtime SQLite schema version, base schema statements, table/column names,
  migration SQL, repository query SQL, app status values, message type values,
  and message progress values
- RN/native login result event name, provider values, and payload keys consumed
  by native producers and the React Native login bridge
- `USER_INFO_BRIDGE_CONTRACT` for UserInfo native module/method/event names and
  user-info/login-status payload keys
- `AUTH_ENVIRONMENT_CONTRACT` for canonical environment names, host suffixes,
  and user-info storage suffixes consumed by RN auth and resource upload
  endpoints
- `AUTH_ENDPOINT_CONTRACT` and generated helper functions for account/resource
  service base URLs, auth API paths, access-control paths, session-check paths,
  client-app paths, and resource upload token paths consumed by RN auth/app and
  upload APIs
- `AUTH_ACCESS_CONTROL_RESPONSE_CONTRACT`, `authIsAuthorizedCode`, and
  `authAccessControlStatus` for RN auth status normalization
- `AUTH_API_RESPONSE_CONTRACT`, `authApiResponseSuccess`, and
  `authIsClientAppNotFound*` helpers for RN auth/account API response envelope
  and client-app not-found detection
- ACTR client config generation: `Actr.toml`, `Actr.lock.toml`, client
  `actr_type`, dependency metadata, signaling/STUN/TURN URL normalization, and
  lock-file fingerprint/timestamps
- shared native-to-RN event names consumed by iOS/Android emitters and RN
  event listeners for tab fullscreen, tab bookmark, extension-panel,
  container-size, and global-search input events
- `ACTR_BRIDGE_CONTRACT` for the ActrBridge native module name, stream-event
  name, payload keys, stream type values, stream metadata terminal markers,
  stream status literals, attachment upload/result metadata, attachment
  chunk/checksum/status/type values, prompt/attachment/local-service RPC route
  keys, platform-specific RPC timeout values, and promise error codes
- `ASR_BRIDGE_CONTRACT` and `TTS_BRIDGE_CONTRACT` for speech-related RN/native
  bridge names, events, payload keys, and option keys

Handwritten runtime files keep their public exports stable, but delegate to the
generated module:

- `constants.ts` re-exports `SUPPORTED_CONTRACT`
- `types.ts` aliases generated `SystemAppType`
- `utils/index.ts` delegates `evaluateTabGate` and `extractAppIdFromUrl`
- `AppSessionService.ts` passes the server-provided `actr_hash_code` into
  `ActrPackageLoader.ts`; the loader downloads the `.actr` once, verifies that same
  temporary zip with `RNFS.hash`, then extracts the verified archive. This keeps
  network IO/hash execution inside the RN loader without adding Rust bridge
  hops or JS byte-array hashing.
- `ActrPackageLoader.ts` also delegates manifest parsing, path/asset helpers, and
  optional manifest-level integrity verification planning to generated
  Rust-owned rules.
- `Sqlite.init.ts` delegates runtime DB schema/version/migration SQL to
  generated Rust-owned constants while keeping platform SQLite execution and
  legacy-version detection local
- `AppRepository.ts`, `AuthRepository.ts`, `MessageRepository.ts`, and
  `BookmarkRepository.ts` delegate SQL strings, sort orders, and runtime state
  constants to the generated Rust-owned contract while keeping row mapping and
  repository execution local
- `LoginBridge.ts` and `useLogin.ts` delegate the login-result event name,
  provider values, and payload keys to generated Rust-owned constants while
  keeping RN event subscription and auth API calls local
- `api/app.ts`, `api/auth.ts`, and `ImageUploadService.ts` delegate
  environment names, service base URLs, and auth/client-app path construction to
  generated auth contracts while keeping HTTP calls, current-env selection, and
  COS upload orchestration local
- `api/https.ts`, `AppSessionService.ts`, and `useLoadApp.tsx` delegate
  success-code and client-app not-found response semantics to generated auth API
  response helpers while keeping transport, UI fallback policy, and alert text
  local
- `AppSessionService.ts`, `useAuth.ts`, and `useAuthorize.ts` delegate
  access-control response status normalization to the generated auth response
  helpers while keeping HTTP execution, cache updates, and token exchange local
- `useAuthTaskStore.ts` sources cached authorization status values from
  `AUTH_ACCESS_CONTROL_RESPONSE_CONTRACT` instead of owning separate status
  literals
- `TabEvents.ts`, `ExtensionEvents.ts`, `ContainerEvents.ts`, and
  `GlobalSearchEvents.ts` delegate shared native event names to generated
  Rust-owned constants while keeping RN event subscription local
- `ActrNativeBridge.ts`, `ActrEvents.ts`, and `ActrStreamRouter.ts` delegate the
  ActrBridge native module lookup, stream-event name, selected payload keys,
  stream type values, stream metadata terminal markers, attachment result
  timeout/status values, prompt RPC route/timeout values, and status/error
  literals to generated Rust-owned constants while keeping RN NativeModules
  invocation, event subscription, and stream routing local
- `ActrConnection.ts` and `ActrConnection.type.ts` delegate attachment chunk
  size, checksum algorithm, attachment type/status values, and stream terminal
  metadata to generated Rust-owned constants while keeping RN file IO and upload
  orchestration local
- `ActrConfigUtils.ts` re-exports Rust-generated config helpers while keeping
  RN file creation in `useActrConnection`
- `TTSBridge.ts` and `TTSEvents.ts` delegate TTS native module lookup, event
  names, speech option keys, and event payload keys to generated Rust-owned
  constants while keeping iOS-only feature gating and RN subscriptions local

This keeps the RN runtime usable without synchronous native FFI while making
Rust the source of truth for non-UI ACTR, auth-environment, bridge, and config
contract rules.

## Current Apple Rust Modules

macOS and iOS consume UniFFI-generated Swift through shared local Swift
Packages instead of copying generated Swift into app targets:

- `packages/actrium-platform/apple/ActriumCore`
- `packages/actrium-platform/apple/ActriumVault`

This keeps each UniFFI module's generated helper symbols isolated.
Each crate's `build-xcframework.sh` produces a multi-platform XCFramework with
macOS, iOS device, and iOS simulator slices, then syncs the generated Swift
binding into the matching package.

The macOS and iOS `DatabaseManager` implementations keep database path,
initialization, schema setup order, and platform migrations in the app targets,
but now delegate low-level SQLite connection/query glue to
`AppleSQLiteDatabase` in the `ActriumCore` Swift package. They still source schema
statements and default tag seed values from Rust-owned `ActriumCore` contracts.
This makes the database contract a Rust-owned cross-platform primitive while
keeping Apple sqlite3 binding code shared without moving app storage lifecycle
into Rust.

The macOS and iOS `Tag` models now source protected system-tag seeds and default
tag color palettes from `ActriumCore`. macOS `AppSourceType` also parses persisted
raw values and displays source labels through `ActriumCore`, while keeping a local
Swift enum shape for Codable and database compatibility.

The macOS and iOS `HistoryRepository` and `TagRepository` implementations now
also source their repository SQL from `ActriumCore` through
`CoreHistoryRepositorySqlInfo` and `CoreTagRepositorySqlInfo`. Rust owns the
query shape, table/column names, ordering, and insert/update/delete statement
text for those repositories. Their history item and tag row-shape mapping also
flows through `ActriumCore`, including history title-search parameter
normalization and tag `is_system` integer-to-boolean normalization. Apple Swift
still owns SQLite handles and statement execution through the shared
`AppleSQLiteDatabase` helper, while app repositories own UUID/URL/Date
conversion, transaction boundaries, and error behavior.

The iOS and macOS `HistoryRepository` / `TagRepository` files are intentionally
kept app-local while they remain byte-for-byte identical. `npm run
check:rust-boundary` parity-locks those files so contract-consuming repository
logic cannot silently drift. Extract them into a shared Apple source/package only
when that removes more complexity than the extra module/model boundary adds.

Apple history/tag writes now also source their ordered bind-parameter vectors
from `ActriumCore` through `CoreSqlValueInfo`. Rust owns the visit/tag insert and
tag update parameter order and primitive value normalization; `AppleSQLiteDatabase`
performs the final sqlite3 binding calls for both app targets.

The macOS-only `BookmarkRepository` and `AppRepository` now follow the same
boundary through `CoreBookmarkRepositorySqlInfo` and
`CoreAppRepositorySqlInfo`. Their bookmark and installed-app row-shape mapping
also flows through `ActriumCore`, including bookmark/app search parameter
normalization, installed-app required-field validation, source type
normalization, and `is_pinned` integer-to-boolean normalization. macOS keeps tag
lookups, duplicate handling orchestration, SQLite execution, UUID/URL/Date
conversion, and error behavior local.

macOS bookmark/app writes now also source their ordered bind-parameter vectors
from `ActriumCore`. Rust owns bookmark insert/update parameter order, installed
app insert/update parameter order, boolean normalization, optional field
normalization, and app source type validation. macOS keeps duplicate handling,
tag relation writes, SQLite execution, UUID/URL/Date conversion, and error
behavior local.

The macOS and iOS `BrowserViewModel.readManifestJSON` implementations now use
`ActriumCore.parseActrPackageManifest` for the main ACTR manifest path, including
contract gating, app id normalization, permissions, visibility defaults, system
app type parsing, and theme/background color normalization. Swift still reads
legacy `layout.leftPanel/rightPanel` script names because that fallback is a
loader compatibility detail for older packages.

iOS links the same `ActriumCore` and `ActriumVault` Swift Package products as
macOS. Its schema/default-tag and ACTR manifest call sites now use `ActriumCore`;
vault call sites remain pending until iOS exposes the vault feature.

`ActriumCore` also exposes `createPackageIntegrityPlan`,
`normalizeIntegrityAlgorithm`, `normalizeIntegrityPath`, and
`normalizeSha256Hex` to Apple clients. The shared
`AppleActrPackageManifestReader` helper in the `ActriumCore` Swift package uses
that Rust-owned integrity plan, keeps Apple file IO and CryptoKit SHA-256
hashing in one place, and rejects missing, outside-root, or mismatched files.
ACTR packages without manifest-level integrity metadata remain a no-op, and the
legacy layout fallback described above still applies.

iOS and macOS now keep ACTR manifest parsing behind target-local
`ActrPackageManifestReader` adapters that delegate to the shared
`AppleActrPackageManifestReader`. The shared helper owns manifest schema,
contract, legacy fallback, and integrity logic; the app-local adapters only map
to platform DTOs such as macOS `SystemTabType`. `BrowserViewModel` receives
platform-local `ActrPackageManifestInfo` and keeps package loading decisions in
Swift.

iOS runtime bridge call sites have started consuming the same `ActriumCore`
bridge contract. `TabModule`, `TabEventEmitter`, and the iOS
`BrowserViewModel` now use Rust-owned canonical key helpers and simple tab
payload constructors for tab ids, title/icon/app URL/status fields, unread and
bookmark status updates, bookmark events, and fullscreen tab events. iOS UI
publishers for bookmark/fullscreen notifications now use the same Rust-backed
payload helpers through the local `Tab` adapter. iOS open/select/close/system
tab NotificationCenter receivers and root-prop system-tab writers now also use
Rust-owned canonical `systemAppType` and `tabId` key helpers through
target-local Swift contracts such as `RuntimeBridgeContract`,
`RuntimeTabPropsContract`, and `RuntimeRootPropsContract`, rather than
scattering generated FFI helper calls through UI views. iOS still owns
NotificationCenter delivery, React Native promise/event plumbing, view model
mutation, and the current iPad/iPhone `openTab` routing behavior. iOS now uses
a local `RuntimeOpenTabContract` Swift adapter to call `runtimeOpenTabPlan` for
canonical open-tab classification and field filtering, but the adapter is only
the contract boundary; Swift still decides iPhone fullscreen, iPad system-tab
reuse, NotificationCenter fan-out, and view-model state transitions.
`openTab` is a platform-shell workflow, so Rust ownership is capped at
canonical keys, simple payload helpers, and lightweight classifiers where they
remove duplicated bridge parsing without moving navigation out of the shell.

iOS React Native root props also started consuming the Rust-owned runtime
tab/root props contract. The iOS `Tab` model now adapts into
`RuntimeTabInfo`; `ReactNativeTabHostView` uses that Rust-shaped base for tab
entries, and `SingleTabRootView` uses `runtimeRootPropsFromRuntimeTabs` for the
single-tab root field shape. The Swift adapter also sources shared tab/root
props wire keys from `runtimeTabProps*Key`, `runtimeRootProps*Key`, and
existing bridge key helpers. Swift still appends iOS-specific ACTR script
fallbacks, contract metadata, permissions, history-system-app data,
`additionalProps`, and UIKit layout values that should remain platform-owned.

macOS tab bundle fingerprints now delegate their stable hashing rule to
`ActriumCore.stableContentFingerprint`. The Rust helper owns only the
deterministic SHA-256-prefix contract used to name script bundles/process
groups; macOS still owns `TabContentType`, Web/RN/native routing, XPC service
selection, and lifecycle orchestration in Swift.
`check:rust-boundary` guards against reintroducing local `CryptoKit` /
`SHA256.hash` fingerprint logic in the macOS tab model.

iOS no longer carries the unused app-local `String.sha256` helper or a stale
`CryptoKit` import in the login manager. That cleanup is intentionally not a
new Rust boundary: there was no active call path to move, so deleting the dead
platform-local hash implementation keeps the ownership graph simpler.
`check:rust-boundary` now guards against reintroducing that iOS string utility.

iOS `UserInfoModule` now sources auth/user-info request field names through
`runtimeAuthContext*Key()`. Rust owns the shared auth context key contract
(`clientId`, `appId`, `tabId`, and `appUrl`); Swift still owns A1SDK
`LoginManager` calls, pending promise queues, local user lookup, HTTP
authorization-check execution, and React Native event delivery.

iOS `LoginManager` now sources environment names, URL-to-environment mapping,
app-URL path validation semantics, user-info storage suffixes, and
access-control URL construction through `authEnvironment*` and
`authAccessControlUrlFromAppUrl` UniFFI helpers. Rust owns the deterministic
dev/beta/prod mapping and shared auth URL shape; Swift still owns A1SDK
configuration calls, login orchestration, HTTP execution, and user persistence
side effects. `LoginManager` also calls `authIsAuthorizedCode` for
access-control response-code semantics, so the authorization success code is no
longer duplicated in Swift.

iOS login result delivery also consumes the Rust-owned login result contract.
`UserInfoModule`, `LoginManager`, and the WeChat callback adapter use
`runtimeLoginResult*` and `runtimeLoginType*` for the `onLoginResult` event,
login provider values, and `type`/`code`/`id_token`/`error`/`message` payload
fields. Swift still owns Apple/WeChat SDK callbacks, NotificationCenter
delivery, native login orchestration, and final RN event emission.

iOS `TabEventEmitter` now consumes Rust-owned native-to-RN event names for tab
fullscreen enter/exit, tab bookmark changes, and global-search input changes.
iOS-only tab lifecycle events such as opened/closed/switched/updated remain
local until another platform or Windows adapter needs the same event surface.

iOS `ActrBridge` now consumes `ActriumCore.actrBridgeContract()` for the
ActrBridge module name, stream-event name, request/result field names,
stream callback payload keys, stream type values, stream status literals,
stream chunk acknowledgement status, attachment upload/result metadata keys, and
promise error codes used by the RN bridge. Swift still owns Actr SDK service
lifecycle, Protobuf request/response mapping, attachment chunk file IO, prompt
RPC timeout selection, promise handling, and React Native event emission.

iOS `ActrBridge` also calls `ActriumCore.parseActrActorType(raw:)` to validate
and split the RN `actorType` wire string before constructing the Swift SDK
`ActrType`. Rust owns the exact `publisher/program@version` parsing rule and rejects
ambiguous or empty values; Swift still owns actor system creation, discovery,
and error mapping into the existing ActrBridge promise flow.

macOS ViewBridge/XPC tab config and related bind/size payloads now also consume
the same Rust-owned canonical key helpers for `tabId`, `title`,
`containerWidth`, `containerHeight`, and `systemAppType` through one local
Swift `RuntimeBridgeContract` adapter in `Tab.swift`. macOS still owns the
DistributedNotification/NSXPC transport, process-pool orchestration, service
readiness, overview-mode flags, panel visibility, script fields, and other
platform-only XPC lifecycle data.

## Current Android Rust Modules

Android consumes UniFFI-generated Kotlin and native shared libraries through a
dedicated local Gradle module:

- `packages/actrium-platform/android`

The Android app includes it as `:actrium-native-kotlin`. Generated Kotlin lives
under `src/main/kotlin/uniffi/*`; Android shared libraries live under
`src/main/jniLibs/{arm64-v8a,x86_64}`. Each Rust crate owns its Android build
script:

- `crates/actrium-core/build-android.sh`
- `crates/actrium-vault/build-android.sh`

The generated UniFFI Kotlin expects `libuniffi_actrium_core.so` and
`libuniffi_actrium_vault.so`, so the scripts copy those loader-name aliases next
to the canonical Cargo outputs.

The stable `io.actrium.app.rust.ActroCore` facade also exposes ACTR integrity
plan and normalization helpers. Android does not currently have a separate
native ACTR package loader; ACTR package loading runs through the shared RN
runtime, which already consumes the Rust-generated TypeScript contract.

The stable Android Rust facade also exposes app source parsing/display helpers,
protected system-tag seeds, and default tag colors. Android does not currently
consume the Apple app-source model directly, but the shared contract is now
ready for future Android or Windows native settings/database screens.

The stable Android facade also exposes `CoreSqlValueInfo` plus native
history/tag/bookmark/app write bind-parameter helpers. Android Room call sites
do not currently consume those vectors because Room owns its generated binding
path, but future Android native repository screens and Windows adapters should
reuse the same Rust-owned parameter contract instead of copying Swift/Kotlin/C#
literal orderings.

Android ASR now delegates pure speech rules to `ActriumCore`: language-code to
bundled Vosk model directory mapping, model asset/storage path planning, ready
marker naming, final-result JSON `text` extraction, and partial-result JSON
`partial` extraction. It also delegates segmented Chinese ASR final-result
punctuation rules through `post_process_zh_asr_segments`. Android still owns
asset copying, Vosk model loading, recognizer lifetime, audio buffering, and
HanLP segmentation; Rust owns how model locations are named and how segmented
`(word, nature)` terms are joined and punctuated. The Android post-processor
must not log recognized text; speech content should only cross this boundary as
in-memory final-result input. Android `AsrModule` and the
RN `SpeechRecognitionService` now also consume the Rust-owned ASR bridge
contract for module/event names, result payload keys, and promise error codes.

Android `AppStatus` now sources its runtime wire values and exact
`alive`/`dead` validation from `ActriumCore`. The Kotlin enum remains UI-friendly
for adapter comparisons, while the persisted Room string and RN bridge status
payloads use Rust-owned runtime status constants.

Android active tab persistence now delegates row/model shape rules to
`ActriumCore` through `ActiveTabInfo` and `ActiveTabRowInfo`. Rust owns the
persisted-row to active-tab mapping, status canonicalization and default
fallback, transient `panelVisible = false` restore behavior, and stable
`sortOrder` assignment for saved active lists. Android still owns Room
annotations, DAO execution, coroutine dispatch, and the UI-facing model class.

Android phone and tablet `updateTabInfo` bridge handling now delegates the
active-tab update patch to `ActriumCore` through `RuntimeTabUpdateInfo`. Rust owns
the field-preserving merge semantics and status canonicalization/defaulting for
RN tab update events. Android still owns extraction from `Map<String, Any?>`,
state-list mutation, and UI model storage.

Android phone and tablet `openTab` bridge handling now delegates branch
classification and parameter-filter policy to `ActriumCore` through
`RuntimeOpenTabPlanInfo`. Rust owns the normal app, system page, and tablet
system tab branch kind; canonical `tabId`, `systemAppType`, and
`tabSystemAppType` key names; and the parameter keys each branch strips before
building root props. Android still owns extraction from `Map<String, Any?>`,
`Bundle` construction, UUID generation for tablet system tabs without a
provided id, StateFlow mutation, and UI navigation.

Android's simple runtime bridge payloads now also flow through `ActriumCore`.
Rust owns the canonical key names and field-presence rules for tab unread
updates, tab bookmark status updates, native-to-runtime bookmark events,
fullscreen tab events, extension-panel visibility events, and container-size
events. Android still owns EventBus dispatch, React Native emitter delivery,
`Bundle`/`WritableMap` conversion, StateFlow mutation, and actual UI behavior.

Android `NativeModuleEvent` now consumes Rust-owned native-to-RN event names for
tab fullscreen enter/exit, tab bookmark changes, extension-panel visibility,
container-size changes, and global-search input changes. Android-only
update-bookmark/update-set events remain local.

Android `ActrBridge` now consumes `ActriumCore.actrBridgeContract()` for the
native module name, stream-event name, request/result field names, status
literals, and promise error codes used by the RN bridge. Android still owns
ReadableMap extraction, WritableMap construction, coroutine dispatch,
`ActrManager` RPC calls, and React Native event emission. The shared contract
also exposes attachment upload/result keys for iOS/RN parity; Android does not
currently implement the attachment-upload native methods.

Android `ActriumDataStreamCallback` now consumes the same contract for stream
event payload keys, metadata entry keys, stream type values, and stream status
values. Android stream events now emit string `type` values matching iOS and RN
(`text`/`voice`) instead of Kotlin enum ordinals; Android still owns chunk
conversion to `WritableMap` and `DeviceEventManager` delivery.

Android `MyUnifiedHandler` now also consumes the Rust-owned stream status
literals for voice chunk acknowledgements. Android still owns prompt route
dispatch, its current 300-second prompt timeout, protobuf forwarding, and
`ContextBridge` stream IO.

Android `ActrBridge.initializeService` now calls
`ActriumCore.parseActrActorType()` and passes the resulting
`ActrActorTypeInfo` into `ActrManager`. Rust owns the `publisher/program@version`
validation and trimming behavior, while Android still owns Actr system
creation, network monitor setup, discovery, and platform SDK `ActrType`
construction.

Android Room compile-time persistence constants now come from Rust-generated
Kotlin source:

- `android/android/app/src/main/java/io/actrium/app/generated/ActriumAndroidPersistenceContract.kt`

The generator lives in `crates/actrium-core/src/bin/generate-android-contracts.rs`
and projects `actrium-core` constants for the Room database name/version,
`active_list` table name, column names, and DAO query SQL. Android annotations
still need Kotlin `const val`s, so this generated source is the boundary between
Rust-owned schema/query contracts and platform-owned Room execution.

Android React Native initial props now delegate tab/root props contract shaping
to `ActriumCore` through `RuntimeTabInfo` and `RuntimeRootPropsInfo`. Rust owns
the single-tab root shape, default runtime tab fields, panel visibility
defaults, current-tab id selection, optional `appUrl` propagation, and the
shared tab/root props wire keys used by `TabBundleUtils`. Android still owns
`Bundle` construction and React RootView lifecycle.

Android system page initial props now also flow through `ActriumCore` via
`RuntimeSystemRootPropsInfo`. Rust owns canonical system app type values for the
Android bundle constructors and preserves the existing `tab.systemAppType` plus
`singleTabMode` shape. Android system-page bundle construction, current-tab id
lookups, and tablet/phone system page wrappers now consume the same root/open
tab key helpers; Android still owns the actual `Bundle` object and host UI
navigation.

The Android stable facade also exposes the Rust-owned auth context key helpers
and auth environment helpers for future native `UserInfoModule`/login work.
Android's current `UserInfoModule` only emits login-result events, so the full
auth context flow is not yet wired on Android. User-info and login-result wire
constants are consumed through the local Kotlin `UserInfoBridgeParams` adapter
instead of direct generated-FFI calls from the module workflow. The facade also
exposes access-control response status helpers plus auth/account API
response-code and client-app not-found helpers for the same future native auth
parity.

Android `UserInfoModule` login result events now use the same Rust-owned login
result contract through the local `UserInfoBridgeParams` adapter. Rust owns the
`onLoginResult` event name, provider values (`wechat`, `apple`, `google`), and
event keys (`type`, `code`, `id_token`, `error`, `message`) consumed by the React
Native login bridge through the generated TypeScript contract. Android still
owns WeChat/Google SDK adapters, coroutine collection, active React instance
checks, `WritableMap` construction, and actual event dispatch.

## Rust Binding Artifact Policy

Use `scripts/actrium/build-actrium-rust-bindings.sh` as the repository-level entrypoint for
local or CI regeneration:

- `runtime` regenerates the React Native TypeScript contract module from
  `actrium-core`.
- `apple` builds `ActriumCore` and `ActriumVault` XCFrameworks and refreshes their
  Swift files in `packages/actrium-platform/apple/*`.
- `android` builds the generated Kotlin bindings and JNI libraries in
  `packages/actrium-platform/android`, and regenerates the Android app's
  Rust-owned compile-time persistence contract.
- `all` runs runtime, apple, and android paths and is the default.

Heavy binary slices are local build artifacts and are ignored by git:

- `crates/actrium-core/ActriumCore.xcframework/**/libactrium_core.a`
- `crates/actrium-vault/ActriumVault.xcframework/**/libactrium_vault.a`
- `packages/actrium-platform/android/src/main/jniLibs/**/*.so`

Lightweight package manifests, generated Swift/Kotlin/TypeScript binding
sources, and XCFramework metadata/headers remain reviewable source artifacts.
Use `npm run check:generated` to verify that the lightweight Rust-generated
TypeScript and Android contract sources are present, match the current
generators, and are consumed through the runtime `coreContract` adapter instead
of direct generated-file imports; `npm run verify:all` includes this check.
This keeps large binary blobs out of normal source commits.

## Next Rust Candidates

1. **Native repository adapters**
   - `actrium-core` now owns native core schema/default seeds plus RN runtime
     schema, migrations, repository query SQL, sort orders, and runtime state
     constants. Android app status wire values and active-list row/model shape
     rules also come from `actrium-core`; Android RN initial props are now shaped
     through Rust-owned runtime tab/root props contracts, including system page
     root props. Apple history/tag repository SQL is also Rust-owned through
     `core_history_repository_sql` and `core_tag_repository_sql`; macOS
     bookmark/app repository SQL is Rust-owned through
     `core_bookmark_repository_sql` and `core_app_repository_sql`. App source
     parsing/display plus system-tag/default-color seed data are now Rust-owned
     domain constants consumed by Apple and exposed to Android. Apple
     history/tag row mapping plus bookmark/app row and search-parameter
     contracts are now Rust-owned through explicit UniFFI row contracts. Apple
     history/tag/bookmark/app write bind-parameter order and primitive value
     normalization are also Rust-owned through `CoreSqlValueInfo`. Android Room
     database name/version, active-list table/column names, and active-list DAO
     SQL now come from a Rust-generated Kotlin contract. Browser
     history/bookmark/session snapshot normalization, browser-specific storage
     schema/repository SQL, and batched browser sync data rules including
     bookmark ownership/source resolution are now Rust-owned through
     `browser_data.rs`; Windows executes that contract through a local SQLite
     adapter with JSON migration tests. The next persistence step is deciding
     whether macOS/iOS and Android browser stores keep platform database handles
     with Rust-owned SQL/row/parameter contracts, or whether a shared Rust SQLite
     execution adapter is introduced.

2. **Native bridge event payloads**
   - `actrium-core` now owns Android `updateTabInfo` active-tab patch semantics
     through `runtime_tab_update_from_fields` and
     `active_tab_apply_runtime_update`. `openTab` route classification is
     intentionally local to native bridge adapters after the anti-fragmentation
     review; Rust owns only stable request/payload keys where they are shared.
     Rust also owns the canonical key names, simple
     payload shapes, and shared native-to-RN event names for Android unread,
     bookmark, fullscreen, extension-panel visibility, container-size, and
     global-search input events. iOS `TabModule`,
     `TabEventEmitter`, and selected `BrowserViewModel` notification payloads
     now consume the same canonical key/simple-payload helpers for their
     matching fields and events, and iOS UI publishers now use those helpers
     for bookmark/fullscreen notifications. iOS open/select/close/system-tab
     NotificationCenter receivers and root-prop system-tab writers now also use
     Rust-owned canonical `systemAppType` and `tabId` key helpers. iOS root
     props now also consume the Rust-owned
     `RuntimeTabInfo`/`RuntimeRootPropsInfo` base contract and the canonical
     tab/root props key helpers while keeping iOS-only ACTR/history/layout
     extras in Swift. Android `TabBundleUtils`, system-page bundle builders,
     and current-tab id readers consume the same root props key helpers. macOS
     ViewBridge/XPC tab config and bind/size payloads now consume the same
     canonical key helpers for shared RN-facing fields while keeping process
     orchestration and platform lifecycle state in Swift. macOS tab bundle
     fingerprints now call `stableContentFingerprint` for the deterministic
     SHA-256-prefix rule, while keeping browser/content routing in Swift. Rust
     also owns the
     `clientId`/`appId`/`tabId`/`appUrl` auth context key contract consumed by
     iOS `UserInfoModule` and exposed through the Android facade for future
     login/auth parity work. Rust now also owns the `onLoginResult` event name,
     login provider values, and login-result payload keys consumed by iOS and
     Android native producers plus the RN TypeScript login consumer. Auth
     environment names, host suffixes, iOS app-URL path semantics, and storage
     suffixes are also Rust-owned through `auth.rs` and generated
     `AUTH_ENVIRONMENT_CONTRACT`. Account/resource service base URLs and
     auth/client-app path construction are Rust-owned through
     `AUTH_ENDPOINT_CONTRACT` and UniFFI helpers. Access-control response status
     normalization is now Rust-owned through `auth_access_control_status` and
     generated `AUTH_ACCESS_CONTROL_RESPONSE_CONTRACT`. Auth/account API
     response envelope success codes and client-app not-found detection are now
     Rust-owned through `AUTH_API_RESPONSE_CONTRACT`. Shared native-to-RN
     event names for fullscreen/bookmark/extension/container/search events are
     Rust-owned as well. iOS/Android/RN ActrBridge module/event
     names, request/result payload keys, stream callback payload keys, stream
     type values, stream metadata terminal markers, stream status literals,
     attachment upload/result metadata, attachment chunk/checksum/status/type
     values, prompt/attachment/local-service RPC route keys,
     platform-specific RPC timeout values, and promise error codes are now
     Rust-owned through
     `actr_bridge_contract` and generated `ACTR_BRIDGE_CONTRACT`.
     `parse_actr_actor_type` now also owns the shared `publisher/program@version`
     actorType parsing rule consumed by iOS and Android. `openTab` is capped at
     key/payload contracts plus local classifier adapters that remove duplicated
     bridge parsing; Apple still keeps its platform route policy, and future
     Windows code should not consume an existing classifier unless its shell
     policy is genuinely the same. The next bridge steps are review-gated
     rather than automatic: unify prompt timeout policy only if the
     product wants one cross-platform timeout, move richer macOS XPC contracts
     only for stable RN-facing data, and move response/result shaping only when
     it removes repeated adapter logic. UI-adjacent notification payloads should
     stay platform-owned unless multiple producers/consumers prove the shared
     contract is simpler than local code.

3. **ASR post-processing split**
   - `actrium-core` now owns ASR language/model-dir mapping, bundled model
     asset/storage path planning, ready-marker naming, Vosk final and partial
     JSON result parsing, segmented Chinese final-result punctuation rules, and
     Android ASR native bridge module/event/result/error-code contracts.
     Android still supplies model file IO, Vosk lifecycle, audio buffering, and
     HanLP segmentation as platform adapters. The Android post-processor does
     not log recognized text, so user speech content stays in the in-memory ASR
     path. The next step is choosing whether
     to keep HanLP as an Android-only adapter, introduce a Rust-owned segmenter,
     or collapse more of the post-processing path if the current
     Vosk/Kotlin/HanLP/Rust call chain keeps creating unnecessary boundary
     hopping.

4. **Remaining native package integrity wiring**
   - `actrium-core` now exposes manifest integrity planning through both the RN
     generated TypeScript contract and native UniFFI. RN plus macOS/iOS now
     execute file hashing against that shared plan. The next step is wiring the
     future Windows loader, plus any future Android native loader if one appears,
     to the same contract. Extract the Apple Swift helper only if more Apple
     loader call sites appear.

## Current Actrium Vault FFI Surface

`actrium-vault` exports the non-UI vault contract that platform shells can call
without taking ownership of cryptographic details:

- `generate_vault_salt_base64`
- `derive_vault_key_base64`
- `create_vault_verification_hash` and `verify_vault_password`
- `encrypt_vault_string` and `decrypt_vault_string`
- `encrypt_vault_fields` and `decrypt_vault_fields`
- `vault_password_strength`
- `parse_vault_category`, `vault_category_info`, and
  `vault_category_templates`

The platform layer still owns how master passwords are collected, stored in
Keychain/Keystore/Credential Locker, and unlocked via biometrics.
