# 14. Privacy, remote certification and credible algorithms

> Security syntax [docs/security](../../architecture/security/actrium/README.zh.md) This chapter only writes how the original language, certification and arithmetic enters the current assessment, without defining the colour or the root.

## 1. Objective

The application of green cannot come from a single technology or a single hardware vendor, but must come from a combination of verifiable and avoidable private language:

```text
Declaration (what do you want to allow)
  ↓
Enforcement (de facto only what to do)
  ↓
Evidence / Attestation(How far away do you know that the binding has entered into force)
  ↓
Policy Appraisal(Current data available)
  ↓
Privacy Graph Aggregation(The article as a whole) Actor Whether or not the data path is established)
```

This chapter answers:

- “Actor Network-wide encryption”(a) Specific guarantees of what;
- WASM, Native Capsule with Confidential Computing (b) Which issues are addressed separately;
- Agent Use CPU, GPU, NPU or how to prove the operating environment when the edge is calculated;
- Different hardware vendors' evidence is mapped to uniform Actro claims;
- When the key will be released;
- How many pieces of evidence enter? `ActorAssurance` the facts of the case (art.`CONTROLLED / UNKNOWN / UNCONTROLLED`).

## 2. Accurate product commitments

### 2.1 Do Not Use“The data is always a secret.”

The normal calculation must be specified within an operational boundary. FHE, MPC The Quakers are the only ones who have a problem with the Quakers.Actro Data should not be claimed to be secret at every point.

Recommended formulation and [Overview](../../architecture/security/actrium/01-overview.zh.md) Unanimously:

> Data exist in secret on protected links and storage; it should only be specified in a processing environment where the identity is known and the boundary meets the requirements. Green ensures only that the current level of readiness is met and the new configuration only binds new data.

“Protection throughout the life cycle”Division:

| Data Status | Main mechanisms |
|---|---|
| in transit | Actor Secure Channel, AEADIdentified, forwarded, protected from re-enactment |
| at rest | Workspace envelope encryptionEquipment Keystore/KMSBackup key policy |
| in use | WASM/Native confinement;sensitive data by policy Request TEE/CC |
| on output | Type results, data labels,declassificationRate/Aggregation/Audit |

### 2.2 “Network-wide encryption”the boundary of the

Actr We have to make sure every network is in place. Actor edge:

- Two-way authentication;
- Confidentiality and integrity;
- (b) Protection against re-emergence;
- Session Keys & Two Sides Actor/runtime (a) Identity binding;
- Use of temporary keys and support for consultations;
- signaling, TURN And normal relay Not held payload key;
- The transfer cannot be downgraded to an uncertified express.

But a distinction must be made between:

```text
Actor A ── encrypted edge ── Actor B ── encrypted edge ── Actor C
```

If B To process data,B It's a mandatory execution. It's a private-constructed requirement. Actor.A→C We can't skip the right link because it's encrypted. B Assessment.

Only not. payload It's... signaling/TURN/relay Not the data execution end. They can still see IP, time series, package length and connection, and“payload Encryption”Not equal to“Metadata Anonymous”.

## 3. Private Source Matrix

| Original language | Main opponent | What can you prove? | It doesn't prove anything. |
|---|---|---|---|
| Actor Secure Channel | Internet bugging, tampering, replaying,relay | payload Decrypted only by a certified endpoint | The endpoint will not be leaked after it's confirmed. |
| `.actr` Signature and provenance | CDN, mirror alteration, supply chain replacement | Run artifact Sources and digest | artifact There's no bad logic in itself. |
| WASM capability sandbox | Malicious./Error workload | importsRAM BORDERS AND POSSIBLE EXPORTS | Actr Host/kernels don't peek; recipients of exports are allowed to abuse |
| Native Capsule | Third parties native workload | native Process cannot be bypassed broker Use of original resources | Zero kernel loophole; and WASM Same portable, verifiable |
| TEE / Confidential Computing | Cloud administrators, host machines, hypervisors Waiting for infrastructure competition | Specifies the execution of the boundary data in use Hardware quarantine. | workload It's not a spontaneous leak. |
| Remote Attestation | Fake operating environment, error version | Some Attester It gives an acceptable view of the state of the environment. Evidence | The state automatically meets business policy |
| KMS / Key Broker | Unrecognized environment pre-empted | ♪ Just by ♪ appraisal , and the data key is available for the example. | After obtaining the key workload Behavior security |
| Workspace encryption | Disk, snapshot, backup leak. | Static data cannot be read without key | The data of the run-time is not leaked after decryption |
| Data tags and declassification | Over-output on a legal channel | What data allows to leave at what particle level? | The common algorithm automatically corresponds to the business minimization |
| DP / MPC / FHE | Specific statistics or multiple counting opponents | Reduce or avoid explicit exposure under qualified models | Universal, low-cost replacement for all common calculations |

Conclusions

```text
WASM ≠ CC
CC ≠ Attestation
Attestation ≠ Authorization
Encryption ≠ Endpoint Privacy
```

The best high-level combination of privacy is usually:

```text
signed WASM workload
inside attested confidential execution environment
+ Actor end-to-end channel
+ attestation-gated key release
+ Host-enforced capability/egress closure
```

## 4. Internal Assurance Vector

Show only green to users / Yellow / Red (or cannot be created), but not only one boolean value can be stored internally.

Every one. Actor runtime At least:

```text
ActorAssuranceVector {
  identity
  artifact_provenance
  workload_confinement
  compute_confidentiality
  channel_security
  storage_protection
  egress_closure
  output_control
}
```

Assessment of each dimension:

```text
pass | indeterminate | fail | not-required
```

Map:

| Dimension Results | The export facts |
| --- | --- |
| All policy-required Dimensions Average pass | `CONTROLLED` |
| At least one. required Dimensions indeterminate | `UNKNOWN` |
| At least one. required Dimensions fail | `UNCONTROLLED` |
| Non-current data/Operating requirements | Not participating in this gathering. |

Presentation: All `CONTROLLED` and `Ready` And the tunnel works. → Green; no red and unknown → yellow; any `UNCONTROLLED` → red;cut face not ready → Do not show the colour.

Not all of them. ActorAll data is mandatory. CCFor example. public Data allows in normal WASM Actr Host (a) Run up;secret The organizational strategy for data can be requested `compute_confidentiality=pass`.

### 4.1 The main green mark:Application Instance Snapshot

Conclusion binds current readiness rather than general ProgramIt's not a short time. transport session.Front / Primary Service Only for position maps, no access `structureId` When the root.

```text
ApplicationStateId =
  H(actrium/application-state/v1,
    structureId,          # EncGraph
    principal,
    MerkleRoot(ActorId + ActorStateId + H(ActorAssurance)))
```

`ActorStateId` Change, evidence or strategy can make old. `ApplicationAssurance` Expiry.Tab Reconnection,direct/TURN Switch (relay never explicitly) without changing the chart.

Program release It provides a static, audited state. Contract and evidence, helps to draw conclusions, but Catalog Show only "Specific Supports Which profileThe government has been working on the issue of the ‘Absorture’ in the past.

### 4.2 Program Privacy Contract

The application of the case structure can change dynamically, but its type of structure and capacity must remain static:

```text
static graph grammar
→ dynamic actor instances
```

Contract Freeze at least:

- Allowed to appear Actor role/type/release or assurance profile;
- required/optional, cardinality and possible establishment edge;
- Every one. edge (a) Portable data categories and operations;
- Actor/network/Workspace/storage/compute/audit capabilities;
- terminal egress, purpose, retention and consent;
- Requirements sandbox, CC, Verifier And trust. Privacy Domain profile;
- output/declassification Rules;
- The action of unknown or invalid evidence.

Indications:

```toml
[actro.privacy_contract]
schema_version = 1
contract_id = "notes.example/team-notes@1.4.2#privacy"
package_digest = "sha256:..."

[[actro.privacy_contract.actor_role]]
role = "primary-service"
actr_type = "notes.example/team-notes-service@1.4.2"
cardinality = "1"
required = true

[[actro.privacy_contract.actor_role]]
role = "ai-inference"
cardinality = "0..n"
required_assurance_profile = "confidential-ai-v1"

[[actro.privacy_contract.edge]]
source = "primary-service"
target = "ai-inference"
data_classes = ["private"]
capabilities = ["actor.call:infer.v1"]
```

The statement is not a statement, but a statement. Actr Host Top-level ability to deliver:

```text
Effective Capability =
  Declared Contract
  ∩ User/Organization Policy
  ∩ Host Policy
  ∩ Instance Grant
```

Undeclared Actor role, edge, audit sink, terminal, documents, networks or computing capabilities are not available handle. Could not use“Connect, report.”Expansion Contract.

Contract It's the effect of changing all the maps. capability It can be stopped before it is created. It is a border, not a green one.

### 4.3 State change

Do not submit changes in full-map distribution. Actor Local Atom Switch `ActorStateId`, synchronized with old keys, old channels and old Capability, the entry security is hung.`ActorState` The new state of affairs is also taken by the one-way revision: the configuration rolls back, abolition is durable, and old power is not restored by rollback.

```text
Change → SUSPENDED → Evidence / Assessment → Resume(To have current AA)→ Try Publish
```

You can not process or send application data to a member who does not depend on the change node. `LocalContinue` Old data cannot be rewrited to new exports.

Control face (college,attestation, UI; agreed) may continue; operational data may not be carried.

### 4.4 Compare downgrade with recovery

The change in structure or state does not necessarily bother the user.

```text
Colours are not weakened and strategy allows
  → Auto-renewal of related work (required) Ready I'm just showing green.

Colours yellow or red
  → Hold the block.
  → Show exact diff
  → Request for risk acceptance (no colour change)

A channel is invalid or touch platform hard bottom line
  → Block
  → User cannot overwrite
```

"Weaker" cannot read only colours. required Dimensions from `pass→indeterminate`, profile Degraded, change of faith,terminal / audit / retention Expansion, all downgraded. One dimension is not enough to offset another.

Risk acceptance binding Principal, object, cause, scope, state of the time, and strategy version. Agree to leave only the example in yellow. / Red-down running cannot turn the missing proof into green.

### 4.5 Which events count as structural changes

Changed structural identity or current status (and thus the old conclusion lapsed):

- Actor/Privacy Domain Add, replace, relocate or remove;
- Actor workload/config/Host/compute/accelerator assurance Changes;
- Add or Expand Actor edge, data class, purpose, terminal;
- audit/log/telemetry sink, field, reading subject or retention Changes;
- Workspace or Workspace Provider changes;
- capability or declassification Expansion.

No changes to the structure of the map (which may still be re-evaluated because of the expiry of the evidence):

- Same Actor/runtime/evidence Down transport reconnect;
- direct/relay/TURN Path Switches and relay Never. payload key;
- No privacy. claims Changes in health, delay or condensation indicators.

The same figure does not mean that the conclusion will never be the same. / TCB / - It's old. `ActorAssurance` Show must still be evaluated. `Ready`.

### 4.6 UI

| Location | Presentation |
|---|---|
| Catalog/Install Page | Program Supported privacy profiles, Contract and static audit information, do not show instance green |
| App Library | Last for each application `ApplicationAssurance`, state root and time of update (must indicate whether or not to remain Ready) |
| Active Tab Address Bar | The green side of the current readiness / Yellow / red; showing 'revalidating 'when re-evaluated or hung / "Communications have been blocked." |
| Degrade confirmation page | Add/Replace Actor, audit/terminal Change, weakest dimensions, impact data and functions |
| Details Panel | Current Actor/Edge, Assured Domain, Evidence* The present document was not edited before being sent to the United Nations translation services. |

## 5. Data Policy Evidence for decision

Indications:

```toml
[[actro.privacy.data_policy]]
class = "public"
required = [
  "identity",
  "artifact_provenance",
  "channel_security",
  "egress_closure"
]

[[actro.privacy.data_policy]]
class = "private"
required = [
  "identity",
  "artifact_provenance",
  "workload_confinement",
  "channel_security",
  "storage_protection",
  "egress_closure"
]

[[actro.privacy.data_policy]]
class = "secret"
required = [
  "identity",
  "artifact_provenance",
  "workload_confinement",
  "compute_confidentiality",
  "channel_security",
  "storage_protection",
  "egress_closure",
  "output_control"
]
```

Publisher Can declare minimum needs, users/Division can be tightened. Publisher Relax the platform or organizational bottom line.

## 6. Actor Secure Channel

### 6.1 Two-level protection.

Actr The network should provide both:

1. transport protection: e.g. TLS 1.3, QUIC or WebRTC DTLS;
2. Actor Secure Envelope: Tie Actor Semantic End to End AEAD.

The second tier is:

- Even if... signaling, TURNor a paragraph transport Replaced,relay Still not available payload;
- Tie the password endpoint to `ActorId + runtimeInstanceId`;
- - Put it on. privacy policy, attestation result and session (a) Tied together;
- It's not legal to just one. Realm credential Bring the traffic to another one. workload.

### 6.2 Handshake Tie

Shake hands. transcript Coverage of at least:

```text
protocolVersion
sourceActorId
targetActorId
sourceActorStateId
targetActorStateId
sourceRuntimeInstanceId
targetRuntimeInstanceId
sourceCredentialDigest
targetCredentialDigest
sourceEphemeralKey
targetEphemeralKey
requestedServiceActorId
policyDigest
attestationResultDigest
channelNonce
roles
```

Current situation of the parties `ActorStateId` We must enter the union. transcript and certified in two-way: only for the field MAC Not enough to resist the old state of greeting again. [Security agreement](../../architecture/security/actrium/03-protocol.zh.md) §3 and form model F-CHAN-01One of them. `ActorStateId` The old channel cannot carry the application data after the change and must be reshocked for the new state.

Actor Actr Host It has to be proven that it is. Actor key & Temporary Sessions key. If the target runs in TEE , the temporary decryption key must be TEE internal generation, or by TEE Safely held and entered attestation Evidence/Result.

### 6.3 Data Face

Every one. encrypted record:

- Use AEAD;
- - Yeah. session id, direction, epoch, sequence;
- Will Actor edge id, data type, message type as authenticated additional data;
- Reject nonce/sequence (a) Reuse;
- Testing duplication, disorder and epoch (a) Reversal;
- Regular or by volume rekey;
- private/secret No change of status may be requested without equivalent value 0-RTT Send.

### 6.4 Channel Evidence

```json
{
  "schemaVersion": 1,
  "edgeId": "edge_01...",
  "sourceActorId": "<ActorId>",
  "targetActorId": "<ActorId>",
  "sourceRuntimeInstanceId": "runtime_01...",
  "targetRuntimeInstanceId": "runtime_02...",
  "suite": "actr-channel-v1",
  "payloadProtection": "actor-aead-envelope",
  "forwardSecrecy": true,
  "antiReplay": true,
  "relayCanDecrypt": false,
  "transcriptDigest": "sha256:...",
  "peerAttestationResultDigest": "sha256:...",
  "sessionKeyThumbprint": "sha256:...",
  "issuedAt": "2026-07-29T12:00:00Z",
  "expiresAt": "2026-07-29T12:10:00Z"
}
```

“TLS Connected”It's not enough to generate it. Actor channel `pass`; must be completed Actor Identity and runtime/session Tie.

## 7. Confidential Computing

### 7.1 What does it solve?

CC The goal is to protect data in use, which makes code and data hardware-supported, verifiable Trusted Execution Environment Run in.

Its main value is to remove or diminish the powers of the following subjects from the privacy and trust borders:

- Cloud Platform Administrator;
- Actr Host OS / Hypervisor;
- Co-permit Other VM(a) The container or process;
- Some physical and equipment attackers.

Which rivals are specifically excluded, depending on the hardware and the profile, cannot be written in unison `cc=true`.

### 7.2 It doesn't solve anything.

CC Inner-run malice workload The data can still be sent to an open web target.

```text
compute_confidentiality = pass
```

No substitute:

```text
workload_confinement = pass
egress_closure = pass
```

I am not a writer.WASM The lockdown doesn't prove that the cloudkeeper can't read normal. Actr Host Memory.

### 7.3 Canonical Compute Claims

Different. TEE/CC backend Must map to unified claims, not expose a hardware vendor's boolean value:

```json
{
  "profile": "actro.compute-assurance.v1",
  "environmentType": "confidential-vm",
  "platformVendor": "vendor-id",
  "platformModel": "model-id",
  "teeType": "vendor-tee-profile",
  "isolationScope": "vm",
  "confidentialityScope": ["cpu-memory", "guest-state"],
  "integrityScope": ["firmware", "boot-chain", "guest-image"],
  "debugDisabled": true,
  "migrationProtection": "measured-and-policy-bound",
  "hostAdminExcluded": true,
  "workloadDigest": "sha256:...",
  "runtimeDigest": "sha256:...",
  "guestImageDigest": "sha256:...",
  "ephemeralKeyThumbprint": "sha256:...",
  "accelerators": [],
  "freshness": {
    "nonce": "...",
    "issuedAt": "2026-07-29T12:00:00Z",
    "expiresAt": "2026-07-29T12:05:00Z"
  }
}
```

Don't put it in the car. vendor/product name It's self-depreciation. `pass`. Only the corresponding Verifier profile, Endorsements, Reference Values, TCB Status and freshness Satisfied policy It's only then.

## 8. RATS Role Model

Actro Use of standard remote proof of role:

| Role | Actro Entities in | Duties |
|---|---|---|
| Attester | Actr Host TEEequipment, equipment,GPU/NPUTrustable launcher | Generates about Target Environment It's... Evidence |
| Verifier | Realm/Organizations/Locally. Verifier with vendor adapter | Validation of signatures, chain of certificates, values measured, reference values, withdrawals and freshness |
| Relying Party | Data release Actor Actr Host, Privacy Agent, Key Broker | Based on Attestation Result and local policy - Make a decision on the authorization. |

A strict distinction is required:

```text
Evidence
  + Endorsements
  + Reference Values
  + Appraisal Policy for Evidence
  → Attestation Result

Attestation Result
  + Data Policy
  → allow | unknown | block
```

Attester You can't give yourself the final word. `ActorAssurance`(`issuer ≠ subject`).Verifier Only a certificate assessment is given; final data release is still determined by the relying party in accordance with current strategy.

Evidence code priority compatibility EAT/CWT/COSE;manufacturing quote/report/document It may be sealed, but the original signature material and the results of the authentication must be retroactive.

### 8.1 Assured Actor Edge with Certified Privacy Domain

Cross-organizational or large cloud services do not necessarily disclose the full internal content to the caller Actor Figure.Actro Allowing credibility Verifier Seal a closed internal sub-graph into a single, sealed, sealed, sealed, sealed, sealed, sealed, sealed, sealed, sealed, sealed, sealed, sealed, sealed, sealed, sealed, sealed, sealed, sealed, sealed, sealed, sealed, sealed, sealed, sealed, sealed, sealed, sealed, sealed, sealed, sealed, sealed, sealed, sealed, sealed, sealed, sealed, sealed, sealed, sealed, sealed, sealed, sealed, sealed, sealed, sealed, sealed, sealed, sealed, sealed, sealed, sealed, sealed, sealed, sealed, sealed, sealed, sealed, sealed, sealed, sealed, sealed, sealed, sealed, sealed, sealed, sealed, sealed, sealed, sealed, sealed, sealed, sealed, sealed, sealed, sealed, sealed, sealed, sealed, sealed, sealed, sealed, sealed, sealed, sealed, sealed, sealed, sealed, sealed, sealed, sealed, sealed, sealed, `CertifiedPrivacyDomain`:

```text
Actor A
  └── AssuredActorEdge
      └── Certified Privacy Domain B
          └── internal Actor graph"No direction A Expand)
```

The boundary certificate contains at least:

- domain/endpoint identity;
- Type of data accepted,purpose and operations;
- Observable exits,retention, audit and deletion contract;
- Internal privacy graph/evidence Merkle root;
- assurance profile and does not cover the attack surface;
- Verifier/issuer, trust chain, policy digest;
- workload/service release Scope;
- freshness, revocation and incident obligations.

Upstream policy Trust in the right. Verifier and profile , you can participate as a proven node. meetNo more re-entry inside the proliferation. unknown. This is not to eliminate the unknown, but to encapsulate the responsibility for internal certification into a clear and revocable subject of trust.

Service Auto-Sign“I'm safe.”Can't form Assured Edge. Only users/Organizations/Realm policy Trusted VerifierCertificate scope With Current endpoint/session Only when you match.

## 9. Attestation Sessions/Key binding

### 9.1 It has to be prevented.“It proves it's true. The connection is fake.”

Legal Evidence If no current binding sessionThe assailants may have put another one in order. TEE The certificate was forwarded and the data was then directed to an environment that was not protected.

And so... attestation challenge At least bind:

```text
fresh nonce
+ ActorId
+ current ActorStateId
+ runtimeInstanceId
+ workload/package digest
+ policy digest
+ ephemeral channel/decryption public key
+ intended use / audience
```

### 9.2 Attestation-gated Key Release

private/secret Data recommended for adoption:

```text
1. Scheduler Select Candidate Compute Actor
2. Relying Party Send nonce and policy digest
3. TEE Internal Generate ephemeral key
4. Attester Generate binding workload, key, nonce It's... Evidence
5. Verifier Generate short-term Attestation Result
6. Relying Party Inspection Actor/edge/data policy
7. Key Broker - Put it on. Workspace/session DEK Encapsulated for attested ephemeral key
8. TEE Unlock and start processing
9. Result Due,TCB Withdrawal or withdrawal graph When change re-attest/rekey
```

It didn't pass. appraisal , the key cannot be released to normal first. Actr Host, again Actr Host Forward TEE.

## 10. GPU, NPU And the isomeric force.

### 10.1 Composite Attestation

Agent Use GPU/NPU ♪ And then, just ♪ CPU TEE The privacy boundary may include:

```text
CPU TEE / Confidential VM
├── Guest kernel
├── runtime / container
├── driver
├── IOMMU / device assignment
├── CPU↔accelerator link
└── GPU/NPU
    ├── device firmware
    ├── CC mode
    └── device memory
```

The blogger says that the government is not a party to the law, but that it is not a party to the law.`compute_confidentiality` Yeah. `indeterminate`.

### 10.2 Accelerator Claims

```json
{
  "deviceClass": "npu",
  "vendor": "vendor-id",
  "model": "model-id",
  "deviceIdentityDigest": "sha256:...",
  "firmwareMeasurement": "sha256:...",
  "driverMeasurement": "sha256:...",
  "confidentialMode": true,
  "deviceMemoryProtected": true,
  "hostLinkProtected": true,
  "iommuBoundToTee": true,
  "debugDisabled": true,
  "attestationEvidenceDigest": "sha256:...",
  "endorsementChainDigest": "sha256:..."
}
```

If accelerator No equipment level certification, or CPU→The equipment will be transferred in normal. Actr Host/Driver-exposure express:

```text
compute_confidentiality = indeterminate
reason = ACCELERATOR_CONFIDENTIALITY_NOT_PROVEN
```

### 10.3 Vendor compatibility

Actro No, no, no. Intel, AMD, Arm, NVIDIAHua Zhuo Ascend Or a cloud service is written as a fixed trust conclusion and is used:

```text
vendor evidence
  → versioned verifier adapter
  → canonical compute claims
  → organization/Realm appraisal policy
```

Every one. adapter It is important to state that:

- Supported hardware, solidware, driver and deployment enhancements;
- vendor trust roots;
- Evidence Format and signature algorithms;
- Endorsements and Reference Values (a) The means of access;
- TCB advisory/(a) Undo the source;
- (b) The face of attack that can and cannot be protected;
- freshness and nonce Semantics;
- Test vector and negative test;
- adapter Sign, version and undo.

A hardware vendor's marketing material can only be used to detect capabilityIt can't be. `CONTROLLED` basis. Only when all the public or organizationally validated authentication interfaces, reference values, certificate chains and repetitious negative testing are available production profile.

## 11. Workspace With static data

### 11.1 Envelope Encryption

Every one. Workspace/Service At least use independence Data Encryption Key:

```text
Workspace data ── DEK
DEK ── KEK in local Keystore / organization KMS / Realm-approved KMS
```

Requests:

- ActorId Not decryption keys;
- DB Save Only ciphertext, key id And necessary metadata;
- backup/snapshot (a) Keep encryption;
- Service Delete, equipment cancel or policy Change is avoidable KEK/DEK Visits;
- key rotation No re-profiling requested ActorId;
- Same log, index, cache and temporary file followed data class.

### 11.2 CC scene

If policy Request confidential compute:

- KMS policy We have to check. Attestation Result;
- DEK Only for proven. ephemeral key;
- Decryption only occurs at the performance boundary that is certified to be covered;
- snapshot, migration and accelerator offload (a) Must be covered by the certificate;
- Proofing expiry does not require immediate loss of active status of memory, but grace window After that, we must stop receiving new ones. secret and re-attest/rekey.

### 11.3 Audit log

Servers must keep the audit log itself static Contract In-Focus storage/audit sink:

```toml
[[actro.privacy_contract.audit_sink]]
kind = "security-audit"
fields = ["actor-pseudonym", "operation", "decision", "timestamp"]
payload = false
data_classes = ["metadata"]
retention = "30d"
operator_visibility = "authorized-security-admin"
encrypted = true
```

Actr Host It has to be a log. capabilityfields schema, target, encryption, retention period, reading the subject and exporting behavior into the private map. payload, prompt, Workspace Content, model input output or stable user identifiers must be genuine data class Re-evaluate, not all.“Audit”Go around the strategy.

Attestation Could not close temporary folder: %s workload/Environment,Host broker It can be proved that only statements are issued. audit capability; it can't just be based on“The server has an audit log.”Proves that the server has no other hidden records.

- WASM/Native confinement It proves that it didn't go around. broker (b) Other log exports;
- CC Prove it. Actr Host/Cloud managers cannot read and record their memory outside the border;
- audit sink Their own storage, administrator and exporter continue to be closed node or Certified Privacy Domain;
- Undeclared logs,Actr Host Class specified or unproven retention And make the facts `UNKNOWN` or `UNCONTROLLED`.

Yeah. LLM Server and GPU/NPU In particular, a distinction is made between:

- accelerator Attestation Usually prove the identity of the equipment, the solid,CC mode - state of equipment, not automatically certifying the upper layer LLM Server No, I didn't. prompt/response Writing logs;
- If logs leave the attested boundary through a controlled file, object storage, the network or an Actor capability, the Actr Host must observe an audit edge;
- If log logic is measured workload The government is not a party to the law.workload/config digest It's a proof.“Which log code and configuration is running”But it's still closed. egress And trust. Verifier The blogger says that the government is not in a position to prove that there are no other words.
- If it is, opaque Third parties LLM APIExternal calls cannot be made by the only means GPU Proof of technical testing for internal retention; reliance only Certified Privacy Domain Yeah. retention/audit Yes. scope Proof. No such certificate is available. `UNKNOWN`.

So...“The server left an audit log.”Not natural. Only the log exits are made. capability Or the entire service boundary log./retention Authentication provided aggregateIt just entered. Actro Proven private maps.

## 12. Output and secondary leakage

“No original socket”It's not a precaution. workload - Put it on. secret I'm gonna put it in a legal one. Actor Response.

Therefore, the high-sensitivity data also requires:

- Input, Internal Values and Output schema - Yeah. data class;
- Cross-category output requires a visible declassification capability;
- Actr Host (a) Validation of structure, size, objectives and uses;
- Aggregation query can be requested k-anonymity(i) Disparity of privacy budget or minimum sample number;
- model inference Clear prompt, embedding, cache, tool result and telemetry Classification of the
- - Yes, high bandwidth. covert channel (b) Quotas, speed rates and call shape limits;
- It can't be completely closed. timing/size/side-channel Yes. assurance Medium-notation disclosure.

This still does not automatically support the minimization of data from any business algorithm; high-risk processes need to be tailored policy, test or formalization models.

## 13. Evidence Bundle

`ActorAssurance` The reference, not all of the inline originals:

```json
{
  "schemaVersion": 1,
  "bundleId": "aeb_01...",
  "subject": {
    "actorId": "<ActorId>",
    "runtimeInstanceId": "runtime_01...",
    "packageDigest": "sha256:...",
    "workloadDigest": "sha256:..."
  },
  "policyDigest": "sha256:...",
  "assurance": {
    "identity": "pass",
    "artifactProvenance": "pass",
    "workloadConfinement": "pass",
    "computeConfidentiality": "pass",
    "channelSecurity": "pass",
    "storageProtection": "pass",
    "egressClosure": "pass",
    "outputControl": "not-required"
  },
  "evidenceRefs": [
    {
      "kind": "keel-wasm-closure",
      "digest": "sha256:..."
    },
    {
      "kind": "rats-attestation-result",
      "profile": "vendor-profile@1",
      "digest": "sha256:..."
    },
    {
      "kind": "actor-channel-evidence",
      "digest": "sha256:..."
    }
  ],
  "binding": {
    "actorStateId": "…",
    "structureId": "…"
  },
  "issuedAt": "2026-07-29T12:00:00Z",
  "expiresAt": "2026-07-29T12:05:00Z",
  "signature": {
    "algorithm": "Ed25519",
    "keyId": "privacy-key-3",
    "value": "<base64>"
  }
}
```

Original hardware Evidence It may contain sensitive data such as the device serial number. peer Defaults only get:

- Attestation Result;
- canonical claims (a) The necessary subsets;
- Evidence digest;
- Verifier identity;
- profile/version;
- freshness;
- Uncheck the status.

## 14. Compositing algorithms

For Transport Category `d` It's... edge `e = A → B`:

```text
Required(d, operation, organizationPolicy)

EdgePrivacy(e, d) = meet(
  RequiredClaims(SourceActor A),
  ChannelEvidence(e),
  RequiredClaims(TargetActor B),
  ComputeEvidence(B, d),
  StorageEvidence(B, d),
  EgressAndOutputPolicy(B, d)
)
```

Application as a whole is no root figure nodes \(R_A\) the weakest item and display of the mustache `Ready`.

Rules:

- Only current data and operations required The dimensions of the project are brought together;
- Missing / Expiry / Unsupported Evidence Yes. `UNKNOWN`;
- Signing failed,measurement Not matching, revocation or strategic rejection is `UNCONTROLLED`;
- Strengths cannot offset either. required fail / indeterminate;
- Figureworkloadchange of equipment or session material `ActorStateId` / `ApplicationStateId`;
- The old-fashioned. Attestation Result No new side can be authorized.

## 15. Control Component

```text
Actro / Actr Privacy Control Plane
├── Policy Registry
│   ├── data class policy
│   ├── organization overrides
│   └── accepted assurance profiles
├── Evidence Collectors
│   ├── Host/WASM/Native
│   ├── Channel
│   ├── Workspace
│   └── CPU/GPU/NPU attesters
├── Verifier Gateway
│   ├── RATS/EAT
│   └── versioned vendor adapters
├── Reference Value & Endorsement Store
├── Revocation/TCB Advisory Feed
├── Privacy Graph Evaluator
├── Key Broker / KMS Adapter
└── Audit & Transparency Log
```

The Realm signing key holder (the owner of the Realm domain) or a Realm Provider it delegates to may operate or delegate Verifier, Reference Value, Revocation and Key Broker services, but it is not the only global root of trust; an individual instance Realm only chooses in its own policy which Verifiers to trust. Organisations and local Actro can choose their own Verifier and appraisal policy.

## 16. Failed and Undo

The following events must trigger a re-evaluation:

- workload, package, guest image, driver, firmware digest Changes;
- Actor runtime (a) Reconstruction or relocation;
- accelerator Add/Remove;
- channel re-handshake or endpoint key Changes;
- Attestation Result/Endorsement (a) Due;
- vendor TCB advisory or key revocation;
- organization policy Changes;
- Workspace key rotation;
- `ActorStateId` or `structureId` Change.

Actions:

```text
indeterminate → Stop sending new private/secret♪ Into ♪ re-attestation
fail          → Close edge/ Withdrawn capabilityStop. key release
recovered     → New nonceNew ResultTie it up. `ActorStateId` Channels and Keys
```

It can't be because Verifier For the time being, the certificate of indefinite cache continues to be used. `expiresAt`, Attestation Result with Capability Local clock used for validity is not available——The time clock to which the operator is bound and the assessor is called back to allow the expired material to be reaccepted.

## 17. Stable Error Code

```text
CHANNEL_IDENTITY_BINDING_MISMATCH
CHANNEL_ATTESTATION_BINDING_MISMATCH
CHANNEL_REPLAY_DETECTED
CHANNEL_DOWNGRADE_REJECTED

ATTESTATION_EVIDENCE_INVALID
ATTESTATION_SUBJECT_MISMATCH
ATTESTATION_NONCE_MISMATCH
ATTESTATION_POLICY_MISMATCH
ATTESTATION_RESULT_EXPIRED
ATTESTATION_TCB_REVOKED
ATTESTATION_PROFILE_UNSUPPORTED
ATTESTATION_VERIFIER_UNTRUSTED

COMPUTE_CONFIDENTIALITY_NOT_PROVEN
ACCELERATOR_ATTESTATION_INCOMPLETE
ACCELERATOR_CONFIDENTIALITY_NOT_PROVEN

KEY_RELEASE_POLICY_NOT_SATISFIED
KEY_RELEASE_TARGET_KEY_MISMATCH

OUTPUT_DECLASSIFICATION_REQUIRED
OUTPUT_POLICY_NOT_SATISFIED

PRIVACY_CONTRACT_INVALID
PRIVACY_CONTRACT_DIGEST_MISMATCH
PRIVACY_CERTIFICATE_EXPIRED
PRIVACY_CERTIFICATE_REVOKED
PRIVACY_RUNTIME_GRAPH_OUTSIDE_CONTRACT
PRIVACY_CAPABILITY_NOT_DECLARED
PRIVACY_GRAPH_MUTATION_PENDING
PRIVACY_DOWNGRADE_CONSENT_REQUIRED
PRIVACY_DOWNGRADE_GRANT_INVALID
PRIVACY_HARD_FLOOR_NOT_SATISFIED
PRIVACY_DOMAIN_CERTIFICATE_INVALID
PRIVACY_DOMAIN_SCOPE_MISMATCH
PRIVACY_AUDIT_SINK_NOT_DECLARED
```

## 18. Order of delivery

### P0: Unique semantic

- Actor Secure Channel requirements;
- Assurance Vector;
- RATS Role and Evidence Bundle;
- data class→required claims policy;
- evidence freshness, binding, revocation.

### P1: Local and ordinary clouds

- WASM egress closure;
- Workspace envelope encryption;
- package provenance;
- Non-auto-signed `ActorAssurance`(Actr Host assessable Guest, no sign-in Actr Host);
- No request. CC It's... public/private policy.

### P2: Confidential CPU

- First production TEE/CVM profile;
- Verifier adapter;
- attestation-bound session key;
- KMS/Key Broker (a) Release on condition;
- migration, snapshot, revocation tests.

### P3: Confidential Accelerator

- CPU+GPU/NPU composite attestation;
- driver/device/firmware/reference values;
- secure device assignment/link/memory claims;
- Conformance suite for each hardware vendor adapter.

### P4: Advanced output privacy

- declassification;
- DP budget;
- MPC/FHE Specific workload profile;
- side-channel assurance disclosure.

## 19. References

- [RFC 9334: RATS Architecture](https://www.rfc-editor.org/rfc/rfc9334.html)
- [RFC 9711: Entity Attestation Token](https://www.rfc-editor.org/rfc/rfc9711.html)
- [RFC 8446: TLS 1.3](https://www.rfc-editor.org/rfc/rfc8446.html)
- [RFC 9180: HPKE](https://www.rfc-editor.org/rfc/rfc9180.html)
- [Confidential Computing Consortium](https://confidentialcomputing.io/)
- [NVIDIA Confidential Containers: CPU + GPU composite attestation](https://docs.nvidia.com/datacenter/cloud-native/confidential-containers/latest/attestation.html)
- [The clouds are so strong QingTian Enclave Password certificate](https://support.huaweicloud.com/intl/zh-cn/twp-ecs/ecs_twp_0010.html)
- [Retrenching communities: based on TEE With remote proof AI Credible calculations](https://www.hiascend.com/activities/dynamic-news/733)
