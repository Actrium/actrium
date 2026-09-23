# 13. Actro Privacy tags and diagrams are assembled

> Security syntax [docs/security](../../architecture/security/actrium/README.zh.md) is the same as the first sentence. Actro How to get the model down to Tab, Front, Actr Host and data. This chapter is changed in case of conflict without changing the current security model.

Mature:`Specified`.

## 1. What's the answer to this chapter?

Actro Green on address bar / Yellow / Red, one for the one.**No root application**Yes.**Current Ready**Top `ApplicationAssurance`No, it's not. Program Name, no. Marketplace The review was not "a whole-scale world existed at the same time."

```text
Tab ──Positioning──▶ Front Actor ──Found──▶ No root application CC(U(D))
                                      │
                                      ▼
                         ActorAssurance(Node, no self-signature)
                                      │
                                      ▼
                    ApplicationStateDigest + ApplicationAssurance
                                      │
                                      ▼
                         Ready? → Show green / Yellow / Red
                         Otherwise...    → Could not close temporary folder: %s / Revalidate
```

The Front Actor is the product entry point, not the graph root. The Primary Service is the Guest's application interface, not the security root.

The internal evidence dimension (identity, product, restraint, computing, access, storage, export, output) is the rating input. Users see only three colours; no colour is considered to be the current application security when the cut is not ready.

Channels, original language and remote certification are available. [14](14-privacy-primitives-and-attestation.md). The complete judgment and proof is in the [Form Model](../../architecture/security/actrium/formal/model.zh.md).

## 2. Colours and Facts

| User saw | Normative meaning | It's not written. |
| --- | --- | --- |
| Green | `Ready` And each node `CONTROLLED`The tunnel is working. Confirm no. `Retain` | Permanent authentication; full-scale world; software without loopholes |
| Yellow | Not red, and at least one necessary fact `UNKNOWN` | "Slightly safe"; leaked. |
| Red | Existing confirmed `UNCONTROLLED`(Incl. confirmed) `Retain`) | The system witnessed a leak. |
| Could not close temporary folder: %s / Revalidate | No chart or channel is valid or either member guarantees hang | Fourth colour; available risk to accept circumvention |

Risk acceptance only allows to continue running without changing the colour. Entering permissions (cam, file) to cross the colour. Multiple context assessments `ActorAssurance` The weakest item cannot be selected as a greener certifier.

Data belts get generations. The new configuration only works on new data; old data goes "state when it is acquired" ∩ The current statement.

## 3. Apply Graphics

Figure valid `Transfer(ActorId)` Formation: the target can be deciphered, the same as the other Principal, the declaration and state are currently valid. Drop the invalid side and ignore the direction and take the connect points. The loop is legal and you must find the side.

```text
structureDigest = H("actrium/structure/v1", EncGraph)
EncGraph    = actrium/graph/v1 Normative JSON(Sorted) ActorId With a positive side)
```

Do not enter the chart: only unimplemented installed Programcommons without effective reliance Realm ActorCross Principal Actor. Cross Principal Or invalid. Actor The export is still on the starter's score. `CONTROLLED`.

Not Actor Target (screen, file,Workspace, HTTPS APIYes Actr Host Forced remittance.TLS The host name only shows that it is sent to the domain, and does not prove that the field does not leak. Target categories are not selected by the declarant: they can be presented Actr Endpoint of identity must be pressed Actor Target rating and acceptance of cross-country Principal Rule, not rewritten. HTTPS Remittances bypass rating; export maintained when category cannot be determined `UNKNOWN`. User trusts a meta-syntax term "Accepts data out of the security range". Only transfers to the end-secret. TURN No graph.

Front The final conclusions of the nodal points are:

```text
R_A(Front) = join(Actro Host Platform facts, Keel Guest The export facts)
```

Actr Host I can do it. Guest The exit that can't be done is still this. Actor The exit. It's not just a comment. Guest.

## 4. State change

Do not submit in full-chart-distribution. Order: Locally submit new `ActorStateDigest` → Abolition of old power → Guaranteed Hanging → The hiking re-establishes the face.`ActorState` Unarranged revision number with state authority: The configuration rolls also have a new root, the old one that was abolished CapabilityThe path and conclusions will not be resurrected by a rollback.

```text
Depends(b, a)           := No-go-effective dependency edges
LocalContinue(b | C)    := ACTIVE ∧ Current AA Still applicable ∧ Not dependent C ∧ Do not display application green
```

Can't judge dependency, stop. `LocalContinue` It's... Actor You can continue working for yourself. No member can hang up and display green.

Add a new node: First, you can hang the relevant side and then transmit the application data after validation.

## 5. Workload How?

### 5.1 WASM

Judgement `CONTROLLED` At least at the same time: the product is consistent with the certified package;import enumerable; none ambient File System / socket / Process API; capacity is fully passed Actr Host;valid declaration is Manifest ∪ Actr Host ∪ Environmental inclusivities; each Actor Targeted maps are also evaluated; each non- Actor Remittances are recognized by strategy; evidence is not expired.

The measures are:

```text
declared ∪ host ∪ env − proven-impossible  = Valid declaration
observed ⊆ Valid declaration and mandatory
```

The business logic is not flawed.

### 5.2 Third parties to the same process Native

Fixed `UNKNOWN`(If confirmed to be bypassed broker And when you're out, `UNCONTROLLED`).manifest, signature, static scanning,API hook Can't get it green.

### 5.3 Native Capsule

Independent process, imposed before start-up OS sandboxNo original network / Any File / Subprocesses, only typed broker IPC , which is not subject to a rule of law. fail closed This proves that the export is closed and that the memory is not safe.

The platform remains the same:v1 WASM Green; third parties in-process Native Not green;desktop Capsule (a) The experimental level;iOS Third-party dynamics Native Do not do product capacity.

### 5.4 Actro Native Front Host

We can't put the platform on the ground. Actr Host The verdict is unknown, or all. Tab Never green.Host The conclusions are drawn from official signatures, installation integrity, version strategy and equipment certification; the same is true of yellow or red when installation cannot be verified or the version cancelled. Front Process as a whole.

Actr Host Yes. Guest Issued as evidence or as a certifier `ActorAssurance`, but **issuer ≠ subject**: Actr Host No self-signed final conclusions may be drawn.

### 5.5 External HTTPS

No, it's not. Actor. No authorization to send:`UNCONTROLLED`. The strategy requires validation of the internal handling of the end but no evidence:`UNKNOWN`. The user explicitly authorized the end-remittance can be `CONTROLLED`, which means that the nodal point is controlled, and not that the opposite end is internal. If the end point is not pressed Actor Assessment: presentation Actr Endpoint of identity must be pressed Actor Target modelling (see §3) ) ) cannot be used HTTPS The name of the remittance is authorized.

## 6. Declaration is input

`.actr` Manifest It's... `[actro.privacy]` with Native Capsule profile Still pending validation of input, not conclusion.Host It has to be compared. importsIf you have a real capacity, you can't. Undeclared and possible exports are found: confirmed red, unrecognized yellow.

## 7. Who issued the conclusion?

```text
Claim      The current instance enabled a constraint
Evidence   Material used to verify the claim
Assurance  Conclusions given by the certifying officer in combination with state, evidence, rules and user strategies
```

Actro Inside Privacy Agent The person must sign the certificate when the cross-process becomes the basis of security. `ActorAssurance`, be precise. `ActorId + ActorStateDigest`The relying party has used the test itself.

Release `ApplicationAssurance` Before: Full discovery, check for each current state, each AA The result is only a consistent reading.

## 8. Product object (magnifying current model)

From the old draft `Privacy Mark` / `GraphRoot` / `epoch` The material corresponds to:

| Old name | Current Object |
| --- | --- |
| Actor Privacy Mark | `ActorAssurance` Nodal conclusion |
| Apply Privacy Mark | `ApplicationAssurance` colour; presentation still required `Ready` |
| privacyGraphId + epoch | `structureDigest` + `ApplicationStateDigest` |
| GraphRoot | None.Front Positioning Only |
| Actr Host Auto-Sign Mark | Prohibition of final conclusions as assessed |

Icon (field names can be used) schema Repository version, not less semantic than the following table:

```json
{
  "object": "actrium/actor-assurance/v1",
  "issuer": "verifier:…",
  "subject": { "actorId": "…", "actorStateDigest": "…" },
  "qid": "actro.consumer.default",
  "ruleset": "…",
  "result": "CONTROLLED",
  "findings": [{ "effect": "Transfer(Actor(b))", "verdict": "CONTROLLED" }],
  "notAfter": "…",
  "signature": "…"
}
```

`ApplicationAssurance` Commitments `structureDigest`, Principal, Merkle Root (leaf bound) `ActorId + ActorStateDigest + H(AA)`. The members certify logarithmic paths.

The handshake must still check the peer before sending the application data `gate=ACTIVE`Current AAThe tunnel is bound by the two parties. `ActorId + ActorStateDigest`.`minimumTarget` Node conclusion, no old one. Mark Name.

## 9. System Protocol

The name may be retained and semantics are given to the object:

```text
actro.privacy.v1.GetActorAssurance
actro.privacy.v1.GetApplicationAssurance
actro.privacy.v1.SubscribeChanges
actro.privacy.v1.RequireReady
```

Attach The whole chart still needs to be completed. `Ready` It's... `ApplicationAssurance`. The application data is blocked directly by not scoring when the channel is not valid.

The scale of the figure is excessive: it cannot be cut off and greened; it cannot be maintained or yellow.

## 10. UI

Address Bar: Green / Yellow / Red, or "Safety not established" / Revalidate / The government has also blocked the communication.

Details must be visible:policy, Actor Number,workload The changes in configuration must indicate that the new declaration is only valid for new data.

We must not write the unknown as "safe" HTTPS The lock says "Applications don't leak" and "Prettys" healthy The site is also being run by the Ministry of Health.

## 11. The figure itself is privacy.

peer Defaults only include a summary of the required member ' s certifications and conclusions. Guest Do Not Take Complete ActorId Chart. Letters and telemetry cannot expand. The conclusion does not include Workspace Path,payload, token.

## 12. Error Code

```text
ASSURANCE_INVALID
ASSURANCE_EXPIRED
ASSURANCE_REVOKED
ASSURANCE_SUBJECT_MISMATCH
ASSURANCE_SELF_SIGNED
ASSURANCE_CONFLICT_WEAKEST
POLICY_UNSUPPORTED
GRAPH_INCOMPLETE
SNAPSHOT_NOT_CURRENT
GRAPH_LIMIT_EXCEEDED
ACTOR_SUSPENDED
UNDECLARED_EGRESS
GENERATION_VIOLATION
TERMINAL_NOT_AUTHORIZED
NATIVE_NOT_MEASURABLE
CHANNEL_INVALID
```

## 13. v1 Border

Support:WASM (a) Export measures;Actro Host As a platform TCB(No self-signature); third party in-process Native Keep unknown or red; compliance Capsule Evidence-based; dynamically validated and transmitted data; intergenerational;Ready Show.

(a) Non-commitment: the business logic is not flawed;opaque HTTPS Non-disclosure; none OS sandbox I'm gonna put a third party in there. Native Greening; global privacy rating; green is absolutely safe.
