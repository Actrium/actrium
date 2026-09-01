# Actro iPhone Analysis and Resolution of Tab Performance Problems

> This document is mainly from **iPhone End** Perspective analysis.iPad End Use Focus/Overview Mode Switch,iPhone End Use `.sheet` Fullscreen Expand + TabView Bottom navigation, with different interactive paths, but bottom memory problems are consistent.

## 1. Problem phenomena

| Problem | iPhone End Performance |
|------|--------------|
| Slow New Tab | The more labels, the slower the new labels respond, the more serious they are. App CrashOOM) |
| The equipment is hot. | After several labels, the cell phone became hot and power dropped. |
| Cut back the old tag Carton. | You still need to recreate when you have opened a tab to cut back RootView♪ And experience the Carden ♪ |

---

## 2. iPhone End Tab Interactive Process

```
PhoneMainView (TabView Bottom Navigator)
  ├─ Bookmark Page (PhoneSystemAppView → SingleTabRootView)
  ├─ Tab List Page (PhoneTabListView → LazyVStack)
  ├─ Set Page (PhoneSystemAppView → SingleTabRootView)
  └─ Search Page (PhoneSystemAppView → SingleTabRootView)

User selects a tab from the tab list page →
  .sheet(item: $fullscreenTab) Popup Full Screen →
    PhoneBrowserView(tab) →
      SingleTabRootView(tab) →
        ActroReactNativeHost.shared.viewForTab(tabId, props) →
          viewCache[tabId] Cache RCTRootView

User exit fullscreen (%1)RN (b) A side notice or gesture) →
  takeSnapshot() Screenshot Save →
  fullscreenTab = nil →
  sheet Put it away. →
  Back to the tab list page (at this time) viewCache Still held RootView)
```

**Key issues**: the tab after exiting fullscreen back to the list `RCTRootView` **Not released.**, still in place `viewCache`. Next time you open the same label, you will retrieve it from the cache (quick), but it also means that every previously opened label will always have a complete copy. React Run time memory.

---

## 3. Ghen analysis

### 3.1 Every one. Tab It's... RCTRootView Permanent Memory (core issues)

**Status**: `ActroReactNativeHost.viewCache` It's a strong reference dictionary. Every one that's been opened. Tab It's... `RCTRootView` Never be released.

```
ActroReactNativeHost.swift:88
private var viewCache: [UUID: RCTRootView] = [:]

// Just... closeTab I'm just cleaning up.
BrowserViewModel.swift:437-438
func closeTab(id: UUID) {
    ActroReactNativeHost.shared.removeViewForTab(tabId: id)
    ...
}
```

**Problem**:
- Every one. `RCTRootView` Inline Complete React Component Tree + JS Bridge. + Yoga Layout Engine
- iPhone Memory ratio iPad More limited (typical equipment) 4-6GB) Accumulation reaches the ceiling faster
- N Used to open tabs = N Package React Run-time → Memory linear growth
- Even if the label is not full screen, backstage RootView It's... React Rendering loop is still running

**Compare Browser**: Chrome/Safari Active retrofit engines are maintained only for current visible labels, and backstage tag processes are hung or released.

---

### 3.2 Backstage. RootView Medium React Component is continuously depleted CPU

**Status**: Each `RCTRootView` Even if it's not on the screen, it's inside. React Component is still in operation:
- ActrBridge It's... `StreamPrintCallback` Continuously receiving current messages, trigger `sendEvent` Update Component
- in the component Timer, Animation, WebSocket Waiting for continuous running
- Trigger of each event React re-render → Yoga Layout Count → View Properties Update

**Problem**:
- N A backstage label. React Rendering loop supersing → CPU Continuing high load → Hot.
- Every one. `sendEvent` Let's go. JS Bridge → Main line Queue Competition → UI - Carton.

---

### 3.3 Rebuild View Costs for Tab List Pages

**Status**: `PhoneTabListView` , and then click the `@ObservedObject var viewModel` Watch the whole thing. `BrowserViewModel`.

```
PhoneTabListView.swift:39
ForEach(viewModel.tabs) { tab in
    PhoneTabCardView(tab: tab, viewModel: viewModel, ...)
}
```

**Problem**:
- `tabs` Yes. `@Published` Value type array, any Tab Properties Change → The whole thing. ViewModel `objectWillChange` → Redraw all cards
- For example, a label receives unread messages → Update `hasUnreadMessage` → Recalculation of all cards body
- The more labels, the higher the cost of redrawing one change.

---

### 3.4 tabs didSet Trigger frequent disk save

**Status**:

```
BrowserViewModel.swift:23-26
@Published var tabs: [Tab] = [] {
    didSet {
        saveTabs()  // ← Every change triggers a permanent
    }
}
```

**Problem**:
- `tabs` is a value type array, and any element modification (title, icon, red dot, etc.) triggers `didSet`
- `saveTabs()` Encoding All Tab Yes JSON Write UserDefaults + ♪ Walking through all Tab Save screenshot to disk
- `setTabUnreadStatus` It's a high frequency operation. → Triggers the full disk every time. I/O
- Yes. iPhone End, frequent disk writing increases the fever (high flash writing)

---

### 3.5 Tab Double occupancy of the memory of the snapshot

**Status**:

```
BrowserViewModel.swift:58
@Published var tabSnapshots: [UUID: UIImage] = [:]
```

`PhoneMainView.takeSnapshot` Capture full screen to `tabSnapshots`, in the meantime `setTabSnapshot` Call `saveSnapshot` Writes to disk.

**Problem**:
- Full Screen UIImage About 3-6 MB(iPhone 14 Pro (i) The number of people who have been killed by the virus is unknown.10 Tags ≈ 30-60 MB
- The screenshot is also in memory (`tabSnapshots`) and disk ( )images (b) (b) (b) (b) (b) (b) (b) (b) (b) (b) (b) (b) (b) (b) (b) (b) (b) (b) (b) (b) (b) (b) (b) (b) (b) (b) (b) (b) (b) (b) (b) (b) (b) (b) (b) (b) (b) (b) (b) (b) (b) (b) (b) (b) (b) (b) (b) (b) (b) (b) (b) (b) (b) (b) (b) (b) (b) (b) (b) (b) (b) (b) (b) (b) (b) (b) (b) (b) (b) (b) (b) (b) (b) (b) (b) (b) (b) (b) (b) (b) (b) (b) (b) (b) (b) (
- The card preview on the tab list page only needs a small thumbnail, but holds a full screen original

---

### 3.6 Lack of label ceiling

**Status**: No label cap mechanism in code.

**Problem**:
- User can create labels indefinitely without any backside protection
- iPhone The memory is more tense than iPad It's easier to trigger. OOM
- Chrome Move the end limit 50-100 A label,Safari There are similar limitations.

---

### 3.7 SingleTabRootView props Redundant Transfer in

**Status**:

```swift
// SingleTabRootView.swift:248-260
var props: [String: Any] = [
    "singleTabMode": true,
    "tabs": [tabDict],       // ← Pack in arrays, compatible with old logic
    "tab": tabDict,          // ← It's been passed again.
    "currentTabId": tab.id.uuidString,
    ...
]
```

**Problem**:
- The same. Tab Data were transmitted twice (`tabs` Array + `tab` Object)
- Script Contents (in`unifiedScript` ) as an inline string, each time props Updates are re-sequenced

---

## 4. Browser Tag Management Policy Reference

| Policy | Chrome iOS | Safari iOS | Firefox iOS |
|------|-----------|-----------|-------------|
| **Process Model** | Backstage Tags WebKit Process Suspend | Backstage label is completely frozen | Backstage label release render resource |
| **Memory Recovery** | Automatically discard background label rendering tree when memory pressure | System-level memory pressure recovery | Backstage label releases non-critical resources |
| **Tab cap** | ~100 One, beyond the automatic discard of the old tag | No hard limit, but memory pressure recovery | No hard limit |
| **Recovery mechanisms** | Show snapshots when cutting back + "Reload" | Reload when you cut back | Reload when you cut back |
| **Cache** | Screenshots when cut off, first display the screenshot when cut back | Similar Chrome | Similar Chrome |
| **Lazy load.** | Backstage tab not executed JS | Backstage label is completely frozen | Delay executing background tabs |

**Core principles**: **Only the current active tab full rendering resource is retained, and the background tab only retains the minimum metadata required for recovery + Quick-Sync**.

---

## 5. Solutions

### 5.1 viewCache For LRU Limited cache (highest priority)

> **Why can't we get it all out? RootView?** Once released, open the same label again and you need to re-go `createRootView`including React Component Tree Build + JS Implementation + Yoga Layout + First refill, time-consuming. 500-1500ms, the user will see Loading Or a blank flash. Full release leads to each reopening being loaded and waiting, which is less than the current scheme.

**Programme**: will `viewCache` From Infinite Cache to LRULimited cache (recent minimum) only recently opened 2-3 A label. RootView.

**Why? 2-3 That's enough.**: iPhone At the same time, only 1 The user's most common switch mode is"Exit A → Open B → Cut back. A", 2-3 A cache covers this back and forth. Unused old labels are automatically released, and avoided."10 All Tabs Memory"Problem.

**Adaptation `ActroReactNativeHost`**:

```swift
// Current: Infinite Cache
private var viewCache: [UUID: RCTRootView] = [:]

// After the modification:LRU Limited Cache
static let maxCacheSize = 3  // iPhone End Reservations 2-3 individual

private var viewCache: [UUID: RCTRootView] = [:]
private var accessOrder: [UUID] = []  // Queue in order of access (most recent visit at the end)

func viewForTab(tabId: UUID, initialProperties: [String: Any]) -> UIView {
    if let cachedView = viewCache[tabId] {
        // Cache on impact: Move to the end of the queue (marked as most recently used)
        accessOrder.removeAll { $0 == tabId }
        accessOrder.append(tabId)
        return cachedView
    }

    // Uncut: create new RootView
    let rootView = createRootView(initialProperties: initialProperties)
    if let rctRootView = rootView as? RCTRootView {
        viewCache[tabId] = rctRootView
        accessOrder.append(tabId)

        // Overceeding the ceiling: release of the longest unused RootView
        while viewCache.count > maxCacheSize {
            let oldestTabId = accessOrder.removeFirst()
            viewCache.removeValue(forKey: oldestTabId)
        }
    }
    return rootView
}
```

**Expected impact**:

| scene | Current | LRU - I'm not. |
|------|------|-----------|
| Most recently 2-3 Tabs Switchback and Up | Open Instant | On-the-spot open (cacheline) |
| I'm not sure. 4 A label and older ones. | On-site Open (all caches) | Need to recreate (approximately) 500-1500ms) |
| Memory occupation | N × ~40MB | max(3, N) × ~40MB |
| 10 Tab Memory | ~400MB | ~120MB |

---

### 5.2 Scratch position: Mitigation LRU The loading experience of the phase-out label

LRU Create new when cache is not hit RootView(About 500-1500ms. The transition to a place using a snapshot thumbnail is felt by the user."Quickshot → Real content"Not"Space → Contents".

**Adaptation `PhoneBrowserView`**:

```swift
struct PhoneBrowserView: View {
    let tab: Tab
    @ObservedObject var viewModel: BrowserViewModel
    var historyVM: HistoryViewModel
    @State private var isRootViewReady = false

    var body: some View {
        GeometryReader { geometry in
            ZStack {
                // Bottom: Snapshot position (%)RootView Show during creation)
                if let snapshot = viewModel.tabSnapshots[tab.id] {
                    Image(uiImage: snapshot)
                        .resizable()
                        .aspectRatio(contentMode: .fill)
                        .frame(width: geometry.size.width, height: geometry.size.height)
                        .opacity(isRootViewReady ? 0 : 1)
                        .animation(.easeOut(duration: 0.3), value: isRootViewReady)
                }

                // Upper: Real RN Contents
                SingleTabRootView(tab: tab, viewModel: viewModel, ...)
                    .id(tab.id)
                    .opacity(isRootViewReady ? 1 : 0)
                    .animation(.easeIn(duration: 0.2), value: isRootViewReady)
                    .onAppear {
                        // RootView Switch to the actual content when retrofitting is ready
                        DispatchQueue.main.asyncAfter(deadline: .now() + 0.5) {
                            isRootViewReady = true
                        }
                    }
            }
            .frame(maxWidth: .infinity, maxHeight: .infinity)
            .background(Color.black)
        }
    }
}
```

---

### 5.3 Scratch-down sampling (co-operation) 5.2)

**Programme**: Cuts out the thumbnails instead of the original screen, and significantly reduces the memory.

**There is a snapshot mechanism in place.**(`PhoneMainView.takeSnapshot`), which requires strengthening:

1. **Samples down**: QuickScalp to the appointment 200x400px(card preview size) 50KB, not fullscreen 3-6MB
2. **Timing**: In: sheet Autocut before close, not rely on extra logic
3. **Load on demand**: Tab List Page `LazyVStack` Only load visible area snapshots, and a copy of the disk is read on demand while rolling

```swift
// Dropping sample
func downsampledSnapshot(_ image: UIImage, targetSize: CGSize) -> UIImage {
    let renderer = UIGraphicsImageRenderer(size: targetSize)
    return renderer.image { _ in
        image.draw(in: CGRect(origin: .zero, size: targetSize))
    }
}
```

---

### 5.4 Exiting fullscreen specific modifications

**Current Code**: Only snapshots are intercepted when exiting full screen, and not released RootView.

```swift
// PhoneMainView.swift:101-109
.onReceive(NotificationCenter.default.publisher(for: .exitFullscreenFromRN)) { _ in
    if let tab = viewModel.tabs.first(where: { $0.id == fullscreenTab?.id }), !tab.title.isEmpty {
        takeSnapshot(for: tab) {
            fullscreenTab = nil  // ← sheet I'm not sure if I can get it.RootView Still viewCache Medium
        }
    } else {
        fullscreenTab = nil
    }
}
```

**- I'm not.**: LRU Managed by `ActroReactNativeHost` Automatically, without manual release. When you exit the full screen, you will only need to intercept the snapshot:

```swift
.onReceive(NotificationCenter.default.publisher(for: .exitFullscreenFromRN)) { _ in
    if let tab = viewModel.tabs.first(where: { $0.id == fullscreenTab?.id }), !tab.title.isEmpty {
        takeThumbnailSnapshot(for: tab) {  // ← Replaced with the subsampling thumbnail
            fullscreenTab = nil
        }
    } else {
        fullscreenTab = nil
    }
}
```

LRU Cache phase-out logic automatically triggers next time you open a new tab——`viewForTab` Create New RootView , if `viewCache.count > maxCacheSize`Automatically release old tags that have been unvisited for the longest time RootView.

---

### 5.4 Long-lasting shiverproofness

**Programme**: will `saveTabs()` From `didSet` Move out and change to tremors-proof saving.

```swift
private var saveTabsWorkItem: DispatchWorkItem?

func scheduleSaveTabs() {
    saveTabsWorkItem?.cancel()
    let workItem = DispatchWorkItem { [weak self] in
        self?.saveTabs()
    }
    saveTabsWorkItem = workItem
    DispatchQueue.main.asyncAfter(deadline: .now() + 2.0, execute: workItem)
}
```

Move screenshot saving to `setTabSnapshot` The project is being implemented independently (which has already been achieved in the current code), and is being implemented independently.`saveTabs()` Only responsible. JSON Encoding + UserDefaults Write, no more cross-references.

---

### 5.5 Set a maximum number of labels

**Programme**: Reference Chrome iOS, set the label cap.

```swift
static let maxTabCount = 30  // iPhone The end suggestions are more conservative.

func addNewTab(...) -> Tab? {
    guard tabs.count < Self.maxTabCount else {
        logger.warning("The label ceiling has been reached \(Self.maxTabCount)")
        // Optional: Automatically mark the longest unused label as suspended
        return nil
    }
    // ...
}
```

It's all possible. **LRU Phase-out**: those that will remain unvised for the longest time when the ceiling is exceeded Tab Mark as `suspended`(not just closes) to recover when the user cuts back.

---

### 5.6 Particle View Update

**Programme**: Avoid one Tab The change in properties triggers the redrawing of all cards.

1. **Tab For class**: `class Tab: ObservableObject, Identifiable`, each attribute used `@Published`
2. **PhoneTabCardView Direct observation Tab Object**: `@ObservedObject var tab: Tab` Alternative `let tab: Tab` Value Transfer
3. **ViewModel No more global broadcasts Tab Properties Change**: only structural changes (additions/deletions) TabIt's only triggered. `objectWillChange`

---

### 5.7 Memory Pressure Interception and AutoRecovery

**Programme**: Warnings are stored in the listening system, and the backstage label is automatically downgraded.

```swift
// AppDelegate
func applicationDidReceiveMemoryWarning(_ application: UIApplication) {
    Task { @MainActor in
        BrowserViewModel.active?.handleMemoryWarning()
    }
}

// BrowserViewModel
func handleMemoryWarning() {
    // 1. Release all inactive Tab It's... RootView
    for tab in tabs where tab.id != currentTabId {
        ActroReactNativeHost.shared.removeViewForTab(tabId: tab.id)
    }
    // 2. Release a copy of the snapshot memory (reduced disk copy, loaded as required)
    tabSnapshots.removeAll()
}
```

---

### 5.8 SingleTabRootView props Redundant Cleanup

**Programme**: Remove `makeProperties` Medium `tabs` Group packing, pass only `tab` Single object.

```swift
var props: [String: Any] = [
    "singleTabMode": true,
    "tab": tabDict,              // ← Only once.
    "currentTabId": tab.id.uuidString,
    // ... Remove "tabs": [tabDict]
]
```

---

## 6. Key issue: Release Backstage RootView What about the news?

Current `viewCache` Keep All Tab It's... `RCTRootView` It's a permanent, so it's a backstage label. React Component is running, receiving real-time information and updating UIRelease. RootView There is a need to ensure that the notification mechanism is not affected.

### 6.1 Current message notification link analysis

**Link A: Actr Real time message (%1)AI ♪ Talk to the ♪**

```
ActrBridge.actorInstances[tabId] → StreamPrintCallback.onStream()
  → bridge.sendStreamEventToRN("ActrStreamMessage")
  → Current Full Screen Tab RN Component Receives and Renders
```

Key points:**ActrBridge It's independent. Native Module, don't depend RCTRootView**.`actorInstances` Press `tabId` Manage connections, through `RCTEventEmitter` Radio incident. Even if it was a certain one. Tab It's... RootView Released, just... Bridge Exists, events still send——Just no components in the listening.

**Link B: Tab Notify unread red dots**

```
RN Side Call TabModule.setTabUnreadStatus(tabId, hasUnread)
  → Update viewModel.tabs[index].hasUnreadMessage
  → SwiftUI Observe changes, update the red dot of the label list card
```

Key points:**Not at all. RCTRootView Cache**- I'm through. Native Module Direct Modify ViewModel, SwiftUI Automatic response.

### 6.2 iPhone Endscape Analysis

iPhone Peer Interactive Model Ratio iPad It's easier on the end.——**At the same time, at most. 1 Tabs in fullscreen status**. This means:

- When Users are Tabs A When fullscreen, tab B It's... RootView Not in View Tree
- Exit full screen after tab A It's... RootView Not in the view tree.
- **Whether or not it's released. viewCacheNon-full-screen label RootView Not in view tree, rendering events cannot be received**

So, in... iPhone End, release. `viewCache` The impact on the notification is actually even less:

| Functions | Current behaviour | Release viewCache Back |
|------|---------|-----------------|
| Red Point Notification | Normal | Normal (TabModule Directly. ViewModel) |
| Streaming of Full Screen Tabs | Normal (Yes) RootView Receivable) | Normal (full-screen label is the only active RootView) |
| Flow Messages for Backstage Tabs | sendEvent Sending without component rendering | Same as the component rendering. |
| Tots back the tabs to resume the dialogue | From viewCache Retrieving old RootView(Quick but with risk of state residual) | Create New RootView(Recoverable strategy) |

### 6.3 Solutions: Layer release + Message Agent

Core thinking:**Release Render Layer (Leave Render Layer)RCTRootView) but keep the connector layer (ActrBridge) and data layers ( )RN Side Status)**.

#### Programme A: Native Layer Information Agent (Recommended)

Yes. Native Creates a message agent for background labels even RootView Released, the information will not be lost:

```
┌─────────────────────────────────────────────────┐
│                  Native Layer                    │
│                                                  │
│  ActrBridge (Independent RootViewAlways alive.)           │
│    ├─ actorInstances[tabId] → Connection maintenance           │
│    ├─ We got a current. → Determines whether fullscreen labels are available              │
│    │     ├─ Fullscreen:forwardToRN(msg) Normal delivery.       │
│    │     └─ Non-fullscreen: deposit message queue + Trigger Red Point        │
│    └─ onTabResumed → flushQueuedMessages Batch Transfer │
│                                                  │
│  TabMessageProxy (Add)                          │
│    ├─ perTabQueues: [UUID: [Message]]            │
│    ├─ onStreamMessage(tabId, msg) {              │
│    │     if isActiveTab(tabId) {                 │
│    │       forwardToRN(msg)                      │
│    │     } else {                                │
│    │       queue.append(msg)                     │
│    │       notifyUnread(tabId)                   │
│    │     }                                       │
│    └─ onTabResumed(tabId) {                      │
│          flushQueuedMessages(tabId)              │
│        }                                         │
└─────────────────────────────────────────────────┘
```

**Elements for achievement**:

1. **ActrBridge Adds message proxy logic**:
   - When you get backstage, Tab ..no more direct messages `sendEvent`
   - Replace message with message cache to `perTabQueues[tabId]`
   - Call at the same time `TabModule.setTabUnreadStatus(tabId, true)` Trigger Red Point

2. **Tab Replay Messages When Restore**:
   - Create New RootView Then, the queued message passes. `sendEvent` Batch Transfer
   - RN Component by `sessionId` The paragraph should be re-emphasized and the dialogue should resume in its context.

3. **ActrBridge Life cycle and RootView Undo the coupling.**:
   - `actorInstances` I don't want to. RootView Released and cleared.
   - Only `closeTab` Times Call `shutdown(tabId:)`
   - Do not call when exit fullscreen `shutdown`, keep the connection active

#### Programme B: RN Enduring side message

Yes. RN Side-use status management (PACT)Redux/Context/Zustand) , / / / / / / / / , / / , / / , / , / , / , / , / , / , / , / , / , / , / , / , / , / / , / , / , / , / / , / , / / / , / / , / , / / / , / / , / / / / / / / / / / , / / / / / / , / / Store:

```
RN Side:
  - Every one. Tab The history of the dialogue is entered in the global context. Store(Unlost with components unloaded)
  - RootView ♪ When Rebuild From ♪ Store Reconstructing the dialogue context
  - Equals"Refresh Pages but Keep Chat Record"
```

Advantages: Simple, not needed Native Layer changes
Disadvantages: Flow-in message in RootView The middle process is lost during absence

#### Programme C: Mixed programme (recommended landing path)

Combined A and B, in phases:

| Phase | Policy | Effects |
|------|------|------|
| **Phase 1** | Release on Exit Full Screen RootView + Intercept thumbnails; red dots normal; recreated when reopened RootView + From RN Store Restoring the history of dialogue | We're losing much of our memory. Red dots are normal. The history of dialogue can be restored. |
| **Phase 2** | Increase Native Message agent, Cache Backstage Tab The current message;Tab Batch Replay When Restore | Flow-of-the-way news is missing and the experience is consistent with the current situation. |

### 6.4 Issues to be identified

1. **RN Where's the side-talking status stored?** If within the component state, RootView It's lost after release. If you're in the whole world, Store(Redux/Zustand) can be restored.
2. **ActrBridge It's... `actorInstances` Can you step over? RootView Life cycle survival?** Current `shutdown` Only RN The side-active call triggers, theoretically.
3. **Is the missing frame of the current message acceptable?** AI The final text can be passed in the dialogue setting Store Recovery; mid-situation in real-time collaboration is not lost and requires a programme A The news agent.

---

## 7. Priorities and implementation routes

| Phase | Programme | Expected income | Complexity |
|------|------|----------|--------|
| **P0 Emergency** | 5.1 viewCache LRU Limited Cache | Memory from O(N)→O(3) | Low |
| **P0 Emergency** | 5.5 Maximum number of labels | Preventing unlimited growth | Low |
| **P0 Emergency** | 5.7 Memory Pressure Interception | System-level bottom protection | Low |
| **P1 Important** | 5.2 Quick-start the choreography. | Mitigation LRU Undecided load | Medium |
| **P1 Important** | 5.3 Scratch-down sample. | Snapshot memory down 90% | Low |
| **P1 Important** | 5.4 Long-lasting shiverproofness | Reduction I/O Block | Low |
| **P2 Improvements** | 5.6 Particle View Update | Reduce list page repainting | Medium |
| **P2 Improvements** | 5.8 props Redundant Cleanup | Reduced bridge costs | Low |

---

## 8. Expected impact

Implementation P0 After the programme:
- **Memory occupation**: From N × ~40MB Down to min(N, 3) × ~40MB + N × 50KBThumbnail
- **Recent Tab Switch**: still open from the cache, experience is consistent with current
- **Reopen old tab**: Required 500-1500ms Create RootView, but the graphs are moving, and the user senses that"Crack in → Real content"
- **Hot.**: Large-scale mitigation (maximum) 3 A backstage. RootView, not N (a)
- **Stability**: Label cap protection + Memory Warning Recovery, No OOM Crash

10 A label. iPhone Memory comparison estimates:

| Item | Current | Optimized |
|------|------|--------|
| RCTRootView | ~40MB × 10 = 400MB | ~40MB × 3 = 120MB(LRU Cache ceiling) |
| Snapshot Memory | ~4MB × 10 = 40MB | ~0.05MB × 10 = 0.5MB(Little sample thumbnails) |
| Backstage. CPU | N individual React Rendering loops | Most 3 A backstage. RootView |
| **Total** | **~440MB** | **~120.5MB** |