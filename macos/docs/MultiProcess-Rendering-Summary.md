# Actrium Multi-Process Rendering - Summary of the period

> History. See the current path. [Renderer-Process-Pool.md](Renderer-Process-Pool.md).
>
> **It was a branch.**: `experiment/viewbridge`
> **The main attack.**: ViewBridge Framework (NSRemoteView)

---

## The branch's experimental direction

### ViewBridge Framework (NSRemoteView) - Main attack!

**Background**:
ViewBridge Yes. Apple Internally used private frame embedded in the cross-process view.macOS It's... Save/Open dialogue box, system preferences panel, etc. using this frame to achieve cross-process UI Show.

**Sources of reference**:
- [ViewBridge Framework Practical articles](https://medium.com/@avaidyam/viewbridge-framework-it-works-484de6f31f7d)
- [macOS Headers - NSRemoteView.h](https://github.com/w0lfschild/macOS_headers/blob/master/macOS/PrivateFrameworks/ViewBridge/383/NSRemoteView.h)

**Why this direction?**:
1. Apple Internal use is high and stability is assured
2. Developmentalists successfully achieve:"it works, and it works great... loads in less than 300ms"
3. Automatically handle event forwarding,Accessibility(Resolving core claims)
4. - Yes. CAContext/CALayerHost Higher abstract

**Core category**:
- `NSRemoteView` - Show remote view in main process
- `NSRemoteViewController` - XPC Service-end view controller
- `NSRemoteViewMarshal` - Cross-process data grouping
- `NSRemoteViewDelegate` - Life cycle echo

**Realising the idea**:
```
┌─────────────────────────────────────────────────────────────┐
│                     Main Process (Actrium)                    │
│  ┌─────────────────┐    ┌─────────────────────────────────┐ │
│  │   TabManager    │───▶│        NSRemoteView             │ │
│  │                 │    │  (displays remote view via XPC) │ │
│  └─────────────────┘    └─────────────────────────────────┘ │
│           │                          ▲                       │
│           │ XPC (ViewBridge)         │ View Content          │
│           ▼                          │                       │
└───────────┼──────────────────────────┼──────────────────────┘
            │                          │
┌───────────▼──────────────────────────┼──────────────────────┐
│                  Renderer Process (XPC Service)              │
│  ┌─────────────────┐    ┌─────────────────────────────────┐ │
│  │ NSRemoteView-   │───▶│     React Native View           │ │
│  │ Controller      │    │  (renders content in service)   │ │
│  └─────────────────┘    └─────────────────────────────────┘ │
└─────────────────────────────────────────────────────────────┘
```

**Key points to study**:
1. Info.plist Configure Item (XPC Service requires specific key value)
2. NSRemoteViewController The correct subclassification
3. Mechanism for transmitting incidents
4. Compared to existing XPC Integration of the structure

**Expected changes in documents**:
- Add `Actrium/MultiProcess/RemoteViewHost.swift` - NSRemoteView Encapsulate
- Modify XPC Services Info.plist - Add ViewBridge Configure
- It may need to create a new one XPC Services target

---

## Objective

Achieved Actrium macOS The applied multi-process architecture will React Native Rendering it to the independent process, the main process is through IPC Displays the rendering result.

**Core claims:**
- Process isolation:React Native The crash does not affect the main process
- Performance optimization: using multi-nucleus CPUAvoid blocking the main process.
- Hold Accessibility Accessibility

---

## Attempted Scheme

### Programme 1: IOSurface + CARenderer ❌ Failed

**Problem**:
- Metal Alignment crashes
- Cross-process IOSurface Failed to find
- It's gonna be lost. Accessibility Functions

### Programme 2: CAContext + CALayerHost ❌ Blackscreen

**Problem**:
- CALayerHost Could not show remote content
- `hasContents=false, sublayers=0`
- Private API Undefined behaviour

---

## References

1. [ViewBridge Framework Practice](https://medium.com/@avaidyam/viewbridge-framework-it-works-484de6f31f7d)
2. [macOS Headers - ViewBridge](https://github.com/w0lfschild/macOS_headers/tree/master/macOS/PrivateFrameworks/ViewBridge)
3. [NSRemoteView.h](https://github.com/w0lfschild/macOS_headers/blob/master/macOS/PrivateFrameworks/ViewBridge/383/NSRemoteView.h)

---

*Last Update: 2026-01-01*
