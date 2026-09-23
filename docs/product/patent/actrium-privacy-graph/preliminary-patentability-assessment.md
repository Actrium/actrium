# Actrium Dynamic privacy chart closed loop: initial patent search and assessment

- Version:0.1
- Other Organiser2026-07-30
- Target audience:Actr Actor Network,Actrium Apply host and running privacy mechanisms
- Main jurisdictional perspective: Chinese patenting of inventions, taking into account PCTUnited States and European applications

> This report is a preliminary material used by the product and technology team to determine whether to enter a formal patent, not a legal opinion, nor complete, novel search, invalid retrieval or free execution (see annex).FTO) Comment. The final application scope, inventor, public day and jurisdiction strategy should be confirmed by the patent agent or lawyer.

> Description of terms: The present report is prepared in 2026-07-30, following the example of the current operation.[docs/security](../../../architecture/security/actrium/README.zh.md)) has been changed to the following objects, which are symmetrically mapd to:`PrivacyGraphRoot` ≈ `structureDigest`;`GraphEpoch` ≈ `ApplicationStateDigest`(Structures) + Commitments in the current state of each node); " Old Epoch "Cutting out" is the equivalent of "Cyber".`ActorStateDigest` The report contains a single-modification number, which permanently abolishes the old binding (renewal of content).

## Summary of conclusions

### 1. Is it worth it?

**It's worth it, but it can't be.“Integrated multiple road safety techniques and display of green or yellow markers for applications”As a core invention.**

Parts covered by this broad formulation——Component maps, weakest aggregates, multi-dimensional credible status, remote certification, certification or release of key, re-evaluation after change of status, suspension and consent when additional authority is granted,Epoch With short-term leases——Both are already in existence, with respect to mature standards, product mechanisms or patents.

The most deserving of application is now centred on a narrower and more technical agreement:

> **Application in operation Actor When a candidate changes in privacy maps, the data plane of the old maps is frozen at the atom and its older parts are frozen until new nodes or new edges are available for operational data Epoch Capability; retention of a type of controlled plane that is restricted and cannot carry the explicit operational context; generation of candidate maps and assessment of certification; user authorization only in case of a downgrading or otherwise of the privacy security vector; eventual binding of new communication keys, data capabilities and authorizations to the candidate root and new Epoch Operational communications will be resumed later.**

This round of search does not find a single information that is complete and complete in disclosure of all the above steps and their accompanying security constraints. It can therefore be the main thrust of the subsequent official search and claim.

### 2. Initial rating

| Candidate theme | Innovative initial review | Creative initial review | Recommendations |
| --- | --- | --- | --- |
| Multiple safety techniques are coming together as green markers./Yellow Bill | Low | Low | No claim of independence |
| Full Application Take Actor The weakest security item | Low | Low | As a strategy implementation example |
| Run-time Actor Figure and its proof of the use of the graph | Low to Medium | Low | As infrastructure or subordination |
| Theisomers are shown to be a multi-dimensional security vector. | Low | Low | Standard compatible design, not core |
| Re-evaluation and restricted access following the change in the figure | Low to Medium | Low to Medium | It has to be combined with the atomic transport mechanism. |
| Only seek permission to secure downgrading | Medium | Low to Medium | Not strong enough to be alone. The browser access mechanism is close. |
| Figure Epoch Forced data key and capability lease | Medium | Medium | Important composition, but lease/Epoch It's already technology. |
| Privacy migration services without leak windows | **Medium to High** | **Medium** | **Master Application Core** |
| Certified log exports, retention and audit configuration | Medium | Medium | Second applications may be considered after further technical development |
| “Credible Privacy Fields”The cover-up subgraph certificate. | Medium | Medium Low | Specific commissioning, revocation and role area agreements need to be designed first |

Here.“Medium to High”It is only indicated that the first round of searches has not yet found a single full coverage of the literature, which does not mean that it has been confirmed as novel.

## II. Technological issues to be addressed

It is not appropriate for the application documents to write technical questions.“How to make users understand that application is safe”or“How to authenticate an application”. This is easy to fall into for information presentation, management rules or abstract evaluation.

It is proposed that the technical issues be expressed as follows:

1. (a) How dynamically distributed applications, when adding, replacing or reconnecting nodes, can prevent new nodes that have not been assessed from receiving sensitive operational data before the assessment is completed;
2. How to prevent the replay of the proven results of the old diagram, user authorization, key or ability tokens after the map has been changed;
3. How to suspend operational data while retaining the necessary activity to certify, consult and authorize, without allowing the control channel to be sidelined to operational data;
4. How to automatically complete the migration when safety is not reduced, and only to interrupt the user when security is actually declining or not comparable;
5. How to ensure that, in the event of a change in the simultaneous distribution, the information is in the process of being transmitted, the certificate is expired, the nodal failure and the recovery, etc.“Freezing, assessing, then entering into force”is the linear order.

Corresponding technical effects should be capable of testing:

- The operational load bytes that can be sent by the candidate side before submission is constant to zero;
- Old Epoch Capability, information and key are rejected for certainty after the migration submission;
- (a) The type, size, speed and field of the message to control the plane is subject to the agreement and cannot carry the operational payload;
- The same or stronger candidate list can be restored without re-inquiring the consent of the user;
- De-escalation authorization cannot be re-instated in other applications, in the form of a graph,Epoch, data categories or strategic versions;
- There will be no two valid maps to carry operational data at the same time as the crash recovery Epoch.

## III. RECOMMENDED DEFINITIONS OF INSTRUMENTS

### 1. Recommended name

Preferred:

> A private data plane atomic transport control method, systems, equipment and storage medium based on the version of the running time component chart

Option:

> A dynamic. Actor The private map migration and the mapping binding capacity control method applied

The need for independence is appropriate“Run-time component”“Distribution of implementation modules”or“Message Processing Unit”Wait for the broader term, in the instructions, Actor, Actr and Actrium As a preferred example, it is necessary to avoid the unnecessary restriction of the protection to product names or browser forms.

### 2. Core status machine

```text
ACCEPTED(GraphRoot=e.root, Epoch=e)
  │
  ├─ Interception of a change of intent in privacy semantics
  ▼
FREEZING
  - Close Old Epoch Business data clearance
  - Prevent candidate sides and nodes from getting business keys/Capacity
  - Open restricted control plane only
  ▼
EVALUATING(CandidateRoot, CandidateEpoch)
  - Collection/Certification
  - Generate candidate guarantee vector
  - Optimal comparison with accepted baseline
  │
  ├─ All the necessary dimensions are not reduced and the strategy is passed.
  │      └─ COMMIT: change key, new capacity, business recovery
  │
  ├─ There is a decline or a lack of comparability
  │      └─ PENDING_CONSENT
  │             ├─ Getting candidate-tap-bound authorization → COMMIT_YELLOW
  │             └─ Reject/Timeout → ABORT Or keep the freeze
  │
  └─ Trigger hard threshold that cannot be covered
         └─ ABORTNo user overstepper channel
```

### 3. Suggested inclusion of the necessary combination of independent claims

The following characteristics should be a combination of interactions rather than an unrelated list:

1. Save running time component snapshots, graph identification or roots, current graphs for which application has been accepted Epochand the accepted multi-dimensional privacy protection baseline;
2. Make operational data plane communication capacity, data keys or short-term permissions at least apply case identification, diagram Epoch, communication margins and data categories;
3. (a) The blocking of the host or data plane gate changes the intent of the privacy semantic before the candidate node or the candidate edge becomes effective;
4. Old Epoch Switch to a freeze, which prevents the old licence from continuing to authorize the operational load and prevents candidate maps from obtaining business data clearance;
5. (a) Only independently defined control information collections are allowed during the freeze, for the purpose of detection, certification, strategic assessment and authorization, and cannot carry operational payloads;
6. (a) Build a candidate map and validate evidence of nodes, sides, hosts and implementation environments, and obtain a multi-dimensional security status;
7. Compare candidate status with accepted baseline in order rather than in order of total:
   - (a) Automatically accept all required dimensions without decreasing and meeting strategies;
   - (a) Maintaining the business freeze and generating precise downgrading differences when any required dimensions are reduced or uncomparable;
8. Submit a new candidate when automatically accepted Epoch, re-create or issue map binding keys/(a) Capacity and restoration of data plane;
9. When downgraded, only bindings to application examples, candidate map identifiers, candidates Epoch, specific downgrading dimensions, affected data categories, strategy summaries and valid authorized objects are restored and the restoration status cannot be labelled green;
10. Prohibiting user overwhelming of hard-line strategy failures;
11. Guaranteed to change the stop, old Epoch Freezing, candidate-activation and new Epoch Fulfill linear sequence between submissions and eliminate leakage windows of pre-assessment data.

Where, no. 3, 4, 5, 8, 9, 11 The linkages of the item are the most important part of the technology available.

### 4. Recommended data structure implementation examples

The statement may be made public but it is not necessary to write the following fields in the broadest of the claims for independence:

```text
ApplicationInstanceId =
    PrimaryServiceActorId
  || ProgramReleaseDigest
  || WorkspaceBindingDigest

PrivacySnapshotId =
    ApplicationInstanceId
  || FrontActorId
  || PrivacyGraphRoot
  || GraphEpoch
  || PolicyDigest
  || EvidenceRoot
  || RevocationEpoch

DataPlaneLease =
    ApplicationInstanceId
  || PrivacyGraphRoot
  || GraphEpoch
  || SourceActorId
  || DestinationActorId
  || DataClass
  || CapabilitySet
  || Expiry
  || Nonce
  || IssuerSignature

GraphDowngradeGrant =
    ApplicationInstanceId
  || CandidateGraphRoot
  || CandidateEpoch
  || WeakenedDimensions
  || AffectedDataClasses
  || AffectedFeatures
  || PolicyDigest
  || Expiry
  || UserOrOrganizationSignature
```

Key-based implementation can be described as:

```text
K_edge = HKDF(
    session_secret,
    ApplicationInstanceId
    || PrivacyGraphRoot
    || GraphEpoch
    || SourceActorId
    || DestinationActorId
    || DataClass
)
```

That'll do it.“Security Conclusion”Implement a password status for physical control of communications and data release, rather than remaining UI Mark.

## IV. The closest available technology

### 1. Risk aggregation, stopping or alerting users

[US8156558B2, Mechanism for evaluating security risks](https://patents.google.com/patent/US8156558B2/en)day of priority 2003-05-17:

- Different risks are evaluated using multiple risk assessment instruments;
- (a) A combination of results by a trust manager;
- Load, stop or alert users based on the results;
- The application of multiple components and lists of competencies can be judged.

Impact: Broad“After a combination of security techniques, decide green, yellow, stop or hint”It is very close to the technology available.

### 2. Change in definitions, cache consent and re-authorization

[US20190102525A1, Identifying and consenting to permissions for workflow and code execution](https://patents.google.com/patent/US20190102525A1/en)day of priority 2017-10-02:

- Scan workflow or code definition and generate a summary of privileges;
- Cache user consent;
- The new summary will be used to revert to the agreement when it is consistent with the original summary;
- The summary does not invalidate and re-introduce the cache consent at the same time.

Impact: Separate claims“Re-request for consent after changes in the structure or summary of authority”The risks are high.Actrium We need to rely on data plane atoms freeze, figure Epoch is different from more specific agreements such as only downgrade tips.

### 3. Disable before hint when privileges are enhanced

[Chrome Extension of Permission Warning Mechanism](https://developer.chrome.com/docs/extensions/develop/concepts/permission-warnings)Specify: When adding new permissions that trigger a warning, it can be disabled first and restarted only after the user accepts the new permission.

Impact:“Weaknesses in security/Pause and seek user consent for additional privileges”The logic of the product is very reasonable, but it is weaker as a single invention point.Actrium The difference should be dynamic distribution. Actor Charts, operational data plane separated from control plane, map binding capacity and atomic migration.

### 4. Revalidation and re-engineering of network access after status changes

[US9436820B1, Controlling access to resources in a network](https://patents.google.com/patent/US9436820B1/en)day of priority 2004-08-02:

- Changes in the security status of the periodic or heterometric detection equipment;
- (a) triggers complete re-certification after the change of status;
- (b) Lower, discontinue or increase resource visits in accordance with the new state of affairs;
- Non-compliance equipment can access the isolation or rehabilitation network while retaining restricted routes.

Impact:“State change—Re-evaluate.—Tightening or easing access—Reservations Control/Fix Path”Early disclosure.Actrium The order of submission of the chart changes must be further defined, old Epoch No operational data are carried by invalid, no leakage of candidate border data and control floors.

### 5. Component relies on diagrams, Hash and change detection

[EP4182826B1, A method of attesting a state of a computing environment](https://patents.google.com/patent/EP4182826B1/en)day of priority 2020-07-14:

- (a) A routing to the unringed is used to indicate the operational component and the dependency relationship;
- Hash for node, dependence and subnode information;
- (a) The components or dependences are detected through node Hashi changes;
- The computational environment can be terminated when unsafe components are found.

Impact: the graph root and Merkle structure depend on component relationships and expose graph changes through hashes. A formal freedom-to-operate review must therefore focus on matching patent claims.

### 6. Apply multi-dimensional proof diagrams for software and hardware

[US20230274034A1, Method and apparatus for multi-dimensional attestation for a software application](https://patents.google.com/patent/US20230274034A1/en)day of priority 2023-05-03:

- (a) Proof of the software and hardware components and their relationship for the image coverage;
- The nodes of the figure indicate the proof of the components and the margin indicates the space or time relationship;
- (b) To cover microservice communications, dependence, execution of the bar and historical versions;
- Multiple certifying officers can collaborate in building incremental proof maps.

Impact:“Application by several Actor Composition, all Actor and reliance on the evidence”It's already crowded. We should avoid putting“DVM”As title and sole independent conception.

### 7. Apply security maps on dynamic running

[US20240205259A1, Multi-layer application security graph for cloud-native applications using runtime telemetry](https://patents.google.com/patent/US20240205259A1/en)day of priority 2022-12-20:

- (a) The dynamic application of safety maps based on operational telemetry;
- Coverage of micro-services, third parties APIData storage, identity and data exports;
- Use to analyse the path of the attack and to demonstrate risk.

Impact: The operational time dynamic chart and security visualization are not core differences.

### 8. Service grid communications after remote certification

[US11399013B2, Secure service mesh](https://patents.google.com/patent/US11399013B2/en)day of priority 2020-04-20:

- Clear. sidecar (a) Local or remote certification of service examples;
- (a) Allowing agency and communication after proving successful;
- The session key generated by the certification process is used to protect service examples and processor identification.

Impact: per article Actor The establishment of secure communications following the certification of border execution is a known direction. FTO Their effective claims should be matched by each other.

### 9. Continuous certification and scheduling of events

[US20220078015A1, Continuous integrity validation-based control plane communication in a container-orchestration system](https://patents.google.com/patent/US20220078015A1/en)The work node was disclosed and the continuation or event of the control plane triggers the integrity certification and can be used to verify the Pod Creates, removes, etc. to trigger a re-certification.

Impact: Putting“Actor Create/Delete Trigger Certificate”Separate claims are weak; they should be reduced to the transactional and data control effects of the migration.

### 10. Certification of results, strategies and resource mandates

[RFC 9334, RATS Architecture](https://www.rfc-editor.org/rfc/rfc9334.html)Defined Attester, Verifier, Relying Party, Evidence, Attestation Results(a) Assessment strategy, anchor of trust and freshness;Relying Party Resource authorization may be judged on the basis of the proven results, and the criteria are time, time, and time.Nonce and Epoch ID.

[IETF AR4SI Early draft](https://www.ietf.org/archive/id/draft-voit-rats-attestation-results-01.html)Yes. 2021 Year described Trustworthiness Vector: Vectors of multiple credible statements,Relying Party Exchange of information may be permitted or refused on this basis.

Impact: Isomers certifying integration, credible certifiers, probative vectors, freshness and post-proven authorization criteria for direct compatibility, not appropriate for declaration Actrium Uninvited.

### 11. Short-term capacity, lease and lease rates, as shown in figure or status binding Epoch

[US8990898B2, Connection leasing for hosted services](https://patents.google.com/patent/US8990898B2/en)A connection lease with a password signature was made public, which binds the client, the host, the port and the resources and supports the expiry, renewal and revocation of the lease.

[WO2019022738A1, Managing entitlement](https://patents.google.com/patent/WO2019022738A1/en)It's open. Epoch Version management of the authorization and withdrawal of the previous delegation of authority.

[US6775703B1, Lease based safety protocol for distributed system with multiple networks](https://patents.google.com/patent/US6775703B1/en)Early disclosure of new Epoch Reconfirm locks and exchange as current before service data Epoch ♪ The locks and the old Epoch The lock is dead.

Impact:EpochThe emphasis must be on how they work with privacy map candidates for migration, proof of comparison and non-spill data.

## V. DIVISION AND CRAZY ADVANCEMENT

### 1. No complete and open combination of single literature found yet

The first round of search did not reveal that a single piece of the literature was also made public:

1. The object is a currently in operation distributed application participant chart, not a single device status, static component dependence or browser extension;
2. Intercept changes before the operational payload can be carried by the candidate map and freeze the atoms Epoch (a) Data plane;
3. Keep proof of type, size, speed and semantic limitations when freezing/(a) Authorizing control of the plane;
4. I'm gonna use a multidimensional privacy security sequence.“Non-deduction”“Down”or“Not like this.”instead of comparing only whether or not there is a change or a single risk score;
5. (a) Requesting user authorization for non-reduce automatic submission, or for decline or non-comparability;
6. The mandate holder binds the candidate to the root,Epoch, specific downgrading dimensions, data categories and version of the strategy;
7. New keys or capabilities only submit new maps Epoch It's not going to be over. Epoch Use;
8. The entire migration meets the verifiable linearity and crash recovery non-variant.

This is the main basis for the current applicability.

### 2. There is still a clear combination of exposure risks.

The reviewers may combine the following information:

- Use Gapfruit or Intel Component/(a) Proof of the map as the basis for the map;
- Use Cisco Re-entry after the change of status as a re-evaluation mechanism;
- Use Chrome or Microsoft Disable, alert and consent to the lapse of the cache as a user authorization mechanism following a change in the permission;
- Use Fortanix, RATS or certifying post-key release technology as access to communications;
- Use tradition. Epoch The lease is used as an old state of lapse and as a new state re-issuance mechanism.

Therefore, it may still be considered obvious that these steps are listed in sequence.**Functional interactions**:

- Figure Epoch Not a normal version number, but a common domain of data plane gates, capabilities, keys, user authorization and proof of freshness;
- Freezing is not normal“Disable Application”instead, a strict distinction is made between operational data levels and control levels that are not capable of carrying operational data;
- The downgrading directly determines whether the displacement of the distribution is automatic or whether the candidate chart binds the authorized branch;
- Atom sequence solves normal.“Change, scan.”It's not gonna work out. TOCTOU Data leakage;
- The crash recovery agreement guarantees that the data capacity of the old and new maps cannot be restored at the same time.

If these details are not supported by design and experiments, the risk of creativity remains medium- and high.

## VI. PROPOSALS FOR THE REPRESENTATION OF THE REQUIRED CLAIMS

### 1. Main application

It is suggested that the layout be structured around the same invention:

- (a) Methodological independence claims;
- (b) Systematic independent claims;
- (a) The independent claim for computing equipment;
- Computer-readable storage medium claims.

Examples of system implementation may include:

- (a) Chart change interceptor;
- (b) Privacy coordination;
- Data plane gates;
- Control of the flat level gateway;
- (a) Proof of the collector and the certification machine;
- (a) Policy and sequence comparators;
- Key or capacity lease agent;
- (a) Authorized controllers;
- Epoch Submit and crash recovery module.

### 2. Claims of dependency

Covers:

- Application Instance (a) The specific composition of the project;
- Graph roots, evidence roots, aggregation strategies, and discard epochs;
- WASM (a) Cross-section of the capacity to declare and the actual capacity to be granted;
- Native workload Default is unknown unless there is verifiable evidence of isolation;
- CPU with GPU/NPU (a) A combination certificate;
- LLM or the log export, retention strategy and key release conditions for accelerator services;
- Certification of expiry, revocation, change of proceedings,Host (a) The same migration is triggered by the configuration change;
- (a) Maximum size of strong connectivity, circulation and map;
- Control white lists, field patterns, maximum length, speed and total volume constraints;
- (a) The priority of user and organizational mandates;
- Hard floors cannot be covered by users;
- (b) Credible privacy domain to perform a limited aggregation of sub-map evidence;
- The state of the interface between green, yellow, frozen and reassessed.

### 3. Elements not suitable for the main centre for independent claims

- The colour of the green or yellow mark;
- “All Actor The weakest is the whole.”This abstract rule;
- Use Simple WASM, TEE, Confidential Computing or end-to-end encryption;
- Generate Simple Merkle (a) Tugen;
- Release key after the simple use of remote certificates;
- (b) Opening windows when the rights are increased;
- `actr://` URI, Tab with Service 1:1 - Like a product model.

These elements may be used as examples of implementation, subordinate claims or otherwise assessed GUI Appearance design protection.

## VII. Technical content of statements that must be completed before an application is filed

The current product document has given the right direction, but at least the following implementable material should be developed before the formal application is made.

### 1. Co-optimal and atomic

- What are the linear points of change in the figure;
- Who is entitled to freeze all business boundaries when they see a change first;
- How do we deal with the information in the process?
- How multiple simultaneous changes can be merged, queued or cancelled;
- (b) How partitions, timeout, conciliators collapse and duplicate submissions can be restored;
- Is there a need for a single co-coordination, log copy, consensus or swirl number.

### 2. Data plane execution

- Each sender, receiver,Relay, TURN, Gateway and Host Where to check? Epoch;
- Old Epoch How to refuse classified texts, re-messages, delayed messages and caches;
- Candidates Actor (a) Whether any declassified operational data are available before the assessment;
- The specific order in which the key is derived, changed, and the capacity is issued and revoked.

### 3. Control the plane is not a sideline.

- (a) The type and field of information allowed;
- (b) The specific prohibited type of operational payload;
- Length, frequency, total, compression and fraction limits;
- (a) The processing of unknown fields, extended fields and encrypted fields;
- Prevent the encoded user data from being used to prove Nonce, the log or free text field rules.

### 4. Guaranteed vectors and sequences

- At least define identity, code source, operational constraints, calculation of confidentiality, communication, storage, export/(b) The dimensions of logs and output processing;
- Values and Values for Each Dimension“Stronger./Same/Weaker./Not like this.”(a) Relationships;
- Missing evidence, unknown Native workload and how expired evidence is mapped;
- Which are the users who can cover the downgrading and which are the hard-core thresholds;
- Local Actor The full figure of how the change is calculated is concluded.

### 5. Experiments and cross-references

It is recommended that at least the following experiments be achieved and documented:

- (b) at the time of the establishment and freezing of the candidate side and when it occurs, certify that the candidate side receives zero operational data;
- Old Epoch Failure to re-establish capacity and authority;
- Control plane during the assessment period will still be able to complete the certification and authorization;
- Control plane cannot transmit operational payloads exceeding the limit;
- Only one activity data exists after the crash has recovered Epoch;
- Automatically restore equal or stronger maps, with downgrading stable in the state to be authorized;
- with“Reconstruct the map and scan it.”.

The effects of quantifiable or counterintuitiveness will significantly improve creative justification.

## VIII. PLACEAL REQUIRED AND CAPACITY ATTENTION

Article 22 of the Chinese Patent Law requires novelty, creativity and practicality of inventions.[Patent law texts published by the National Intellectual Property Agency](https://english.cnipa.gov.cn/art/2022/10/13/art_3068_179273.html).

2026 The new edition of the Patent Review Guide, which has been in force since then, emphasizes that:

- (b) Creative, a holistic evaluation of the technical programmes that are limited to claims;
- (b) The characteristics that contribute to technical issues should be included in the claims;
- The contribution of algorithms or rules and technical features should be considered when they are functionally supportive and produce technical effects.

See[National Intellectual Property Authority 84 Order.](https://www.cnipa.gov.cn/art/2025/11/13/art_99_202568.html).

Therefore, the program has a good basis to be a patent-protected object in China, but the claim must be on the Internet, data planed gates, passwords,Epoch Status and atoms submitted, not just written“Assess privacy and display markings”.

In Europe, software is also usually required to produce further technical effects; network security, password control and computer internal operation controls are easier to meet technical requirements than pure information displays, see[EPO Guidelines G-II 3.6.1](https://www.epo.org/en/legal/guidelines-epc/2025/g_ii_3_6_1.html).

The United States also needs to consider the appropriateness of abstract concepts, their obviousness and the full disclosure of the instructions./Capacity status management,“Compute a security phase”Steady.

## IX. Open risk

This is the most pressing issue of the day.

- `kookyleo/actrium`, `Actrium/keel` and `Actrium/actr` It's all public. GitHub (a) Warehouses;
- Local `docs/product/` Currently untraceed, local Git It's not even in history. `PrivacyGraphRoot`, `GraphDowngradeGrant` or“Privacy Chart”(a) Exact terms;
- This only means that we're currently here. Git History does not show the exacts of the above, and cannot prove that it never passed through chats, presentations, meetings, emails, white papers,Issue, PR Or other warehouses are public.

The six-month non-lost exception to article 24 of the Chinese Patent Law is narrower, and includes, inter alia, public disclosure for the public interest, accredited international fairs, prescribed academic or technical meetings, and non-consensual disclosure during a given emergency; GitHub Self-disclosure cannot normally rely on this exception for safety.[Note by the National Intellectual Property Agency](https://www.cnipa.gov.cn/jact/front/mailpubdetail.do?sysid=6&transactId=440198).

Recommends that, without delay:

1. Do not put this report, product document, or first entry before first application 13/14 (a) The chapter or the related agreement is pushed to a public warehouse;
2. Take an inventory of all possible public records. URL, Git commit(a) Time, access, presentation and confidentiality obligations;
3. If core agreements have been made public, the proxy will be given as soon as possible the scope of the jurisdiction to judge whether they are still available;
4. After the first application, if an international layout is required, it should be managed 12 (a) The monthly priority period;PCT For the basic definition of the priority period, see[WIPO PCT Applicant's Guide](https://pctlegal.wipo.int/eGuide/view-doc.xhtml?doc-code=pctip&doc-lang=en&doc-type=guide).

## X. FREEDOM OF IMPLEMENTATION (art.FTORisk

“Your own patent is authorized.”Not equal to“The product is free to be implemented.”. A narrower improved patent may still fall into the other's base patent.

Official FTO At least one by one:

1. EP4182826B1 and US/PCT Congener: component dependence maps, Tuhahi and status certificates;
2. US11399013B2 and peers: safety service grids, service case certificates and session communications;
3. US20230274034A1 and subsequent mandates/(b) Congener: application of multidimensional proof maps;
4. US9436820B1: Network access and access control in a changed state;
5. And then the key is released,Confidential Computing Effective patents associated with the service grid and continuous integrity certification.

FTO It must be done on a case-by-case basis, in accordance with the target country, effective claims and plans, and not read only in summary form.

## XI. RECOMMENDATIONS FOR THE NEXT STEPS

### Phase A: Within one to two weeks

1. Freezing of the public publication of the core agreement;
2. (b) Establishment of the first public list and the list of the contribution of natural persons;
3. (a) Writing of the core status machine, message sequence, capability fields and crash recovery into a technical bottom paper;
4. A professional search, focused. CPC:
   - `G06F21/57`: credible platform, integrity and certification;
   - `G06F21/62`: data access and privacy;
   - `H04L63/08`(a) Accreditation of network entities;
   - `H04L63/10`: Web access control;
   - `H04L63/20`(a) Cybersecurity strategy management;
   - `H04L9/32`: Digital signature and authentication.

### Phase B: Application policy

It's suggested that we prepare a complete Chinese invention application instead of trying to take it all at once.“Close security system”. The main application is for a focus on privacy chart migration; the other directions are to be divided or otherwise determined by technical specifications:

1. Main case: Atom data plane migration in dynamic privacy maps;
2. Alternative: Proven log export and privacy calculation service data path;
3. Alternative: a credible, revocable domain certificate of privacy mandate with restricted domain;
4. Appearance or GUI: Actrium Privacy status, graphic differences and delegation interface.

### Phase C: A message to the agent's core

> We're not applying.“Give the application a security score.”, instead of applying for a distributed safe migration protocol: it uses the old one when the dynamic implementation chart changes Epoch Atomic freeze operational data capability, proof of completion of candidate map using independent control plane that does not carry operational data, distinction between non-degrading and downgrading in a multi-dimensional order; then only accepted candidate chart Epoch Reissuance of key and capacity and rendering any downgrading authorization unmaximised and unperformed Epoch Or re-allocation across data categories to eliminate explicit leak windows between expansion and safety assessments.

## XII. GENERAL DELIBERATIONS

Consolidated round of retrieval:

- **Applicationability: Yes.**
- **The licensing prospects of a broad system of patents: weak.**
- **The future of authorization after the Focus Privacy Map Atomic Migration Protocol: medium, worth entering the bottom of formal retrieval and technology.**
- **The most important mandated risk: the existing technology mix leads to a lack of creativity.**
- **Top commercial risk: Chart proof of underlying patents associated with the safety services grid FTO.**
- **The most urgent actions: apply for, then make public, and translate abstract safety concepts into achievable, testable distributed protocols without variables.**
