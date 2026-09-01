# 10. Demand and acceptance criteria

## 1. Functional Needs

### 1.1 Shell with Tab

| ID | Requirements |
|---|---|
| FR-SH-001 | Every one. resident app Tab Yes. loading/running/suspended It has to be just one time. live Front Actor;Shell Pages & & & dormant Tab I don't think so. Front. |
| FR-SH-002 | Front Actor After startup, you must bind and only one. Primary Service Actor. |
| FR-SH-003 | Refreshing must be rebuilt. Front ♪ And connects to the same Service,No new Service. |
| FR-SH-004 | Close Tab It has to be withdrawn. session And destroy it. Front, may not be stopped or deleted Service. |
| FR-SH-005 | Backstage. Tab I have to. pause Keel Engine;restoration must resume Or reconstructed Attach. |
| FR-SH-006 | This machine opens again the same exclusive Service Default Focus Already Tab. |
| FR-SH-007 | The address bar must be shown separately Publisher, examples,Realm And connect to security. |
| FR-SH-008 | Guest Navigation must pass. `host:navigate`We can't change it. Shell History, cross-examples capability Or put it on. secret Writing URI. |

### 1.2 URI

| ID | Requirements |
|---|---|
| FR-URI-001 | Must support `actr://publisher/program@exact-version`. |
| FR-URI-002 | None `service` Must Parsing As Create Intent. |
| FR-URI-003 | Yes. `service` Must Parsing As Attach Intent. |
| FR-URI-004 | Attach I'm not going to fail. LB, downgraded or create alternative examples. |
| FR-URI-005 | ActorId type I have to. URI coordinate Perfect match. |
| FR-URI-006 | It's a must to be created. replace For the tape. `service` It's... URI. |
| FR-URI-007 | portable Attach URI Must carry the parseable Realm authority. |
| FR-URI-008 | URI Not to include credential, session secret, Workspace secret or bearer invite. |
| FR-URI-009 | Official Publisher authority Do not contain ports; develop sources for use `actr-dev://`. |
| FR-URI-010 | Parser Must be implemented IDNA, repeats the reservation parameters, illegal coding and control character checks. |

### 1.3 Program Catalog and distribution

| ID | Requirements |
|---|---|
| FR-PKG-001 | Program Catalog with Service Inventory We have to split up. |
| FR-PKG-002 | coordinate It's got to be passed. Publisher HTTPS well-known Path resolution. |
| FR-PKG-003 | Every precision coordinate It has to be fixed. package digest. |
| FR-PKG-004 | `.actr` Validation required Publisher key, manifest signature And all the files. hash. |
| FR-PKG-005 | I must refuse. zip traversal, the number of documents exceeding the limit, the total amount of depressives and the compressed bomb. |
| FR-PKG-006 | Third parties package Not to include Front Host/native plugin. |
| FR-PKG-007 | Guest Must Valid Local bytes Load, no choice Keel Direct Trust Far URL. |
| FR-PKG-008 | Same coordinate Different. digest I have to. hard fail. |
| FR-PKG-009 | Still being Service Reference package digest No Cache Cleaning. |
| FR-PKG-010 | Marketplace metadata Not covered Publisher package identity. |
| FR-PKG-011 | We must support local `.actr` and clearly marked development sources. |
| FR-PKG-012 | Must support Advisory/Block new/Quarantine/Critical block Dismissed level. |
| FR-PKG-013 | Publisher bootstrap It has to be fixed. HTTPS 443;Profile with Release There must be a regular signature, serial number/Time limits and rollback protection. |
| FR-PKG-014 | Official distribution tools must be supported build, validate, signPublication metadata, press the web coordinate Read back to verify. |

### 1.4 Front/Keel

| ID | Requirements |
|---|---|
| FR-FRONT-001 | Front Host It has to be. Actro and Publisher Cannot be replaced. |
| FR-FRONT-002 | v1 Official Guest Support only Keel JS Guest. |
| FR-FRONT-003 | It has to be fixed. QuickJS & Enable hard timeout with heap cap. |
| FR-FRONT-004 | Guest Modules,capabilityComponent and RemoteRef It must be white. |
| FR-FRONT-005 | Guest Default not available fetch/socket/filesystem/native module. |
| FR-FRONT-006 | Guest Only get fixed target It's... PrimaryService capability. |
| FR-FRONT-007 | All Guest RPC/input/output/event I have to. schema validate. |
| FR-FRONT-008 | Image The resource component shall not be exported into an undeclared network. |
| FR-FRONT-009 | Engine close/crash We have to clean it up. timer, callback, listener and capability. |
| FR-FRONT-010 | UI We have to. v1 Description same-process sandboxNo claims OS process isolation. |

### 1.5 Service with Workspace

| ID | Requirements |
|---|---|
| FR-SVC-001 | Local New must be created for a lasting Service ActorId. |
| FR-SVC-002 | Service Re-launch, renew,signaling Toggle must be kept after ActorId. |
| FR-SVC-003 | Service It has to be fixed. Program Release digest. |
| FR-SVC-004 | Service It has to be independent. ActrType It's... per-instance store. |
| FR-SVC-005 | Workspace I have to. Service Host broker Hold on,Front No path to be obtained/credential. |
| FR-SVC-006 | v1 Default Workspace Writing mode is exclusive writer. |
| FR-SVC-007 | The creation of the business must be something like a mere one and the collapse can be restored. |
| FR-SVC-008 | Stop, Archive, Delete Service with Delete Workspace Must be different. |
| FR-SVC-009 | Third parties Service Default must be limited WASM Component workload. |
| FR-SVC-010 | Service It's a little more like a little bit of a little bit of a support. Actor, but Front Still tied. Primary Service. |
| FR-SVC-011 | v1 Version upgrade must be new Actor + Visible migration. |
| FR-SVC-012 | Service After the clean-up, ActorId Reuse is not permitted. |
| FR-SVC-013 | Workspace It's a separate declaration. requirement, provisioning, access And allowed providers. |
| FR-SVC-014 | The migration to the wastebin must retain and recover identity and case data; the removal must be de-identified and not recoverable. |
| FR-SVC-015 | Life cycle, accessibility, health,placement with attachment Status must be recorded in field segments. |
| FR-SVC-016 | v1 Create Intent Only locally supported current device Host creation;remote management must be awarded by another host management capability. |

### 1.6 RealmIdentities and networks

| ID | Requirements |
|---|---|
| FR-NET-001 | Actro Must Embe Inline signaling client and local Realm, don't write the only global server that's dead. |
| FR-NET-002 | Realm Profile It's got to be passed. HTTPS and sign the certificate. |
| FR-NET-003 | One. Realm There must be more than one assignment signaling/TURN endpoint. |
| FR-NET-004 | v1 Front with Primary Service It has to be the same. Realm. |
| FR-NET-005 | Actor credential We have to tie it down. ActorId, Actor key, ActorType, Realm and the duration. |
| FR-NET-006 | Service identity It's got to be supported by the long-term. key reclaim Same ActorId. |
| FR-NET-007 | signaling credential No longer to be put in URL query. |
| FR-NET-008 | TURN Short term, limited audience, Rotation credential. |
| FR-NET-009 | Actr Realm with TURN auth realm Must use different types/Named. |
| FR-NET-010 | endpoint Non-standard ports are available, but must be credible Profile/Publisher metadata. |
| FR-NET-011 | Service offline It must be distinguishable and not degenerate to the same type of discovery. |
| FR-NET-012 | v1 Multiple signaling endpoint It's the same thing. Realm Control side. failover, do not support hidden cross-control federation. |
| FR-NET-013 | Local Realm ActorId We have to tie it down. installation authorityBan copying as cross-equipment portable link. |
| FR-NET-014 | Service I can't just do it. Front Self-reported. ActorType, Host Version or digest High-risk delegation of authority; independence must be validated when required attestation. |
| FR-NET-015 | Realm bootstrap It has to be fixed. HTTPS 443and Profile continuity key, serial number, validity period and rotation chain to prevent rollback. |
| FR-NET-016 | Actro Must provide Realm Accession, recertification, equipment cancellation, departure and key Changed disposal process. |

### 1.7 PermissionSecurity and privacy

| ID | Requirements |
|---|---|
| FR-SEC-001 | effective capability Must be a declaration,Host Support, User/Manager ' s delegation of authority, platform strategy intersection. |
| FR-SEC-002 | Permissions grant We have to tie it down. Publisher/Program/Version or digest with scope. |
| FR-SEC-003 | New or expanded high-risk competencies must be reconfirmed. |
| FR-SEC-004 | ActorId Must be considered as an open address, not an access token. |
| FR-SEC-005 | Actor key, Realm refresh, Workspace credential We have to get in. OS Safe storage. |
| FR-SEC-006 | Logs and crash report No record. secretOperations payload Or complete Workspace Path. |
| FR-SEC-007 | package/Realm/Network URL Must be implemented SSRF with redirect policy. |
| FR-SEC-008 | Must support signed emergency blocklistOverdue and audit. |
| FR-SEC-009 | Wastebin, isolation or clean-up Service No automatic deletions Workspace. |
| FR-SEC-010 | Telemetry must be closed by default; the user or administrator must minimize sensitive markings after they are enabled. |
| FR-SEC-011 | Undo grant It must be terminated immediately. capability handleand terminates or reduces the power of active sessions. |
| FR-SEC-012 | Service Invitations must be one-time, expired, revocable and inapplicable bearer secret Entry instance URI. |
| FR-SEC-013 | Service ACL At least support it. owner/editor/viewer ♪ The role and the turn ♪ Realm membership Separate from the case-by-case authorization. |
| FR-SEC-014 | Apply colours must lifecycle, reachability, health and enforcement field. |
| FR-SEC-015 | Application maps must be non-existent and effectively dependent on the connectivity weight and include all of them `Transfer(ActorId)` Members. |
| FR-SEC-016 | The passage is valid and `Ready` , the colour must be displayed to take the weakest of all nodes. |
| FR-SEC-017 | The top floor is green only. / Yellow / Red; no colours to be displayed when cut face is not ready; unknowns not to be greened. |
| FR-SEC-018 | The person assessed cannot sign out for final `ActorAssurance`;a valid declaration by Host Closed, conclusions issued by the certifying officer. |
| FR-SEC-019 | `ActorAssurance` You have to sign and tie. `ActorId + ActorStateId`The evidence fingerprints, strategies, rules and validity period. |
| FR-SEC-020 | New Actor The side must be on the other side `ACTIVE` And for the time being AA Only after acceptance can applied data be received. |
| FR-SEC-021 | package, Host Evidence keys,credential, CapabilityThe strategy or conclusion must be communicated to the weakest and synchronized with the implementation point. |
| FR-SEC-022 | After the invalid dependency, Actor I'm not going to go to the next one. `structureId`. |
| FR-SEC-023 | Third parties in-process NativeCdylib No way. `CONTROLLED`;no declaration, signature or static scanning may be used to upgrade. |
| FR-SEC-024 | Attach We have to negotiate strategies.nonceCurrent `ActorStateId` with `ApplicationStateId`, can't rely on GraphRoot/epoch. |
| FR-SEC-025 | Complete Actor Figure must be minimally disclosed and sent by default signalingNormal peer Or telemetry. |
| FR-SEC-026 | Access to protected data opaque HTTPS I have to be right. Actor Clear delegation of authority;TLS It cannot prove its internal privacy. |
| FR-SEC-027 | Every one of them protected Actor The call must be checked with data generation permission according to current status. |
| FR-SEC-028 | WASM Only in declarations,importsand potential for/Only when the export is made mandatory. `CONTROLLED`. |
| FR-SEC-029 | NativeCapsule Independent process only completed before third-party code execution OS sandbox♪ Close the original exit and open it only ♪ brokered IPC ♪ And then you can be rated ♪ `CONTROLLED`. |
| FR-SEC-030 | The three colors must be kept inside. identity, artifact provenance, workload confinement, compute confidentiality, channel security, storage protection, egress closure and output control Wait for the multidimensional facts. |
| FR-SEC-031 | Every network Actor edge (a) Must have two-way authentication, confidentiality, integrity, forward confidentiality and re-allocation;signaling, TURN & Normal relay Not held payload key. |
| FR-SEC-032 | Actor Secure Channel The handshake must be tied to both ends. ActorIdCurrent `ActorStateId`, runtime, credentialTemporary keyObjectives Service, policy, attestation result, nonce and role, and two-way authentication transcript. |
| FR-SEC-033 | transport TLS/DTLS/QUIC Success cannot be generated alone channel security pass;must be completed Actor Identity, examples of operations and session key Tie. |
| FR-SEC-034 | Workspace, snapshot, backup, cache, index And the temporary documents must be pressed. data class Implementation envelope encryption;ActorId Do not act as a key. |
| FR-SEC-035 | Data Policy Must be declared by data type and operation required assurance dimensions;n required The dimensions must not make public data unconditional. unknown. |
| FR-SEC-036 | Remote certificate must be distinguished Attester, Verifier with Relying Party;Evidence/self-assertion It cannot be directly delegated. |
| FR-SEC-037 | Attestation We have to tie it down. fresh nonce, Actor/runtime, workload/package digest, policy, audience and current ephemeral channel/decryption key. |
| FR-SEC-038 | policy Request confidential compute I'm not sure if I'm going to be able to do this.private/secret DEK Only appraisal Enclosed to Attestation Overwrite ephemeral key. |
| FR-SEC-039 | Confidential Computing Only the matching. compute confidentiality claimsNo substitute WASM/Native confinement, egress closure or output control. |
| FR-SEC-040 | Use GPU/NPU It's... confidential workload It must be combined. CPU TEE, guest, runtime, driver, IOMMU/device assignment, device firmware, CC mode, link and device memory. |
| FR-SEC-041 | Any visible and explicit accelerator The link is missing. Evidence I'm not sure if I'm going to be able to do this.compute confidentiality I must. indeterminate/unknown. |
| FR-SEC-042 | Vendors Evidence Required to be signed, formatted, avoidable Verifier adapter Map to canonical claims; the manufacturer model or marketing statement shall not be directly mapped pass. |
| FR-SEC-043 | Evidence, Attestation Result, Reference Value, Endorsement or TCB Status due/New sensitive data and data must be stopped when they are withdrawn key release, and trigger re-attestation/rekey. |
| FR-SEC-044 | High-sensitivity output crossing data class It must be visible. declassification/output policy;legal Actor response You can't be a bypass. egress closure The hidden exit. |
| FR-SEC-045 | Green must bind the current `ApplicationStateId`(`structureId` + Principal + Merkle Roots) and `Ready`;not to be given Publisher or Program General green marker. |
| FR-SEC-046 | Program Privacy Contract It's a static declaration. Actor roles/types/profiles, cardinality, edges, data classes, capabilities, terminals, audit sinks and assurance requirements. |
| FR-SEC-047 | Host Validity available capability It must be. ContractUsers/Organizations policy, Host policy and instance grant "of the people of the country; no capacity to acquire without declaration handle. |
| FR-SEC-048 | Actual Actor/runtime The figure must be Contract graph grammar examples; all mutation It has to be formed first. candidate graph, the approved data chart cannot be directly altered. |
| FR-SEC-049 | The initial chart must release protected data or DEK Identities, evidence, strategy and `ApplicationAssurance` Assessment. |
| FR-SEC-050 | Cannot meet the base profile It's... optional The function must be separated into separate functions. feature profile/capability;no contamination of certified base closed packages after use. |
| FR-SEC-051 | Lettered Verifier You can issue internal closed sub-maps. scope, policy, evidence rootand cancellation of information Certified Privacy Domain aggregate;upstreamable policy As a single node. |
| FR-SEC-052 | Actor/Service It's... self-signed privacy declaration No formation. Assured Actor Edge;Verifier trust, certificate scope, endpoint, session and data class Must be all matched. |
| FR-SEC-053 | All audit logs must be Contract In-Focus audit sinkand declare fields,payload, data class, retention, operator visibility, encryption and export rules. |
| FR-SEC-054 | `ActorStateId` Change must take place locally and abolish old power; those who do not depend can `LocalContinue`, but not green; control side shall not be bound by operational data. |
| FR-SEC-055 | Colours are not weakened and can be restored automatically when the strategy allows; still required `Ready` It's just green. |
| FR-SEC-056 | Weak colours must be kept in the process and the reasons for them must be shown, and only when the risk is accepted can the work continue; the risk is accepted without changing the colour. |
| FR-SEC-057 | Risk acceptance cannot raise the missing certificate to green; the platform ' s hard failure and the ineffective channel cannot be covered by the user. |
| FR-SEC-058 | The figure remains unchanged but the evidence expires, the person certifying it / TCB / The same is true for the withdrawal of evidence, for the change of strategy or state fingerprints. |
| FR-SEC-059 | GPU/NPU Attestation It's not to be interpreted as LLM Server No log; only measured log configuration, closed egress/audit capabilities Or credible. Certified Privacy Domain retention claim The blog is a tool for assessing the blog. |
| FR-SEC-060 | `ActorState.revision` It has to be added and entered by the state authority. `ActorStateId`;results rolling cannot reproduce old root, old Capability / The conclusion must not be restored to effect by rollback. |
| FR-SEC-061 | - I can show you. Actr Endpoint of identity must be pressed Actor Target assessment, not to be declared HTTPS/Domain names round and round Principal Rating; maintaining the export when the target category is not determinable `UNKNOWN`; the authorized interface for the trust pool must be "data exits" |
| FR-SEC-062 | The local clocks used by the executing party and the rating party for the period, the evidence and the validity of the certificate are not retrievable. |
| FR-SEC-063 | `EncGraph` The identifier must be set in a restricted area. ASCII Character set;`ApplicationStateId` The leaves must be pressed. `ActorId` Sort and Each `ActorId` Just a leaf. |

## 2. Unfunctional requirements

### 2.1 Performance

The following budget does not include the first package Download and user stopover:

| ID | Indicators | v1 Objective |
|---|---|---|
| NFR-PERF-001 | Cache Local Service Attach To Interactable | p50 ≤ 800 ms, p95 ≤ 2 s |
| NFR-PERF-002 | Cached Far Service Attach | Networks when available p50 ≤ 2 s, p95 ≤ 5 s |
| NFR-PERF-003 | Front Guest cold load | p95 ≤ 1.5 s |
| NFR-PERF-004 | Enter to Original UI Update | p95 ≤ 100 ms, without remote RPC |
| NFR-PERF-005 | Backstage. Tab pause | ≤ 250 ms |
| NFR-PERF-006 | Service Reboot. | p95 ≤ 3 s |

### 2.2 Resources

Default configurable budget:

| ID | Item | Default |
|---|---|---|
| NFR-RES-001 | Keel Guest heap | desktop 64 MiB;mobile 48 MiB |
| NFR-RES-002 | Guest Single JS turn | 100 ms;the start-up phase can be given separately 1 s Total budget |
| NFR-RES-003 | Guest render nodes | 10,000 |
| NFR-RES-004 | Single batch render ops | 5,000 |
| NFR-RES-005 | Front RPC Side-by-side | 32 |
| NFR-RES-006 | Default package Download Size | 200 MiB hard cap, can be reduced by enterprise strategy |
| NFR-RES-007 | archive Number of documents | 10,000 |
| NFR-RES-008 | archive Total depressure | 500 MiB |
| NFR-RES-009 | Guest Movement control | Every one. Guest Use Host Disable from the special execution thread/workerNo, no, no. Shell/UI Main |

Any increase must be subject to the Platform ' s cap and authority policy.

### 2.3 Reliability

| ID | Requirements |
|---|---|
| NFR-REL-001 | No creation can result in two after the collapse of any phase Service. |
| NFR-REL-002 | Service Restart 10,000 In the secondary identity recovery test ActorId Change to 0. |
| NFR-REL-003 | Attach The number of times a substitute instance in the failed path is created 0. |
| NFR-REL-004 | package cache Writing must be atoms, and old versions can still be verified after power cuts. |
| NFR-REL-005 | Front crash Don't damage it. Service store or Workspace. |
| NFR-REL-006 | signaling Provider Toggle cannot change ActorId. |
| NFR-REL-007 | lease Network vibrating tests should not produce two that work at the same time. exclusive attachment. |
| NFR-REL-008 | beta It's... crash-free app session It has to be done. 99.5%, stable It has to be done. 99.9%. |

### 2.4 Clear.

| ID | Requirements |
|---|---|
| NFR-SEC-001 | All parser Must be included in the ongoing fuzz. |
| NFR-SEC-002 | Third parties package/Guest/Service Don't go around. capability allowlist. |
| NFR-SEC-003 | secret scanner The test is not allowed. URI, logs,crash fixture Found real in it. credential. |
| NFR-SEC-004 | The high-risk security border is open. beta Independent penetration tests completed. |
| NFR-SEC-005 | Security updates and emergency blocklist Could not close temporary folder: %s fail closed. |
| NFR-SEC-006 | The release build must be generated SBOM And sign. provenance. |
| NFR-SEC-007 | When the conclusions of the nodes become weaker, the associated protected effects must lapse simultaneously before the next implementation.Shell Update p95 ≤ 250 ms. |
| NFR-SEC-008 | 1,024 Actor, 4,096 edge The smallest increase within is in the reference desktop Go, go, go! p95 ≤ 100 ms;excess must unknown. |
| NFR-SEC-009 | Diagrams,epoch Competition,Mark Expiry, reliance on revocation, undeclared export and Host The cover-up simulation must be included. property/fault-injection tests. |
| NFR-SEC-010 | Actor channel, Attestation Result and Evidence Bundle Must have a cross-achievement test vector, reset/Wrong tie./Downgrade negative test. |
| NFR-SEC-011 | Every one. production Verifier profile It has to be recorded. trust root, covered, uncovered, lowest TCB, freshness* The present document was not edited before being sent to the United Nations translation services. conformance tests. |
| NFR-SEC-012 | Verifier/Key Broker Not to be used indefinitely when temporarily unavailable Result;grace window I have to. data policy Clear limits. |
| NFR-SEC-013 | graph mutation Request, old epoch lease Freezing and candidate edge Create must be linear; not present in the simultaneous test mutation Later, before the freeze. payload window. |
| NFR-SEC-014 | downgrade partial orderIt's not comparable. vector, auto-recovery, user rejection and hard-floor Non-coverable must be included property/fault-injection tests. |

### 2.5 Accessibility and internationalization

| ID | Requirements |
|---|---|
| NFR-A11Y-001 | Shell and system page reached WCAG 2.2 AA Response to platform requirements. |
| NFR-A11Y-002 | All security states have text/icon, not just by color. |
| NFR-A11Y-003 | Keyboard, screen reader, dynamic font and high contrast support. |
| NFR-I18N-001 | The Stable Error Code is separated from the Localized Texts case. |
| NFR-I18N-002 | IDNA Domain names can be checked simultaneously Unicode with A-label. |

### 2.6 Maintenanceability

| ID | Requirements |
|---|---|
| NFR-MNT-001 | URI, manifest, Realm Profile and session protocol Must be adapted. |
| NFR-MNT-002 | The platforms share the same standardized and validated test vector. |
| NFR-MNT-003 | Host capability and Safe Component schema Type to be generated from a single source/Test. |
| NFR-MNT-004 | All the retrofitters must be regenerable. transition And stability. error code. |

## 3. End to End Receiving scene

### AC-001: First created

**Given** User not installed `notes.example/team-notes@1.4.2`<br>
**When** Open None `service` URIand confirm the authority/Workspace<br>
**Then** Actro Authentication package, create only one Service♪ Getting a hold of yourself ♪ ActorIdRewriting URI, Attach Show Guest.

### AC-002: Refresh without duplicate creation

**Given** Create just finished<br>
**When** User refreshs, crashes and backslides/Forward!<br>
**Then** Service Inventory It's still just the old one. ActorId.

### AC-003: Precise examples offline

**Given** Attach URI Point offline Service<br>
**When** Open URI<br>
**Then** Local Inventory Known stopped Shows the start-up entrance at times; at remote end when no host status certificate is available offline/unreachable; neither creates the same type of example or calls the type of discovery.

### AC-004: URI Type does not match

**Given** URI coordinate with `service` ActorId type Different.<br>
**When** Parsing<br>
**Then** On any network or Guest Before execution `URI_ACTOR_TYPE_MISMATCH` Failure.

### AC-005: Same version of the variant

**Given** Locally Fixed digest A<br>
**When** Publisher Same coordinate Back digest B<br>
**Then** Here. `PACKAGE_EQUIVOCATION` Stop installation and running new bytes, keep A For audit.

### AC-006: Front Guest Exceeding authority

**Given** Manifest Undeclared network<br>
**When** Guest Try require Network modules, usage Image headers Or call `host:net`<br>
**Then** All routes were denied and there were no confidential audit incidents.

### AC-007: Guest - Cut!

**Given** Guest Execute Infinite Cycle<br>
**When** More than turn timeout<br>
**Then** Just... Keel Engine The government has been forced to stop.Service Other Tab Keep it available.

### AC-008: Tab Close

**Given** An active instance<br>
**When** Close Tab<br>
**Then** Front/session Destroyed,Service/Workspace/ActorId Reservations and from Inventory Reopen.

### AC-009: Actro Restart

**Given** Local Service Created<br>
**When** Quit completely and restart Actro<br>
**Then** Service Use the same ActorIdrestoring;collection and history URI Still connected.

### AC-010: Monarchy session

**Given** Service Has been one Front Hold exclusive lease<br>
**When** Second device Attach<br>
**Then** Service Back occupied; a second cannot be established without authorization active attachment.

### AC-011: Realm Provider Switch

**Given** signaling A Fault,Profile Statement signaling B<br>
**When** Actor Reconnect<br>
**Then** Use B and ActorId no change;no declaration endpoint Not to try.

### AC-012: TURN

**Given** P2P Unattainable.<br>
**When** Use TURN<br>
**Then** Use Realm issuer Short-term audience credential, UI Show relayed, the log does not contain a password.

### AC-013: Workspace Conflict

**Given** Workspace Existing exclusive writer Service<br>
**When** Create Second Writing Service<br>
**Then** Unequivocal rejection or request for relocation before creation/Read-only, without silent writing.

### AC-014: Trash and cleanup

**Given** Service Tie External Workspace<br>
**When** Users move to the wastebin before recovery<br>
**Then** Service Use the same ActorId The government is not in a position to do so.Workspace content unchanged; user subsequently completely removed without choosing to delete Workspace I'm not sure if I'm going to be able to do this.Actor credential/key/Examples store The blogger says that the government is not in a position to do so.Workspace Still.

### AC-015: Upgrade Failed Rollback

**Given** v1 Service and Workspace<br>
**When** v2 Migration validation failed<br>
**Then** v1 ActorId, Workspace lease And old Tab I'm not sure if I can get back to you.v2 Don't take over.

### AC-016: transfer of domain name/New key

**Given** Installed version by old key Signature<br>
**When** Publisher Profile New life without a rotation chain key<br>
**Then** Old Fixed digestOffline run, new installation/Updates to show identity changes and requests confirmation.

### AC-017: Mobile platform suspend

**Given** Local Service System paused in the mobile backstage<br>
**When** Far-end or local FrontConnection<br>
**Then** Show suspended/host offlineNo alternative example is reported and no new one is created.

### AC-018: Security aborted

**Given** digest Marked Critical block<br>
**When** User Open Existing Examples<br>
**Then** Guest/Service I'm not going to take it.Workspace Export and safety statements are still available.

### AC-019: Local Realm Link

**Given** Service ActorId Use another installation `local:<installation-id>` authority<br>
**When** Current device opens or tries sharing<br>
**Then** Here. `URI_LOCAL_REALM_FOREIGN` Rejects connection and disables portable share;nto be interpreted as being the same name DNS Realm.

### AC-020: Front Self-reported official Host

**Given** Unsupervised client self-reported correct Front ActorType, Host Version and package digestBut no. runtime attestation<br>
**When** Service policy When the official running is required<br>
**Then** Attach Rejected; general business ACL Nor should it be granted automatically on the basis of these self-reported fields alone.

### AC-021: Profile Roll back

**Given** Locally Accepted sequence 31 It's... Realm/Profile continuity key history<br>
**When** endpoint Return is still valid but sequence 30 It's... Profileor not old key Authorized new root<br>
**Then** Refuse with a security error. Do not switch. issuer/signaling endpoint.

### AC-022: Invitations are not URL Permissions

**Given** owner One-time issuance to members editor Invitations<br>
**When** Reuse of invitation after acceptance by member or only the same example obtained by third party URI<br>
**Then** Invitation to replay failed; third parties remain absent ACL permissions;examples URI Could not close temporary folder: %s secret.

### AC-023: Remote management of borders

**Given** Inventory Far end in Service Only granted AttachNot awarded host management<br>
**When** User click to start, stop or clear<br>
**Then** Actro Shows that remote host authorization is required, that no successful forgery, no local substitution is required Service.

### AC-024: Necessary support Actor Lost.

**Given** Front with Primary Service It's green, but it's a necessary supplement to the application data. Actor Packed Critical Undo<br>
**When** Undo event reaches any of the Host<br>
**Then** The node is red, the weakest is red, and relevant Capability revoked; not "majority" Actor "Secret" continues to show green.

### AC-025: Unknown evidence is not safe

**Given** Other Actor Both. `CONTROLLED`But a faraway end `ActorAssurance` Expired<br>
**When** Application rating<br>
**Then** Yellow as a whole or not available, not green; application data will not be entered by default Actor.

### AC-026: optional functional security isolation

**Given** Optional Index Actor And become `UNKNOWN`<br>
**When** Host Rejecting the side, withdrawing it. Capability, confirm that protected data are no longer in possession and exchange `structureId`<br>
**Then** index functionality disabled; green cannot be displayed until isolation is completed; map out and reset `Ready` The rest of the picture can be green again.

### AC-027: Dynamic dependence first verify

**Given** Primary Service Requesting a new one for the first time in operation Actor<br>
**When** The peer is not currently active AA Or hang up still<br>
**Then** Application data may not be sent; applications may not be shown as green.

### AC-028: Malicious Service Concealing dependency

**Given** Service Not a statement in the self-report. Actor<br>
**When** It's being activated by the controlled capacity. `Transfer(Actor)`<br>
**Then** The border remains unsettled and re-evaluated; self-reporting does not cover actual and effective reliance.

### AC-029: Safety map encyclopedia or limit

**Given** Support Actor Form call rings or map above strategy ceiling<br>
**When** Gather<br>
**Then** Ring heavy and stable with no-directed connectivity; over-limit return `GRAPH_LIMIT_EXCEEDED`Can't cut it and turn green.

### AC-030: Health and safety are at the top of the line

**Given** Actor It's functional, but it's a third party. in-process Native, and the other one WASM Actor The crashes, but the exit locks still work.<br>
**When** UI Show Status<br>
**Then** The former shows healthy + Yellow, the latter shows failed + Green (if it's still on the other side) Ready) may not be merged into a vague state.

### AC-031External: HTTPS It's not automatic.

**Given** Greened applications Workspace Content to a permitted domain name HTTPS API♪ But it's time API No proof of strict strategy.<br>
**When** Assessment of Africa Actor Reconnaissance<br>
**Then** It can only serve as a terminal for which the strategy is clearly authorized; if the strategy requires the validation of its follow-up, the item is `UNKNOWN`.TLS Hostname authentication cannot upgrade the application to green.

### AC-032: Native Capsule Constraints

**Given** Same native workload Separate by NativeCdylib and NativeCapsule Run<br>
**When** Capsule Independent before code execution OS sandbox, proscribe socket/Any File/Subprocesses and open only Host typed IPC<br>
**Then** NativeCdylib No way. `CONTROLLED`;Capsule It's practical. sandbox Evidence assessment. When any platform is not bound to enter into force Capsule I have to. fail closed Or keep unknown.

### AC-033: TURN The message cannot be decrypted.

**Given** Actor A with Actor B I'll have to do it in bad faith. signaling and TURN relay Communications<br>
**When** relay Record, replay or replace consultation messages<br>
**Then** relay Only minimally connected metadata can be observed. payload key or express; identity/handshake transcript When matching Actor edge Building failed.

### AC-034: CC No substitute Workload Constraints

**Given** Service ♪ Runs right through ♪ Attestation It's... confidential VM ♪ of the but ♪ workload Export with undeclared network<br>
**When** Calculate Assurance Vector<br>
**Then** compute confidentiality Yes. pass, but egress Yes `UNCONTROLLED`, applied in red; not due to hardware TEE You're a badass. workload Green.

### AC-035: Prove that the current session is mislinked

**Given** The assailants have another set of compliance. TEE New legal. Attestation Result<br>
**When** It's the one that... Result For the current Actor/runtime, policy, nonce or ephemeral key Different Connections<br>
**Then** Relying Party Here. subject/channel binding mismatch Reject, Do Not Send private/secret payload.

### AC-036: Release key after certificate of pass

**Given** secret Workspace Will be at a remote end confidential Actor Processing<br>
**When** workload/guest image/TCB appraisal Not yet adopted or adopted ephemeral key Not entered Evidence<br>
**Then** Key Broker Not Released DEK;only after adoption DEK Cover it to the proven. keyNormal Host Not specified DEK.

### AC-037: CPU with NPU Group Certificate

**Given** confidential VM Pass. CPU TEE Prove it, but... Agent - Put the word on it. tensor Send to Unproven firmware, device memory Or a secure link. GPU/NPU<br>
**When** secret policy Request compute confidentiality<br>
**Then** compute confidentiality Yes unknown, edge Do Not Receive secret; only complete composite attestation Satisfy policy I'll be back in a minute.

### AC-038: Policy Evidence by data category

**Given** Same WASM Actor No, I don't. CC Evidence, but identity, provenance, channel and egress closure Average pass<br>
**When** It's handled first. public Data, re-request processing requests CC It's... secret Data<br>
**Then** public Operation can be green;secret Fuck as yellow or red. Actor The single static level of the decision-making is covered by two types of decision-making.

### AC-039: Proof of expiry and revocation

**Given** Running confidential Actor It's... Attestation Result due, or vendor TCB advisory Undo its reference value<br>
**When** Next time. private/secret Operation or undo event arrival<br>
**Then** Stop New payload and key release, trigger re-attestation/rekey; old cannot be used indefinitely Result.

### AC-040: Legal response cannot be stolen Secret

**Given** workload Only To Front Legitimate return of the typified result Actor edge♪ And try to put ♪ secret Encoding public Fields<br>
**When** output schema/data class Over and out. declassification capability<br>
**Then** Host Reject output and will output control Mark as fail;the channel is legal and does not cover the data classification strategy.

### AC-041: Apply the example of the initial green label

**Given** New Application Instance Established Primary Service ActorId, Program/Workspace digest And initial candidate graph<br>
**When** All Actor/Edge/Storage/Compute Evidence Meet current policy<br>
**Then** Host Yes. `Ready` It's... `ApplicationAssurance` Show green and release protected data only after green lease. Conclusions `structureId` With the current state root, no. GraphRoot.

### AC-042: Declaration is the limit of power

**Given** Program Privacy Contract Not stated Actor roleNetwork origin or audit sink<br>
**When** workload Try to create a corresponding match while running edge or application capability<br>
**Then** Host Creating handle/Reject before sending data; no connection should be created before marking application as yellow/Red.

### AC-043: Auto-recovery of compliance

**Given** Contract Allow `ai-inference` role It's... cardinality Yes `0..n`, and requires each example to be satisfied `confidential-ai-v1`<br>
**When** Scheduler Requesting three different entries. ActorIdAnd... assurance Not weaker than baseline The reasoning. Actor<br>
**Then** Hangs the change node and re-evaluates; the colour is not weakened and the user is automatically restored and asked no questions. Creates undeclared role We have to fail. We can't rely on them. `LocalContinue`, but not until the application is green `Ready`.

### AC-044: Trustable Actor Edge Encapsulating Internal Subgraph

**Given** External reasoning services do not disclose internal Actor Figure, but local policy Trusted. Verifier Signed Certified Privacy Domain aggregate<br>
**When** aggregate It's... endpoint, profile, data class, evidence root, life and current session Match All<br>
**Then** Upstream you can put that on. domain Gathering as a node and drawing conclusions to the weakest; if only self-signed or scope If it doesn't match, then it's `UNKNOWN` or `UNCONTROLLED`.

### AC-045: Server Audit Log Recognizing

**Given** LLM Server It's... NVIDIA/Ascend Category accelerator Attestation Yes, but it may be retained at the upper level of service prompt/response Audit log<br>
**When** audit sink Declarations and measured config + sealed egress Or credible. Certified Privacy Domain Proof that only permitted fields,retention And read the subject<br>
**Then** Login entry maps and can be evaluated; only accelerator EvidenceNo proof of internal retention, or if an additional undeclared explicit log is required `UNKNOWN` or `UNCONTROLLED`.

### AC-046: no change in the chart but certification of revocation

**Given** The case chart is unchanged and has green markers, but later vendor Undo Current TCB reference value<br>
**When** Undo arrival Host / Authenticator<br>
**Then** The relevant powers are abolished and re-evaluated before the next protected operation; the old conclusions cannot be followed because the structure remains unchanged.

### AC-047: Structural change and downgraded security

**Given** Green Marker Example Requesting Original confidential LLM Actor Replace with Not provided compute Attestation Convergence between Actor<br>
**When** Host Create candidate graph<br>
**Then** (a) The change node is hung;compute confidentiality From pass Reduce to indeterminate, UI Show Replace Actor And affected data, which can continue under yellow only after risk is accepted. Colours remain yellow.

### AC-048: Structural changes but no change or enhancement of security

**Given** Green cut-off for one. `CONTROLLED` Actor Replace With identity, confinement, compute, storage, egress It's not weaker than the original. Actor<br>
**When** The evidence assessment for the new state was adopted and `Ready`<br>
**Then** Automatically restores and does not bomb user authorization; changes are written into local audits.

### AC-049: User cannot overwrite hard failure

**Given** candidate Actor It's... workload digest Inconsistent with the certificate or undeclared terminal egress<br>
**When** User tries to accept risk<br>
**Then** Host Still block and do not provide a button to wash the item green; users agree that hard failure or unknown cannot be raised to green.

### AC-050: Freeze unblocked control

**Given** graph mutation Freezing application face<br>
**When** Host Access Attestation, Reference Values And Shell Show downgrade diff<br>
**Then** The minimum control surface can be communicated, but the node is suspended and the relying party cannot process the applied data; old state payload Can't pass.

## 4. Compatibility acceptance matrix

Each published candidate should be covered at least:

| Dimensions | Group |
|---|---|
| Platform | macOS, Windows, iOS, Android |
| Service | local, remote |
| Network | loopback, direct WebRTC, TURN, offline, provider failover |
| Package | cached, first download, revoked, digest conflict, dev |
| Workspace | requirement: none/optional/required;provisioning: managed/user-selected;access: read-only/read-write;local/remote/locked Provider |
| Lifecycle | create, refresh, close, crash, restart, suspend, trash, restore, purge, migrate |
| Privacy | Ready ♪ Cracking the face, generational ♪LocalContinue, Program Privacy Contract, WASM Statement/Undeclared exports, exports,NativeCdylib, NativeCapsuleExpiration AA♪ Red-aided nodes, concealment of dependence,Assured Actor Edge, LLM Audit remittances,opaque HTTPSThe graph./Maximum limits, access,CC♪ The key is released, ♪CPU+GPU/NPU Group certification, output control, de-dispersion |

### 4.1 Platform capacity commitments

| Capacity | macOS | Windows | iOS | Android |
|---|---|---|---|---|
| Local Create/Attach | Complete | Complete | Front desk available | Front desk available |
| Local long-term online Host | Supported, controlled by user back-office strategy | Supported, controlled by user back-office strategy | No promises. | Only foreground service Commitment to conditionality |
| Far Attach | Support | Support | Support | Support |
| Backstage. Guest | pause/Recoverable | pause/Recoverable | System decisions, recoverable | System decisions, recoverable |
| local-folder Workspace | Support | Support | security-scoped provider | SAF/provider |
| Far Host Management | Yes. management capability Time | Yes. management capability Time | Yes. management capability Time | Yes. management capability Time |

Linux/General server host, cloud automatic movement, browser Web Version & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & / Realm federation Not Actro v1 Client commitments; they can be independent Actr Host Subsequent definition.

## 5. Product indicators

### 5.1 Core funnel

```text
URI/Program opened
→ package verified
→ user approved
→ Service created or attached
→ Guest interactive
→ reopened successfully
```

Concerns:

- (a) The first-time creation completion rate;
- Attach Success rate;
- Re-launching the recovery success rate;
- package (a) The reasons for the failure to verify;
- Realm/Network failure distribution;
- Guest/Service crash-free session;
- (a) Rate of denial of authority;
- Workspace Conflict rate;
- Error Create Duplicate Service Number (target) 0).

### 5.2 Privacy

Indicator default not to collect:

- Complete ActorId;
- Workspace Synchronising folder/Path/(a) Content;
- Guest route/query;
- RPC payload;
- User input.

Telemetry is default. Release indicators are obtained first and foremost by local aggregation, voluntary diagnostics and quality experiments without operational markings.

### 5.3 Capabilities calibration

- Use of each platform for publication reference hardwareFixed package fixture and re-emergible networks profile;
- At least for each one. 30 Colder./Hot running, report. p50, p95(a) Sample numbers and failure rates;
- “Interactive”Define Guest First Screen commit Completed, entered handler Registered and registered Primary Service contract (b) Available;
- Remote indicators reported separately direct with TURNNo, no, no, no. DNS, login or user-authorized time in Attach;
- Release report to retain original anonymous measurements and build digest, so that it can be compared.

## 6. Release threshold

### Developer Preview

- URI/Create/Attach (b) Local closed rings;
- fixed QuickJS;
- signed package;
- Service same-ID restart;
- Basic Workspace broker.

### Public Beta

- (a) Full platform;
- Networking Realm, P2P/TURN;
- (a) Jurisdiction and withdrawal;
- Fault injection,fuzz(c) Osmosis tests;
- Program/Service Management UI;
- Move and delete protection.

### v1 Stable

- All MUST (a) Need to pass;
- AC-001 to AC-050 Automation or repetitivability;
- (a) The high-risk audit of security is zero;
- crash-free and the recovery target meets the release threshold;
- Publication of the version of the agreement and the compatibility commitments;
- Transport, cancellation,key compromise And the data is restored. runbook Done.

## 7. Key target tracking

| Product objectives | Main requirements | Acceptance and inspection | Delivery phase |
|---|---|---|---|
| Create/Attach It's a no-show. | FR-URI-002—007, FR-SVC-007 | AC-001—004 | P0, P2-B, P3-A |
| Tab/Front/Service Unified Model | FR-SH-001—006, FR-FRONT-001 | AC-002, AC-008—010 | P2-A, P3-A |
| Accuracy, duration Actor Identity | FR-SVC-001—003, FR-NET-005—006 | AC-009, AC-011, AC-019 | P1-A |
| Publisher Supply chain with bag | FR-PKG-002—014 | AC-005, AC-016, AC-021 | P1-B |
| Realm/Letters are authorized but limited. | FR-NET-001—016 | AC-011, AC-012, AC-020—023 | P1-A, P3-B |
| Workspace Not UI/Remove Error | FR-SVC-005—016, FR-SEC-009 | AC-013—015, AC-023 | P2-B |
| Guest It's operational but not transboundary. | FR-FRONT-002—010, NFR-SEC-001—004 | AC-006, AC-007 | P2-A, P4 |
| Apply the example of a re-evaluation of the privacy snapshot and dynamic map | FR-SEC-014—059, NFR-SEC-007—014 | AC-024—050 | P0, P1-A, P2, P3-A, P4 |
