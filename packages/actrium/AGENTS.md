# Actro Runtime (ActroRuntime) Operational nuclear architecture

`ActroRuntime` It is the cross-platform business logic centre of the project. It exists as a stand-alone warehouse and does not depend on a specific project. Native UI, but by standard. Bridge Communication with the host shell.

## 1. Core module division

```
ActroRuntime/src/
├── TabRouter.tsx          # Top route route: to be distributed RuntimeApp (Plugin) or SystemApp (Internal Functions)
├── runtime-host/             # Core Host Logic
│   ├── RuntimeApp.tsx        #   More Tab Manage and global monitoring of delivery
│   ├── RuntimeTabView.tsx    #   Box box (Keel Engine) and the full life cycle management of the United Nations system
│   ├── components/        #   Two-end support for responsiveness UI Component Library
│   └── store/             #   Zustand Status machine (Message, App, Input in particular)
├── native-bridge/         # Primary telecommunications layer: sealed Actr WebRTC The Complex Signal Logic
├── repository/            # Data Layer: Based on SQLite Local Endurance (MessageRepository)
├── services/              # Organization services:App Session Initialization, Attachment Uploading, Resource Download
└── i18n/                  # Internationalized support
```

---

## 2. Key design models

### A. Double Response Fit (Layout Tokens)
Pass. `src/utils/layout.ts` Define a set of visuals Token. System listening `window` Size change:
- **Pad (>=768px)**: Enable `AppInputPadView`, with a fixed width side toolbar and three-bar layout.
- **Phone**: Enable `AppInputMobileView`, with a wide input frame, large button gesture area and bottom Overlay.

### B. Box box (Host-Guest)
- Each plugin is running in isolation `Engine` Example.
- `ActrHost` As a control, pass. `eventHandler` Listening Guest Triggered request (e.g. HTTP Agent,TTS Play, voice-gathering notice.

---

## 3. Core data stream

### AI Dialogue stream (Flow)
1.  **Input**: User Trigger `processMessageSend`.
2.  **Persistence**: Can not open message SQLite (`MessageRepository`) , and update Zustand Store.
3.  **Transport**: Call `native-bridge` ♪ In the channel, push the data to the original Actr SDK.
4.  **Reaction**: Original hiking back to the message clip, via `ActrStreamRouter` Return to corresponding business listening, achieved UI Message flow-cum-cum-s.

### Voice-processing stream (Audio)
- **Pad**: Click to trigger the recording, by host UI Real time wave shape.
- **Phone**: Long press input area trigger, use `VoiceOverlay` Override global, support“Decline”Wait for gestures.
- **Send**: Slice on a continuous basis (Slices) Cache -> Start real time after trigger threshold WebRTC Transfer -> Let go of me.

---

## 4. Code of collaboration
- **UI Process**: Avoid in UI Write complex business decisions, and try to sink to the bottom. `hooks` or `services`.
- **Data consistency**: Memory Store It has to be local. SQLite ♪ And the final agreement is in principle ♪ Store Trigger Repository Write.

## 5. iOS / Android The safe zone handles the engagements.

### Top security area (Purpose security zone)Header)
- The top security zone is not a single point of the global, but is separated by sub-area:
  - `runtime-host`(plug-in session domain): by `runtime-host/components/ChatHeader.tsx` Top of the configuration inset.
  - `system-apps`(system application domain): Pages/Component control top retention (mostly fixed currently) `paddingTop`for example `SettingsMain.tsx` / `GlobalSearchApp.tsx` It's... `paddingTop: 60`).
- `ChatHeader` (only applicable to `runtime-host`):
  - Use `useSafeAreaInsets().top` Read System Top inset.
  - `includeSafeTopInset=true`(default)Header Height is `CHAT_HEADER_HEIGHT + topInset`and through `paddingTop` Eat Liu Hai./status bar area.
  - Close internal when top deviation is processed inset, avoid overlapping (e.g., `RuntimeTabView.tsx` Outer `marginTop: topInsetRaw`Counterpart `includeSafeTopInset={false}`).

### Bottom security zone (underground)Input/Dock)
- Adopted unanimously `runtime-host/components/SafeBottomView.tsx` Process the bottom security area and do not duplicate the platform branch on the business page.
- `SafeBottomView` Rules:
  - iOS: `extraBottom = 0`, no extra supersing `insets.bottom`(by) `SafeAreaView` It is a security zone that is at the bottom of the area.
  - Android: `extraBottom = max(insets.bottom, minBottom)`, ensures that the hand-nap and general models are sufficiently bottom-white.
  - Eventually. `paddingBottom = Original paddingBottom + extraBottom`, the caller does not overwrite the existing style.
- Typical usage:
  - `AppInputMobileView.tsx`: `<SafeBottomView minBottom={10} />`
  - `LoadApp.tsx`(Phone Input Dock): `<SafeBottomView minBottom={12} />`

### Principles of implementation
- Add bottom input bar, float Dock, suspended lines, first reuse `SafeBottomView`,do not write directly on page `Platform.OS` + `insets.bottom`.
- `runtime-host` Add Top Header/When you're in the navigational mode, you'll have to make a determination as to whether the outer layer is done. `top` Offset, then decide. `includeSafeTopInset`Avoid a double safe zone.
- `system-apps` Do not use default reuses when adding pages `ChatHeader` semantics;specify clearly on page“Fixed top white”or“Based on `useSafeAreaInsets().top` Dynamically leave a message.”and keep the same system pages.


### About Keyboard


| Platform | Native Process | RN Process |
| :--- | :--- | :--- |
| iOS Phone | None | KeyboardStickyView Sorrow Input Box + useReanimatedKeyboardAnimation Real-time compression of content areas (and manual animation) |
| iOS Pad | None | keyboardInset Static offset (incident driven, keyboard jump after impact) |
| Android Phone | Host Native windowSoftInputMode Push the whole view | No additional offset |
| Android Pad | Host Native windowSoftInputMode Push the whole view | Zero-zero intervention. |


### Important

1. All changes to be considered iOS and Android Compatibility
2. Dark and light mode compatible

```js
  const makeStyles = (colors: ColorTokens) => StyleSheet.create({ ... })
  const styles = useMemo(() => makeStyles(colors), [colors])
```