# 02. Area model

## 1. Overview

```mermaid
flowchart LR
    P["Program Release<br/>publisher/program@version"] -->|Provision| G["Keel Guest"]
    P -->|Provision| W["Service Workload"]
    T["Tab"] -->|Possession 1| F["Front Actor"]
    F -->|Load| G
    F -->|Precise binding 1| S["Primary Service Actor"]
    S -->|Run| W
    S -->|Tie 0..1| WS["Workspace"]
    S -->|Organization| A["Auxiliary Actors"]
    S -->|belong to| R["Realm"]
    F -->|belong to| R
    R -->|named in| NS["Domain<br/>domain / installation-id.actr.alt"]
    F -.Privacy Convergence.-> PRIV["Application Privacy Closure"]
    PRIV -.Overwrite.-> S
    PRIV -.Overwrite.-> A
    PRIV -.Overwrite.-> WS
```

## 2. Seven core objects

### 2.1 Program

Program stable name, does not contain examples of installation status and operation.

```text
ProgramId = publisher/program
```

Example:

```text
notes.example/team-notes
```

`publisher` It's a regularity. DNS Domain names;`program` is a program name unique under that Publisher.

### 2.2 Program Release

Unchangeable version of the procedure:

```text
ProgramReleaseId = publisher/program@version
```

It has to be analyzed to a fixed `.actr` package digest.Program Release Description:

- Service workload;
- Keel Guest (a) Entry points;
- Front–Service (a) Contracts;
- Capacity and components to apply;
- Keel/Actrium/Actr ABI Compatibility scope;
- (a) Resource quotas;
- Workspace (a) Modes;
- Publisher Can not open message

### 2.3 Service Actor

A real, addressable and state-of-the-art example of a procedure:

- Last `ActorId`;
- Fixed Run Some Program Release digest;
- Hold Workspace capability;
- Holds case profiles and access controls;
- Life cycle independent of Tab;
- It can be run locally or at a remote end;
- It may orchestrate other Actors while exposing one primary interface to the Front.

Service No, it's not.“Program Type”Nor is it a freely replaceable load-balanced candidate.

### 2.4 Front Actor

One. resident app Tab Corresponding short life cycle interactions Actor:

- workload By Actrium (b) Fixed;
- Inline Keel Host, Actr client, capability broker and life-cycle management;
- Load certified Keel Guest;
- Only binds Service ♪ I'm limited ♪ capability;
- Tab (a) Rebuilding after closure, collapse or system recovery;
- ActorId It can be session-level and does not assume a permanent business identity.

### 2.5 Tab

Actrium Shell .Tab Saves local browsing status, e.g.:

- Local `tab_id`;
- Current Normalization `actr://` URI;
- Page route;
- Back/Forwarding the bar;
- Displays titles, icons;
- suspend/restore Metadata.

Tab Do Not Save Workspace Key, no business data.

Tab record It can continue after memory recovery. It is at this time dormant TabNo, I don't. live Front Actor;by same when reactivated URI Rebuild New Front Actor.

### 2.6 Workspace

The durable data boundary of a Service. It is not a simple file path but a capability hosted by the Actr Host:

```text
WorkspaceBinding
├── workspace_id
├── provider_type
├── provider_locator
├── access_mode
├── credential_ref
├── lease_policy
└── backup/migration metadata
```

`credential_ref` Point to system secure storage. Not allowed to be present. URL, general database or Guest launch context Medium.

### 2.7 Realm

The trust and governance domain of one application instance. When a Create Intent creates a Primary Service instance, it also creates a new Realm under the selected domain; the instance's Primary Service, the Auxiliary Actors it orchestrates, and the Fronts attached to it all belong to that Realm. A Realm does not follow the Program, and it is not an organization-level membership domain.

```text
Realm name = <label>@<domain>                      # networked, e.g. k7m2q9xh@acme.com
           | <label>@<installation-id>.actr.alt    # local
```

- The domain belongs to its owner and is used only as a naturally unique, ownership-verifiable name; it hosts no infrastructure. A local Realm uses the pseudo-domain `<installation-id>.actr.alt` (generated at random at installation, never resolved through DNS), which belongs to that Actrium installation;
- The domain's Realm signing keys are the long-term trust root for every Realm under it and sign each Realm Profile;
- `label` is unique within the domain, immutable after creation, and never reused after retirement;
- One Realm Provider (signaling, TURN, Issuer) hosts many Realms; a Realm is bound to one Realm Provider when it is created and can never switch.

A Publisher domain names a program (who published it); a Realm domain names a deployment (who operates it). The same Program Release can run in `notes@acme.com` and in `notes@foo.org`.

## 3. “Tab–Service 1:1”Accurate definition of

### 3.1 Product is not variable

At any time of operation:

- One in the middle of a loading/running/suspended It's... resident Tab Just happens to have one. Front Actor;
- One by Shell Abandoned but kept in Tab strip It's... dormant Tab No, I don't. live Front Actor;
- One. Front Actor One at most. Primary Service Actor;
- After starting, one Front ActorWe have to tie one to the right one. Primary Service Actor;
- One. Service Not yet. Front;
- Service And history Front Not a permanent double.

Formalized means:

```text
Tab 1 ── 0..1 Live Front Actor
Live Front Actor 1 ── 1 Primary Service Actor
Primary Service Actor 1 ── 0..1 Active Front Lease(v1 exclusive)
Primary Service Actor 1 ── 0..1 Workspace
```

The same. Service You can be multiple in time. Front Actor reconnect; this is not the running-time base at the same time.v1 Default Session Policy is `exclusive`, only one interactive at the same time Front Main lease.

### 3.2 Why not define a strict double-fire across the board?

If Service It's always just one specific thing. Front Actor, and it immediately creates a contradiction:

- Close Tab Back Service Whether it is necessary to delete;
- Actrium Restart New Front Actor How to connect;
- Front Whether the crash caused Workspace (a) Loss of ownership;
- The following video shows the event:
- The same example is why it is necessary to change identity after moving to new equipment.

So the simplest model is not“Two life-cycles tied to death.”and instead:

> Front Always face one. Service;Service Independent presence and acceptance of a new mandate Front Session.

### 3.3 Monopoly and sharing

Program Manifest Declaration:

```toml
[actrium.session]
attachment_policy = "exclusive"
```

v1 Promises only `exclusive`:

- The machine is open for the same Service , open the same URI Focus already. Tab;
- The other device is requested to connect.Service (b) May refuse, wait for the expiration of the lease or trigger a user-confirmed takeover;
- There's a limit to the network after it's broken. lease grace period, to prevent the instant break-through from being miscalculated as release.

`shared` It is a follow-up capability that requires additional definition of multi-session privileges, simultaneous writing, focus and collaborative status, and inaction v1 Invisible behavior.

## 4. Identity, type and example

### 4.1 Program coordinate with ActrType

Program Release with Service `ActrType` One to one:

```text
publisher/program@version
            ↓
ActrType {
  publisher    = publisher
  name         = program
  version      = version
}
```

Actr currently applies the shorter `Name` constraint to both publisher and name. The target design must validate Publishers with full DNS/IDNA rules and support A-label domains of up to 253 bytes.

### 4.2 ActorId

Service ActorId It is essential to meet:

- `serial + Realm name` is globally distinguishable as a whole;
- It carries a verifiable, resolvable Realm name;
- Yes. Realm (a) Permanent and sole;
- Realm We must not use them. ID (b) To allocate another example;
- On the basis of normal renewal,hard re-auth, the change of letters, the re-launching of processes and the relocation of operational positions remain the same;
- with Actor (b) Long-term key binding;
- You can interpret it as the current online address or you can report it clearly. offline;
- The ActorType is not part of the ID: it is carried by registration and the Actor credential, and must equal the Program Release.

Current Actr It's... legacy The text is in the form of:

```text
<serial_hex>@<realm_id>/<publisher>:<program>:<version>
```

It only carries a numeric `realm_id`, lacks a resolvable Realm name, and embeds the program type in the identity. v1 must rework the ActorId data model and standard formatter so the Realm can be resolved from the ActorId itself and the type moves out of the identity:

```text
<serial>.<label>@<domain>
e.g. 2af01.notes@acme.com
```

A local Realm looks like `2af01.h3x8p2wq@<installation-id>.actr.alt`. The numeric `realm_id` is only a internal handle of the Realm Provider and never appears in the ActorId. The final wire/text encoding is frozen by an Actr ADR, but it must not fall back to "give only a realm_id and let Actrium privately guess the Realm"; importing a legacy ActorId must explicitly supply the Realm name and rewrite it.

At the same time, the semantics of its life cycle must be derived from“Current registration address”Upgrade to“Durable logic of renewable rent Actor Identity”.

### 4.3 ActorId It's not a secret.

ActorId The connection must also be verified:

- Realm Issued Actor credential;
- application permissions of Service owners and members;
- Front Session challenge;
- Program coordinate, package digest and agreements fingerprint;
- attachment lease.

### 4.4 No increase in public ServiceInstanceId

Target model no longer oriented URL Add a public layer“Logical Service Key”. If ActrId If we can't stabilize, we should fix it. ActrIdInstead of keeping all the upper levels long:

```text
ServiceInstanceId → current ActorId
```

Actrium Local databases can have pure storage. `record_id`But it's not a network identity, and it's not shared. URL Medium.

## 5. Status Attribution

| Status | Author of authority | Annotations |
|---|---|---|
| Tab Arrange, move forward./Back | Actrium Shell | It's synchronous. The loss should not disrupt the business. |
| Guest UI Temporary state | Front Actor / Keel Guest | suspend But a snapshot. Once the crash is over, it's allowed to rebuild. |
| Current binding Service ActorId | Tab Session | From Regularization URI |
| Business status | Service Actor | Not on it. Front |
| Workspace Contents | Workspace Provider | By Service capability Visits |
| Workspace Tie | Service Actor | Front No re-tabling. |
| Program Release metadata | Program Catalog | Here. coordinate + digest As Primary Key |
| Actor Long-term private key | Secure Storage | Not normal SQLite |
| Realm domain trust (Realm signing key pin) | Actrium Realm Manager | Pinned after first confirmation; updated only via a signed rotation chain |
| Realm client key | Secure Storage | Separate per Realm (i.e. per instance); registered with the Realm Provider it constitutes Realm membership; revocable |
| Permissions granted | Actrium Permission Store | Tie publisher/program/major/digest/Workspace Scope |
| ActorAssurance | Authenticator (%1)issuer ≠ subject) | For accuracy `ActorStateDigest` Nodal conclusion |
| ApplicationAssurance | Applied assessor | Consemination conclusions of current no root map cut-off; presentation required Ready |

## 6. Create, detect and accurately connect

The three acts must be separated:

### 6.1 Create

None `service` It's... URI Yes. Create Intent.Actrium:

1. Parse and validate the Program Release;
2. Select a Realm domain (default: a local Realm), generate a random `label` (the user may explicitly choose a readable one) and select a Workspace;
3. Create a new Realm under that Realm domain: for a local Realm Actrium only creates a local record, with no Realm Profile and no Realm management request; for any other Realm Actrium obtains its Realm Profile and Realm membership, and creation must be authorized by the Realm signing key holder of that Realm domain: for a Realm under a Realm Provider's domain, that Realm Provider authenticates the user with a Realm Provider account and signs; for any other Realm, the holder's signing service signs the Actr management envelope and the Realm Profile;
4. Create a new Service Actor in that Realm and obtain its durable ActorId;
5. Use the... ID Create Front Sessions;
6. In-place Rewrite URI.

Steps 3–4 form one transaction keyed by `idempotency_key` and run strictly in order: obtain the holder's authorization → the Realm Provider creates a pending Realm → sign the Realm Profile and submit it with `publish_profile` → the Realm becomes active → create the Service Actor and obtain its ActorId. If any step fails, no Service Actor is created; the Realm Provider cleans up expired pending Realms automatically; the random Realm label is generated once in the Create Intent and reused on retries with the same `idempotency_key`; the Realm Provider treats `(realm, requester)` as the idempotency key and continues the same pending Realm instead of creating another, returning a conflict for a different requester using the same name or for an existing active Realm or Realm Provider tombstone (the Actr management envelope's nonce only prevents byte-for-byte replay). For a local Realm only the local record and the Service Actor are created.

Create never places a new instance into an existing Realm. When a Service upgrades to a new Program Release, the new Service Actor is created in the same Realm as the original instance; members and application permissions carry over, and the ActorId changes according to the migration rules.

### 6.2 Type found

Discovery by `ActrType` suits auxiliary services or management operations within the instance, scoped to this Realm (the instance's own Actors); v1 has no cross-Realm discovery. It is not used to replace a Workspace-bound Service the user has already designated.

### 6.3 Exact Connection

Yes. `service` It's... URI We have to go. `Dest::Peer(ActorId)`:

- Do not do LB;
- No candidate of the same type;
- Not because offline The project will be a new one.
- Program/Version Reject when matching;
- If the ActorId's Realm domain is not yet trusted, ask the user to confirm trust in that Realm domain; if the user has not joined the Realm, go through the Realm invitation/join flow.

## 7. Primary Service Interface

To keep it. Front Very simple.Guest Only:

```typescript
primaryService.call(route, request)
primaryService.tell(route, event)
primaryService.on(eventName, handler)
```

Guest Not directly obtained:

- `discover(ActrType)`;
- `call(ActorId)`;
- Letter/TURN Configuration;
- Workspace handle;
- Service credential;
- Any socket.

If the program needs multiple background Actors, the Primary Service discovers, creates and orchestrates them within its own permission boundary; these Actors belong to the same Realm. Cross-instance (cross-Realm) calls are handled by the Service backend, which obtains a client identity in the target Realm according to that Realm's join policy.

## 8. System Program

Actrium Shell address bar,Tab strip, access to bullet windows and security information belongs to ShellNot normal. Tab Content.

History, collection,Program Catalog, Service Inventory The visible pages should be either:

1. As Shell Native management panel, not in normal applications Tab(a) The contract or other contract; or
2. As Actrium Own-owned, signed systems ProgramStill in compliance Front–Service Models.

Do not expose a third party hidden form for the system page Guest Permissions. System-level capabilities must be signed at different levels and visible Host policy Granted.

## 9. Apply Graphics

Front Just tie it down. Primary Service, but the guarantee must cover Front From the one you own.**No root.**Effective reliance on connectivity.Front and Primary Service It's just a location entrance, not a Tugan.

```text
Front ──Positioning──▶ CC(U(Valid. Transfer(ActorId)))
```

It's not. Publisher Static list, but valid statement. Actor Dependence. The channel is not valid, it is blocked and no rating is given. Displays the colours to take the weakest entries from each node and must `Ready`.

Key constraints:

- Actor There's no substitute for health. `ActorAssurance`;
- Actor Self-reporting cannot replace the findings of the certifying officer, nor can it sign out;
- Any Node `UNKNOWN`, application of maximum yellow;
- Any Node `UNCONTROLLED`, applied in red;
- (a) A member hangs up and shall not exhibit green;
- Members who do not depend on the change node `LocalContinue`;
- The new configuration will only be used for new data;
- Full semantics [docs/security](../../architecture/security/actrium/README.zh.md) with [13](13-privacy-state-and-aggregation.md).
