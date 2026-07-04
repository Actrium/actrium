# Browser Sync Cloud Drive Architecture

Actro browser sync should use the user's own cloud storage as the product
sync medium. Actro should not operate a browser sync service for the normal
product path. If a user wants cross-device browser history, bookmarks, and
sessions, they connect the same cloud drive account on each device, such as
iCloud Drive, Google Drive, OneDrive, or another provider-backed file store.

The browser client must not embed a sync server. Actro is not trying to be
Google Chrome with a first-party browser account backend; it is a browser client
that can read and write user-owned cloud files when the user chooses to sync.

The existing `crates/actro-sync-server` crate is therefore a reference executor
and local protocol probe. It is useful for testing merge rules, schema
evolution, and end-to-end sync-cycle behavior, but it is not the target
production architecture.

## Ownership

- Rust owns deterministic sync semantics: schema versions, record identity,
  tombstones, merge policy, conflict summaries, local SQLite plans, cloud file
  manifests, and retry/status policy.
- Platform code owns cloud provider integration: user sign-in, OAuth or system
  account sessions, file picker or app-folder authorization, SDK calls,
  provider-specific background scheduling, and OS-specific secure token storage.
- Product UI owns provider choice, connection state, recovery prompts, and
  conflict presentation.

## Provider Model

Each cloud provider should implement the same platform-local store boundary:

- `list(prefix)`: enumerate Actro browser sync objects and provider metadata.
- `read(path)`: fetch bytes plus revision metadata such as ETag, file version,
  or modification generation.
- `write(path, bytes, expectedRevision)`: optimistic write with conflict
  detection when the provider supports compare-and-swap semantics.
- `delete(path, expectedRevision)`: tombstone or remove an obsolete object.
- `capabilities()`: advertise provider limits, background behavior, and
  available revision/conflict metadata.

Google Drive, iCloud Drive/CloudKit, OneDrive, and local folder sync can all
fit this boundary while keeping their SDK details out of Rust.

The Rust boundary for this shape is `BrowserStorageSyncCloudStore`, and every
platform-visible binding can inspect the companion
`browser_storage_sync_cloud_store_contract_json()` contract. The contract is
intentionally about object-store semantics, not provider login UX: OpenDAL,
Apple native adapters, platform SDK adapters, and in-memory test stores should
all translate into the same `list/read/write/delete/capabilities` behavior.
`browser_storage_sync_cloud_store_capability_preflight_json()` is the provider
readiness gate for that contract: adapters pass their current
`BrowserStorageSyncCloudStoreCapabilities`, and Rust classifies the provider as
ready, degraded with adapter emulation, setup-required, limit-too-small, or
conflict-unsafe before any browser sync IO starts. Provider SDK calls, account
UI, and concrete fallback/emulation still stay platform/provider-owned.
`browser_storage_sync_cloud_provider_adapter_contract_json()` and
`browser_storage_sync_cloud_provider_adapter_plan_json()` define the object-space
adapter layer above raw capabilities and below connection status. Platforms pass
the selected provider kind, adapter family, root path, capabilities, or an
existing capability preflight; Rust returns the canonical `Actro/BrowserSync`
layout, required manifest/device/log/snapshot/recovery objects and prefixes,
coarse operation groups, production-readiness flags, root setup actions,
adapter-emulation requirements, and blocked reasons. This keeps path mapping,
provider id versus sync identity rules, and coarse provider IO boundaries shared
without turning provider `list/read/write/delete` into tiny cross-FFI calls.
`browser_storage_sync_cloud_provider_connection_contract_json()` and
`browser_storage_sync_cloud_provider_connection_status_policy_json()` define the
next status layer above those capabilities. Platforms pass a coarse provider
snapshot containing the selected provider, provider-owned account/session
identity, credential state, optional capability preflight, and local device
membership status. Rust returns stable classifications such as `ready`,
`signInRequired`, `authExpired`, `storageSetupRequired`, `quotaExceeded`,
`providerUnavailable`, `adapterUnsupported`, and `membershipRequired`, plus
`shouldAttemptSync`, `shouldAttemptMembership`, scheduler action, provider
action, user action, retry timing, and diagnostic reasons. This keeps provider
status priority rules shared across Apple, Android, Windows, and future
providers without moving OAuth, secure credential storage, SDK calls, or UI
wording into Rust, and without introducing an Actro browser-sync account
service.
`browser_storage_sync_cloud_provider_error_policy_json()` and
`browser_storage_sync_cloud_provider_error_classification_json()` define the
provider error classification layer beside that connection status policy.
Adapters pass coarse SDK/OpenDAL/native error batches, failed operation results,
or single provider errors with status/code/kind/message/path fields. Rust
normalizes them into shared kinds such as `unauthorized`, `quotaExceeded`,
`rateLimited`, `network`, `conflict`, `notFound`, `storage`, `invalidPath`,
`unsupported`, `cloudDocumentInvalid`, and `providerFailure`, then returns batch
classifications such as `authExpired`, `quotaExceeded`,
`cloudStateRereadRequired`, `providerUnavailable`,
`providerRecoveryRequired`, `adapterUnsupported`, and `storageSetupRequired`.
The output carries retry, reread, provider-recovery, user-action, scheduler, and
telemetry intent. This keeps provider error priority, backoff, and reread policy
shared without turning provider exceptions or object IO into tiny cross-FFI
calls; provider SDK calls and user-facing wording remain platform-owned.
The same classification entry also accepts nested round/client result shapes
directly: adapters may pass a whole `cycleRound`, `pushRound`, `pullRound`,
`membershipRound`, `compactionRound`, `cleanupRound`, `recoverySnapshotRound`,
or `resultPlan` payload, and Rust extracts the failed provider
read/write/delete operation results plus `resultPlan.issues` from the nested
structure. Apple, Android, and Windows local-folder clients use this to build
a `localFolderProviderRoundErrors` source from the cycle, membership,
maintenance, and recovery rounds they already produced, so provider
diagnostics consume real provider failures without new per-operation FFI
calls.
Rust also owns the cloud file data model exposed by
`browser_storage_sync_cloud_data_model_json()`: manifest, device, log, and
snapshot documents share one version gate, path layout, retention policy, and
normalization surface across Apple, Android, Windows, and future providers.
`browser_storage_sync_cloud_compatibility_status_json()` is the preflight gate
for those bytes: before a provider adapter normalizes, reads, writes, or merges a
manifest/device/log/snapshot document, Rust classifies it as compatible,
client-upgrade-required, cloud-data-migration-required, or invalid/recoverable.
`browser_storage_sync_cloud_compatibility_preflight_json()` is the batched
version of the same gate: provider adapters can pass the whole manifest/device/
log/snapshot batch read from cloud storage once, and Rust returns the aggregate
continue/block/upgrade/migrate/recover decision without per-file FFI calls.
`browser_storage_sync_cloud_schema_rollout_plan_json()` turns that preflight into
the client/cloud-file rollout decision. The current plan is explicit:
strict-current-schema, current-only compatibility window, no N-1 read/write
window, newer cloud files require a client upgrade, older files require an
explicit cloud-data migration playbook, and invalid files require repair before
sync continues.
`browser_storage_sync_cloud_schema_migration_playbook_json()` turns that rollout
decision into the explicit migration/repair handoff. It freezes normal provider
writes, requires affected cloud files to be snapshotted before migration or
repair, keeps migrated/repaired writes out of automatic SDK execution for now,
and requires compatibility preflight to pass again before normal sync resumes.
`browser_storage_sync_cloud_recovery_snapshot_plan_json()` turns that playbook
plus the same compatibility preflight into a provider-executable batch of
`writeRecoveryCopy` operations under `Actro/BrowserSync/recovery/...`. Rust
chooses the affected migration/repair source files and deterministic snapshot
paths in one call, while provider adapters perform the actual read/write copies
through their existing object-store layer. This keeps recovery snapshots
coarse-grained and avoids adding a per-file copy trait or tiny FFI calls.
`browser_storage_sync_cloud_recovery_snapshot_result_plan_json()` is the matching
result handoff: provider adapters pass the whole batch of `writeRecoveryCopy`
receipts or errors back to Rust, and Rust marks the snapshot stage complete only
after every planned copy succeeds. Partial success, conflicts, missing sources,
auth failures, and quota failures become Rust-owned reread/replan, retry, or
provider/user recovery actions before migration or repair can continue.
`browser_storage_sync_cloud_local_folder_recovery_snapshot_round_json()` is the
coarse local-folder execution entry for that same flow: native tests or a
local-folder provider pass root/playbook/preflight once, Rust reads the planned
source objects, writes recovery copies through `BrowserStorageSyncCloudStore`,
and returns the plan/result bundle without exposing per-file provider FFI. The
result includes a `BrowserStorageSyncCloudRecoverySnapshotRoundSummary` with
read/write counts, planned snapshot counts, migration/repair document counts,
snapshot mode/root, source and snapshot paths, successful snapshot paths, reasons,
and issue diagnostics so recovery UI/schedulers do not rederive copy state from
per-file receipts.
The Apple, Android, and Windows local-folder sync clients now run this handoff
when compatibility preflight blocks normal sync. Their sync result includes
`schemaRolloutPlan`, `schemaMigrationPlaybook`, and `recoverySnapshotRound` for
blocked providers, while successful normal cycles keep those fields `null` and
continue through the regular cycle/compaction/cleanup path.
Device membership writes are planned by
`browser_storage_sync_cloud_membership_plan_json()`: provider adapters pass the
current manifest/device bytes, provider revision tokens, actor/member device
identity, and a join/revoke action; Rust returns ordered `writeDevice` and
`writeManifest` operations plus the next document bytes. Each planned write
carries `writeReason` diagnostics so provider adapters do not rederive whether
the object is a device membership document or a manifest commit.
Native/provider code still owns provider sign-in, connection UI, SDK calls, and
optimistic-write retry loops.
`browser_storage_sync_cloud_membership_result_plan_json()` is the matching
post-write handoff: adapters pass provider write receipts/errors for that
membership plan, and Rust decides whether the join/revoke committed, whether to
reread cloud membership state before retrying, or whether provider auth/quota
recovery is required.
The first provider IO planner is
`browser_storage_sync_cloud_push_plan_json()`: after a provider adapter reads
the current `manifest.json` and `devices/<device-id>.json` bytes plus revision
tokens, Rust returns the ordered `writeDevice`, per-collection `writeLog`, and
`writeManifest` operations with `writeReason` diagnostics plus a `writeSummary`
that counts required writes, optimistic-revision writes, device/log/manifest
write types, touched collections, write reasons, write paths, and log paths.
Native/provider code executes those writes in one adapter layer and uses
provider conflicts to reread, merge through Rust, and retry.
The push result handoff is
`browser_storage_sync_cloud_push_result_plan_json()`: provider adapters pass the
write receipts/errors for that whole push plan, and Rust decides whether local
dirty rows can be acknowledged, whether manifest/device state must be reread
before rebuilding a push plan, or whether provider-auth/quota/error recovery is
required. Push result issues preserve planned `writeReason` diagnostics, so
status and recovery UI do not remap failed writes back to device/log/manifest
roles. Successful push results expose `ackAction` / `ackReason`, so later
client plans do not infer why local dirty rows can be marked synced. This keeps
partial-write, local-ack, and conflict recovery policy out of each platform
adapter.
The companion read planner is
`browser_storage_sync_cloud_pull_plan_json()`: after a provider adapter reads
`manifest.json`, Rust compares manifest collection state with local cursors and
returns collection-level `readSnapshot` / `listLogs` operations plus a
`readSummary` that counts required reads, active collections, snapshot reads,
log listings, direct log/object reads, cloud-state rereads, page cursors, read
reasons, read paths, and read prefixes. Native/provider code still owns object
bytes IO, while each planned read carries `readReason` diagnostics so adapters
do not rederive why a snapshot or log listing is needed.
Rust-executed planned read results preserve `readReason`; log objects expanded
from a list result carry `readListedCloudLogObject`. Providers then pass
snapshot/log JSON back through Rust normalizers and remote-apply planning.
`browser_storage_sync_cloud_pull_continuation_plan_json()` turns provider
`listLogs` read results with `nextCursor` into follow-up `listLogs` operations
that carry `pageCursor` plus `readReason`, and returns a `continuationSummary`
with read-result counts, continuable list-result counts, collections, prefixes,
page cursors, read reasons, and source operation indices. Pagination
continuation planning stays shared while the concrete provider SDK execution
loop stays native.
The remote-apply handoff is
`browser_storage_sync_cloud_pull_apply_plan_json()`: provider adapters pass the
snapshot/log objects they read, and Rust builds a normalized `remoteBatchJson`
plus the existing native-executable `remoteApplyPlan`. That keeps log ordering,
self-device log skipping, redundant-log filtering, and cursor merging out of
platform code. Skipped snapshot/log diagnostics are surfaced as
`skippedSnapshots` and `skippedLogs` with stable `skipReason` / `action` values,
so provider adapters and UI do not rederive why incremental snapshots,
same-device logs, or already-current logs were ignored. The same plan exposes
`applyAction` / `applyReason` plus an `applySummary` for imported collections,
snapshot/log imports, skipped snapshot/log reasons and paths, remote record
types, cursor collections, remote apply statement keys, and the final apply
decision, so native adapters do not infer whether to run remote SQLite apply
from record/cursor counts or parse `remoteBatchJson` for diagnostics.
The compaction planner is
`browser_storage_sync_cloud_compaction_plan_json()`: provider adapters pass the
manifest, current snapshot objects, and listed log objects, and Rust decides
whether enough contiguous logs exist to write a compacted snapshot, which old
logs may be deleted after the manifest update succeeds, and which
`writeSnapshot` / `writeManifest` / `deleteLog` operations the provider layer
should execute. Planned writes carry `writeReason` diagnostics and planned
deletes carry `deleteReason` diagnostics so provider adapters do not rederive
why a snapshot/manifest is written or why a log is safe to remove. That keeps
retention thresholds, log-gap safety, and manifest snapshot heads shared across
platforms.
The compaction result handoff is
`browser_storage_sync_cloud_compaction_result_plan_json()`: provider adapters
pass the write and delete receipts for the whole compaction plan, and Rust
decides whether compaction committed, whether manifest/snapshot/log state must
be reread before replanning, or whether only old-log cleanup should be retried.
Compaction result issues preserve planned `writeReason` / `deleteReason`
diagnostics for failed writes and deletes. That keeps partial compaction,
provider conflict, and cleanup retry policy out of each platform/provider
adapter.
The coarse local-folder compaction runner is
`browser_storage_sync_cloud_local_folder_compaction_round_json()`: it preflights
cloud-file compatibility, reads real local-folder manifest/snapshot/log files
through `BrowserStorageSyncCloudStore`, builds the same compaction plan, writes
snapshot/manifest objects, and deletes old logs only after the manifest commit
succeeds. The result includes a
`BrowserStorageSyncCloudCompactionRoundSummary` with read/write/delete counts,
planned compaction write/delete counts, compacted collection/log counts,
manifest commit state, cleanup completion/retry state, reasons, paths, and
issue diagnostics. This keeps compaction sequencing and maintenance diagnostics
in Rust without exposing list/read/write/delete as a per-operation FFI surface.
The high-watermark retention planner is
`browser_storage_sync_cloud_high_watermark_plan_json()`, described by
`browser_storage_sync_cloud_high_watermark_policy_json()`: provider adapters pass
manifest, snapshot, log, and local cursor state once, and Rust classifies each
collection as current, lagging, or missing required logs. It also reports
compaction pressure and non-required `candidateDeleteLog` cleanup candidates for
logs already covered by snapshots and outside the retained log window. Cleanup
candidates also carry `deleteReason` diagnostics. Provider IO remains
native/provider-owned; cleanup execution should only happen after Rust-planned
compaction has safely committed the snapshot and manifest state.
The cleanup result handoff is
`browser_storage_sync_cloud_cleanup_result_plan_json()`: provider adapters pass
the delete receipts for Rust-selected high-watermark cleanup candidates, and
Rust decides whether cleanup finished, whether missing logs are idempotent
success, whether conflicts require rereading retention state before replanning,
or whether transient failures should retry only those same candidate log
deletes. Cleanup result issues preserve planned `deleteReason` diagnostics, so
provider status surfaces can explain why Rust chose that old log without
reimplementing retention logic. Provider SDK delete calls remain
native/provider-owned.
The coarse local-folder cleanup runner is
`browser_storage_sync_cloud_local_folder_cleanup_round_json()`: it preflights
cloud-file compatibility, reads real local-folder manifest/snapshot/log files
through `BrowserStorageSyncCloudStore`, builds the same high-watermark plan,
executes only Rust-selected non-required `candidateDeleteLog` deletes, and
classifies delete receipts through the shared cleanup result planner. The result
includes a `BrowserStorageSyncCloudCleanupRoundSummary` with read/delete counts,
planned cleanup delete counts, retained-log and collection currency counts,
cleanup completion/retry state, reasons, deleted paths, and issue diagnostics.
This keeps local-folder cleanup sequencing and maintenance diagnostics in Rust
without exposing provider list/read/delete calls as tiny FFI operations.
Rust also provides `BrowserStorageSyncMemoryCloudStore` plus
`browser_storage_sync_cloud_execute_planned_writes()` /
`browser_storage_sync_cloud_execute_planned_deletes()` as a coarse Rust-side
provider fixture and planned-operation executor. This is not a product storage
provider or a UI binding; it lets OpenDAL, iCloud, Google Drive, OneDrive, and
local-folder adapters validate the same revision, conflict, receipt, and error
mapping rules without each platform inventing a parallel test drive.
`BrowserStorageSyncLocalFolderCloudStore` is the first file-backed Rust store
for the same boundary. It maps cloud object paths onto a local Actro folder,
uses content hashes as optimistic revisions, and exercises real filesystem
list/read/write/delete behavior for provider compatibility tests. It is still a
coarse injected-store fixture, not a per-operation FFI surface; product
iCloud/Google Drive/OneDrive/OpenDAL adapters should execute their own SDK IO
and pass batched documents/receipts through the existing Rust planners.
`browser_storage_sync_cloud_local_folder_capabilities_json()` exposes that
store's Rust-owned capability declaration, and
`browser_storage_sync_cloud_local_folder_capability_preflight_json()` runs the
same readiness gate without requiring Swift, Kotlin, or C# clients to recreate
the capability JSON.
`browser_storage_sync_cloud_local_folder_compatibility_preflight_json()` runs the
cloud-file compatibility preflight through the same local-folder store: native
clients pass only the root directory and observation time, Rust lists and reads
real manifest/device/log/snapshot files, and the result uses the same
`cloudCompatibilityPreflight` plan as batched provider reads. That keeps
client-upgrade, migration-required, and cloud-repair blocking before local-folder
cycle/apply work starts without exposing per-file provider reads over FFI.
The local-folder membership round also runs the same compatibility preflight in
Rust before mutating `manifest.json` or `devices/<device-id>.json`, and returns
that `compatibilityPreflight` inside the membership-round result so native
connect/disconnect flows cannot bypass cloud-file schema gates.
`browser_storage_sync_cloud_local_folder_cycle_round_json()` exposes that
file-backed store as one coarse native-callable cycle round: platform tests or a
local-folder provider pass a root directory, optional push request, cursors, and
device identity, while Rust performs the local file read/write/list operations
and returns the same cycle-round/client-plan JSON used by other cloud stores.
This keeps local-folder provider sequencing in Rust without exposing
list/read/write/delete as tiny FFI calls or pulling iCloud/Google/OneDrive SDK
execution into Rust.
`browser_storage_sync_cloud_local_folder_membership_round_json()` exposes the
same file-backed store as one coarse native-callable membership round:
platform tests or a local-folder provider pass a root directory and a join/revoke
membership request once, while Rust reads manifest/device documents, plans the
membership writes, executes them against real files, and classifies the result.
This keeps local-folder connect/disconnect membership sequencing in Rust without
turning provider document reads and writes into tiny FFI calls.
Windows, the Apple shared package, and Android now have concrete native
consumers of those coarse local-folder cycle and membership entries. Windows
`BrowserStorageSyncClient.RunLocalFolderSyncCycleAsync`, Apple
`AppleBrowserStorageSyncClient.runLocalFolderSyncCycle`, and Android
`BrowserStorageSyncClient.runLocalFolderSyncCycle` export dirty rows through the
existing Rust push builder, call the local-folder cycle helper once, ask Rust for
the local-folder provider capability preflight, local-folder cloud-file
compatibility preflight, status policy, schedule state, provider diagnostics,
observability event, attempt plan, and loop plan, then
apply the Rust cloud-cycle client plan through native SQLite
(`BrowserStorageDatabase.ApplyCloudCycleClientPlan`,
`AppleBrowserStorageDatabase.applyCloudCycleClientPlan`, or Android
`BrowserStorageDatabase.applyCloudCycleClientPlan`). After native ack/apply,
the same clients call
`browser_storage_sync_cloud_local_folder_compaction_round_json()` and then
`browser_storage_sync_cloud_local_folder_cleanup_round_json()` with the latest
local cursors, returning Rust maintenance results as `compactionRound` and
`cleanupRound` in the local-folder cycle result. The main cycle
classification/action still comes from the cycle status policy; compaction and
high-watermark cleanup sequencing stays coarse-grained and Rust-owned. Windows
`BrowserStorageSyncClient.RunLocalFolderMembershipRoundAsync`, Apple
`AppleBrowserStorageSyncClient.runLocalFolderMembershipRound`, and Android
`BrowserStorageSyncClient.runLocalFolderMembershipRound` wrap the same
capability preflight around
`browser_storage_sync_cloud_local_folder_membership_round_json()` so local-folder
connect/disconnect flows also pass through Rust once instead of reimplementing
manifest/device membership sequencing or compatibility blocking in C#, Swift, or
Kotlin. These membership results also include Rust provider diagnostics and a
Rust cloud observability event, so connect/revoke status surfaces, diagnostics
export, and provider recovery UI share the same priority/action vocabulary as
ordinary sync cycles. They now also include the Rust-owned
`statusPolicy` from
`browser_storage_sync_cloud_membership_status_policy_json()`, which classifies a
whole connect/revoke round as `success`, `clientUpgradeRequired`,
`providerAuthRequired`, `providerQuotaExceeded`, `cloudConflict`, or
`providerUnavailable` before native UX/schedulers decide what to show. The
adapters still execute SQLite locally and do not expose per-object provider
operations over FFI.
`browser_storage_sync_cloud_run_push_round()` composes the push side into one
Rust-owned provider round: read `manifest.json` and the device document through
`BrowserStorageSyncCloudStore`, build the push write plan, execute the planned
writes, classify the receipts through the shared push result planner, and
return a `BrowserStorageSyncCloudPushRoundSummary` with read/write counts,
planned write type counts, touched collections, read/write reasons, successful
paths, failed provider error kinds, issue reasons, and the Rust-owned ack /
reread / retry action. It is still a single round; retry timing, OS scheduling,
provider sign-in recovery, and user-visible status stay with the existing
scheduler/product layer.
`browser_storage_sync_cloud_execute_planned_reads()` and
`browser_storage_sync_cloud_run_pull_round()` do the same for one pull attempt:
read `manifest.json`, execute Rust-planned `readSnapshot` / `listLogs`
operations through the store, follow `listLogs` continuation pages through
`browser_storage_sync_cloud_pull_continuation_plan_json()` via
`browser_storage_sync_cloud_execute_planned_reads_with_continuations()`, parse
snapshot/log objects, build the shared remote-apply plan, and return a
`BrowserStorageSyncCloudPullRoundSummary` with read counts, pagination
continuation counts, imported/skipped object counts, active collections,
read reasons, failed provider error kinds, and the Rust-owned pull action /
reason. Multi-round retry orchestration and concrete cloud SDK execution remain
outside this helper.
`browser_storage_sync_cloud_run_cycle_round()` composes those coarse helpers into
one Rust-owned sync cycle attempt: optionally push local changes, stop before
pull when the push round needs reread/retry/recovery, then run one pull round
and return the required local ack / remote apply actions. Provider SDK calls,
native SQLite apply execution, OS scheduling, and multi-round retries stay
outside the helper. Each cycle result also carries a
`BrowserStorageSyncCloudCycleRoundSummary` plus a
`BrowserStorageSyncCloudCycleClientPlan`, and
`browser_storage_sync_cloud_cycle_client_plan_json()` can rebuild that plan from
a round result JSON. The summary counts push/pull presence, completion,
client-step categories, read/write failures, conflicts, retryable failures,
remote imports/skips, affected collections, and diagnostics so native UX and
schedulers do not parse nested push/pull rounds differently. The plan flattens
local ack, remote apply, reread, and retry steps so native adapters do not
duplicate nested push/pull result interpretation. Local ack steps carry
`stepReason` from the push result `ackReason`, and remote apply steps carry
`stepReason` from the pull apply `applyReason`, so native SQLite adapters do
not rediscover why local rows are acked or why a remote batch is being applied.
`browser_storage_sync_cloud_run_membership_round()` gives Rust-side provider
fixtures the same coarse shape for connect/disconnect: read manifest/device,
build the membership write plan, execute writes, and classify the result without
pulling provider login UX or concrete SDK calls into Rust. The result includes a
`BrowserStorageSyncCloudMembershipRoundSummary` with read/write counts, planned
membership write counts, compatibility-blocked state, read/write reasons,
successful paths, failed provider error kinds, issue reasons, and the
connect/revoke action/reason so native provider UX does not rebuild those
diagnostics separately.
The local-folder membership JSON entry uses that same runner with
`BrowserStorageSyncLocalFolderCloudStore`, so local-folder provider membership
tests and prototypes exercise real file IO without duplicating membership
mutation or partial-write classification in Swift, Kotlin, or C#. Apple,
Android, and Windows local-folder membership clients also surface the same
`schemaRolloutPlan`, `schemaMigrationPlaybook`, and `recoverySnapshotRound`
bundle when membership is compatibility-blocked, so connect/revoke UI and
schedulers follow the same Rust-owned recovery handoff as ordinary sync cycles.
They also include `providerDiagnostics` and `observabilityEvent` for successful
and blocked membership rounds, keeping provider status/export semantics shared
without adding a separate service or tiny provider-operation FFI.
`browser_storage_sync_cloud_membership_status_policy_json()` is the matching
coarse membership status helper: platforms pass the completed membership-round
JSON once and receive classification, retry/reread flags, provider/user actions,
and retry timing without reimplementing connect/revoke status policy in Swift,
Kotlin, or C#.
`browser_storage_sync_cloud_membership_schedule_state_json()` turns that
membership status policy plus an observed timestamp into the same scheduler
shape used by cycle rounds: next retry/run timing, provider credential/storage
waiting flags, user-action flags, and pending cloud reread state. Apple,
Android, and Windows local-folder membership clients include it as
`scheduleState` and pass it into provider diagnostics, keeping connect/revoke
retry and recovery behavior aligned with ordinary cloud sync cycles without
moving provider SDK calls or UI copy into Rust.
`browser_storage_sync_cloud_membership_attempt_plan_json()` composes the
membership round, membership status policy, and membership schedule state into
one provider-neutral connect/revoke attempt plan. Apple, Android, and Windows
local-folder membership clients include it as `attemptPlan` and pass it into the
same observability event contract, so finish/reread/retry/recovery decisions are
not reassembled differently in Swift, Kotlin, or C#. Provider SDK calls,
provider account UI, OS scheduling, and user-facing copy remain platform-owned.
`browser_storage_sync_cloud_membership_loop_plan_json()` then consumes one or
more already-produced membership rounds and turns the connect/revoke attempt
plans into bounded loop state: whether to run another provider round, wait for
retry backoff, stop for provider or user recovery, or stop after max attempts.
It also returns `BrowserStorageSyncCloudMembershipLoopSummary` as membership
`loopSummary`, grouping attempt counts, finish/reread/retry categories, provider
operation and recovery-stop counts, retry backoff, final action/classification,
and diagnostics so provider status surfaces do not rescan the attempt list.
Apple, Android, and Windows local-folder membership clients include this as
`loopPlan` and feed it into observability, keeping orchestration policy
Rust-owned while provider adapters still own concrete cloud APIs.
`browser_storage_sync_cloud_cycle_status_policy_json()` classifies the same
cycle round for native schedulers and provider recovery UX. It uses cloud/provider
terms such as `cloudConflict`, `providerUnavailable`, `providerAuthRequired`,
`providerQuotaExceeded`, and `cloudDocumentInvalid`, so platforms do not map
cloud-drive failures back onto server-oriented status labels.
`browser_storage_sync_cloud_cycle_schedule_state_json()` then turns that policy
and an observed timestamp into `nextRetryAttempt`, `nextRunDelaySeconds`,
`nextRunAtUnixSeconds`, provider-waiting flags, user-action flags, and pending
cloud reread state for native schedulers.
`browser_storage_sync_cloud_provider_diagnostics_json()` sits beside those
scheduler helpers as a coarse provider-visible diagnostic summary. Platforms pass
the Rust-owned capability preflight, compatibility preflight, provider error
classification, cycle status policy, and schedule state JSON they already have;
Rust returns one stable `cloudProviderDiagnostics` payload with priority,
severity, actions, reasons, and source summaries. Product UI still owns wording
and presentation, and provider adapters still own SDK calls. Provider error
classifications feed the same scheduler vocabulary: conflict-like provider
errors request a cloud reread, transient provider/network failures hand retry
timing back to the scheduler, and auth/quota/unsupported/recovery failures
surface provider or user actions without introducing service-side status labels.
Diagnostics preserve the raw `providerErrorClassification` while mapping it into
provider-facing classifications such as `providerAuthRequired`, `cloudConflict`,
`providerQuotaExceeded`, `providerAdapterUnsupported`, and
`providerSetupRequired`.
`browser_storage_sync_cloud_observability_event_json()` turns the Rust-owned
diagnostics, attempt plan, and loop plan into one local/exportable
`cloudObservabilityEvent` payload. The event separates redacted attributes from
local-only identifiers such as scope, device identity, and provider root path so
native clients can share local logs, diagnostic export, and user-approved product
telemetry semantics without adding a browser sync service or duplicating event
field priority rules.
`browser_storage_sync_cloud_cycle_attempt_plan_json()` composes the client plan,
status policy, and schedule state into one provider-neutral attempt plan with
the next local-ack, remote-apply, reread, retry, stop, and provider/user recovery
flags. This keeps multi-round orchestration policy in Rust without turning
concrete provider SDK calls, OS background scheduling, or native SQLite apply
execution into Rust-owned micro-steps.
`browser_storage_sync_cloud_cycle_loop_plan_json()` sits one level above attempt
planning: platform adapters pass the cycle rounds they have already produced, and
Rust decides whether the bounded loop may continue, whether native SQLite ack or
remote apply is a barrier before another provider round, whether retry backoff
must return control to the scheduler, and whether provider/user recovery must
stop the loop. The plan includes `BrowserStorageSyncCloudCycleLoopSummary` as
cycle `loopSummary` for attempt counts, ack/apply/reread/retry categories, native
apply barriers, retry backoff, final action/classification, and diagnostic
reason groups, so provider adapters do not rescan every attempt for status UI.
Provider SDK calls and native SQLite execution still stay outside Rust.

Provider connection and device membership are therefore provider/cloud-file
work, not a first-party Actro account service. Provider sign-in, OAuth or
system account refresh, and secure token storage remain platform/provider
responsibilities. Device membership is cloud data: a device writes
`devices/<device-id>.json` and participates through `manifest.json`; revocation
disables or removes that device in the manifest or disconnects provider access.
No Actro product server allocates device identity.

## Cloud Data Shape

The cloud folder should be a provider-owned Actro app directory, not a public
web endpoint. The first durable layout should be explicit and append-friendly:

```text
Actro/
  BrowserSync/
    manifest.json
    devices/
      <device-id>.json
    logs/
      <collection>/
        <device-id>-<sequence>.json
    snapshots/
      <collection>.json
```

`manifest.json` records the sync format version, active devices, known
snapshots, and compacted log ranges. Device files describe per-device identity
and capability metadata. Log files contain normalized record batches. Snapshot
files are compacted collection views used to bound startup and pull cost.
The Rust helpers `browser_storage_sync_cloud_default_manifest_json`,
`normalize_browser_storage_sync_cloud_manifest_json`,
`normalize_browser_storage_sync_cloud_device_json`,
`browser_storage_sync_cloud_log_object_json`, and
`browser_storage_sync_cloud_snapshot_object_json` are the source of truth for
those bytes. `browser_storage_sync_cloud_push_plan_json` is the source of truth
for push write ordering and manifest head updates; provider adapters should not
invent parallel document shapes or write-order rules.
`browser_storage_sync_cloud_store_capability_preflight_json` is the source of
truth for provider readiness gates: adapters should not independently decide
whether missing optimistic revisions, an incorrect Actro root folder, small
object limits, or list/read/delete emulation can safely proceed.
`browser_storage_sync_cloud_membership_plan_json` is the source of truth for
cloud device membership join/revoke writes: provider adapters should not
independently mutate manifest membership rows, revive revoked devices, or update
device metadata documents out of order.
`browser_storage_sync_cloud_membership_result_plan_json` is the source of truth
for membership write receipt classification: provider adapters should not treat a
device join/revoke as committed after partial writes or conflict errors without
first rereading membership documents and rebuilding the Rust plan.
Compatibility-blocked membership results must preserve the Rust rollout/playbook/
recovery snapshot bundle instead of inventing a separate connect/revoke recovery
path.
`browser_storage_sync_cloud_local_folder_recovery_snapshot_round_json` is the
source of truth for local-folder recovery snapshot sequencing and
`BrowserStorageSyncCloudRecoverySnapshotRoundSummary` diagnostics: provider
adapters should not independently choose recovery snapshot paths, summarize
source/snapshot copy outcomes, or decide whether migration/repair may continue
after conflicts, missing sources, auth failures, or quota failures.
`browser_storage_sync_cloud_push_result_plan_json` is the source of truth for
turning provider write receipts/errors into local ack or reread/retry decisions;
provider adapters should not mark local dirty rows synced after partial or
conflicted cloud writes.
`browser_storage_sync_cloud_pull_plan_json` is the source of truth for pull read
selection: platform/provider adapters should not independently decide when a
collection needs a snapshot read or log listing.
`browser_storage_sync_cloud_pull_continuation_plan_json` is the source of truth
for turning provider `nextCursor` values into follow-up list operations;
provider adapters should not independently decide continuation cursor shape or
operation metadata.
`browser_storage_sync_cloud_pull_apply_plan_json` is the source of truth for
turning the resulting snapshot/log bytes into a remote apply batch; provider
adapters should not independently sort cloud logs or merge cloud cursors.
`browser_storage_sync_cloud_compaction_plan_json` is the source of truth for
snapshot/log retention planning: provider adapters should not independently
decide compaction thresholds, delete old logs, or advance manifest
`snapshotRevision` values.
`browser_storage_sync_cloud_compaction_result_plan_json` is the source of truth
for compaction write/delete result handling: provider adapters should not
independently decide whether a partial compaction requires reread/retry or
whether failed old-log deletion is only cleanup retry.
`browser_storage_sync_cloud_local_folder_compaction_round_json` is the source
of truth for local-folder compaction sequencing: platform tests and local-folder
provider prototypes should not reimplement manifest/snapshot/log reads, write
ordering, or post-manifest log cleanup in Swift, Kotlin, or C#.
`browser_storage_sync_cloud_high_watermark_policy_json` and
`browser_storage_sync_cloud_high_watermark_plan_json` are the source of truth for
collection currency and cloud log cleanup status: provider adapters should not
independently decide that a local cursor is current, a required log gap is safe,
or a covered old log can be deleted outside the Rust retention plan.
`browser_storage_sync_cloud_cleanup_result_plan_json` is the source of truth for
high-watermark cleanup delete result handling: provider adapters should not
independently decide that `notFound` is success, that a conflict can be retried
without rereading retention state, or that a transient cleanup failure should
retry anything other than Rust-selected `candidateDeleteLog` operations.
`browser_storage_sync_cloud_local_folder_cleanup_round_json` is the source of
truth for local-folder high-watermark cleanup sequencing: platform tests and
local-folder provider prototypes should not reimplement compatibility preflight,
manifest/snapshot/log reads, candidate deletion, or cleanup result
classification in Swift, Kotlin, or C#. Its
`BrowserStorageSyncCloudCleanupRoundSummary` is also the source of truth for
cleanup read/delete counts, planned delete counts, retained-log and collection
currency counts, cleanup completion/retry state, reasons, deleted paths, and
issue diagnostics.
`browser_storage_sync_cloud_compatibility_status_json` is the source of truth for
cloud-file compatibility gates: provider adapters should not independently
decide whether an older file can be migrated, a newer file requires a client
upgrade, or an invalid document needs recovery.
`browser_storage_sync_cloud_compatibility_preflight_json` is the source of truth
for aggregate provider-read preflight: provider adapters should not
independently decide whether a mixed batch may continue or which blocked file
class takes priority.
`browser_storage_sync_cloud_schema_rollout_plan_json` is the source of truth for
client/cloud-file rollout policy: provider adapters should not independently
open N-1 windows, decide upgrade-versus-migration priority, or resume read/write
work after an incompatible preflight.
`browser_storage_sync_cloud_schema_migration_playbook_json` is the source of
truth for explicit cloud-file migration and repair preparation: provider
adapters should not independently skip write-freeze, backup/snapshot, or
post-migration compatibility validation steps.
`browser_storage_sync_cloud_recovery_snapshot_plan_json` is the source of truth
for the recovery snapshot copy batch: provider adapters should not independently
choose which incompatible cloud files to copy, invent recovery paths, or split
that planning into per-file FFI calls.
`browser_storage_sync_cloud_recovery_snapshot_result_plan_json` is the source of
truth for recovery snapshot write receipt classification: provider adapters
should not independently decide that migration or repair may continue after
partial `writeRecoveryCopy` success, conflicts, missing sources, auth failures,
or quota failures.

The exact file names can evolve, but the constraints should not:

- No central Actro server is required to allocate revisions.
- No Actro browser app starts or embeds a local sync service.
- Device-generated operations must have stable IDs, monotonic per-device
  sequence numbers, and deterministic merge ordering.
- Provider revision metadata is used for optimistic retries, not as the
  product data model.
- Cloud conflicts are resolved by rereading, merging through Rust, and writing a
  new manifest/log/snapshot plan.

Sharing a data structure and syncing files is necessary, but not sufficient.
Rust still owns deterministic sync semantics that cloud drives do not provide:
history visit aggregation, URL canonicalization, bookmark folder cycle
prevention, bookmark restore placement, bookmark ownership/source resolution,
session tab ordering, session active-index consistency, history/session retention pruning,
delete/restore conflict handling, per-device
high-watermarks, high-watermark retention status, and log/snapshot compaction.
Cloud storage moves bytes; Rust decides what those bytes mean after concurrent
edits.
The first persisted/sync address canonicalization policy now lives in Rust:
`browser_storage_address_canonicalization_policy_json()` describes accepted
external/internal/session address forms, and
`browser_storage_canonicalize_addresses_json()` canonicalizes address batches
for storage/sync normalization. This is deliberately a batch data boundary, not
a per-navigation FFI surface; native address bars still own user input parsing,
search expansion, and WebView navigation. Rust also strips username/password
userinfo from persisted/synced external addresses and exposes
`credentialStripped` diagnostics so browser storage does not leak credentials
through history, bookmark, session, or sync files. The batch result also returns
`canonicalizationSummary` counts and address groups for accepted/rejected,
external/internal/session, credential-stripped, unsupported-scheme, invalid URL,
empty, and unique canonical address outcomes so product diagnostics do not
rescan result rows per platform.
The first bookmark folder tree integrity policy also lives in Rust:
`browser_storage_bookmark_folder_tree_integrity_policy_json()` describes the
active folder graph rules, and
`browser_storage_bookmark_folder_tree_integrity_plan_json()` validates folder
arrays, current/candidate move batches, or sync-record batches in one call. The
plan treats self-parent as no parent, ignores deleted folders in the active
graph, tolerates missing parents for partial provider batches, and returns
`cycleDetected` / `rejectFolderMove` issues before native SQLite apply. Each
normalized folder also carries `integrityAction` and `integrityReason`
diagnostics for active graph membership, partial-parent references, preserved
tombstones, or rejected cycle members. The plan also returns
`integritySummary` counts and id groups for active graph folders, roots,
partial parents, tombstones, rejected cycle members, cycle issues, and duplicate
folder-id overlays so product diagnostics do not rescan normalized folders per
platform.
The first bookmark restore-placement policy builds on that folder graph:
`browser_storage_bookmark_restore_placement_policy_json()` documents the
data-layer placement rules, and
`browser_storage_bookmark_restore_placement_plan_json()` accepts bookmarks plus
optional folders/current-candidate/sync-record batches in one call. It overlays
duplicate bookmark ids with a deterministic delete/restore state timestamp,
detects restores from tombstones, exposes `stateConflictAction` and
`stateConflicts` diagnostics for stale tombstones or restores, returns
`stateConflictSummary` counts and winner-id groups for product conflict
surfaces, returns `placementReason` and `stateConflictReason` on each normalized bookmark,
resolves bookmarks pointing at deleted/rejected/missing folders back to root
(`resolvedFolderId = null`), and
leaves bookmark list disclosure, selection, drag/drop affordances, and conflict
presentation platform/product-owned.
The first bookmark ownership/source-resolution policy stays at the same batched
data boundary: `browser_storage_bookmark_ownership_policy_json()` documents
same-address source precedence, and
`browser_storage_bookmark_ownership_plan_json()` accepts bookmark arrays,
current/candidate batches, or sync-record batches in one call. It resolves
active `user` bookmarks ahead of tab-derived `tab` bookmarks, preserves tab
unstar/delete tombstones without hiding an active user-owned bookmark for the
same address, and returns `retentionReason` plus `suppressedBookmarkIds`
diagnostics plus `ownershipSummary` counts and winner/suppressed/tombstone id
groups for deterministic same-address source decisions. It still leaves v7
multi-source schema, source labels, SQLite writes, and cross-device conflict UI
platform/product-owned.
The first history visit aggregation policy now lives in Rust too:
`browser_storage_history_visit_aggregation_policy_json()` describes how unique
visit events become page aggregates, and
`browser_storage_history_visit_aggregation_plan_json()` accepts visit arrays,
current/candidate batches, or sync-record batches in one call. It canonicalizes
addresses, deduplicates by visit id, keeps deleted visits out of page
aggregates, and derives `visitCount`, first/latest timestamps, titles, and
source visit ids before native SQLite apply. Normalized visits also return
`aggregationAction` and `aggregationReason` diagnostics for included active
visits or preserved tombstones, and the plan returns `aggregationSummary`
counts plus page/visit id groups so product diagnostics do not rederive
duplicate overlays, tombstones, or multi-visit pages per platform.
The first session tab ordering policy is also Rust-owned:
`browser_storage_session_tab_ordering_policy_json()` describes stable tab
identity ordering, and `browser_storage_session_tab_ordering_plan_json()` accepts
tab arrays, current/candidate batches, or sync-record batches in one call. It
canonicalizes tab addresses, overlays duplicate `(sessionId, tabId)` rows,
keeps deleted tabs out of active ordering, and rewrites active tab indexes to a
contiguous zero-based sequence before native SQLite apply. It returns
`orderingAction` and `orderingReason` diagnostics for already-contiguous,
renumbered, or tombstone rows plus `orderingSummary` counts and
ordered/renumbered/tombstone id groups so platforms do not rederive tab-order
explanations or status summaries.
Session tab stable-id fallback is Rust-owned as well:
`browser_storage_session_tab_id_plan_json()` takes a whole session's
`{id, index, address}` tabs in one batch and returns an index-aligned array of
canonical tab ids (non-empty ids pass through trimmed and capped at 128 chars;
empty ids derive a deterministic UUID-shaped id from the tab index and the
normalized address). All platform databases call it from `saveSession`/`loadSession`
instead of hashing locally, so the merge identity key `(scope, sessionId, tabId)`
never drifts across platforms. On top of that, every platform `saveSession`
now runs the whole snapshot through `browser_session_snapshot_normalize_json()`
and persists the Rust output verbatim (canonical addresses, title rules, tab
cap, active-index clamp, stable tab ids); the batched tab-id plan remains the
fallback when the normalizer resolves the snapshot to no persistable tabs.
The first session active-index consistency policy follows that ordering:
`browser_storage_session_active_tab_consistency_policy_json()` documents the
data-layer clamp rules, and
`browser_storage_session_active_tab_consistency_plan_json()` accepts sessions
plus optional tabs/current-candidate/sync-record batches in one call. It clamps
persisted `activeTabIndex` values into the ordered active tab range, returns an
`activeTabId` when one exists, returns `consistencyReason` diagnostics for
within-range, clamped, empty-session, or tombstone decisions, preserves session
tombstones for diagnostics, and returns `consistencySummary` counts plus
active/tombstone/empty/clamped session id groups and active tab ids so platforms
do not rederive restore-state summaries. Live foreground focus, WebView
activation, private windows, and shortcuts stay platform-owned.
The first history/session retention pruning policy now shares those batch-only
rules too: `browser_storage_retention_pruning_policy_json()` describes page and
session-tab limits, while `browser_storage_retention_pruning_plan_json()` accepts
history pages, history visits, session tabs, current/candidate batches, or sync
records in one call. It keeps the newest active history pages globally, can also
apply `maxHistoryPagesPerOriginDevice` windows using normalized `originDeviceId`
metadata from direct rows or sync records, marks overflow pages plus their active
visits as prune candidates, reuses Rust session-tab ordering before applying
per-session tab limits, can also apply `maxSessionTabsPerOriginDevice` windows
after ordering, preserves tombstones, and returns per-row `retentionReason`
diagnostics so platforms do not rederive whether a row was kept, tombstone
preserved, globally pruned, per-origin-device pruned, or visit-pruned with its
parent page. It also returns `retentionSummary` counts plus kept/pruned/
tombstone page, visit, and session-tab id groups, including duplicate-overlay
and origin-device pruning groups, so product diagnostics do not rescan
normalized records per platform. SQLite writes, provider IO, history grouping,
and session restore UX remain platform-owned.

## Migration From HTTP Cycle Work

The current HTTP sync-cycle reference work should be treated as reusable sync
core scaffolding, not as the final transport:

- Keep the Rust-owned browser storage schema, dirty-row export, normalized
  batch payloads, remote apply plans, ack plans, merge/conflict policies,
  scheduler status, and backoff policy.
- Reuse reference server tests where they prove merge behavior.
- Replace product-facing HTTP endpoint credentials with cloud-drive provider
  connections.
- Build on the Rust-owned cloud document helpers and cloud planning/result
	  handoffs to finish provider read execution loops, provider conflict retry
	  execution, and concrete provider adapters. Push write result
	  classification, local ack/reread decisions, snapshot compaction planning,
	  compaction write/delete result handling, the local-folder compaction round
	  runner, high-watermark retention status, cleanup result planning, and the
	  local-folder cleanup round runner already live in Rust. Recovery snapshot
	  round sequencing and `roundSummary` diagnostics for blocked migration/repair
	  paths also live in Rust. Cloud-file
  compatibility gates also live in Rust through
  `browser_storage_sync_cloud_compatibility_status_json()` and the batched
  provider preflight lives in
  `browser_storage_sync_cloud_compatibility_preflight_json()`. Schema rollout
  governance now also lives in Rust through
  `browser_storage_sync_cloud_schema_rollout_plan_json()`, which records the
  current strict schema window and upgrade/migration/repair actions before
  provider read/write work resumes. The matching migration playbook lives in
  `browser_storage_sync_cloud_schema_migration_playbook_json()`, which freezes
  normal sync writes, snapshots affected cloud files, blocks automatic migrated
  writes, and requires a fresh compatibility preflight before sync resumes. The in-memory
  cloud store, local-folder cloud store, and planned write/delete executor give
  provider adapters shared Rust fixtures for optimistic revision and
  error-mapping tests. The local-folder capability/preflight and compatibility
  preflight helpers now keep the file-backed provider declaration and real-file
  cloud-document gate in Rust, and the local-folder cycle JSON entry runs one
  complete local-folder provider cycle through real files in Rust
  without exposing a per-operation FFI surface. The push round runner now also keeps read/plan/write/
  result handoff sequencing plus `roundSummary` diagnostics in Rust for a
  single provider-backed push attempt.
  The pull round runner mirrors that for manifest/snapshot/log reads, Rust-owned
  `continueProviderLogListing` pagination, remote-apply planning, and the
  `roundSummary` diagnostics native UX/schedulers would otherwise rederive.
  The cleanup round runner does the same for high-watermark cleanup read/delete
  counts, planned cleanup delete counts, retained-log and collection currency
  counts, cleanup completion/retry state, reasons, deleted paths, and issue
  diagnostics.
  The cycle round runner combines one optional push round and one pull round,
  including the push-failure short-circuit policy, without taking over provider
  SDK calls or native SQLite apply execution. Its cycle `roundSummary` captures
  push/pull presence, client-step categories, failures, conflicts,
  imports/skips, affected collections, and diagnostic reasons; its client plan
  flattens ack/apply/retry steps so native adapters execute the same order after
  every cycle. Its cloud cycle status policy and schedule-state helpers classify
  round outcomes and derive native scheduler timing/provider recovery flags
  without reintroducing service/server terminology. Its provider
  diagnostics helper summarizes capability, compatibility, cycle status, and
  schedule state as one UI-consumable Rust decision without taking over UI copy
  or provider SDK calls. Its observability event helper turns diagnostics,
  attempt, and loop plans into one local/exportable event with redacted
  attributes separated from local-only identifiers, leaving telemetry transport
  and user consent to product/platform code. Its attempt-plan helper composes
  those three outputs into one provider-neutral next-step
  decision so native adapters do not reassemble retry, reread, stop, and
  recovery booleans independently. Its membership loop-plan helper returns
  membership `loopSummary` attempt/action counts for connect/revoke diagnostics.
  Its cycle loop-plan helper now adds bounded cross-round decisions,
  native-apply barriers, retry-backoff handoff, cycle `loopSummary`
  attempt/action counts, and stop/recovery classification over already-produced
  cycle rounds. Cloud pull
  continuation planning also lives in Rust through
  `browser_storage_sync_cloud_pull_continuation_plan_json`, and Rust-backed
  store runners execute continuation pages up to a safety cap with
  `paginationLimitExceeded` diagnostics. Concrete external provider adapters
  still own their SDK calls while adapting those list pages into the same store
  contract. Windows, Apple, and Android already consume the
  local-folder cycle entry as concrete native adapter paths, applying local ack
  and remote batches through native SQLite while Rust owns the
  capability-preflight/compatibility-preflight/round/status/schedule/diagnostics/
  observability/attempt/loop/client plans.
- Keep native provider adapters injectable so tests can use an in-memory drive
  store without Google/iCloud SDKs.
- Replace server-style cursor retention with cloud log/snapshot retention,
  manifest compaction, and cloud-file high-watermark planning.

## Open Decisions

- Whether iCloud uses iCloud Drive files, CloudKit records, or both.
- Whether Google Drive stores data in the appDataFolder or a visible Actro
  folder.
- Whether cloud sync is encrypted before upload, and where the user-controlled
  key comes from.
- Whether provider-specific quotas require lower product defaults than the first
  Rust retention pruning policy.
- How users disconnect a provider, rotate device identity, and resolve
  provider-level conflicts.
- Remaining URL canonicalization refinements beyond the first persisted/sync
  policy and credential-stripping rule.
- Bookmark multi-source schema, source labels, and delete/restore conflict UX
  beyond the first data-layer timestamp, restore-placement, and
  ownership/source-resolution policies.
