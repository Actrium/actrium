# Actrium v1 Product demand statement

> Status: Draft target design (P.S.)Target State Draft)<br>
> Document version:0.1<br>
> Date:2026-07-29<br>
> Scope of application:Actrium, Actr, Keel Next stage of product and protocol design<br>
> Principle: Define the ideal model before adjusting the existing realization; this paper is not premised on historical compatibility

## 1. Definition of a sentence

Actrium Yes. Actrium The world.“Program Browser”: User in resident app Tab Running by Keel Guest Description of original interface, each resident app Tab A trusted one. Front Actor Carrying and adopting a clear Service Actor Status of access procedures,Workspace with Actor Network.

## 2. Overall conclusion

The revised set is well-directed, unified and technically feasible:

```text
Tab
└── Front Actor
    ├── Actrium Fixed Native Workload
    │   ├── Keel Host
    │   ├── Capability Broker
    │   └── Actr Client
    ├── Publisher Provided Keel Guest
    └── Primary Service Actor ── Workspace
```

Its main value is not to draw fewer layers, but to make each layer assume only one responsibility:

- Tab is an interactive user session.
- Front Actor Yes, it is. Actrium Controlled front-office secure borders.
- Keel Guest is the only pre-office code that the program developer can customise.
- Service Actor There are examples of state-of-the-state programs, too. Front Enter Actor The only main entrance to the world.
- Workspace By Service Hold on,Front Do not touch path, key or bottom storage.
- An `ActorId` has the form `<serial>.<label>@<domain>`; it carries its own Realm name, identifies the Service instance exactly and contains no program type; once an `ActorId` is known, it must not be load-balanced.
- `publisher/program@version` Accurately identify the unchangeable version of the program.
- A Realm corresponds to one application instance and governs that instance's identity, admission and network entry; the domain is only used to name Realms; signaling and TURN are shared infrastructure delegated by the Realm.

But it relies on three basic capabilities that must be completed first:

1. **Persistent, resolvable ActorId**: a Service Actor must keep the same `ActorId` across restarts, network loss, credential renewal or migration, and the Realm name must be derivable from the ID. Today's Actr hard rebind may change the ID, and the ID only carries a numeric Realm internal to the Realm Provider, so it cannot directly serve as long-term, shareable URL semantics.
2. **Domain-name Publisher**: Actr's publisher semantics and length limits must change so that a normalized DNS domain becomes the program publisher's identity.
3. **It's really closed. Front Host**: Actrium It must be. Keel . . . . . . . . . . . . . . . . . . . . . . . . . . . . . . . . . . . . . . . . . . . . . . . . . . . . . . . . . . . . . . . . . . . . . . . . . . . . . . . . . . . . . . . . . . . . . . . . . . capability, white list of components, resource quotas and identified sandbox backend limits Guest;cannotsat affix URL Just give it to me. Keel Load.

## 3. Core decision-making for freezing

| Numbering | Decision-making |
|---|---|
| D-001 | Every application in a hold on load, running or front desk Tab Just run one. Front Actor; being Shell Abandoned but retained dormant Tab I don't think so. live Actor. |
| D-002 | Every one. Front Actor The state of operation happens to bind one. Primary Service Actor. Create/Temporary zero is allowed during the connection. |
| D-003 | “1:1”Yes. live Front The only binding of the angles, not the one. Service And history Tab (b) Permanent double-fire;Service Alive and can be replaced. Front Reconnection. |
| D-004 | v1 Default use of monogamous interactive sessions; same Service URL Focus now when this machine is open again. Tab. Sharing sessions is a follow-up and visible capability. |
| D-005 | Service Tie zero or one. Workspace;required more Actor Procedure by Primary Service In the back. |
| D-006 | Front workload By Actrium It's a natural.Publisher Can not replace Actr Host;Publisher Only if you're available. Keel Guestresources, statements and Service workload. |
| D-007 | v1 Third parties Front Guest Open only Keel JS Guestand fixed-use with hard timeout and stack caps QuickJS Backend. |
| D-008 | One. Actrium Program Release It's a signature. `.actr` Package:Service workload Is the subject of enforceability,Keel Guest It is a package of resources protected by a signature. |
| D-009 | `actr://publisher/program@version` indicating creation;adding `service=<ActorId>` is only connected, and cannot be built by sneaking up. |
| D-010 | Publisher Identity is a standardized domain name, without port; credible Publisher Profile Actual HTTPS, WSS, STUN, TURN End points can be used in a visible manner. |
| D-011 | Marketplace It's a source of information, not a source of information. Publisher Name authority; domain name and package signature are the source chain. |
| D-012 | A v1 ActorId must contain a verifiable Realm name (`<label>@<domain>`, e.g. `notes@acme.com`), and no program type; it is a public address and cryptographic identity, not a bearer token: knowing the URL does not grant the right to connect. |
| D-013 | A Realm is a logical trust domain corresponding to one application instance and may delegate multiple signaling/TURN services; a Realm is not the same thing as a physical signaling process, and one Realm Provider hosts many Realms; a Realm is bound to one Realm Provider when it is created and can never switch. |
| D-014 | Service Examples fixed to exact version and package digest;v1 Upgrade Creates new by Visible Migration Actor, instead of replacing it. workload. |
| D-015 | Realm credential Prove it. Actor keyand claims, does not automatically prove that the other side is actually running the official Actrium Host;Service We have to. Front Consider it independent of the letter client. |
| D-016 | A Workspace's requirement, provisioning, access and allowed Workspace Providers (`providers`) are independent dimensions and must not be merged into one enum. |
| D-017 | Remove as Restoreable Trashed And irrecoverable. Purged/Deleted;deleting Workspace Always acting independently. |
| D-018 | Domain bootstrap for Publishers and Realm domains always uses HTTPS 443 (the `https://<domain>/.well-known/actr` authority descriptor); a Realm Profile comes from the HTTPS location declared by the core relation `realms` (ports allowed); actual endpoints delegated by a verified Realm Profile may carry ports. The `realm_signing_keys` member and the `realms` relation are defined in Actr Authority Discovery §4.3–4.4; when a Realm Profile cannot be fetched or verified, clients must not use any Realm Provider other than the Realm Profile's. |
| D-019 | Application safeguards are aggregated on a no root-effective basis; access is effective and `Ready` , displays the weakest item of the colour to take all nodes from.Front Just locate, not Tugan. |
| D-020 | `ActorAssurance` The issue is issued by the certifier.`issuer ≠ subject`.Host assessable GuestEvidence may be provided and no self-signed final conclusions may be made. |
| D-021 | The top floor is green only. / Yellow / Red; the cut is not a fourth colour when it is not ready. Unknown is not a confirmed leak, but must never be green. |
| D-022 | WASM Yes. import / Effectiveness / Export closed-convey measures; third-party co-processing Native No green. Compliance. out-of-process Native Capsule It's possible to raise it. |
| D-023 | Conclusions must be linked to the data face: hang up to Africa `LocalContinue` Members may not process applied data; the new configuration only works on new data. |
| D-024 | User interfaces are three colours, internalally maintained identity, provenance, confinement, compute, channel, storage, egress and output Wait for the multidimensional facts to be mapped `CONTROLLED / UNKNOWN / UNCONTROLLED`. |
| D-025 | Actr ♪ Every network ♪ Actor edge (b) Must be encrypted, authenticated, protected from integrity and protected from re-entry;signaling, TURN & Normal relay Not held payload key. |
| D-026 | “Encryption of the whole process”This means transmission and static storage encryption, which is only available policy The data shall not be declared secret during the ordinary calculation period. |
| D-027 | Confidential Computing Protection data in use, WASM/Native confinement Constraints workload Exports; the two are cross-cutting and non-replacement. |
| D-028 | Remote certification is used Attester, Verifier, Relying Party (b) Division of labour;Evidence Not a trust conclusion. The final authorization is local. data policy Decision. |
| D-029 | private/secret Decrypt key only if required attestation Enclosed to pass Evidence Binding interim implementation environment key. |
| D-030 | Use GPU/NPU It's... Actor It has to be done. CPU, Guest, driverequipment, equipment,firmware, IOMMU/link/memory ; when any of the specified links cannot be proved compute confidentiality Yes unknown. |
| D-031 | Green binding current readiness face (`ApplicationStateDigest` + `ApplicationAssurance` + `Ready`No, it's not. Program The whole version, not a short one. transport session. |
| D-032 | Program Privacy Contract Still. Actor The static upper boundary of graphs and capabilities is limited to candidate maps and does not produce greens directly. |
| D-033 | Initial diagrams must be evaluated before release of protected data or decryption key; conclusions bound to the current `ActorStateDigest`, `structureDigest`The government has also been involved in the development of the project. |
| D-034 | `ActorStateDigest` Locally hangs and abolishes old powers when changes occur; members who do not depend on the change nodes can `LocalContinue`, but not green applications. |
| D-035 | The colour is not weakened and automatically recovers; the weakness is maintained and the risk acceptance is requested (no colour changes); the user cannot overwhelm the channel when it is not valid or when the platform is touched by a hard bottom line. |
| D-036 | Lettered Verifier You can put it inside. Actor Subchart Envelope As Certified Privacy Domainand through Assured Actor Edge Provided scopeand cancellations aggregate;symbol invalid. |
| D-037 | Audit log is visible storage/audit sink, must declare fields, data categories, targets, read subjects and retention;LLM/GPU Attestation It can't prove it. Server No separate logs are kept. |

## 4. Document Navigator

The recommendations are read in order:

1. [Product definition](01-product-definition.md): Why, for whom, what, what, what.
2. [Area model](02-domain-model.md): Tab, Front, Service, Workspace, Program, Realm Relationship.
3. [User Experience](03-user-experience.md): Restore from address bar to library, experiential library and error.
4. [URI And parsing norms](04-uri-and-resolution.md): `actr://` Syntax:/Connect semantics and regularizes.
5. [Packages and HTTPS Distribution](05-program-package-and-distribution.md)Domain name Publisher, `.actr` Packages, markets and updates.
6. [Run-time with Front–Service Agreements](06-runtime-and-protocol.md): Native Host, Keel Guest, the capacity boundary conversation agreement.
7. [Realm♪ By letter, by letter, by letter, by letter, by letter, by letter, by letter, by letter, by letter, by letter, by letter, by letter, by letter, by letter, by letter, by letter, by letter, by letter, by letter, by letter, by letter, by letter, by letter, by letter, by letter, by letter, by letter, by letter, by letter, by letter, by letter, by letter, by letter, by letter, by letter, by letter, by letter, by letter, by letter, by letter, by letter, by letter,TURN And identity.](07-realm-network-and-identity.md): chain of trust, network discovery and port policy.
8. [WorkspaceLife cycle and local storage](08-workspace-lifecycle-and-storage.md): Attribution, creation, reconnection, deletion and migration.
9. [Security and privacy](09-security-and-privacy.md)Threat model, access, sandbox and secret management.
10. [Demand and acceptance criteria](10-requirements-and-acceptance.md): Functional needs, non-functional needs and end-to-end acceptance.
11. [Delivery plan and feasibility](11-delivery-plan.md): Basic adaptation, phasing, risk and conditions of exit.
12. [Reference protocol and sample](12-reference-spec.md): Manifest, Publisher/Realm Profile# And the handshake news and the terminology. #
13. [Actrium Privacy tags and diagrams are assembled](13-privacy-state-and-aggregation.md): to bring the current security model to Tab / Front / Actr Host.
14. [Privacy, remote certification and credible algorithms](14-privacy-primitives-and-attestation.md): All-process encryption,WASM/CC Division of labour,RATSKey Releases and CPU/GPU/NPU Group certification.

## 5. Normative Use of Terms

This paper uses the following intensity:

- **I have to. / MUST**: Missing can destroy product models, secure borders or interoperability.
- **I think so. / SHOULD**: Default should be satisfied and the reason and the alternative must be recorded in case of deviation.
- **Yes. / MAY**: Compatibility or subsequent expansion.

Example: JSON/TOML/Protocol fields for freezing product syntax; official wire schema Still need to be here. Actr/Actrium It's... schema Version in the warehouse.

### 5.1 Document Effectiveness

In the event of a conflict, the following order of interpretation is given:

1. **Semantics of security and privacy**: [docs/security](../../architecture/security/actrium/README.zh.md);
2. Ben. README (a) Decision-making on products not in conflict with the preceding article;
3. [Demand and acceptance criteria](10-requirements-and-acceptance.md)Medium MUST/FR/NFR;
4. URI, distribution, running time,Realm, Workspace and product topics not covered by this chapter;
5. [Reference Agreements](12-reference-spec.md)in the
6. Product narrative, user processes and delivery plans.

`01` with `03` Mainly to help understand products;`04`—`09`, `12`—`14` Yes, I'm waiting. P0 Draft norms on freezing;`10` (a) is the baseline for acceptance and inspection;`11` It's a plan, not a plan. wire contract.

## 6. Factual basis

This design is based on the following publicly available feasibility but is not bound by its historical structure:

- [Actrium/actr](https://github.com/Actrium/actr): Actor model, precise `ActrId` addressing, Realm/connection ACL, signaling, WebRTC, signed `.actr` packages and WASM workloads.
- [Actrium/keel](https://github.com/Actrium/keel): Keel Host/Guest, Bridge, JS Sandboxes, contracts and raw components rendered.
- [Actrium Overview of the current structure](../../architecture/actrium/overview.md): Multiplatforms Shell, share running time with Actr SDK Borders.

Key facts that have been verified include:

- Actr's current `ActrId` text form is `<serial_hex>@<realm_id>/<publisher>:<program>:<version>`; the target format is `<serial>.<label>@<domain>`: the Realm name replaces the numeric realm_id, and the program type moves into registration and the Actor credential.
- Actr Supported `Dest::Peer(ActrId)` Accurately locate and press `ActrType` It was found that the two could be clearly divided.
- `.actr` Existing Ed25519 manifest Signature,SHA-256 File binding and resources; relying on lock-down workload/The build process is optional.
- Keel It's... Host/Guest Border and White List module mechanism suitable for carrying FrontBut it's... `Engine.loadBundle(URL)` It is not a credible distribution system in itself.
- Most Keel "native" sandboxes are still language-runtime isolation inside one OS process and cannot be claimed as browser-grade process isolation.

These documents use the canonical names Loran, Publisher Registry, Issuer and publisher throughout; historical names found in code and older documents are listed in one place, the "历史名称对照" (historical names) section of the [glossary](../../GLOSSARY.zh.md).
