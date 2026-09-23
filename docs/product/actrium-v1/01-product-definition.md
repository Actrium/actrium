# 01. Product definition

## 1. Product vision

Actrium To... Actor The program becomes a product form that ordinary users can understand, install, create, open and restore.

Users see familiar browsers:

- There's an address bar.Tab(a) History, collection, library and security tips;
- An address can create an example of a new program or can return to an existing example;
- The program can be local or can connect to the remote end;
- Close Tab (a) Do not miss working conditions;
- The installation procedure does not allow it to execute its own code at will.

The developers see the bound little program platform:

- Only for front desk. Keel Guest;
- You don't need to do it yourself.Actor Registration, correspondence, access window and cross-platform Actr Host;
- The front desk is always a type. Primary Service capability (a) Visiting operations;
- Backstage. Service Available Actor Modelling of more complex services;
- A precise coordinates can be used. HTTPS Access, validation, cache and recurrence.

## 2. Current issues

If you don't freeze the model first,Actrium It is easy to carry both browsers, plugin systems, remote desktops, raw application containers and Actor Multiple semantics such as debuggers, which eventually leads to the following problems:

- Tab, pages, plugins,Actor, Service and Workspace (b) Life cycle coverage;
- The front desk code is both clear. Keel Bridge, and it could be bypassed by direct access to the original module or Actor Networks;
- “Open Application”Sometimes examples are created, services are found, and fixed services are connected, and address columns are not explained;
- Status Type Service (a) Be treated as a non-state copy that can be balanced by type of load;
- Marketplace, Publisher, Realm The letter was mixed with a central source of trust;
- Use HTTPS Downloaded the package, but no version was unchangeable.digest Fixed and offline verification;
- Close Tab, refresh, crash restoration may be created, replaced or cleared by error Service.

Ben. PRD The aim is to eliminate these ambiguities with a small number of stable non-variant variables.

## 3. Target users

### 3.1 End-user

Lightweight to run agentor Workspace Tools, but not intended to understand Actor, Realm, the user of the message and the details of the time of operation.

Core tasks:

- (a) Install a programme from the market or locally;
- Selection Workspace And create an example;
- Work will continue later from history, collection or exemplification library;
- There are examples of connections on another device;
- Know clearly who the program comes from, which Realm (and domain) the instance belongs to, and which permissions have been granted.

### 3.2 Program Developer

Develop a native UI, Actor Backend and Workspace State of procedure.

Core tasks:

- Only Keel Guest API Writing across the platform ' s front desk;
- Use Protobuf/Contract description Front–Service API;
- Build Service workload;
- Declaration of components, competences, resource quotas and Workspace (a) Modes;
- Release unchangeable with your own domain name `.actr` Packages;
- Quick debugging in local development mode.

### 3.3 Publisher

Publish programs through a domain and maintain Publisher signing keys, versions and revocation information. Realms are created per instance, so a Publisher cannot designate a particular Realm in advance.

### 3.4 Realm Signing Key Holder and Realm Owner

The owner of a Realm domain is the Realm signing key holder: it publishes the Realm signing keys in the authority descriptor, authorizes Realm creation under that domain after authenticating requesters under its own policy, decides which Realm Provider to use and is responsible for Realm signing key rotation; it may also delegate custody of the keys to a Realm Provider. No domain, no Realm (except local Realms).

The Realm owner is the creator of an application instance and manages that instance's Realm membership, Actor admission, connection ACL, Realm invitations, client revocation and audit.

### 3.5 Marketplace Operator

Provide information on process discovery, classification, audit, risk labelling and revocation, but does not own any Publisher name.

## 4. Core user story

### 4.1 New instance

> As a user, when I type or click an `actr://` address without a `service` parameter, Actrium should first validate the program, show permissions and the Workspace and Realm domain choices, create a new Realm under the chosen domain (a local Realm by default) with a Service inside it, and then rewrite the address into one that reconnects to that Service.

### 4.2 Connection already exists

> As a user, I opened the tape. `service=<ActorId>` The address is on the page of the article.Actrium Only the example can be connected; if it is offline or I have no access, it must be clearly reported and no other one can be built“It looks the same.”Example.

### 4.3 Close & & Restore

> As a user, I close Tab Only for closing the current desk session.Service with Workspace Continued unless I visibly stop, archive, move to the wastebin or be completely removed in the case management.

### 4.4 Install and Validate

> As a user, I install `publisher/program@version` , you can confirm Publisher Domain name, exact version, package digest; the same version installed will not become another byte in the backstage.

### 4.5 Local development

> As a developer, I can come from the locals. `.actr` File or development HTTPS Origin Load unpublished programs, but Actrium It must be clearly marked.“Development sources”and forbids it to pretend to be a validated domain name Publisher.

### 4.6 Realm With Network

> As a user, I don't need to fill in signaling and TURN addresses; Actrium selects endpoints and short-lived credentials from a Realm Profile signed by a trusted Realm signing key, while showing me the current instance's Realm, its domain and connection status.

### 4.7 More Actor Privacy status

> As a user, when the application is multiple Actor When you combine, the green I see must represent the control of the whole map export on the current pre-position; the yellow of any node is unknown and the red of any uncontrolled; the green cannot be shown when the cut is not ready.

## 5. Product objectives

### G-01: to form a uniform application model

All General Applications Tab And they all follow the same main chain:

```text
Tab → Front Actor → Primary Service Actor → Workspace
```

UI-only Procedures are also provided at the minimum provided by the Platform ServiceInstead of creating a second set.“Apply Pure Pages”Models.

### G-02: Put Publisher Code restricted to auditable boundaries

- Publisher No replacement Front Native Host.
- Front Only carry Keel Guest.
- Guest Defaults no arbitrary network, file,Actor discovery Or original modular capacity.
- All capabilities are stated, authorized by the user and Actr Host The three-way cross.

### G-03: To make the address readable, replicable, recoverable

- Program coordinate Responsible“Run what?”.
- Service ActorId Responsible“Which instance to connect to”.
- The Realm name decides where to verify and address.
- URL Don't carry secrets.

### G-04: Reuse Web and the security infrastructure

Reuse DNS, IDNA, TLS, HTTPSTransparency of certificates, caches,CDN, standard port and business agent; both signed and digest Completing HTTPS Inadequate for offline caches, mirrors and versions.

### G-05: Hold on Actor Combination capacity of models

The Front sees only one Primary Service, but the backend may contain multiple Actors. The Primary Service can act as an interface and supervisor for worker, media, indexing, synchronization, and other supporting Actors.

### G-06: Provide end-to-end application security

- The conclusions are assembled along the lines of an effective and unincorporated `dependency map ' ;Front (a) Positioning only;
- WASM the Declaration,imports, capacity and actual export Actr Host (a) Cross-certification;
- Just... `Ready` And all of them. `CONTROLLED` The green is the only way to show it.
- A clear distinction between yellow and red; no green display on hang-up;
- New configuration only for new data; state change connection,Capability And the data sheet.

## 6. Non-target

v1 Do not:

- Any Web Browser Compatibility or Complete Web API;
- Publisher Custom Front Primary plugin, dynamic library or Host Module Achieved;
- Guest Direct access is arbitrary ActorId Or find any kind of Actor;
- Service Transparent load balance of examples;
- (b) Multi-person real-time collaboration agreements;
- In situ no sense replaces running Service workload;
- Automatic cross-Realm (cross-instance) federation and global Actor search;
- Domain-level SSO or member directory;
- Open Anonymous TURN;
- Only by Marketplace System clearance is granted when the marking is reviewed;
- Commitments on mobile platforms 24×7 Local Backstage Service Online.

## 7. Product principles

### 7.1 Clearer than guess.

URL Yes. `service` Connected, none. `service` Just create; do not change intent when it fails.

### 7.2 Address does not equal permissions

An ActorId is a location and identity; access is decided by the Realm credential, application permissions and session authorization.

### 7.3 Status Service

Tab, Front Actor, Guest It's all re-established;Workspace Tie, state of operations and case permissions Service.

### 7.4 The exact version is unchangeable

`publisher/program@version` ♪ Once I'm in the middle of Actrium Accept, you must be permanently the same. package digest.

### 7.5 Default Minimum Capabilities

Capabilities that are undeclared, unauthorized or not implemented by the Actr Host are unavailable.

### 7.6 Platform limit visible

The facts of moving end backstage limit, dross strength of the same process, out-of-line cancellation of information must be shown and cannot be masked in abstracto.

### 7.7 Privacy depends on the weakest, not on the average.

One controlled node cannot offset another unknown or uncontrolled node; there is no evidence to follow. Green only proves that the current level of readiness may be controlled, does not claim that the program is "absolutely free of loopholes" or that there was a whole world in which the whole picture existed.

## 8. v1 Scope

### 8.1 Must deliver

- macOS/Windows (a) Full closed ring of the desktop end;
- iOS/Android The front desk runs and restores the closed loop;
- `actr://` (a) Both intents to create and connect;
- Program Catalog with Service Inventory;
- Domain Name Publisher Query, parse.HTTPS Download, sign and digest Validation;
- Fixed Native Front Host + Keel JS Guest;
- Local Service Create, stop, start, connect, recycle and clear;
- Workspace (a) Tie;
- Realm signing keys, per-instance Realms and their Realm Profiles, identity, signaling and TURN short-lived credentials;
- `ActorAssurance` / `ApplicationAssurance`, no root-dependent maps, the weakest items to be gathered `Ready` (a) Demonstration;
- (a) Competence, quotas, audits and error pages;
- Local development sources.

### 8.2 It can be postponed.

- Marketplace (b) Trading capacity;
- Service (b) Trans-equipment migration;
- Shared attachment;
- More Realm (b) The Confederation;
- Forced independence OS Process Front;
- Keel Native WASM Guest;
- Version transparency log and recapable builder certificate.

## 9. Successful definition

v1 It's not the sign that's made.“Can show one Keel Page”The following closed rings are established at the same time:

1. User installs program from a trusted coordinates.
2. User Create Workspace-bound Service.
3. Addressbar In situs Accessed ActorId the address of the connection.
4. Front Only callable by a declared contract Service.
5. Close, restart Actrium And then it's the same. Service ActorId Restore.
6. The same exact version is still valid when running offline.
7. Service Not online, not authorized,Realm There are different and operational errors when the incompatibility and the package are not compatible.
8. Guest Excess of authority, time, memory or breakdown does not result in other Tab Business status is damaged.
9. No green displays may continue to be displayed when a member of any chart becomes unknown, uncontrolled or guaranteed to hang, and the corresponding data flow is restricted.
