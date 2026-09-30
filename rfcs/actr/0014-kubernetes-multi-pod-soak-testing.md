# RFC-0014: actr Kubernetes multi-Pod soak testing

- Status: Proposed
- Date: 2026-09-04
- RFC PR: [Actrium/actrium#198](https://github.com/Actrium/actrium/pull/198)
- Tracking issue: [#14](https://github.com/Actrium/actrium/issues/14)
- Superseded by:
- Related: [#270](https://github.com/Actrium/actr/issues/270), [#363](https://github.com/Actrium/actr/issues/363), [#383](https://github.com/Actrium/actr/issues/383), [#426](https://github.com/Actrium/actr/issues/426), [#567](https://github.com/Actrium/actr/issues/567), [#581](https://github.com/Actrium/actr/issues/581), [#583](https://github.com/Actrium/actr/issues/583), [#608](https://github.com/Actrium/actr/issues/608), [#609](https://github.com/Actrium/actr/issues/609), [#610](https://github.com/Actrium/actr/issues/610), [#622](https://github.com/Actrium/actr/pull/622)

- Plan status: Proposed, serving as the implementation design and operational-validation baseline
- Code baseline: `main`; every run fixes and records the actual checked-out commit SHA
- RTC baseline: WebRTC 0.21, pinned by the `webrtc` and `rtc` 0.21.0 dependencies and implemented under `crates/actr-hyper/src/wire/webrtc/rtc/`
- Execution environment: GitHub Actions, a self-hosted Kubernetes cluster, and no physical devices
- Scope: actr RTC, signaling, recovery, business transport, and long-term resource stability

This RFC proposes a soak-test system; the component names, routes, metrics,
and deployment resources below describe planned work and do not claim that the
soak stack has already been implemented. The RFC metadata and final discussion
sections complete the repository RFC process.

## Test workflow

```text
Historical Issues / test failures
        -> event catalog and fixed-seed plan
        -> GitHub Actions deploys the Kubernetes test environment
        -> single-Actr preflight / multi-Actr Endpoints establish
           bidirectional communication through Loran
        -> Dashboard starts a long-running soak test
        -> continuous business traffic plus event injection
             - mobile/network state -> NetworkEventHandle
             - RTC network fault   -> WebRTC 0.21 Runtime/UDP Socket layer
             - signaling fault     -> signaling transport layer
        -> real Controller / Session / DataChannel path executes
        -> logs, state, and metrics are collected
        -> Result Evaluator + AI classify the failure
        -> Issue draft, replay seed, and analysis are produced
```

## Terminology and naming

The test-side process is named **Actr Soak Endpoint**. Its planned binary,
image, and Kubernetes application name is `actr-soak-endpoint`. This RFC uses
that name consistently; it does not define aliases or a migration from an
older soak implementation.

The canonical identity of each independent endpoint remains `actr-0`,
`actr-1`, ..., `actr-N`. Each identity maps to one Kubernetes Pod and one Actr
RTC process. From the perspective of one endpoint, another endpoint is a
**Peer** (for example, `actr-1` is a Peer of `actr-0`). `peer-0`/`peer-1` may be
used as display aliases, but they are not stable Actr identities and MUST NOT
replace the `actr-N` endpoint names in topology or credential configuration.

The terms below have one fixed meaning in this RFC:

| Term | Definition and use |
| --- | --- |
| **Actr Soak Endpoint** | One test Pod/process running a real Actr RTC endpoint and the test-only control adapter. Preferred implementation name: `actr-soak-endpoint`. |
| **Endpoint identity** | Stable topology name `actr-N`; it is separate from the ActrId, Pod IP, and Pod UID. |
| **Peer** | The remote endpoint at one connection edge. A Peer relationship is directional from the observer's point of view. |
| **Endpoint pool** | The fixed-size set of Endpoints deployed by CI with no baked Peer graph; pool members are leased to runs. |
| **Peer wiring** | Runtime establishment and teardown of Peer edges by the SoakController through the peer-management API; the wired graph is recorded in `plan.json`. |
| **Lease** | The exclusive assignment of a pool subset to one run; disjoint leases MAY run concurrently. |
| **ActrNode** | The production Actr node hosted inside an Actr Soak Endpoint. |
| **Loran** | The current name in this repository for the signaling/STUN/TURN/AIS service suite historically called Actrix in related actr issues; it is not an endpoint or test controller. |
| **Soak Dashboard** | The UI/API and persistent run store. It owns manual test runs and embeds the SoakController. |
| **SoakController** | The deterministic planner/executor that expands a seed into actions, calls endpoints, collects evidence, and invokes the Result Evaluator. |
| **Endpoint Control API** | Test-namespace HTTP adapter exposed by each Actr Soak Endpoint. It accepts only predefined, authenticated actions and observations. |
| **`soak-kubectl`** | Helper script bundled only in the Dashboard image. It invokes the in-cluster `kubectl` with the Dashboard's projected ServiceAccount token and fixed API-server settings for namespaced Kubernetes evidence/actions; it is not a separate collector service. The Controller CLI Job is a Dashboard API client and does not invoke kubectl. |
| **Seed** | The recorded random seed from which the event plan and action order are reproduced. |
| **Event plan** | The concrete, timestamped actions generated from a seed, including business probes and injected events. |
| **Lifecycle event** | A logical mobile fact such as Background, Foreground, Available, or Unavailable delivered through `NetworkEventHandle`; it is not a real device operation. |
| **Fault** | A test-only, scoped failure rule at an RTC or signaling seam, such as drop, delay, or blackhole. |
| **Business probe** | A request/response or DataChunk operation sent through the formal Actr business path. |
| **Snapshot / Hook** | Read-only state snapshots and state-transition callbacks exported for test observation. |
| **Result Evaluator** | The planned deterministic pass/fail evaluator for business correctness, recovery, fencing, isolation, and resource convergence. |
| **Artifact** | Immutable run evidence: plan, timeline, logs, snapshots, metrics, Result Evaluator output, and replay script. |
| **Preflight** | A short deployment and cross-Pod readiness run that validates images, credentials, Loran connectivity, and one business exchange before a long soak. |
| **Resource convergence** | The requirement that pending work, sessions, lanes, file descriptors, sockets, RSS, and CPU return to a stable envelope after repeated recovery instead of growing without bound. |

## CI and Dashboard responsibilities

The proposed operating model is “CI deploys; Dashboard controls the test”:

| Component | Responsibilities | Explicitly does not do |
| --- | --- | --- |
| GitHub Actions / CI | Pin the SHA; build and push Loran, Endpoint, standalone Controller CLI, and Dashboard images; deploy the namespace, Secrets, RBAC, NetworkPolicy, Loran, the unwired endpoint pool (`actr-0` ... `actr-N`), and Dashboard; wait for Pod and Dashboard health | Start a Controller CLI Job in the deployment workflow, generate a Dashboard run plan, wire Peer edges, inject Faults, or start the soak business loop |
| `soak-dashboard` | Serve the UI/API; preview and start plans; lease pool subsets and wire/tear down Peer graphs; call endpoint control APIs; inject lifecycle/network/RTC/signaling events; collect metrics, logs, Result Evaluator output, and artifacts | Build images, publish releases, or receive `cluster-admin` |
| Actr endpoint (`actr-N`) | Run the real Actr RTC endpoint and test workload; establish signaling/ICE/DTLS/SCTP/DataChannel through Loran; expose control and observation APIs | Modify Controller-internal state directly or bypass `PeerGate` to operate a DataChannel |

The Dashboard will embed the SoakController, so a run will not require a CI Job.
CI only deploys the pool and MUST NOT start or control a soak run. CI and
Dashboard MUST NOT control the same endpoint set at the same time. Nightly and
Weekend profiles are submitted to the Dashboard queue (manually or by the
Dashboard scheduler) and hold the pool exclusively while they run.

The deployment workflow succeeds when the deployed Pods and Dashboard are
healthy. That health result is sufficient for GitHub Actions to report a
successful deployment; the workflow does not wait for a soak run, publish its
PASS/FAIL result to GitHub, or use it as a required GitHub check. Soak runs are
started and evaluated independently by the Dashboard. The `release` profile is
an operator-triggered pre-release validation run, not a GitHub deployment or
release gate.

Dashboard run state is persisted to its PVC. If the Dashboard Pod restarts, an
active run is marked `Interrupted` rather than resumed; unattended release runs
therefore need an external scheduler and recovery policy.

### Topology ownership and plan compatibility

CI owns the endpoint pool, not the topology. A deployment fixes only the pool
size, canonical Endpoint identities (`actr-0` ... `actr-N`), Actr credentials,
and images. Every Endpoint MUST start without a Peer graph; deployment
configuration MUST NOT encode Peer wiring.

The Dashboard owns the topology as plan data. Every plan carries an explicit
Peer graph in `plan.json`; at run start the embedded SoakController wires the
graph through the peer-management endpoints of the Endpoint Control API,
verifies connectivity, and tears the wiring down when the run ends. Changing
the topology shape (Pair to Multi-peer, ring to hub+leaf) is a plan change, not
a redeployment.

Plan compatibility reduces to pool fit: a plan is valid when its Endpoint count
fits the idle pool and its shape is supported by the SoakController. The
Dashboard MUST reject a plan that does not fit before it starts.

The Dashboard leases an exclusive subset of the pool to each run.
Fault/event-coverage profiles (pair-event, process-churn, multi-peer) MAY run
concurrently on disjoint subsets; resource-sensitive profiles (Nightly,
Weekend, release) MUST hold the entire pool exclusively. Runs that cannot be
placed wait in the Dashboard queue; the 23:00 CST scheduler also submits runs
to this queue. This replaces the earlier hard limit of one active run per
namespace.

Pool reuse hygiene is part of the contract:

- a healthy Endpoint returns to the idle pool only after the final quiescence
  check confirms pending work, Fault leases, and recovery obligations are zero;
- a failed run has a bounded recovery path: allow at most 30 seconds for
  endpoint cleanup and quiescence, then replace any unresponsive Endpoint Pod;
  wait at most 120 seconds for readiness, clear test seams, and verify a fresh
  baseline;
- an Endpoint that cannot be cleaned or replaced is quarantined and removed
  from the schedulable pool; clean Endpoints from the same failed run may be
  released after their wiring is removed, and the Dashboard updates usable pool
  capacity before admitting queued runs;
- every run begins with `/v1/actions/cleanup` on its leased Endpoints and
  samples a fresh resource baseline;
- Endpoint `ActrId`s and realm are provisioned for the pool and remain stable
  across Pod rebuilds. Exclusive leases prevent two runs from sharing an
  Endpoint; run-scoped Peer graphs are fully unwired before reuse, and the
  Dashboard verifies any available Loran allocation/registration drain gauges;
- the pool is rolling-rebuilt on a schedule and before operator-triggered
  `release` profile runs.

Resizing the pool still requires a new CI deployment. CI MUST NOT redeploy the
shared namespace while its Dashboard has an active run. The initial deployment
uses one Dashboard per namespace; a future multi-namespace Dashboard is a
separate feature and does not weaken this ownership boundary.

## Summary

CI builds and deploys real Loran, an unwired pool of Actr endpoints, and the
`soak-dashboard` into Kubernetes, but it does not
start a soak run. The
Dashboard is the interactive test owner: an operator previews or starts a
plan, the embedded SoakController wires the plan's Peer graph over a leased
pool subset, injects lifecycle/network/RTC/signaling events, and observes the
endpoints' bidirectional business traffic. The Dashboard's embedded
SoakController collects endpoint, Loran, and Kubernetes evidence and evaluates
the deterministic Result Evaluator. CI remains responsible for image provenance
and deployment health; it does not start, control, or schedule the soak run.
CI and Dashboard MUST NOT control the same endpoint pool concurrently.

## Motivation

The WebRTC 0.21 RTC implementation contains per-peer Controllers, real
WebRTC sessions, DataChannel lanes, signaling recovery, and network-event
recovery. Short integration tests cannot reliably expose leaks or ordering
races that appear only after repeated recovery, process replacement, or
multi-peer pressure. Historical examples include [ICE socket exhaustion
(Actrium/actr#270)](https://github.com/Actrium/actr/issues/270),
[background/reconnect races (Actrium/actr#363)](https://github.com/Actrium/actr/issues/363)
and [Actrium/actr#383](https://github.com/Actrium/actr/issues/383),
[silent peer death (Actrium/actr#583)](https://github.com/Actrium/actr/issues/583),
[stalled signaling (Actrium/actr#608)](https://github.com/Actrium/actr/issues/608),
[failed SDP delivery (Actrium/actr#609)](https://github.com/Actrium/actr/issues/609),
and [stale data-plane state (Actrium/actr#610)](https://github.com/Actrium/actr/issues/610).

This plan turns those regression themes into a reproducible long-running test in
the available environment: GitHub Actions, a self-hosted Kubernetes cluster,
and no physical mobile devices. It exercises server-side actr Endpoints while
publishing mobile lifecycle and network facts through the existing production
event entry point. It does not change the wire format or formal SDK semantics.

## Detailed design

### 1. One-sentence summary and complete flow

One sentence:

> CI builds and deploys real Loran, an unwired endpoint pool (`actr-0` ... `actr-N`), and the `soak-dashboard` into Kubernetes; the Dashboard then wires the plan's Peer graph over a leased pool subset, starts a fixed-seed soak run, continuously sends bidirectional business traffic, injects mobile events and RTC/signaling Faults, and uses endpoint state, business results, Loran metrics, and Kubernetes resource metrics to judge recovery correctness and long-term stability.

Complete flow:

```text
GitHub Actions / ARC Runner
  -> fix the main SHA and build/push Loran, Endpoint, standalone Controller CLI, and Dashboard images
  -> create/update the Kubernetes namespace, Secrets, RBAC, and NetworkPolicy
  -> deploy Loran, the unwired endpoint pool (actr-0 ... actr-N), Dashboard, and collectors
  -> wait for Pod/Dashboard health only; do not deploy/start a Controller Job or Dashboard soak plan

soak-dashboard (operator/API)
  -> preview or start a run and generate the event plan from a fixed seed
  -> lease a pool subset and wire the plan's Peer graph through the peer-management API
  -> embedded SoakController performs preflight and starts bidirectional traffic
  -> continuously send RPC, DataChunk, and fragmented business traffic
  -> inject lifecycle, network, RTC, and signaling events through the endpoint control API
  -> an operator or explicitly enabled lifecycle controller may perform SIGTERM, SIGKILL, Pod rebuild, and Loran restart
  -> collect Snapshots, Hooks, Fault hits, business results, Loran metrics, and K8s metrics
  -> Result Evaluator judges business, recovery, isolation, generation fencing, and resource convergence
  -> evaluate the run and stage the complete artifact bundle in the Dashboard PVC
  -> pass the deterministic redaction gate and export the redacted bundle to S3/MinIO
  -> pass the quiescence gate, tear down the wiring, and return healthy leased Endpoints to the pool
  -> quarantine or rebuild any Endpoint that cannot be returned cleanly
  -> the operator retrieves the bundle and analyzes it directly in Claude
  -> after a human fix, replay with the original seed
```

Run times:

- Nightly: 23:00 to 07:00 China Standard Time, continuously for 8 hours;
- Weekend: starts at 23:00 on Friday and runs for 48 hours;
- Before a release: add 8--24 hours according to resource capacity; automatic retries are forbidden.

### 2. Test goals and boundaries

#### 2.1 Goals

- Verify that actr Endpoints in multiple Kubernetes Pods can complete bidirectional RTC business through real Loran;
- verify that background, foreground, logical offline, network recovery, and network-type events drive production recovery;
- verify Soft/Hard Recovery after RTC UDP Faults, signaling Faults, and in-flight requests;
- verify that one Peer failure does not block signaling, business, or the Controller of other Peers;
- verify `pc_generation`, `transport_epoch`, `revision`, `role`, `exchange`, and stale fencing;
- verify process restart, same-identity Pod rebuild, and Loran restart;
- verify that pending work, sessions, lanes, file descriptors, sockets, RSS, and CPU do not grow continuously during long runs;
- form a replayable, auditable test record; CI builds and deploys the
  environment, while the Dashboard starts and controls the test.

#### 2.2 Non-goals

- This does not claim to replace real Android/iOS process freezing, system network switching, or wireless-baseband behavior;
- the test Fault Registry is not a complete simulation of the Linux kernel, CNI, NAT, MTU, or carrier networks;
- formal SDK business semantics are not changed during the test;
- AI cannot change PASS/FAIL, relax thresholds, retry automatically, or directly fix production code;
- Kubernetes liveness is not treated as proof of successful RTC recovery.

#### 2.3 Four invariants

1. RPC, DataChunk, and fragmented business traffic MUST pass through the real actr RTC data plane;
2. lifecycle events MUST enter recovery through the production `NetworkEventHandle`;
3. Fault injection MUST be available only through explicit test builds and the test control plane;
4. PASS/FAIL MUST be decided by a deterministic Result Evaluator; AI only produces auxiliary analysis.

### 3. Overall architecture

#### 3.1 Simplified topology

```text
GitHub Actions
      |
      v
  build/deploy only
      |
      v
+------------------+       endpoint control API       +----------------+
| soak-dashboard   | --------------------------------> | actr-0         |
| UI + BFF +       |                                   | Actr + ctl    |
| embedded         |                                   +-------|--------+
| SoakController   |                                           |
+------------------+                                           | real RTC
      |                                                        |
      +---------------- endpoint control API ----------------> +-------v--------+
                                                               | actr-1 ... N  |
                                                               | Actr + ctl    |
                                                               +-------|--------+
                                                                       |
                                                                       v
                                                               +---------------+
                                                               | Loran        |
                                                               | WS/STUN/TURN  |
                                                               +---------------+

Kubernetes / cAdvisor / Prometheus / procfs collector
  `-> Pod, process, CPU, RSS, FD, Socket, restart, OOM, and Loran metrics
```

The diagram shows logical roles, not a fixed Peer graph. CI deploys the
endpoints as an unwired pool; the SoakController wires the plan's Peer graph at
run start through the peer-management API and tears it down at run end. Two
endpoints therefore exchange real RTC traffic only when a run has wired an
edge between them.

#### 3.2 Endpoint Pod shape

The control adapter inside each `actr-N` Pod needs access to the local Node's
`NetworkEventHandle`, RTC observation, and test Fault Registry. Therefore the
recommended shape is to run the endpoint control adapter and `ActrNode` in the
same test process:

```text
actr-N Pod
  `-- actr-soak-endpoint [test-only soak build]
      |-- ActrNode
      |-- production RuntimeContext / PeerGate / RTC Manager
      |-- endpoint control API
      |-- test business Probe Driver
      |-- test Fault Adapter
      `-- Snapshot / Hook / Metrics Adapter
```

The Collector may be a sidecar. The endpoint control adapter is not
recommended as a separate process because a separate process cannot hold the in-process
`RuntimeContext`, Manager, or Fault Registry.

#### 3.3 Control, business, and observation planes

```text
Control plane:
soak-dashboard / embedded SoakController -> endpoint control API

Event plane:
actr-N control adapter -> NetworkEventHandle -> RecoverySupervisor -> RtcPeerController

Business plane:
Test Workload -> RuntimeContext -> PeerGate -> PeerTransport
  -> RtcConnectionManager -> RtcPeerController
  -> PeerConnectionSession -> WebRTC DataChannel -> remote Handler

Fault plane:
actr-N control adapter -> test-only Fault Registry
  -> PeerConnectionFactory Runtime/UDP Socket seam

Observation plane:
Endpoint Manager Snapshot + Hook + Fault + business metrics
  -> authenticated Endpoint API -> embedded Dashboard SoakController / Result Evaluator
Loran, Kubernetes, procfs, and log evidence
  -> Collector -> Dashboard
Kubernetes objects, events, and Pod logs
  -> Dashboard's `soak-kubectl` helper -> embedded SoakController
```

CI is a deployment plane, not a business-control plane. It builds/pushes images,
applies Kubernetes resources, and waits for health; it does not inject events
or start a soak plan in the Dashboard-controlled topology. The Dashboard's
SoakController MUST NOT write Controller-internal state directly and MUST NOT
bypass `PeerGate` to operate a DataChannel directly.

### 4. Production path and test-reuse boundary

#### 4.1 Production business path

```text
RuntimeContext
  -> Gate::Peer
  -> PeerGate
  -> PeerTransport
  -> RtcConnectionManager
  -> RtcPeerController
  -> PeerConnectionSession
  -> webrtc-rs PeerConnection / DataChannel
  -> remote InboundRouter / business Handler
```

`RuntimeContext` is the context for each business call; it is not manually
fabricated by the Controller. The `actr-N` endpoint process SHOULD start a test workload so
actr creates the context normally, then execute `call`, `tell`, or
`send_data_chunk`.

`PeerGate` is the remote-send gateway. `RuntimeContext` selects `PeerGate` for
`Dest::Peer`; `PeerGate` serializes, sends, and manages pending requests.

#### 4.2 Test Fault insertion point

For a test build, `PeerConnectionFactory` wraps the real Runtime when building
each new generation:

```text
RtcRuntimeConfig::runtime()
  -> webrtc-rs default Runtime
  -> test-utils may wrap it as ScopedFaultRuntime
  -> PeerConnectionBuilder.with_runtime(...)
  -> real PeerConnection
```

`DirectedRtcFaultRegistry` therefore does not replace `PeerGate`, Manager,
Controller, or Session. It applies blackhole, drop, delay, or deterministic-loss
rules at the real UDP Socket boundary.

#### 4.3 Coverage conclusion

| Verification | Passes through production path |
| --- | --- |
| RPC/DataChunk, Offer/Answer, ICE, DTLS, SCTP, DataChannel | Yes |
| Controller recovery, sendability, and generation | Yes |
| Background/Foreground and network facts | Yes |
| Directed A-to-B UDP Fault | Production path plus test Runtime wrapper |
| Offer/Answer/Candidate Fault | Production signaling path plus test transport decorator |
| Pod/CNI/node/process Fault | Production path plus external Kubernetes Fault |

Test Faults can cover production RTC control logic, but do not alone prove
real CNI, NAT, MTU, kernel-buffer, or carrier-network behavior. Nightly runs
start with deterministic Faults; Kubernetes external-network profiles are added
later.

### 5. Design approach and implementation plan

#### 5.1 Design approach

##### Production path first

The test only produces business traffic, lifecycle facts, and controlled
Faults; it does not reimplement the RTC protocol. Business traffic MUST enter
`RuntimeContext`, then pass through `PeerGate`, `RtcPeerController`, and a real
DataChannel. Recovery MUST be completed by the production
`RecoverySupervisor` and Controller.

##### Separation of three planes

```text
Control plane:     soak-dashboard / embedded SoakController -> endpoint control API
Business plane:    Test Workload -> RuntimeContext -> PeerGate -> RTC
Observation plane: Snapshot/Hook/Metrics -> Collector -> Result Evaluator
```

The control plane sends actions, the business plane runs real traffic, and the
observation plane only reads state. This prevents direct Controller mutation
and distinguishes “the command was accepted” from “business recovered”.

##### Faults by boundary

- mobile and network facts enter production recovery through `NetworkEventHandle`;
- RTC UDP and signaling-message Faults are injected precisely through test-only transport/Runtime seams;
- Pod, process, node, and CNI Faults are executed by Kubernetes;
- Nightly first uses deterministic Faults, then uses external Kubernetes Faults for real-environment comparison.

##### Determinism and replay

Every run fixes `run_id`, main SHA, image digest, seed, and event plan. Every
action carries `action_id` and `expected_generation`; the endpoint control API is idempotent,
and a failed run can be replayed with the same `plan.json`.

##### Closed-loop judgment

Every cycle follows “inject action -> send business -> wait for state -> check
result -> record evidence”. The Result Evaluator deterministically decides PASS/FAIL;
AI only reads evidence and provides initial attribution.

#### 5.2 Implementation plan

| Module | Implementation | Responsibilities |
| --- | --- | --- |
| Actr Endpoint (`actr-N`) | Standalone `Actrium/actr-soak` crate, built with the `real-rtc` feature; links the pinned `Actrium/actrium` `actr-hyper` dependency with its `test-utils` feature | `actr-soak-endpoint` hosts the production ActrNode adapter in `src/node_endpoint.rs` and starts it through `src/node_runtime.rs`, holding `NetworkEventHandle`, Fault facades, and Snapshot/Hook observers |
| Test Workload | Test Actor/Probe Driver inside `actr-N` | Send RPC, Tell, and DataChunk through `RuntimeContext`; continuously produce bidirectional traffic |
| Soak Dashboard | Kubernetes Deployment | Serve the UI/BFF, persist run state, and expose authenticated manual control |
| Soak Controller | Embedded in Dashboard | Generate seed plans, call endpoint control APIs, schedule events, run the Result Evaluator, and archive artifacts |
| Standalone Controller CLI | Optional one-shot Job/image | Submit an explicitly invoked smoke/preflight or replay request through the authenticated Dashboard run API and report that run's status; it does not call Endpoint APIs, access the Kubernetes API, or execute a run itself |
| Pool lease manager | Dashboard module | Lease exclusive pool subsets to runs, queue runs that do not fit, and enforce profile-level concurrency (coverage profiles concurrent, resource profiles exclusive) |
| RTC/signaling Fault Adapter | Wrap production Runtime and signaling transport | Call `DirectedRtcFaultRegistry` and `DirectedSignalingFaultRegistry` without changing Controller/Session logic |
| Observation Adapter | Read Manager Snapshot, Hooks, and test counters | Convert results to endpoint JSON, event streams, and Prometheus metrics |
| Loran | Kubernetes StatefulSet | Provide real Signaling, STUN, TURN, and AIS; Endpoints use Service DNS |
| Collector | Sidecar or separate Deployment | Collect Loran and Kubernetes metrics, procfs, and logs. The embedded SoakController alone fetches Endpoint metrics through the authenticated Endpoint API on TCP 9090; the Collector does not scrape Endpoint Pods. |
| Result Evaluator | Planned deterministic Controller module | Check business correctness, recovery deadlines, Peer isolation, generation fencing, and resource trends |
| CI deploy workflow | GitHub Actions | Build/push immutable images, deploy the namespace and the endpoint pool, and wait for health; it MUST NOT start the test plan |

Endpoint control call relationships:

```text
POST /v1/probes/rpc
  -> Test Workload
  -> RuntimeContext.call/tell
  -> PeerGate
  -> RtcConnectionManager / RtcPeerController
  -> PeerConnectionSession / DataChannel

POST /v1/events/lifecycle
  -> NetworkEventHandle
  -> RecoverySupervisor
  -> RtcPeerController

POST /v1/faults/rtc or /v1/faults/signaling
  -> test-only Fault Adapter
  -> production Runtime/signaling transport seam

POST /v1/peers or DELETE /v1/peers
  -> ActrNode peer establishment or teardown
  -> Peer Snapshot confirms the wired edge

GET /v1/peers, /v1/events, /metrics (same Endpoint HTTP listener, TCP 9090)
  -> Snapshot / Hook / Metrics Adapter
  -> Dashboard's embedded SoakController / Result Evaluator
```

The `/metrics` and `/v1/metrics` routes share the Endpoint Control API's TCP
9090 listener. The embedded SoakController scrapes `/metrics` through that
listener using the configured Endpoint bearer token; the Endpoint does not
serve metrics on a separate TCP 9091 port.

Implementation repositories and deliverables:

```text
Actrium/actr-soak (standalone repository; Cargo package `actr-soak`)
  src/bin/actr-soak-endpoint.rs
  src/bin/actr-soak-controller.rs
  src/node_endpoint.rs
  src/node_runtime.rs
  src/endpoint_api.rs
  deploy/k8s/
  deploy/docker/endpoint.Dockerfile
  deploy/docker/controller.Dockerfile

Actrium/actrium (production dependency, pinned by actr-soak)
  crates/actr-hyper/ (`test-utils` feature supplies the test-only RTC seams)
```

If a separate endpoint process needs current crate-private observation, add a
test-only/soak facade. It exports only read-only Snapshot, Hook, Fault, and
resource observation and does not change formal recovery, sendability,
signaling, or FFI semantics.

### 6. Endpoint Control API

#### 6.1 API positioning

The Endpoint Control API is a planned remote-control adapter in the test
namespace, hosted by `actr-soak-endpoint`. It is not the formal Actr SDK or a
production service API. It can call only predefined actions and cannot execute
shell, `eval`, or arbitrary memory mutation.

#### 6.2 API routes

| Method | Path | Purpose |
| --- | --- | --- |
| GET | `/livez` | `actr-N` process liveness |
| GET | `/readyz` | Node, ActrId, Loran, and test workload readiness |
| GET | `/v1/identity` | run, Pod, ActrId, pod UID, and process incarnation |
| GET | `/v1/peers` | Peer Snapshot and resource observation |
| POST | `/v1/peers` | Wire a Peer edge to a target endpoint (run-scoped, idempotent) |
| DELETE | `/v1/peers` | Tear down Peer wiring installed by a run |
| GET | `/v1/events` | Hook, state edges, Fault hits, and signaling trace |
| GET | `/metrics` | Prometheus text metrics on the same Endpoint HTTP listener as the control API (TCP 9090); bearer token required in Kubernetes |
| GET | `/v1/metrics` | Optional JSON metric snapshot |
| GET | `/v1/resources` | Process resource snapshot (RSS, FDs, sockets) |
| GET | `/v1/diagnostics` | Endpoint diagnostics snapshot |
| GET | `/v1/redaction-inventory?run_id=...` | Bearer-authenticated Dashboard-only response with transient literals for the inventory window opened by `/v1/actions/run-start`; reject any other run ID and never persist the response |
| GET | `/v1/runtime-thresholds` | Recovery timing values read from the linked Endpoint build so plan generation does not copy stale constants |
| POST | `/v1/observations/data-chunk` | Record a DataChunk receipt observation |
| POST | `/v1/probes/rpc` | Send a probe through the formal RPC business path |
| POST | `/v1/probes/data-chunk` | Send a probe through the formal DataChunk path |
| POST | `/v1/events/lifecycle` | Background/Foreground |
| POST | `/v1/events/network-path` | Available/Unavailable/transport |
| POST | `/v1/faults/rtc` | Install, update, or heal an RTC Fault |
| POST | `/v1/faults/rtc/prepare` | Prepare a two-sided RTC Fault transaction (run-scoped, idempotent) |
| POST | `/v1/faults/rtc/commit` | Commit a prepared RTC Fault transaction on both endpoints |
| POST | `/v1/faults/rtc/rollback` | Roll back a prepared RTC Fault transaction |
| POST | `/v1/faults/signaling` | Install, update, or heal a signaling Fault |
| POST | `/v1/actions/cleanup` | Call the production cleanup entry point |
| POST | `/v1/actions/run-start` | Begin a run-scoped credential-redaction inventory for credentials used or renewed during this run; retain only this run's literals in endpoint memory |
| POST | `/v1/actions/run-end` | Close this run's credential-redaction inventory and clear its in-memory credential literals after the redaction gate |
| POST | `/v1/actions/force-reconnect` | Call the production force-reconnect entry point |
| POST | `/v1/actions/turn-relay` | Opt this Endpoint into relay-only ICE policy for a run; default remains disabled |
| POST | `/v1/actions/turn-credential-rotation` | Trigger normal AIS credential renewal and report safe revision/expiry evidence |

Every write request carries `run_id`, `action_id`, `deadline_ms`, and
`expected_generation`; the endpoint control API makes `action_id` idempotent.
In a Kubernetes deployment, the Dashboard MUST configure a bearer token and
the Endpoint API MUST reject unauthenticated requests on every route except
`/livez` and `/readyz`. The redaction-inventory route additionally MUST require
that bearer token and the `run_id` whose inventory window was opened by
`/v1/actions/run-start`. It returns only that run's credentials and MUST reject
absent, different, or closed run IDs. The inventory window remains open through
artifact collection and the redaction scan, even after business execution has
finished. The Dashboard calls `/v1/actions/run-end` after the scan/export
decision; the Endpoint then clears the run's credential literals from memory.
Only the Dashboard receives the Endpoint Control API bearer token. A
standalone Controller CLI Job receives a separate, short-lived Dashboard API
credential scoped to submitting and observing its one requested run; it never
receives or forwards the Endpoint token.

Peer-wiring writes are run-scoped: a run MUST tear down every edge it wired
before its leased subset returns to the pool, and the SoakController MUST NOT
wire an edge to an endpoint outside its lease.

#### 6.3 Business probes

RPC:

```json
{
  "run_id": "nightly-001",
  "action_id": "rpc-1001",
  "target": "actr-1",
  "route_key": "soak.echo.request",
  "payload_base64": "...",
  "timeout_ms": 5000,
  "sequence": 1024,
  "hash": "sha256:..."
}
```

The test workload uses:

```text
RuntimeContext.call(Dest::Peer(peer_b), request, timeout_ms)
  -> PeerGate
  -> real WebRTC DataChannel
```

DataChunk requests use `stream_id`, `sequence`, payload size, and hash to check
reordering, duplication, loss, and continued sending after recovery.

The API returns “action accepted/execution result”; an HTTP 200 is not treated
as RTC recovery. Recovery requires subsequent Snapshot and business-probe
confirmation.

#### 6.4 Lifecycle events

```json
{
  "run_id": "nightly-001",
  "action_id": "lifecycle-2001",
  "state": "foreground",
  "background_duration_ms": 30000
}
```

The `actr-N` control adapter calls the production entry point:

```text
NetworkEventHandle.handle_app_lifecycle_changed(...)
  -> NetworkEventProcessor
  -> RecoverySupervisor
  -> RtcPeerController
```

Network-path events use `NetworkSnapshot` Available, Unavailable, and
transport flags. The first phase does not change the Pod IP; it focuses on the
production path from a logical network fact to ICE/recovery.

#### 6.5 Security

- The endpoint control API binds only to a test Service or Pod network;
- NetworkPolicy permits only the Dashboard to access the endpoint control port;
- Kubernetes deployments MUST set a non-empty bearer token; `/livez` and `/readyz` are the only unauthenticated routes. mTLS MAY be added as defense in depth;
- the endpoint process has no cluster-admin credentials;
- reject shell, arbitrary URLs, dynamic Rust code, or arbitrary protobuf types;
- never emit credential literals in ordinary Endpoint responses, metrics,
  logs, or artifacts; the sole exception is the authenticated, run-scoped
  redaction inventory returned directly to the Dashboard and cleared after
  scanning. Never expose private keys or complete business payloads;
- production images do not contain the endpoint control API, `test-utils`, or the Fault Registry.

### 7. Event and Fault design

#### 7.1 Mobile and network events

| Event | Injection | Main verification |
| --- | --- | --- |
| Background | `NetworkEventHandle` | Sending and recovery strategy after entering background |
| Foreground, 1 second | `NetworkEventHandle` | Short background |
| Foreground, 29.999 seconds | `NetworkEventHandle` | Just before the 30-second boundary |
| Foreground, 30 seconds | `NetworkEventHandle` | Long-background threshold boundary |
| Foreground, 60 seconds | `NetworkEventHandle` | Recovery-timeout boundary |
| `NetworkUnavailable` | `NetworkSnapshot` | Logical offline and pending handling |
| `NetworkUnavailable -> Available` | `NetworkSnapshot` | ICE restart/recovery |
| `Unknown -> Available` | `NetworkSnapshot` | Wake-up on validated network |
| Wi-Fi <-> Cellular | Transport flags | Network-type change |
| `ForceReconnect` | `NetworkEventHandle` | Explicit cleanup and reconnect |
| `CleanupConnections` | `NetworkEventHandle` | Cleanup without automatic reconnect |
| Duplicate/out-of-order monitor events | Source epoch/sequence | Stale fencing and deduplication |

The long-background recovery threshold is read from each leased Endpoint's
`/v1/runtime-thresholds`. The Dashboard rejects mixed threshold values in one
lease and the plan generator includes threshold−1 ms, the threshold, and
threshold+1 ms in lifecycle and network startup cases. Replayed plans retain
the threshold recorded in the source run. The Controller MUST NOT copy a
potentially stale constant.

#### 7.2 Optional TURN relay and credential rotation

TURN URLs remain configured on the Endpoint pool, but relay-only policy is
opt-in per Dashboard run and defaults to disabled. With the option disabled,
the production ICE policy remains `All` and the run does not force TURN use or
rotate credentials. When enabled, the SoakController must, before wiring the
run's Peer graph:

1. call the authenticated `/v1/actions/turn-relay` control action on every
   leased Endpoint; the Endpoint retires any previous RTC sessions and applies
   relay-only policy to every subsequently built PeerConnection;
2. request credential renewal through the Node's normal AIS credential
   manager, then confirm that the TURN credential revision/expiry advanced
   without returning credential material to the Dashboard;
3. wire each Peer edge, run business probes, and wait for both endpoints'
   selected candidate-pair snapshots to report `Relayed`;
4. fail preflight if any selected path is direct or remains unknown at the
   bounded verification deadline; never count an enabled policy by itself as
   evidence of relay use;
5. on completion, failure, or cancellation, disable relay-only policy and
   retire the run's connections before returning the leased subset. If an
   Endpoint cannot be cleaned and confirmed, quarantine it.

The relay switch, credential rotation, selected-path evidence, and reset
outcome are recorded with the run's action IDs. TURN passwords and AId
credentials are never returned in ordinary action responses, metrics,
artifacts, or logs. The sole exception is the bearer-authenticated
`/v1/redaction-inventory?run_id=...` response to the Dashboard, scoped to the
inventory window opened for that run through `/v1/actions/run-start`. The
Endpoint rejects other run IDs; the Dashboard holds the returned literals in
memory only for scanning and never persists or exports them. After the scan,
the Dashboard calls `/v1/actions/run-end` and discards its in-memory copy.
Credential rotation is a run-scoped endpoint operation and does not restart
the shared Loran process.

#### 7.3 `DirectedRtcFaultRegistry`

The Registry binds a rule to:

```text
(local ActrId, remote ActrId, pc_generation)
```

Supported `PacketRule` values:

```text
Pass
Blackhole
DropEvery(n)
Delay(duration)
DeterministicLoss(percent, seed)
```

Initialization and use:

```text
PeerConnectionFactory.with_directed_faults(local_id, registry)
  -> each generation build calls registry.scoped_runtime(...)
  -> ScopedFaultRuntime
  -> ScopedFaultUdpSocket
  -> apply rules to real UDP traffic and record hits
```

Single-process tests can use `set_link_rule`:

```rust
registry.set_link_rule(
    (&peer_a, generation_a),
    (&peer_b, generation_b),
    LinkDirection::AToB,
    PacketRule::Blackhole,
);
```

Registries do not share memory across Pods, so A-to-B in a multi-Pod test
requires two endpoint control actions:

```text
actr-0: local=actr-0, remote=actr-1, set outgoing=Blackhole
actr-1: local=actr-1, remote=actr-0, set incoming=Blackhole
Controller: wait for installation acknowledgements from both endpoints and a
hit at the effective drop point, then run the recovery Result Evaluator
```

Hit expectations follow the effective drop point, not the side count. With
outgoing=Blackhole on A, A records a send hit without ever transmitting a
datagram, so B's incoming rule MUST NOT be expected to record a hit;
requiring both would fail a correctly injected Fault under §12.1 ("a declared
Fault records no hit"). Exercising outgoing and incoming blackholes as
separate actions is an acceptable alternative.

When healing, both sides return to `Pass`. A new generation does not implicitly
inherit an old generation's Fault.

#### 7.4 Fault Registry coverage boundary

Covered:

- real-PeerConnection UDP blackhole, delay, and deterministic loss;
- real DataChannel business failure after Offer/Answer;
- real Controller handling of socket failure, sendability, and recovery;
- generation-scoped stale Faults not contaminating a new connection.

Not covered alone:

- WebSocket signaling, which requires `DirectedSignalingFaultRegistry`;
- killed Pods, node drain, or OOM, which require Kubernetes;
- CNI, NAT, MTU, or kernel socket-buffer behavior, which require external network profiles;
- physical wireless networking and system backgrounding, which require real devices.

#### 7.5 Signaling Faults

Prioritize:

- drop the first `IceRestartOffer`;
- drop `IceRestartRequest`;
- delay or duplicate an Answer;
- drop a Candidate;
- duplicate `RoleAssignment`;
- disconnect signaling while RTC remains alive;
- recover sending after the SDP queue becomes full.

Fault class, direction, exchange, attempt, and ordinal are written to the event
trace for replay.

#### 7.6 Pod and Loran lifecycle

Kubernetes performs:

```text
normal deletion: grace-period=15
forced deletion: grace-period=0
same-identity Endpoint Pod rebuild
Loran Pod restart
Endpoint rolling restart
```

Every action records `pod_uid`, `pod_ip`, `node_name`, `ActrId`, process
incarnation, and `action_id`.

### 8. Fixed seed and random-event scheduling

#### 8.1 Seed composition

```text
run_seed
  |-- topology_seed
  |-- peer_seed
  |-- traffic_seed
  |-- event_seed
  |-- duration_seed
  |-- rtc_fault_seed
  |-- signaling_fault_seed
  `-- lifecycle_seed
```

Use an explicit reproducible PRNG. Before every action is generated, write the
plan to `plan.json`; do not depend on Pod return order, system time, or an
unrecorded random number. `topology_seed` drives the generation of the Peer
graph recorded in `plan.json` and wired at run start; the graph is plan data,
not a deployment constraint.

#### 8.2 Scheduling constraints

- Each Endpoint executes at most one destructive action at a time;
- the same Peer waits for the previous action to recover or time out before another action is injected;
- Fault installation and healing are paired;
- Pod deletion MUST NOT run concurrently with a Fault commit for the same Pod;
- every coverage window MUST cover the key event classes;
- a plan containing Loran-wide or shared-infrastructure actions (Loran
  restart, TURN credential rotation, or similar) MUST hold the whole pool
  exclusively, and profiles that MAY run concurrently on disjoint pool
  subsets MUST NOT include shared-infrastructure actions; realms isolate
  identities, not process restarts, and one run restarting the shared Loran
  would make another run report an unexpected exit or a non-target-Peer
  failure under §12.1;
- the event generator MAY be random, but the Result Evaluator and recovery deadlines MUST NOT be random;
- every action records target, direction, generation, start time, end time, and result.

#### 8.3 Recommended event weights

Initial weights are intended to balance coverage and can later be adjusted by
historical failure rate:

| Category | Initial weight |
| --- | ---: |
| Normal business window | 30% |
| Sustained traffic | 12% |
| Connection churn | 8% |
| In-flight business | 5% |
| Background/Foreground | 15% |
| NetworkUnavailable/Available | 5% |
| Wi-Fi/Cellular switch | 3% |
| Duplicate/out-of-order network monitor events | 2% |
| RTC Fault | 10% |
| Signaling Fault | 6% |
| Same-identity Endpoint restart | 3% |
| Loran restart | 1% (exclusive profiles only; concurrent profiles sample normal business instead) |
| External network Fault | 0% (deferred until its Kubernetes/CNI adapter exists; sample normal business instead) |

The final 1% slot is a Loran restart only for exclusive profiles; concurrent
profiles use it for normal business so a shared Loran action cannot disturb a
disjoint lease.

Run a coverage check every 30--60 minutes so random distribution does not
permanently miss rare events.

### 9. Topology and business load

#### 9.1 Topology profiles

Topology profiles are wiring recipes. The SoakController applies a profile over
its leased pool subset at run start through the peer-management API; profiles
do not require different deployments.

| Profile | Topology |
| --- | --- |
| `pair` | `actr-0` and `actr-1`, one Peer edge |
| `small` | `actr-0` ... `actr-N`, fixed ring plus random cross-Peers |
| `multi-peer` | `actr-0` Hub plus up to 9 Leaves; the Hub may have 9 active Peers in the 10-Endpoint profile, while each Leaf has 4--8 active Peers including its Hub edge and seeded cross-Peer edges |
| `restart` | `actr-0` ... `actr-N`, periodically rebuild identities |
| `relay` | Optional Dashboard run setting; when enabled all connections MUST use TURN relay and the controller verifies selected paths from endpoint diagnostics. Disabled by default. |

Do not fully interconnect 50 Endpoints; avoid O(N^2) connections dominating
test resources. Every profile MUST record its wired Peer graph in `plan.json`.

#### 9.2 Business model

Every payload contains:

```text
run_id, cycle_id, source, target, direction, sequence, request_id, payload_hash
```

| Business traffic | Initial frequency | Purpose |
| --- | ---: | --- |
| Bidirectional small RPC | 1 request/s per direction | Connectivity, latency, and response matching |
| Bidirectional DataChunk | 1 per 10s per direction | Stream lane and continued sending after recovery |
| 200 KiB fragmented RPC | 1 per 30s per direction | Fragmentation, reassembly, reordering, and budget |
| 1 MiB burst | Rotate every 5 min | Resource recovery after pressure |
| In-flight RPC | Randomly before an event | Timeout, cancellation, and recovery competition |

Business operations MAY fail within their deadline during a Fault, but MUST
NOT remain pending forever, produce cross-Peer responses, execute twice, or
remain permanently unusable after recovery.

### 10. Historical issue regression

| Issue/PR | Soak-test action |
| --- | --- |
| [#583](https://github.com/Actrium/actr/issues/583) | Remote Endpoint silent death and same-identity Pod recovery |
| [#608](https://github.com/Actrium/actr/issues/608) | Stalled Controller concurrent with other-Peer signaling |
| [#609](https://github.com/Actrium/actr/issues/609) | SDP queue full, send failure, and wake-up |
| [#610](https://github.com/Actrium/actr/issues/610) | Stale data-plane state is retired by recovery and later business traffic succeeds |
| [#604](https://github.com/Actrium/actr/issues/604) | Pair-contract concurrent recovery and repeated runs |
| [#492](https://github.com/Actrium/actr/issues/492) | Setup/send-admission timeout |
| [#270](https://github.com/Actrium/actr/issues/270) | Long-term ICE/UDP socket and FD trend |
| [#297](https://github.com/Actrium/actr/issues/297) | RTC timeout and WebSocket fallback |
| [#339](https://github.com/Actrium/actr/issues/339) | RPC timeout/cancellation and recovery |
| [#359](https://github.com/Actrium/actr/issues/359) | Stale DataChannel falsely reported as sendable |
| [#363](https://github.com/Actrium/actr/issues/363) | Answerer short background and restart request |
| [#383](https://github.com/Actrium/actr/issues/383) | Long background, signaling reconnect, and first foreground |
| [#403](https://github.com/Actrium/actr/issues/403) | Signaling socket supervisor ownership |
| [#426](https://github.com/Actrium/actr/issues/426) / [#622](https://github.com/Actrium/actr/pull/622) | Per-Peer recovery completion |
| [#436](https://github.com/Actrium/actr/issues/436) / [#508](https://github.com/Actrium/actr/issues/508) | ICE restart Offer/Request loss |
| [#465](https://github.com/Actrium/actr/issues/465) | TURN credential rotation |
| [#503](https://github.com/Actrium/actr/issues/503) | Self-healing after passive WebSocket close on tell/stream |
| [#567](https://github.com/Actrium/actr/issues/567) | Background stale UDP, rebind, and ICE restart |
| [#581](https://github.com/Actrium/actr/issues/581) | Broadcast lagged, close cleanup, and pending requests |
| [#594](https://github.com/Actrium/actr/pull/594) / [#596](https://github.com/Actrium/actr/pull/596) | Offerer/Answerer Soft Recovery |

### 11. Metrics, Snapshot, Hook, and Evidence

#### 11.1 Production observation sources

The production observation sources are distributed across layers; the soak
implementation will aggregate them into run evidence and endpoint metrics:

| Layer | Production capability | Planned soak use |
| --- | --- | --- |
| RTC Manager | peer count, per-Peer Snapshot, live lanes, inbound drops | Manager observation, mainly for test adapters |
| Snapshot | status, role, `pc_generation`, revision, sendability, network path, reason, signaling drops, mailbox overflows | State assertions |
| Hook | Connecting, Connected, Disconnected, signaling lifecycle | Edges and timelines |
| RTC Fault | outgoing/incoming hits | Verify that a Fault really took effect |
| Runtime Fault Probe | task, UDP Socket, TCP, DNS, sleep, and injected-Fault counts | Test-only diagnosis |
| Evidence | merged state, Hook, Fault, and signaling trace | Failure replay |
| Loran | system metrics, signaling queue metrics, and partial metrics endpoint | Loran health and load |
| Kubernetes | CPU, memory, restart, OOM, throttling, and Pod state | System-resource trend |

The plan therefore needs an endpoint aggregation layer and long-term trend
metrics, not a second implementation of all low-level observation.

#### 11.2 Current gaps

- There is no unified test-endpoint `/metrics` yet;
- the Probe Driver must count RPC/DataChunk success, latency, payload hashes, and duplicates;
- the endpoint observation adapter must correlate recovery attempts, duration, outcomes, and deadlines;
- the public Snapshot does not expose every internal reducer field, such as ICE generation; if required, use test-only observation/evidence rather than extending formal FFI;
- the deleted historical Actr-wide `total_outstanding_bytes` must not be treated as an existing metric;
- preflight MUST verify whether the target Loran image enables its exporter.

#### 11.3 Recommended endpoint metrics

Use `actr_soak_endpoint_` as the sole metric prefix for the planned endpoint
exporter. These names are new; they do not preserve a prefix from an existing
soak implementation. Topology labels and dashboards use `actr-0`, `actr-1`,
..., while `peer` means the remote side of a particular connection.

Counters:

```text
actr_soak_endpoint_rpc_sent_total
actr_soak_endpoint_rpc_success_total
actr_soak_endpoint_rpc_timeout_total
actr_soak_endpoint_data_chunk_sent_total
actr_soak_endpoint_data_chunk_hash_error_total
actr_soak_endpoint_recovery_total{kind,outcome}
actr_soak_endpoint_fault_hits_total{kind,direction}
actr_soak_endpoint_hook_events_total{event}
actr_soak_endpoint_actions_total{kind,result}
```

Gauges:

```text
actr_soak_endpoint_peer_count
actr_soak_endpoint_pending_requests
actr_soak_endpoint_active_lanes
actr_soak_endpoint_recovery_obligations
actr_soak_endpoint_fault_leases
```

Histograms:

```text
actr_soak_endpoint_rpc_latency_seconds
actr_soak_endpoint_recovery_duration_seconds
actr_soak_endpoint_reconnect_duration_seconds
```

#### 11.4 Difference between the three observation types

- Snapshot: answers “what is the current state?”;
- Hook: answers “in what order did state change?”;
- Metrics: answer “what are the counts, latency, and resource trends?”;
- Evidence: connects one action's plan, state, Hook, Fault, business, and K8s events into a replayable timeline.

#### 11.5 Resource collection

Kubernetes can directly collect Pod CPU, memory, restart, OOM, and throttling.
FDs, Sockets, threads, and RSS require one of:

- the `actr-N` process collecting procfs itself;
- a restricted sidecar reading the target process procfs with `shareProcessNamespace`; or
- a node-level Collector associating procfs data with the Pod/container.

Resource metrics MUST NOT be written into the actr business state machine.
Read-only observation and system collection remain separate from Fault control.

### 12. Result Evaluator and result judgment

#### 12.1 Immediate failures

- endpoint adapter, Actr, or Loran panic, hang, or unexpected exit;
- RPC/DataChunk exceeds its deadline;
- response request ID, source, target, sequence, or hash is wrong;
- a non-target Peer is affected by a Fault or recovery action;
- a stale generation modifies the current generation;
- a declared Fault records no hit;
- a Pod has unexpected OOMKilled, eviction, or restart;
- the Controller retries or violates action constraints;
- the artifact redaction gate fails, or a secret or unredacted payload is
  detected in an exported artifact (see §14.1).

#### 12.2 Business correctness

Every successful or allowed-to-fail request writes a result row:

```text
action_id, request_id, source, target, direction, sequence
sent_at, received_at, response_at, error, payload_hash
```

Judgment rules:

- in a no-Fault window, business success rate MUST meet the profile threshold;
- in a Fault window, failure within the deadline is allowed, but new business MUST succeed after recovery;
- one request MUST NOT execute twice;
- an old Peer's response MUST NOT enter a new Peer or generation;
- one Peer failure MUST NOT cause other Peers' business to fail for a long period.

#### 12.3 Recovery judgment

Soft Recovery:

```text
Peer enters Recovering or becomes unavailable
  -> expected signaling/ICE events complete
  -> sendability returns
  -> bidirectional RPC/DataChunk succeeds
```

Hard Recovery:

```text
old Session/lane retires
  -> new pc_generation is created
  -> Offer/Answer, ICE, DTLS, and SCTP complete
  -> sendability returns
  -> bidirectional business succeeds
```

The A and B generation numbers do not need to be equal; each side is checked
against its own baseline and expected monotonic relationship.

Recovery deadlines use runtime configuration and actual event time. The
Controller MUST NOT duplicate a fixed sleep. The current default recovery
budget is the initial threshold; a profile MAY explicitly override it.

#### 12.4 Resource convergence

After each recovery cycle, enter quiescence and check:

```text
pending_requests = 0, or only explicitly in-flight requests remain
old session/lane/Fault lease = 0
recovery obligation = 0
```

The final quiescence check is the normal pool handoff gate. Bound cleanup and
quiescence to 30 seconds. If a failed or cancelled run does not quiesce in that
interval, the Dashboard MUST heal run-scoped Fault leases, then delete and
recreate each affected Endpoint Pod using its existing ActrId. A replacement
returns to the idle pool only if it becomes Ready within 120 seconds, reports
the same provisioned `ActrId` and a fresh process incarnation, has no Peer
wiring, pending work, Fault lease, or recovery obligation, and passes the
Endpoint readiness check.
If cleanup or same-identity replacement fails, the Endpoint MUST be
quarantined and excluded from new leases; other cleaned Endpoints from that
run may be released and the usable pool capacity updated. A lease MUST NOT be
held indefinitely solely because an Endpoint cannot quiesce.

After warm-up, use the median, P95, and linear slope of the long-run window:

- FD, Socket, task, and pending MUST NOT have a sustained positive slope;
- RSS need not return to its startup value, but MUST NOT grow continuously under the same load and cycle;
- CPU is judged using steady-state utilization and throttling;
- absolute thresholds belong in profile configuration and MUST NOT be hard-coded in the random generator.

#### 12.5 Result classification

```text
PASS
FAIL_SUT
FAIL_HARNESS
INVALID_INFRASTRUCTURE
KNOWN_ISSUE
CANCELLED
```

### 13. Kubernetes rollout

#### 13.1 Namespace

```text
actr-soak
  |-- loran-0
  |-- endpoint pool: actr-0 ... actr-N (unwired)
  |-- soak-dashboard
  |-- collector
  `-- artifact-uploader
```

`actr-soak` is a stable deployment namespace created before runs are scheduled.
It contains the shared Loran service, endpoint pool, Dashboard, and collectors;
it is not created or deleted per run. Stable pool identities, disjoint Endpoint
leases, run-scoped Peer graphs and Fault actions, and per-run artifact prefixes
isolate concurrent runs within that namespace. Pod replacement retains the
Endpoint's provisioned ActrId; rotating identity or realm per run is out of
scope.

#### 13.2 Workloads

| Component | Kubernetes type | Description |
| --- | --- | --- |
| Loran | StatefulSet, initially one replica | Signaling, STUN, TURN, AIS |
| Actr Endpoint (`actr-N`) | StatefulSet | Stable ordinal; the Pod IP may change after rebuild; pool members start unwired and Peer edges are established at run start |
| Soak Dashboard | Deployment | UI/BFF, manual control, embedded Controller, and run-state persistence |
| Standalone Controller CLI | Optional one-shot Job | Dashboard API client for an explicitly invoked smoke/preflight or replay request; it uses `POST /api/v1/runs` and polls `GET /api/v1/runs/{run_id}`. The Dashboard's embedded SoakController executes the admitted run. |
| Collector | Sidecar or separate Deployment | Collect Loran and Kubernetes metrics, procfs, and logs; it does not scrape Endpoint Pods. The embedded SoakController alone fetches Endpoint `/metrics` over its authenticated TCP 9090 API. |
| Artifact uploader | Isolated Job/worker or Dashboard worker | After evaluation and successful redaction, uploads only the run's staged evidence bundle to S3/MinIO with a short-lived run-prefix credential, then records the object location and export status in the Dashboard |

The StatefulSet ordinal is only a role label; it is not a permanent identity.
The Controller CLI Job MUST submit every cluster-backed request through the
Dashboard API; it MUST NOT invoke the local direct-Endpoint `run` path against
the shared pool. The Dashboard's pool lease manager admits the request,
queues it when the required subset is unavailable, and prevents overlapping
leases. Coverage runs may execute concurrently only on disjoint leased
subsets; resource-sensitive runs hold the entire pool. Thus the CLI cannot
bypass the same admission and exclusivity rules used by the Dashboard UI. If
submission is rejected or the Dashboard is unavailable, the Job MUST fail
closed and MUST NOT fall back to direct Endpoint access.

#### 13.3 Ports and addresses

Ports follow the actual image configuration. Suggested values:

```text
Loran HTTP/Signaling: TCP 8080
Soak Dashboard UI/API: TCP 9080
Loran STUN/TURN: UDP 3478
TURN relay range: dedicated UDP range
Endpoint Control API, observations, and metrics: TCP 9090
```

The `/metrics` and `/v1/metrics` routes are served by the same HTTP listener
as the Endpoint Control API. Metrics requests require the Endpoint bearer
token in Kubernetes; this deployment has no separate Endpoint metrics port.

The service suite is called Loran in this RFC. The current actr-soak deployment
retains the Kubernetes Service resource names `actrix` and `actrix-turn-relay`
for compatibility; both names route to Loran and do not identify a separate
service. Outside these literal Kubernetes resource names, this RFC refers to
the service suite as Loran.
The in-cluster `actrix` Service advertises its stable ClusterIP. The deployment
also creates an `actrix-turn-relay` ClusterIP Service with same-port UDP
mappings for the configured relay allocation range (`49152-49407` in the
current test deployment), so relay candidates are routable through kube-proxy.
Loran configuration and Service ports MUST use the same range. External TURN
clients require a separately exposed LoadBalancer/MetalLB or node-reachable
address and are outside this in-cluster relay profile.

#### 13.4 NetworkPolicy

```text
Controller CLI Job -> Dashboard: TCP 9080 (authenticated run submission/status)
Dashboard / embedded SoakController -> `actr-N`: TCP 9090
                              (control, observations, `/metrics`; bearer token required)
Kubelet/node probe source CIDRs -> `actr-N`: TCP 9090 (`/livez`, `/readyz`)
Dashboard -> Kubernetes API:  TCP 443/6443 (Pods, Events, logs, and metrics
                              reads; approved lifecycle verbs only)
Collector/Prometheus -> Loran: TCP metrics port (Loran metrics scrape)
`actr-N` -> Loran:           TCP HTTP/Signaling, UDP STUN/TURN 3478
`actr-N` <-> Loran:          TURN relay UDP range (relayed media to and from
                              relay allocations; required by the relay profile)
`actr-N` <-> `actr-N`:        WebRTC ephemeral UDP
All Pods -> kube-dns:         TCP/UDP 53
artifact-uploader (or inline Dashboard worker) -> approved S3/MinIO object store: TCP 443 or configured port
Other traffic:                denied
```

The deployment MUST substitute `KUBELET_PROBE_CIDRS` with the node source
CIDRs seen by the target CNI, or configure that CNI's explicit host-to-Pod
health-probe exception. The CI preflight MUST verify `/livez` and `/readyz`
while default-deny policies are active. NetworkPolicy cannot restrict these
flows by HTTP path, so the Endpoint bearer token remains mandatory for every
route except those two kubelet probes.

The matrix MUST list every flow the design depends on; a missing allow rule
fails the harness, not the SUT, and is classified INVALID_INFRASTRUCTURE. The
Dashboard egress to the Kubernetes API MUST be scoped to the API server only,
and the TURN relay range MUST be the configured dedicated range rather than a
broad UDP allowance. The artifact-uploader MUST be allowed to reach only the
approved S3/MinIO object-store endpoint. Enforce the destination with an egress
gateway or approved address allowlist when NetworkPolicy cannot select the
object-store hostname directly. Its storage credential MUST be scoped to the
soak artifact prefix. If a centralized log service is used, list its explicit
destination and port instead of opening general egress.

The Controller CLI Job's only cluster egress is to the Dashboard Service on
TCP 9080 and kube-dns; NetworkPolicy MUST deny its direct access to Endpoint
Pods and the Kubernetes API. The Dashboard Service policy MUST allow this
authenticated client flow.

The pool model relies on the endpoint-to-endpoint UDP allowance; it MUST NOT
be narrowed per run, because any pool member may be wired to any other by a
plan.

The first phase uses only the endpoint Fault Registry and lifecycle events.

#### 13.5 RBAC and resources

The Dashboard uses a namespace-scoped Role. CI renders the Pod-delete
`resourceNames` from the deployed pool roster:

| API group | Resource | Verbs | Scope |
| --- | --- | --- | --- |
| core | `pods` | `get`, `list`, `watch` | All Pods in `actr-soak` |
| core | `pods/log` | `get` | Logs for Pods in `actr-soak` |
| core | `events` | `get`, `list`, `watch` | Events in `actr-soak` |
| `metrics.k8s.io` | `pods` | `get`, `list` | Pod resource metrics in `actr-soak` |
| core | `pods` | `delete` | Only `actr-0` ... `actr-N` and `loran-0`, for approved restart actions |

Pod deletion is the only write operation granted to the Dashboard and is the
mechanism for graceful/forced Endpoint or Loran restart. It receives no
permission to create, patch, or update Deployments/StatefulSets, Jobs, Secrets,
or RBAC; CI uses a separate deployment identity.

The optional Controller CLI Job uses the dedicated
`actr-soak-controller-client` ServiceAccount with
`automountServiceAccountToken: false` and no Role or RoleBinding. It submits
and observes runs only through the Dashboard API, so it needs no Kubernetes
API permissions and cannot perform Pod lifecycle actions. Before creating the
Job-specific Secret, the operator calls `POST /api/v1/cli-credentials` with a
separately provisioned Dashboard administrator credential; the issuance API
returns an opaque token once, permits one `POST /api/v1/runs`, then binds to
that returned `run_id` and permits status/result reads for that run only. The
Dashboard expires it at the run deadline plus the cleanup margin and revokes
it when the run becomes terminal; the caller deletes the Secret when the Job
exits. The administrator credential is never mounted in the Job. The run
credential is distinct from the Endpoint Control API bearer token and does not
grant access to endpoint credentials.

It MUST NOT receive `cluster-admin`, cross-namespace write access, or arbitrary
Secret-list permission.

Starting resources are for preflight only and MUST be calibrated against the
baseline:

```text
Loran:     2 CPU / 2 GiB
Endpoint:   2 CPU / 2 GiB
Dashboard (embedded Controller): 1 CPU / 512 MiB
Collector (Loran/Kubernetes/procfs/logs; no Endpoint scrape): 0.5 CPU / 512 MiB
```

Long runs MUST set requests, limits, node selection, Pod anti-affinity, and a
dedicated resource quota to avoid CPU contention being reported as a test
failure.

### 14. Artifacts, replay, and GitHub Actions

#### 14.1 Artifacts

```text
<run-id>/
  |-- manifest.json
  |-- git.json
  |-- plan.json
  |-- timeline.jsonl
  |-- operations.jsonl
  |-- business-results.jsonl
  |-- endpoint-snapshots/*.jsonl
  |-- endpoint-metrics/*.prom
  |-- loran-metrics/*.prom
  |-- k8s/events.json
  |-- k8s/objects.json
  |-- logs/
  |-- summary.json
  |-- junit.xml
  `-- reproduce.sh
```

Record the main SHA, image digest, configuration summary, seed, Pod UID,
ActrId, process incarnation, action ID, and Collector version.

After Pod deletion, artifacts MUST NOT depend on `emptyDir`. S3/MinIO or an
approved S3-compatible object store is the durable archive. The Dashboard PVC
is the run-time staging and recovery copy, retained until the object store
confirms a complete upload. `junit.xml`, logs, snapshots, and the rest of the
evidence bundle are stored with the run artifacts, not uploaded as a GitHub
Actions test report.

The Dashboard owns artifact export. After the Result Evaluator finishes, a
dedicated artifact-uploader (or an equivalent isolated Dashboard worker) reads
only that run's staged artifact directory, waits for the redaction gate to
pass, and writes an immutable bundle under the run ID in S3/MinIO using a
short-lived, prefix-scoped credential. It records the object-store location
and upload status in the Dashboard run record. On upload failure, it retains
the PVC copy and reports export as failed for operator recovery; this does not
change the GitHub deployment result.

Redaction is a deterministic gate, not a best-effort filter. A dedicated
redaction step MUST run after evidence collection and before any artifact
upload, export, or Claude Code input is assembled. It scans logs, snapshots,
and evidence JSON for TURN usernames/passwords, AId credentials, renewal
tokens, endpoint control tokens, private keys, credential-bearing URLs, and
complete business payloads. The authenticated Endpoint API supplies the
current run's credential literals directly to the Dashboard in memory while
the run-start inventory window is open. The Dashboard requests them with the
active `run_id`, scans the evidence, then calls `/v1/actions/run-end` and drops
its in-memory copy whether the scan passes or fails. The gate records only the
inventory count in `redaction.json`, never the literals or a reversible
encoding. Fixed patterns plus this transient inventory make the scan
deterministic, with no model involvement.
The gate is part of artifact-upload acceptance:

- artifacts MUST NOT leave the namespace or PVC before the gate passes, and
  the gate result is itself recorded as an artifact (`redaction.json`);
- a gate failure (scanner error) or an unredactable finding blocks artifact
  export and prevents a PASS classification; the run is classified
  FAIL_HARNESS because the evidence pipeline is unsafe, never FAIL_SUT;
- a secret detected in an already-exported artifact is an immediate failure
  (§12.1) and triggers credential rotation for the affected material.

At minimum, `reproduce.sh` can:

1. use the same main SHA and image digest;
2. use the same `plan.json`;
3. re-wire the same Peer graph recorded in `plan.json`, with the same configuration;
4. replay the plan from its clean initial state through the selected failure
   window's end. If the operator selects an action range, execution includes
   all preceding actions needed to recreate generation/recovery state and
   records the selected focus range in the manifest;
5. preserve a source action ID mapping and output the same generation history
   and Result Evaluator signal for the focused window.

#### 14.2 Workflow

The deployment workflow deliberately contains no test execution. Pod and
Dashboard health is its success criterion; a Dashboard soak verdict is not
reported as a GitHub check or used to block deployment:

```text
CI (build/deploy workflow)
  -> checkout a fixed SHA
  -> cargo fmt and build test images
  -> image-build workflow pushes Endpoint, standalone Controller CLI, Dashboard, and Loran images; record digests
  -> create namespace, Secret, RBAC, and NetworkPolicy
  -> deploy Loran, the unwired endpoint pool, soak-dashboard, and collectors
  -> wait for Pod and Dashboard health only
  -> stop; do not deploy/start a Controller Job, generate a plan, inject events, or start a soak plan

Dashboard (operator/API)
  -> preview and start one run
  -> lease a pool subset and wire the plan's Peer graph
  -> embedded SoakController performs endpoint preflight and business traffic
  -> inject lifecycle/network/RTC/signaling events
  -> collect endpoint, Loran, and Kubernetes evidence
  -> evaluate with the Result Evaluator and stage the complete artifact bundle in the Dashboard PVC
  -> run the deterministic redaction gate, then export the redacted bundle to S3/MinIO
  -> pass the quiescence gate and return the leased subset to the pool
  -> operator reviews the Dashboard result and opens the stored bundle for analysis

Controller CLI client Job (explicit operator request)
  -> POST /api/v1/runs with its short-lived, run-scoped Dashboard API credential
  -> poll GET /api/v1/runs/{run_id} for the terminal status and result
  -> Dashboard queues or admits the request through the same pool lease manager
  -> embedded SoakController executes it and owns all Endpoint/Kubernetes access
  -> CLI reports the result and exits; it holds no Endpoint token or Kubernetes API token
```

Nightly profile:

```text
Dashboard scheduler or operator trigger
23:00 China Standard Time, run for 8 hours
```

Weekend profile:

```text
Dashboard scheduler or operator trigger
23:00 Friday China Standard Time, run for 48 hours
```

The Dashboard schedules the Nightly profile every day at 23:00 CST and the
Weekend profile Friday at 23:00 CST. Scheduled runs enter the same lease queue
as operator-created runs, and the run manifest is authoritative for actual
start and end times.

ARC only creates the GitHub Actions Runner Pod. It does not perform RTC Faults,
Endpoint lifecycle actions, Result Evaluator evaluation, or upload run evidence
to GitHub. The Runner needs pinned `kubectl`, container tools, and Rust cache.

### 15. Claude Code initial analysis

An operator can retrieve the redacted bundle from S3/MinIO and analyze it
directly in Claude (including Claude Code). A separate AI gateway or GitHub
Actions analysis job is not required. Optional read-only automation may invoke
Claude Code after export, but it does not participate in the business control
loop or affect the deployment result.

Inputs:

- `summary.json`, `plan.json`, `timeline.jsonl`;
- endpoint Snapshot, Hook, Fault, and business results;
- Loran metrics;
- Kubernetes events, Pod describe output, and resource trends;
- redacted logs and `reproduce.sh`.

Outputs:

- SUT, Harness, Infrastructure, Flaky, and Known Issue classification;
- the first abnormal event and its timeline;
- affected Endpoint, Peer, generation, and exchange;
- potentially related historical Issues;
- missing evidence, confidence, and the minimum replay command.

Security constraints:

- AI does not change the Result Evaluator result;
- AI does not automatically retry a failed run;
- AI does not change thresholds, Kubernetes objects, or production Secrets;
- AI output contains no sensitive tokens;
- only a human can start the fix and replay flow.

Human loop:

```text
AI initial attribution
  -> human confirmation
  -> code or environment fix
  -> replay the original seed
  -> fixed regression test
  -> Nightly 8 hours
  -> Weekend 48 hours
```

### 16. Profiles and implementation phases

#### 16.1 Profiles

| Profile | Topology | Duration | Purpose | Pool use |
| --- | --- | ---: | --- | --- |
| `preflight` | 1 Loran + 2 Endpoints | 10 min | Image, network, API, and artifacts | Concurrent-capable |
| `pair-event` | 2 Endpoints | 30--60 min | Events and RTC/signaling Faults | Concurrent on disjoint subsets |
| `process-churn` | 2 Endpoints | 30--60 min | SIGTERM, SIGKILL, and same-identity Pod rebuild | Concurrent on disjoint subsets |
| `multi-peer` | Hub + 9 Leaves | 2 h | Peer isolation and concurrent recovery | Concurrent on disjoint subsets |
| `nightly` | 2--10 Endpoints | 8 h | Long-term overnight stability | Exclusive pool |
| `weekend` | 10--50 Endpoints | 48 h | Weekend resource and recovery soak | Exclusive pool |
| `release` | 50--100 Endpoints | 8--24 h | Operator-triggered pre-release validation | Exclusive pool, fresh rolling rebuild |

Report steady and recovery windows separately: steady windows do not inject
destructive Faults; recovery windows periodically inject and heal them. Runs
that do not fit the idle pool wait in the Dashboard queue. A "Concurrent on
disjoint subsets" profile MAY share the pool only while its sampled event set
excludes Loran-wide and other shared-infrastructure actions (§8.2); if such
an action is sampled for a run, that run MUST take the pool exclusively.

#### 16.2 Implementation phases

1. **Phase 0: baseline and preflight** — fix SHA, images, Loran address,
   namespace, artifacts, and no-Fault bidirectional business.
2. **Phase 1: minimum endpoint adapter** — implement readiness, identity, RPC/DataChunk
   probes, peers, and minimum events.
3. **Phase 2: events and deterministic Faults** — connect
   `NetworkEventHandle`, RTC/signaling Registries, idempotency, generation
   fencing, and historical Issue scenarios.
4. **Phase 3: multi-Pod and lifecycle** — add StatefulSets, the endpoint pool
   with runtime Peer wiring and leases, two-sided Fault commit, Pod/Loran
   restart, resource collection, and the Result Evaluator.
5. **Phase 4: Nightly 8 hours** — add fixed-seed random events, coverage checks,
   `retry=0`, and Claude Code analysis.
6. **Phase 5: Weekend 48 hours and external network** — add rolling restart
   and NetworkPolicy/Chaos Mesh/tc-netem comparison. The Dashboard's optional
   relay-only setting, credential rotation, selected-path verification, and
   policy reset are available to any profile; relay-only is disabled by default.

### 17. Required resources, risks, and definition of done

#### 17.1 Required resources

- self-hosted Kubernetes cluster and test namespace;
- GitHub Actions ARC Runner;
- pinned `kubectl`, container build/push capability;
- GHCR or an internal image registry;
- Loran, Endpoint, and Dashboard images for the shared deployment;
- standalone Controller CLI image for optional one-shot smoke/preflight or replay Jobs;
- a Dashboard PVC for run state and evidence;
- S3/MinIO/PVC artifact storage;
- DNS, Pod-to-Pod UDP, and STUN/TURN relay ports;
- test realm, short-lived credentials, and Secret management;
- stable advertised address or Pod-IP re-render strategy;
- dedicated node pool, resource quota, and anti-affinity.

Recommended additions are Prometheus/Grafana, Loki/Vector/OTLP, cAdvisor,
Kube State Metrics, a namespace TTL janitor, Rust/container caches, and Claude
Code OIDC.

#### 17.2 Risk controls

| Risk | Control |
| --- | --- |
| Test Fault diverges from real networking | Pass baseline plus Kubernetes external-Fault comparison |
| Fault does not reach WebRTC | Factory wiring contract test plus hit counter |
| Endpoint adapter bypasses business path | Only the test workload may call `RuntimeContext` |
| Old generation contaminates a new connection | `expected_generation` and generation-scoped Fault lease |
| K8s CPU contention creates false failure | Dedicated nodes, requests/limits, resource classification |
| Pod deletion loses evidence | Centralized artifact/log storage |
| API becomes a production attack surface | Test-only image, NetworkPolicy, short-lived token, no shell |
| Controller CLI bypasses leases or gains cluster access | Dashboard API submission only; run-scoped Dashboard credential, no Endpoint token, no Kubernetes RBAC, and NetworkPolicy blocks direct access |
| Random failure cannot be replayed | Fixed seed, `plan.json`, image digest, `reproduce.sh` |
| AI misclassifies a failure | Deterministic Result Evaluator and gate |
| Loran address changes after rebuild | Stable advertised address or re-render and re-register |

#### 17.3 Definition of done

- main SHA, image digest, configuration, and seed are fully recorded;
- real Loran and multiple actr Pods complete bidirectional RTC business;
- RPC/DataChunk passes through `RuntimeContext`, `PeerGate`, and a real DataChannel;
- lifecycle events pass through `NetworkEventHandle`;
- RTC/signaling Faults have install, hit, heal, and generation evidence;
- relay-only is disabled by default; an enabled run renews TURN credentials,
  proves both ends of every selected Peer path are relayed, and resets the
  policy before pool handoff;
- the credential redaction inventory is collected from every leased Endpoint
  through its bearer-authenticated, run-scoped route, includes each observed
  TURN/AId/renewal credential revision, and is held only in memory through the
  deterministic redaction scan; `/v1/actions/run-end` clears it afterward, and
  the artifact records only the inventory count;
- multi-Pod A-to-B Fault can be committed consistently on both sides;
- Pod/Loran restart can be executed and replayed;
- one Peer failure does not block another Peer;
- Snapshot, Hook, business, Loran, and Kubernetes results can be correlated by `action_id`;
- pending, session, lane, and recovery obligation have convergence checks;
- FD, Socket, RSS, CPU, and memory have long-term trends;
- Nightly 8-hour and Weekend 48-hour profiles are executable;
- `retry=0` and `backoffLimit=0`;
- failure artifacts remain available after Pod deletion;
- Claude Code performs only initial analysis, and human fixes can replay the original seed;
- run-scoped Jobs, artifact prefixes, and temporary credentials can be cleaned up automatically; the shared namespace, pool, Dashboard PVC, and deployment credentials remain until the environment is decommissioned.
- CI image/deployment jobs do not start a Dashboard-controlled run;
- Dashboard is the sole test owner per namespace;
- Peer graphs are wired and torn down through the peer-management API and recorded in `plan.json`;
- pool subsets are leased exclusively and returned only after the quiescence gate;
- coverage profiles can run concurrently on disjoint subsets, while Nightly, Weekend, and release runs hold the pool exclusively;
- the pool can be rolling-rebuilt on schedule and before operator-triggered `release` profile runs without losing artifacts.

### 18. References

- [Kubernetes Jobs](https://kubernetes.io/docs/concepts/workloads/controllers/job/)
- [Kubernetes StatefulSets](https://kubernetes.io/docs/concepts/workloads/controllers/statefulset/)
- [Kubernetes NetworkPolicy](https://kubernetes.io/docs/concepts/services-networking/network-policies/)
- [Kubernetes Downward API](https://kubernetes.io/docs/concepts/workloads/pods/downward-api/)
- [GitHub Actions self-hosted runners](https://docs.github.com/en/actions/hosting-your-own-runners/managing-self-hosted-runners)
- [Chaos Mesh NetworkChaos](https://chaos-mesh.org/docs/next/simulate-network-chaos-on-kubernetes/)

## Drawbacks

- An 8-hour or 48-hour cluster run consumes dedicated CPU, memory, UDP ports, storage, and operator attention.
- A same-process endpoint adapter is more invasive to build and package than a generic black-box driver, and crate-private observations need a test-only facade.
- Runtime/socket Faults are deterministic and targeted, but cannot model every kernel, CNI, NAT, MTU, or carrier-network behavior.
- Resource trends can be noisy under shared capacity; dedicated nodes and explicit infrastructure classification add operational cost.
- An always-on endpoint pool consumes CPU and memory between runs; pool sizing trades idle cost against queue wait time.
- Reusing pool Pods across runs risks residual-state contamination; the quiescence handoff gate, per-run cleanup and realm isolation, and scheduled rolling rebuilds bound but do not eliminate this risk.
- The first phase validates server-side lifecycle facts rather than real mobile process suspension and radio handover.

## Alternatives

The alternatives are retained here because the actr RFC process requires a
genuine comparison; the numbered sections above define the proposed design.

### Single-process in-process soak harness

A single-process in-process harness is useful for fast characterization and
deterministic protocol cases. It does not exercise Pod identity changes,
Kubernetes scheduling, process death, or cross-process network addresses, so
it remains a preflight/regression layer rather than the long-run architecture.

### k6-centric load test

k6 can exercise HTTP, WebSocket, gRPC, or an endpoint control API, but it cannot provide
the actr workload and real WebRTC/ICE/DTLS/SCTP driver required here. Rust/actr
workloads are therefore the primary load source. k6 is deferred to a possible
independent Loran/control-plane benchmark.

### Chaos Mesh or CNI chaos as the first fault mechanism

Kubernetes network chaos is valuable for kernel/CNI realism, but it is coarse
for a specific Actor pair, WebRTC generation, or signaling message class, and
it makes failures harder to reproduce. The first phase uses directed test-only
seams; external network profiles are an explicit comparison.

### External endpoint sidecar

A sidecar isolates the process and can collect `/proc`, but cannot directly hold
the Node's `NetworkEventHandle`, Manager subscriptions, or process-local Fault
Registry. The endpoint control adapter is colocated with the Node; a sidecar remains suitable for
system metrics.

### Full-mesh topology

Full mesh maximizes pair combinations, but connection count grows quadratically
and can make the cluster the bottleneck. Ring, seeded cross edges, and Hub/Leaf
profiles provide controlled coverage and explicit fan-out pressure.

### CI-owned fixed topology with an immutable topology ConfigMap

An earlier proposal had CI bake a fixed Peer graph into the deployment and
publish a topology ConfigMap for the Dashboard to validate against. That model
keeps two sources of truth (deployment parameters and Dashboard plan state)
that can silently drift, turns every topology change into a redeployment, and
degrades compatibility checking to special cases. The pool model makes the
topology plan data instead: the plan is the only source of truth and
compatibility reduces to pool fit.

### No change

Existing short tests catch many protocol regressions, but cannot provide a
reproducible long-duration signal for resource leaks, repeated recovery races,
or Pod lifecycle behavior. No change leaves those regressions dependent on
production incidents or manual testing.

## Compatibility and phasing

This RFC changes neither the actr wire format nor production signaling and RTC
protocols. It adds test infrastructure, Kubernetes manifests, and a test-only
endpoint adapter/facade. The endpoint adapter and Fault adapters MUST be excluded from normal
production builds. A test-only observation facade MUST NOT become a public SDK
or FFI contract.

The required phases and acceptance criteria are defined in sections 16 and 17:
Phase 0 establishes a no-Fault baseline; Phase 1 proves the endpoint adapter's production
business path; Phase 2 adds events and deterministic Faults; Phase 3 adds
multi-Pod lifecycle; Phase 4 runs Nightly; and Phase 5 runs the Weekend profile
and optional external-network comparisons. k6 and TURN allocation accounting
are explicitly deferred.

Each soak Endpoint runs the production ActrNode path with test-only seams for
Fault registries, preregistered credentials, and read-only observation. The
design defines one endpoint type and performs Peer wiring through the
run-scoped peer-management API; deployment-time Peer wiring is out of scope.

## Unresolved questions

- Which Kubernetes metrics backend is available on the target cluster?
- What per-profile CPU, RSS, FD, Socket, pending, lane, and recovery thresholds should be accepted after preflight baselines?
- Which object storage (PVC, MinIO, or S3-compatible storage) is approved for immutable artifacts and Claude Code input?
- Which external network comparison profiles should be added after the deterministic phase, and how should infrastructure noise be classified?
- Does the target Loran deployment expose sufficient signaling and service metrics, or require a separate observability change?
- How does the production ActrNode learn about and connect to a Peer at runtime (Loran realm discovery, explicit dial, or first-message establishment), and what work is needed to expose run-scoped Peer add/remove through the Endpoint Control API?
- How should the endpoint pool be sized, and should Dashboard queue depth drive pool autoscaling?
- What cross-run resource-contention noise is acceptable for concurrent coverage profiles before a run is classified INVALID_INFRASTRUCTURE?

These questions do not block Phase 0. They MUST be resolved in the tracking
issue before the corresponding phase is accepted.

## Future possibilities

- Add k6 for an independent Loran HTTP/control-plane benchmark without making it the RTC load driver;
- run the endpoint control protocol against real Android/iOS builds when devices or device farms are available;
- add Loran-side TURN allocation accounting in a separate observability RFC;
- add CNI, `tc/netem`, or Chaos Mesh profiles for node and network realism;
- run Loran replicas, rolling upgrades, and cross-version actr/Loran matrices;
- autoscale the endpoint pool from Dashboard queue depth and profile demand;
- manage several pools across namespaces from one Dashboard;
- feed structured Result Evaluator evidence from Dashboard-controlled runs into an issue-triage bot without changing the independently defined deployment success criterion.
