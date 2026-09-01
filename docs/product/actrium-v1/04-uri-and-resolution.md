# 04. `actr://` URI And parsing norms

## 1. Design objectives

`actr://` It must be accompanied by:

- People can see it. Publisher, Program with Version;
- The machine can be precise. Create and Attach;
- DNS Realm It's... URI Could be linked to address columns, history, collections and cross-equipment sharing;Local Realm URI (a) The address specified as the inside address of the installation;
- URI (b) No secrets;
- Assigned ActorId Do not introduce service discovery or LB;
- The same meaning after normativeization is only a stable expression.

## 2. Basic Syntax:

```text
actr://<publisher>/<program>@<version>[/<guest-route>][?<query>][#<fragment>]
```

Example:

```text
actr://notes.example/team-notes@1.4.2
actr://notes.example/team-notes@1.4.2/today
```

Simplified ABNF:

```abnf
actr-uri       = "actr://" publisher "/" program "@" version
                 [ "/" guest-route ] [ "?" query ] [ "#" fragment ]
publisher      = reg-name
program        = ALPHA *( ALPHA / DIGIT / "-" / "_" / "." )
version        = semver
guest-route    = *pchar *( "/" *pchar )
```

Official registration scheme The time should be taken to follow URI scheme Registration regulation and by RFC 3986 It's... authority, path, query, fragment The rule is deciphered as a baseline.

## 3. Publisher

### 3.1 Rule of identity

Official `actr://` URI Medium Publisher:

- It must be. DNS Domain names;
- Convert to IDNA A-label (b) Post-lower-case regularization;
- Not permitted userinfo;
- Not permitted IP literal;
- Do not contain ports;
- Do not automatically add or delete `www`;
- Maximum length to follow DNS Name limits;
- Unicode Displays that a homogeneity risk hint must be provided.

```text
Example.COM       → example.com
<!-- language-allow: literal Unicode hostname required to demonstrate IDNA normalization -->
bücher.example    → xn--bcher-kva.example
example.com       ≠ www.example.com
```

### 3.2 Why? Publisher Do not Port

Ports don't break. TLS Certificate authentication of host name, but changes the semantic of product identity:

```text
example.com:8443 ≠ example.com:9443
```

Include Port Publisher This would cause:

- The same organization was decomposed into a large number of publishers;
- Market weight, rights memory and domain name validation are more complex;
- (a) Users are difficult to identify;
- (b) The link is more easily blocked by proxy and firewalls;
- loopback♪ I'm not gonna get away with it ♪ SSRF Risks are mixed into the official naming space.

Therefore:

- Publisher ID Use domain names only;
- Publisher/Realm Domain Name bootstrap Hold on. HTTPS 443;
- Profile Validation of the assignment package resolver, CDN, issuer, WSS, STUN/TURN endpoint Visible ports are available;
- Non-standard port is the transfer configuration, not Publisher Part of the name.

## 4. Program with Version

### 4.1 Program

- On a single. Publisher (a) The sole of the following;
- (b) Case sensitivity prohibits ambiguity in publication and regulates the use of lowercase forms;
- The length of the recommendation is not greater 64 Bytes;
- Bans path separator, percentage number and control character.

### 4.2 Version

- Official URI Precision must be used SemVer;
- Not permitted `latest`, `stable`, range expression or default version;
- prerelease Allowed, but must be written in full;
- build metadata I'm sure I'm gonna have to get in. ActrType Harmonization in the norms;v1 It is recommended that this be prohibited in order to avoid a trade-off in value judgements.

Legal:

```text
actr://example.com/editor@1.2.3
actr://example.com/editor@2.0.0-beta.4
```

Illegal:

```text
actr://example.com/editor
actr://example.com/editor@latest
actr://example.com/editor@^1.2
```

Program Catalog Available“Use the recommended version”button, but first the exact version must be resolved and then written to the address bar.

## 5. Create URI

No, I don't. `service` Query Parameters URI Organisation Create Intent:

```text
actr://notes.example/team-notes@1.4.2
```

Semantics:

- Download/Read Precision Program Release;
- Create a new local Service;
- Select or Create Workspace;
- Selection Realm, Default Local Realm;
- Replace address in situ after success with Attach URI.

Create URI is an operational intent, not a permanent address of an existing resource. URI Equivalent to“New Template”.

## 6. Attach URI

- Yeah. `service` It's... URI Organisation Attach Intent:

```text
actr://notes.example/team-notes@1.4.2/today
  ?service=2af01%4042~dns%3Ateam.example%2Fnotes.example%3Ateam-notes%3A1.4.2
```

Line in one line:

```text
actr://notes.example/team-notes@1.4.2/today?service=2af01%4042~dns%3Ateam.example%2Fnotes.example%3Ateam-notes%3A1.4.2
```

### 6.1 `service`

- Value is Actr Normative `ActorId` String UTF-8 percent-encoding;
- Must be fully inclusive Realm authority, Realm ID, serial with ActrType;
- After parsing ActrType I have to. URI It's... publisher/program/version Exactly the same;
- One is not allowed. query Multiple `service`;
- (a) Empty values are not allowed;
- Not allowed Actor Type, aliases, market ID Or local database ID Alternative.

### 6.2 `realm`

v1 Official ActorId It must be carried. Realm authority♪ And so ♪ Attach URI No second position parameter is required.Actro From `service` Undo Realm:

```text
2af01@42~dns:team.example/notes.example:team-notes:1.4.2
```

`realm` For two cases only:

- Create URI Visible selection network Realmfor example `?realm=team.example`; creating locally when omitted Realm;
- Import Current Actr It's... legacy ActorId Time-completing Missing authority.

Legacy Attach The resolution must be upgraded to a component after success authority It's... v1 ActorId, from the regularization URI Remove `realm`. Can't let `realm` with `service` It constitutes two potentially contradictory sources of positioning over time.

Local Realm ActorId Use `local:<installation-id>` authority. it can be reconnected in the machine with precision, but not deciphered by other equipment;Actro This must be disabled“Copy portable link”Not forger. DNS Realm.

### 6.3 Attach The hard rules

Attach Other Organiser

- No substitution by type may be found;
- Do not create local examples;
- No automatic downgrading of the version;
- Don't change. Realm;
- It's not to be ignored. ActorType or contract mismatch.

## 7. Guest route, query with fragment

### 7.1 Path

Program/version After path Yes. Guest Frontline route:

```text
/today/note/123
```

It's not involved. Service Choice, and it doesn't change. Workspace.

### 7.2 Reserved query

v1 Reservations:

- `service`
- `realm`

Other query Fields as read-only after removing the reserved field launch parameters For Guest.Program Business fields may not be named as reserved fields.

### 7.3 Fragment

Fragment For Only Guest , do not participate:

- Program (a) Parsing;
- Service (b) Positioning;
- Realm (b) Selection;
- (a) Competence;
- (a) The package version;
- Actor Identification.

Actro Not allowed in fragment Upload bearer invite. Invitation and authorization through independence Realm/Service Process complete.

## 8. Normative

Actro Before preserving history, collections and copying links, the following must be done:

1. scheme (a) Lowercase;
2. Publisher and Realm Turn to lowercase IDNA A-label;
3. Remove Default Empty path;
4. Program Use the standard case in the release record;
5. Version Use criteria SemVer Strings;
6. ActorId First parseAgain. Actr Standard formatter Output and percent-encode;
7. query Let's go first. `service`;Create/legacy URI I'm not allowed to do that. `realm`, press the rest of the field key/value (a) Stable sequencing;
8. No change. Guest path meaningful case;
9. (b) Reject the repetition of the reservation parameters;
10. Control character after decoded,NUL And illegal. UTF-8.

## 9. Parsing algorithm

```text
parse(uri):
  1. Authentication scheme
  2. Authentication authority Just... publisher host
  3. Normative publisher
  4. Parsing program@exact-version
  5. Parsing guest route
  6. Ripping and Heavy reserved query
  7. If service Missing → CreateIntent
  8. If service Exists:
       parse ActorId
       assert ActorId.type == coordinate
       resolve ActorId.realm_authority
       assert ActorId.realm_id == profile.realm_id(DNS Realm)
       or assert ActorId.local_installation == this installation(Local Realm)
       → AttachIntent
  9. fragment As Guest Status Save
```

Parsing only creates intended objects, not downloading packages, not creating Service, do not connect to the network. The side effects must be performed after user confirmation and strategic checks.

## 10. After Created URI Rewrite

Created transaction returned successfully ActorId After:

```text
replaceState(
  actr://publisher/program@version/path
  ?service=<new-actor-id>
)
```

Must use replace, not push:

- Back (a) Should not return to the confirmation page that will be created again;
- Refresh It has to connect to new Service;
- History Only the address of the final instance is recorded.

If Service Created but URI The #GazaTalkPressPressPressPressPressPressPressPressPressPressPressPressPressPressPressPressPressPressPressPressPressPressPressPressPressPressPressPressPressPressPressPressPressPressPressPressPressPressPressPressPressPressPressPressPressPressPressPressPressPressPressPressPressPressPressPressPressPressPressPressPressPressPressPressPoped that the #PopPopPopPopPopPopPopPopPopPopPopPopPopPopPopPopPopPedPopedPopPopPopedPopedPopedPopPopPopPopedPopedPopPopPopPopPopPopPopPedPopPopPopPopPopPopPopPopPopPPopPopPActro It has to be from `creation_intent_id` The result of the restoration is that it cannot be created again.

## 11. Development URI

Use of independent development sources scheme:

```text
actr-dev://localhost:7443/team-notes@0.0.0-dev
```

Rules:

- Registration only in visible development mode;
- authority Yes. HTTPS Origin, can contain ports;
- Allow localhost, loopback And the private one of the visible trust. CA;
- Do Not Write verified Program Catalog;
- Clear. UI Always Show“Development sources”;
- Give risk tips before exporting or sharing;
- `actr-dev://` No official permission. Marketplace Accepted.

Official `actr://` Not supported HTTP fallback.

## 12. External Call

From web pages, 2D code, mail or other App It's... URI:

- We have to show you how it's done. Publisher, Program, Version and Create/Attach (a) Intention;
- Create Intent It must be confirmed;
- Attach You can open it directly with user settings, but for the first time Realm/Publisher Trust remains to be confirmed;
- Trigger the limit when multiple examples are created in a short time from the same source;
- External URI (b) Not to carry pre-authorization of authority;
- Unknown query Only Guest Visible after startup without prejudice to the installation or permission page.

## 13. URI Error Code

| Code | Meaning |
|---|---|
| `URI_SCHEME_INVALID` | Not supported scheme |
| `URI_PUBLISHER_INVALID` | Publisher Not an official domain name |
| `URI_PORT_NOT_ALLOWED` | Official Publisher Port |
| `URI_PROGRAM_INVALID` | Program Syntax: |
| `URI_VERSION_REQUIRED` | Missing exact version |
| `URI_VERSION_INVALID` | Version is not allowed SemVer |
| `URI_RESERVED_DUPLICATE` | Repeat retention parameters |
| `URI_ACTOR_ID_INVALID` | Service ActorId Could not close temporary folder: %s |
| `URI_ACTOR_TYPE_MISMATCH` | ActorId type with Program coordinate Not in line |
| `URI_REALM_REQUIRED` | Legacy ActorId Missing authorityCould not import |
| `URI_REALM_MISMATCH` | Legacy `realm`, Realm Profile with ActorId It's... realm Not in line |
| `URI_LOCAL_REALM_FOREIGN` | Local Realm ActorId Not part of this installation |

Error code for logs, tests and developers; user files must state the operational reasons.
