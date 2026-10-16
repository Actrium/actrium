# Actrium Renderer Process Pool

## 1. Objective

Copy the current build script by `RCTViewService0...3.xpc` It's... ViewBridge Replace Architecture with
Chrome Style Broker/Renderer Structure:

- Only one in the installation package `ActriumRenderer.app` Helper;
- Broker We can start the same one more than once. Helper (a) Executable documents;
- Every one. Helper Examples of individual address space,Hermes VM, React Native Host and sandboxes;
- Large binary and resources are not replicated with pool capacity;
- Maintain existing fingerprint, Tab, preheat, size upgrade, system application and crash isolation.

## 2. Businesses not variable

The relocation must be carried out in such a way as to:

1. `system.*` fingerprint Use system Renderer.
2. Same fingerprint Multiple Tab Share the same. Renderer Processes and RCTHost, but each
   Tab Stay independent. RN root, properties and rendering surfaces.
3. Different normal fingerprint Different uses Renderer process, without sharing a variable state of operation.
4. Tab Only destroy the item when closing Tab It's... root;Renderer Last one inside. Tab Enter the free state after closing.
5. Renderer The crash cannot be stopped. Actrium or other Renderer;affected Tab Reloadable.
6. Focus/Overview Switch Not Rebuild Renderer, still completed by real-time size and attributes update.
7. StartPage, History, Bookmarks, ACTR Scripts and left- and right-panel properties remain in the same data structure.
8. Mouse, keyboard, wheel, Zooming and Window resize We have to get the right way. Tab.

## 3. Process Model

```text
Actrium.app
└── RendererProcessPool(Broker)
    ├── system.*  ──> ActriumRenderer PID A
    │                 ├── Tab root 1 + CAContext 1
    │                 └── Tab root 2 + CAContext 2
    ├── fingerprint X -> ActriumRenderer PID B
    │                 └── Tab root 3 + CAContext 3
    └── fingerprint Y -> ActriumRenderer PID C
                      └── Tab root 4 + CAContext 4
```

Renderer I.D. Broker Generated UUIDNot anymore. XPC Bundle ID.Broker Save:

- fingerprint -> rendererId
- tabId -> rendererId
- rendererId -> Process, IPC connection, PID, state, reference count

The maximum capacity of the pool is the running time policy, which does not affect the size of the package. The initial policy continues to maintain the system Renderer and
A normal one. Renderer . The current limit is:

```text
min(12, max(4, ceil(activeProcessorCount / 2)))
```

For example: 10 The maximum nuclear machine is 5The cap is a safety valve to prevent uncontrolledness and does not pre-create these processes;
Actual fingerprint Demand dynamic growth, no priority recovery when receiving memory pressure Tab Normal
Renderer.

## 4. Start with IPC

Broker For each Renderer Create a pair of anonymous Unix `socketpair`.Broker Keep one end, the other end
As Helper . The standard input file describer inherits the past and then uses `Process` Start the same signature several times
Helper:

```text
ActriumRenderer
  --renderer-id <UUID>
  --fingerprint <fingerprint>
  --ipc-stdin
  --host-pid <PID>
```

Communication is already in use `ActriumIPC` Length prefix Protobuf Frame. One. Renderer Connect to Reuse All
Tab Message, so all Tab Class message must be carried `tab_id`.socket Enable `SO_NOSIGPIPE`,
Write Serialization and Process partial write / `EINTR`, turn off the end only to return the error, not to use
`SIGPIPE` Kill the process.

Shake hands:

1. Broker Create `socketpair`.
2. Broker Use subprocesses as stdin Start Helper.
3. Helper Take over connected describers and send `renderer_ready`.
4. Broker Will Renderer Mark as ready.
5. First Tab Configure Trigger RN root and CAContext Create.
6. Helper Return to the Tab It's... `context_id`.

Configure, size, input, title, navigation, log, crash and system application actions are all stylished Protobuf.
System Action Usage `HostEvent` Back Broker, Broker Only accept a fixed white list, re-engineer the cost
Notification;Renderer Not dependent on the cross-sand box. `DistributedNotificationCenter`.
And then you're off the old one. pendingTabId/internalId/FIFO Tie the competition.

## 5. Render & & Input

Every one. Renderer Tab Create:

- `ResizableContainerView`
- `RCTSurfaceHostingProxyRootView`
- Hidden `NSWindow`(routing of events and First Responder)
- Private `CAContext`

Helper Will Tab Root layer Set As `CAContext.layer`♪ Put on ♪ `contextId` Pass. IPC Send
Broker. Main process is used `CALayerHost` Show Remote layer. The path is shared only WindowServer /
CoreAnimation Manages the surface, does not copy each frame pixel.

Main process `RendererSurfaceHostView` Capture AppKit Events and Passes IPC Sends standardized events.
Helper Based on Tab Rebuilds the corresponding hidden window `NSEvent`, by window RN View.Overview
Model prohibition of hit testing;Focus Mode Access First Responder . Crush back
Overview Process in main process without relying on remote NSView.

Cross-process Accessibility Same one. IPC, does not depend on mature third-party banks (Chrome Remote Cocoa
It's closed.Helper - Put it on. NSView / `NSAccessibility` Trees. `AccessibilityTree`,
Main process standard `NSAccessibilityElement` Agent this tree.VoiceOver I saw the system.
(buttons, grouping, text) Re-transformation by press pressure, etc. IPC Back Renderer. The pixel layer sharing is still not
Complete NSView semantics; this is the least available AX Proxy, not complete remote AX Tree sync.

## 6. Life cycle and failure recovery

Renderer Status:

```text
idle -> launching -> ready -> busy
                    |         |
                    +-> crashed/terminated
```

- `allocate(tab,fingerprint)` Reuse Same fingerprint Renderer, otherwise start a new example.
- Tab ♪ Without rebuilding the host ♪ NSView Change of circumstances fingerprint , the identity must be re-checked before loading:
  - In the old days. Renderer Destroy the TabAnd then we'll tie it up. Renderer, and forced replacement `CALayerHost`
  context;not because context The value is exactly the same and the layer of the old process is preserved.
- `destroyTab(tab)` Release root, CAContext, hide the window and route chart.
- Last ordinary Tab After closing,Renderer Keep it short warm TTLAnd then exit with grace.
- Helper The blogger says:Broker Clean up all the maps and notify the affected. Tab;hosts will try again up to three times automatically,
  Use original configuration in substitution Renderer Restore up.
- critical memory pressure Only the next one is not recovered. Tab It's... warm Renderer, do not migrate active Tab.
- Actrium To Every Time Exit Helper Send terminate And end the subprocess.

## 7. Pack up.

Objective structure:

```text
Actrium.app
├── Contents/MacOS/Actrium
├── Contents/Helpers/ActriumRenderer.app
│   ├── Contents/MacOS/ActriumRenderer
│   ├── Contents/Frameworks/hermes.framework
│   └── Contents/Resources/system.jsbundle
└── Contents/Resources/...
```

React Native, Hermes, Renderer Operational achievements and `system.jsbundle` There's only one. Helper
Bundle in; main application is no longer linked RN/HermesRun. N In one instance, the disk was still only this executable
And the resources, the kernel can share the same. Mach-O the only page; stacks of each example,Hermes VM, RCTHost
And the sandbox is independent. That's what it means. Chrome Pattern“Single process, examples of multi-process”# No need to
Copying from each sink Bundle.

Other Organiser

- `Generate XPC Service Pool` build phase with `generate_xpc_pool.sh`;
- `XPCServicePool` With unused `ViewBridgeMultiProcessTabView`;
- `RCTViewService0...3.xpc` Scan logic.

## 8. Validation threshold

The completion of the migration requires that it be met simultaneously:

1. Clean. Debug Build and Related ActriumTests Pass.
2. App Not at all. `RCTViewService*.xpc`Only one. `ActriumRenderer.app`.
3. Open system app & Normal fingerprint Different observations. Renderer PID.
4. Same fingerprint Two. Tab Map to the same PIDDifferent. contextId.
5. Focus/Overview, resize, mouse, keyboard, wheel, panel switching is available.
6. One manual termination. Renderer Post-main processes and others Renderer Keep running, affected Tab Autorecover.
7. `codesign --verify --deep --strict` Pass.
8. Record migration before and after App Overall size and main composition.

## 9. This validation results

- New DerivedData Other Organiser Debug Build successful.
- `ActriumIPC` Transfer Test 5/5 Pass. Cover the roads. Tab, HostEvent And close the end without trigger
  `SIGPIPE`.
- URL Passed route and process pool strategy test; strategy includes hardware nuclear boundaries,`system.*` Normalize,
  Normal fingerprint Exact sharing and cross-identity prohibition.
- History with Bookmarks Same as always. Renderer PIDDifferent. `CAContext`.
- Manual termination system Renderer After, two. Tab Both in substitution process, main process and general preheating
  Renderer Not affected.
- Signatory `codesign --verify --deep --strict` Adopted;Helper Sandboxes, sandboxesApp Group,
  JIT And network client entitlement Entry into force.
- Old Debug App About 347 MiB;new Debug App Yes 143,144 KiB(About 139.8 MiB) ) , reduction approximately
  60%. Installed as
  0 individual `.xpc`, 1 individual `ActriumRenderer.app`, 1 individual `system.jsbundle`.
- ACTR / Keel Guest The acceptance range for this round is only for host viewing structures, loading links and fingerprint (a) Separation of borders;
  Its internal behaviour is left for subsequent adjustments before it is fully adapted.
- Thin Helper Current non-link to moving-end page WebView, DeviceInfo, Reanimated and
  GestureHandler (a) Realization of primary;React Native The option to generate a module table will still be unconnected WebView
  provider Recording a detection alarm, but now macOS - It's a real tactful. History / Bookmarks Not dependent on it,
  Two pages. Surface Average physical access Running Status. Follow-up ACTR / Keel Guest When you are in a situation, you press the actual.
  capability Select a parent module or macOS Platform view to avoid pre-positioning the entire mobile package to plug back in.

## 10. Signing and Platform Boundary

This machine has developed certificates for this machine Helper Do a nested signature with the main program and run a validation.Xcode Autosignature still needs new
Bundle ID `io.actrium.actro.renderer` Create provisioning profile; This is account side set.
Not affecting the manual development of a signature or architecture itself.

`CAContext`, `CALayerHost` With Current ViewBridge Same as the one. macOS Private framework capacity, not suitable
Mac App Store.Actrium Currently private ViewBridgeSo this migration does not add a new category of distribution channels, and the new ones are not available.
But it has to be supported. macOS Versions run detection and fail downgrade.

Chrome Remote Cocoa Complete AX The agent does not have a mature open-source bank to quote.Actrium With minimum realization:
Renderer Export `AccessibilityTree`, `RendererSurfaceHostView` As AX Group Exposure
The children, the movement. `accessibility_action` Back. Keyboard focus still goes in IPC. No remote
Complete AX API Overwrite (autodefined parameters, scroll area details, real-time text selection range).
