# Password Vault Flow Diagrams

Password Vault module.

Documents:
- `Actrium/Settings/Security/VaultManager.swift` — Core status machine, encryption,Keychain, auto-lock, hint Storage
- `Actrium/Settings/Security/SetupMasterPasswordView.swift` — First setting password UI
- `Actrium/Settings/Security/UnlockVaultView.swift` — Unlock UI(Show) hint)
- `Actrium/Settings/Security/PasswordField.swift` — Reversible password input component + PasswordStrength enum
- `Actrium/Settings/PasswordSettingsView.swift` — Unlocked management page (substance list,Security section(Class change)

---

## 1. Vault State Machine

Tri-state limited-state machine, durable. DB Existence in `salt` Record.

```mermaid
stateDiagram-v2
    [*] --> notSetup : DB None salt
    [*] --> locked : DB Yes. salt

    notSetup --> unlocked : setupMasterPassword()
    locked --> unlocked : unlock() / unlockWithBiometrics()
    unlocked --> locked : lock() / auto-lock timer
```

| State | Meaning | UI |
|-------|------|-----|
| `notSetup` | Never set master password | SetupMasterPasswordView |
| `locked` | Set up, password required | UnlockVaultView |
| `unlocked` | Unlocked, operational evidence | UnlockedVaultView |

---

## 2. Setup Master Password

Sets the main password for the first time. Creates salt- It's a birth-coded code. key, store the authentication badge, and enter an optional password hint.

**Note:** Bio-identification (BIO)Touch IDI'm not here. Setup Phase setup to unlock and be in Security section Independent management.

```mermaid
flowchart TD
    A[User Enter Password + Confirm password + Optional hint] --> B{password >= 5 and<br/>Twice.?}
    B -- No --> A
    B -- Yes --> C[button to click]
    C --> D[isProcessing = true]
    D --> E[Generate Random salt 32B]
    E --> F["🔄 Backstage thread<br/>PBKDF2 Pumpkin. key (100k Wheel)"]
    F --> G[Save salt → DB]
    G --> H[Encryption verification token → DB]
    H --> I{hint Non-empty?}
    I -- Yes --> J[Save hint → DB vault_meta]
    I -- No --> K[Skip]
    J --> L[state = unlocked]
    K --> L
    L --> M[Start auto-lock timer]
    M --> N[isProcessing = false]

    F -. Failed .-> ERR[Throw keyDerivationFailed]
    ERR --> N
```

**Key design decisions:**
- PBKDF2 Pass. `Task.detached` Run backstage lines to avoid jamming. UI
- Touch ID Not anymore. setup Phase Settings, after Unlock Security section Independent administration
- Password Hint (Phone)hint) Optionally to fill in DB `vault_meta` Table

---

## 3. Unlock with Password

Password unlocking process. DB Read salt- What? key, verify if the decryption is correct verification token.

If it's set up, password hint, unlocking the page will show a hint to help the user remember the password.

```mermaid
flowchart TD
    A[User Enter Password] --> B{Password is not empty?}
    B -- No --> A
    B -- Yes --> C[isProcessing = true]
    C --> D[From DB Read salt]
    D --> E{salt Existence?}
    E -- No --> F[Throw notSetup]
    E -- Yes --> G["🔄 Backstage. PBKDF2 Pumpkin. key"]
    G --> H[From DB Read verification_hash]
    H --> I[- With a birth. key Decrypt token]
    I --> J{Decrypt Results ==<br/>vault-verification-token?}
    J -- No --> K[derivedKey = nil]
    K --> L[Show Incorrect password]
    J -- Yes --> M[state = unlocked]
    M --> N[Load and decrypt all the documents]
    N --> O[Start auto-lock timer]
    O --> P[isProcessing = false]
    L --> P

    style A fill:#e8f4fd
    note right of A: Page Display hint Tips (if any)
```

---

## 4. Unlock with Biometrics (Touch ID)

Touch ID Unlock is the shortcut to unlock from the password: Keychain Checkout Password → Take the normal password to unlock.

```mermaid
flowchart TD
    A[UnlockVaultView onAppear] --> B{isTouchIDEnabled<br/>And hardware is available.?}
    B -- No --> C[Show password input box only]
    B -- Yes --> D[Show Touch ID button<br/>+ Auto-activate unlock]
    D --> E[Keychain Read Password<br/>Trigger System Touch ID Blast the window.]
    E --> F{User Authentication Successful?}
    F -- Yes --> G[Get an express password]
    G --> H["Call unlock(password:) Process"]
    F -- No --> I[Silence fails.<br/>User can manually enter password]
```

**Design decision-making:** Automatically unlocks without a bug hint to avoid interfering with the user's manual input.

---

## 5. Change Master Password

Modifying the master password requires: Verifying the old password → Decrypt all the evidence. → Generate new salt/key → Reencrypt all the documents. You can also change the password hint.

```mermaid
flowchart TD
    A[Input: Old password + New Password + Confirm. + hint] --> B{Old password is not empty<br/>New Password >= 5<br/>Twice.?}
    B -- No --> A
    B -- Yes --> C[isProcessing = true]
    C --> D["🔄 Use old password + Old salt Pumpkin. oldKey"]
    D --> E[Use oldKey Decrypt verification token]
    E --> F{Authentication pass?}
    F -- No --> G[Show wrongPassword]
    F -- Yes --> H[Use oldKey Decrypt all the documents to memory]
    H --> I[Generate new salt 32B]
    I --> J["🔄 Use new password + New salt Pumpkin. newKey"]
    J --> K[Save salt → DB]
    K --> L[Use newKey Encrypt New verification token → DB]
    L --> M[Use newKey Reencrypted article by article → DB]
    M --> M2[Update hint → DB]
    M2 --> N{isTouchIDEnabled?}
    N -- Yes --> O[Update Keychain Password in]
    N -- No --> P[Skip]
    O --> Q["derivedKey = newKey"]
    P --> Q
    Q --> R[isProcessing = false, dismiss]
    G --> R
```

**Note:** The change of the password involves two times. PBKDF2 Derivative (old) key + New keyIt's a long time,loading The state of affairs is particularly important.

---

## 6. Security Section (Unlocked View)

After unlocking footer Area to structured Security section:

```
┌─ Security ────────────────────────────┐
│  Auto-lock:      [Never ▾]            │
│  Touch ID:       [Toggle] (Hardware required)  │
│  Password hint:  xxxxxxx  [Edit]      │
│  [Change Password...]                 │
└───────────────────────────────────────┘
```

- **Auto-lock**: Select auto-lock time (%1)Never / 1 / 5 / 10 / 15 min)
- **Touch ID**: Open with password confirmation (%1)TouchIDPasswordSheet) Closes and deletes directly Keychain
- **Password hint**: Unlocked to directly edit (inline) TextField) , no password to confirm
- **Change Password**: Open Change Password Sheet, and can be modified at the same time hint

---

## 7. Enable Touch ID (Post-Setup)

Unlocked to open Touch ID, the master password must be verified before the password can be stored Keychain.

```mermaid
flowchart TD
    A[User Open Touch ID Switches] --> B[Popup Password Confirm Sheet]
    B --> C[User Enter Master Password]
    C --> D["🔄 PBKDF2 Pumpkin. key"]
    D --> E[Decrypt verification token Authentication password]
    E --> F{The password is correct.?}
    F -- No --> G[Show Errors, Reservations Sheet]
    G --> C
    F -- Yes --> H[Save Password To Keychain]
    H --> I[isTouchIDEnabled = true]
    I --> J[dismiss Sheet]

    B -. User Cancel .-> K[toggle Back to false]
```

**Security considerations:** Not a direct binding. toggle Present. `vault.isTouchIDEnabled`I'm using it. `@State` Intermediate variable + `onChange` + Sheet Confirm process.

---

## 8. Disable Touch ID

Close Touch ID No password authentication required, just delete Keychain entry.

```mermaid
flowchart TD
    A[User Close Touch ID Switches] --> B[Delete Keychain Password in]
    B --> C[isTouchIDEnabled = false]
```

---

## 9. Auto-Lock Timer

Automatic lock timer, reset after multiple operations.`timeout = 0` Organisation Never.

```mermaid
flowchart TD
    A[Trigger setupAutoLockTimer] --> B[Cancel Old timer]
    B --> C{"state == unlocked<br/>and timeout > 0?"}
    C -- "No (locked or Never)" --> D[Do Not Set timer]
    C -- Yes --> E["Create timer = timeout × 60s"]
    E --> F[Waiting for timeout...]
    F --> G["lock(): Clear. key + Clear evidence<br/>state → locked"]

    H[Reset the following operation timer] --> A
    H1[addCredential] --> H
    H2[updateCredential] --> H
    H3[deleteCredential] --> H
    H4[unlock Success] --> H
    H5[setupMasterPassword] --> H
    H6[User Changes timeout Value] --> H
```

**Timeout Options:** `Never (0)` / `1 min` / `5 min` / `10 min` / `15 min`Default 5 min.

---

## 10. Password Strength Evaluation

Validity of passwords, in real time, for use Setup and Change Password page.

```mermaid
flowchart LR
    P[Password Input] --> S1{"Length >= 5?"}
    S1 -- "+1" --> S2{"Length >= 10?"}
    S1 -- "+0" --> S2
    S2 -- "+1" --> S3{"Capitalise<br/>And with lowercase?"}
    S2 -- "+0" --> S3
    S3 -- "+1" --> S4{"Organisation?"}
    S3 -- "+0" --> S4
    S4 -- "+1" --> S5{"Organisation<br/>or symbol?"}
    S4 -- "+0" --> S5
    S5 -- "+1" --> SUM["Total 0~5"]
    S5 -- "+0" --> SUM

    SUM --> R1["0-1 → Weak 🔴"]
    SUM --> R2["2 → Fair 🟠"]
    SUM --> R3["3 → Good 🟡"]
    SUM --> R4["4-5 → Strong 🟢"]
```

| Score | Level | Colours | Shows the number of bright start points |
|------|------|------|--------------|
| 0-1 | Weak | Red | 1/4 |
| 2 | Fair | Orange | 2/4 |
| 3 | Good | Yellow | 3/4 |
| 4-5 | Strong | Green | 4/4 |

---

## 11. Reusable PasswordField Component

`PasswordField.swift` Provide reusable password input areas, using each scene:

| scene | showStrength | showConfirm | placeholder |
|------|-------------|-------------|-------------|
| Setup | true | true | "Master password" |
| Unlock | false | false | "Master password" |
| Change (current) | false | false | "Current password" |
| Change (new) | true | true | "New password" |
| Touch ID confirm | false | false | "Master password" |

---

## Crypto Summary

| Component | Algorithm/Parameters |
|------|----------|
| Key Derivation | PBKDF2-HMAC-SHA256, 100,000 iterations, 32B salt |
| Encryption | AES-256-GCM (CryptoKit) |
| Key Storage | macOS Keychain (Touch ID protected) |
| Verification | Encrypt Fixed String `"vault-verification-token"` Validate as password |
