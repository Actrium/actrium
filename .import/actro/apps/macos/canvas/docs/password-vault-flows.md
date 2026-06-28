# Password Vault Flow Diagrams

Password Vault 模块的完整逻辑流程文档。

涉及文件：
- `Canvas/Settings/Security/VaultManager.swift` — 核心状态机、加密、Keychain、auto-lock、hint 存储
- `Canvas/Settings/Security/SetupMasterPasswordView.swift` — 首次设置密码 UI
- `Canvas/Settings/Security/UnlockVaultView.swift` — 解锁 UI（显示 hint）
- `Canvas/Settings/Security/PasswordField.swift` — 可复用密码输入组件 + PasswordStrength enum
- `Canvas/Settings/PasswordSettingsView.swift` — 已解锁管理页（凭据列表、Security section、改密码）

---

## 1. Vault State Machine

三态有限状态机，持久化判据为 DB 中是否存在 `salt` 记录。

```mermaid
stateDiagram-v2
    [*] --> notSetup : DB 无 salt
    [*] --> locked : DB 有 salt

    notSetup --> unlocked : setupMasterPassword()
    locked --> unlocked : unlock() / unlockWithBiometrics()
    unlocked --> locked : lock() / auto-lock timer
```

| State | 含义 | UI |
|-------|------|-----|
| `notSetup` | 从未设置过主密码 | SetupMasterPasswordView |
| `locked` | 已设置，需输入密码 | UnlockVaultView |
| `unlocked` | 已解锁，可操作凭据 | UnlockedVaultView |

---

## 2. Setup Master Password

首次设置主密码。创建 salt、派生加密 key、存储验证令牌、可选填写密码提示。

**注意：** 生物识别（Touch ID）不在 Setup 阶段设置，需解锁后在 Security section 中独立管理。

```mermaid
flowchart TD
    A[用户输入密码 + 确认密码 + 可选 hint] --> B{password >= 5 且<br/>两次一致?}
    B -- No --> A
    B -- Yes --> C[按钮可点击]
    C --> D[isProcessing = true]
    D --> E[生成随机 salt 32B]
    E --> F["🔄 后台线程<br/>PBKDF2 派生 key (100k 轮)"]
    F --> G[存 salt → DB]
    G --> H[加密 verification token → DB]
    H --> I{hint 非空?}
    I -- Yes --> J[存 hint → DB vault_meta]
    I -- No --> K[跳过]
    J --> L[state = unlocked]
    K --> L
    L --> M[启动 auto-lock timer]
    M --> N[isProcessing = false]

    F -. 失败 .-> ERR[抛出 keyDerivationFailed]
    ERR --> N
```

**关键设计决策：**
- PBKDF2 通过 `Task.detached` 在后台线程执行，避免阻塞 UI
- Touch ID 不再在 setup 阶段设置，改为解锁后 Security section 独立管理
- 密码提示（hint）可选填写，存储在 DB `vault_meta` 表

---

## 3. Unlock with Password

密码解锁流程。从 DB 读取 salt，派生 key，验证是否能正确解密 verification token。

如果设置了 password hint，解锁页面会显示提示帮助用户回忆密码。

```mermaid
flowchart TD
    A[用户输入密码] --> B{密码非空?}
    B -- No --> A
    B -- Yes --> C[isProcessing = true]
    C --> D[从 DB 读取 salt]
    D --> E{salt 存在?}
    E -- No --> F[抛出 notSetup]
    E -- Yes --> G["🔄 后台 PBKDF2 派生 key"]
    G --> H[从 DB 读取 verification_hash]
    H --> I[用派生 key 解密 token]
    I --> J{解密结果 ==<br/>vault-verification-token?}
    J -- No --> K[derivedKey = nil]
    K --> L[显示 Incorrect password]
    J -- Yes --> M[state = unlocked]
    M --> N[加载并解密所有凭据]
    N --> O[启动 auto-lock timer]
    O --> P[isProcessing = false]
    L --> P

    style A fill:#e8f4fd
    note right of A: 页面显示 hint 提示（如有）
```

---

## 4. Unlock with Biometrics (Touch ID)

Touch ID 解锁是密码解锁的快捷路径：从 Keychain 取出密码 → 走常规密码解锁。

```mermaid
flowchart TD
    A[UnlockVaultView onAppear] --> B{isTouchIDEnabled<br/>且硬件可用?}
    B -- No --> C[仅显示密码输入框]
    B -- Yes --> D[显示 Touch ID 按钮<br/>+ 自动触发解锁]
    D --> E[Keychain 读取密码<br/>触发系统 Touch ID 弹窗]
    E --> F{用户验证成功?}
    F -- Yes --> G[取得明文密码]
    G --> H["调用 unlock(password:) 流程"]
    F -- No --> I[静默失败<br/>用户可手动输入密码]
```

**设计决策：** 自动解锁失败时不弹错误提示，避免干扰用户手动输入。

---

## 5. Change Master Password

修改主密码需要：验证旧密码 → 解密所有凭据 → 生成新 salt/key → 重新加密所有凭据。同时可修改密码提示。

```mermaid
flowchart TD
    A[输入: 旧密码 + 新密码 + 确认 + hint] --> B{旧密码非空<br/>新密码 >= 5<br/>两次一致?}
    B -- No --> A
    B -- Yes --> C[isProcessing = true]
    C --> D["🔄 用旧密码 + 旧 salt 派生 oldKey"]
    D --> E[用 oldKey 解密 verification token]
    E --> F{验证通过?}
    F -- No --> G[显示 wrongPassword]
    F -- Yes --> H[用 oldKey 解密所有凭据到内存]
    H --> I[生成新 salt 32B]
    I --> J["🔄 用新密码 + 新 salt 派生 newKey"]
    J --> K[存新 salt → DB]
    K --> L[用 newKey 加密新 verification token → DB]
    L --> M[用 newKey 逐条重新加密凭据 → DB]
    M --> M2[更新 hint → DB]
    M2 --> N{isTouchIDEnabled?}
    N -- Yes --> O[更新 Keychain 中的密码]
    N -- No --> P[跳过]
    O --> Q["derivedKey = newKey"]
    P --> Q
    Q --> R[isProcessing = false, dismiss]
    G --> R
```

**注意：** 改密码涉及两次 PBKDF2 派生（旧 key + 新 key），耗时较长，loading 状态尤为重要。

---

## 6. Security Section (Unlocked View)

解锁后的 footer 区域改为结构化的 Security section：

```
┌─ Security ────────────────────────────┐
│  Auto-lock:      [Never ▾]            │
│  Touch ID:       [Toggle] (需硬件可用)  │
│  Password hint:  xxxxxxx  [Edit]      │
│  [Change Password...]                 │
└───────────────────────────────────────┘
```

- **Auto-lock**: 选择自动锁定时间（Never / 1 / 5 / 10 / 15 min）
- **Touch ID**: 开启需密码确认（TouchIDPasswordSheet），关闭直接删除 Keychain
- **Password hint**: 已解锁状态可直接编辑（内联 TextField），无需密码确认
- **Change Password**: 打开修改密码 Sheet，可同时修改 hint

---

## 7. Enable Touch ID (Post-Setup)

已解锁状态下开启 Touch ID，必须验证主密码后才能将密码存入 Keychain。

```mermaid
flowchart TD
    A[用户打开 Touch ID 开关] --> B[弹出密码确认 Sheet]
    B --> C[用户输入主密码]
    C --> D["🔄 PBKDF2 派生 key"]
    D --> E[解密 verification token 验证密码]
    E --> F{密码正确?}
    F -- No --> G[显示错误, 保留 Sheet]
    G --> C
    F -- Yes --> H[保存密码到 Keychain]
    H --> I[isTouchIDEnabled = true]
    I --> J[dismiss Sheet]

    B -. 用户取消 .-> K[toggle 回退为 false]
```

**安全考量：** 不直接绑定 toggle 到 `vault.isTouchIDEnabled`，而是用 `@State` 中间变量 + `onChange` + Sheet 确认流程。

---

## 8. Disable Touch ID

关闭 Touch ID 不需要密码验证，直接删除 Keychain 条目。

```mermaid
flowchart TD
    A[用户关闭 Touch ID 开关] --> B[删除 Keychain 中的密码]
    B --> C[isTouchIDEnabled = false]
```

---

## 9. Auto-Lock Timer

自动锁定计时器，在多个操作后重置。`timeout = 0` 表示 Never。

```mermaid
flowchart TD
    A[触发 setupAutoLockTimer] --> B[取消旧 timer]
    B --> C{"state == unlocked<br/>且 timeout > 0?"}
    C -- "No (locked 或 Never)" --> D[不设 timer]
    C -- Yes --> E["创建 timer = timeout × 60s"]
    E --> F[等待超时...]
    F --> G["lock(): 清 key + 清凭据<br/>state → locked"]

    H[以下操作重置 timer] --> A
    H1[addCredential] --> H
    H2[updateCredential] --> H
    H3[deleteCredential] --> H
    H4[unlock 成功] --> H
    H5[setupMasterPassword] --> H
    H6[用户修改 timeout 值] --> H
```

**Timeout 选项：** `Never (0)` / `1 min` / `5 min` / `10 min` / `15 min`，默认 5 min。

---

## 10. Password Strength Evaluation

密码强度实时评估，用于 Setup 和 Change Password 页面的强度指示条。

```mermaid
flowchart LR
    P[密码输入] --> S1{"长度 >= 5?"}
    S1 -- "+1" --> S2{"长度 >= 10?"}
    S1 -- "+0" --> S2
    S2 -- "+1" --> S3{"含大写<br/>且含小写?"}
    S2 -- "+0" --> S3
    S3 -- "+1" --> S4{"含数字?"}
    S3 -- "+0" --> S4
    S4 -- "+1" --> S5{"含标点<br/>或符号?"}
    S4 -- "+0" --> S5
    S5 -- "+1" --> SUM["总分 0~5"]
    S5 -- "+0" --> SUM

    SUM --> R1["0-1 → Weak 🔴"]
    SUM --> R2["2 → Fair 🟠"]
    SUM --> R3["3 → Good 🟡"]
    SUM --> R4["4-5 → Strong 🟢"]
```

| 得分 | 等级 | 颜色 | 指示条亮起段数 |
|------|------|------|--------------|
| 0-1 | Weak | 红色 | 1/4 |
| 2 | Fair | 橙色 | 2/4 |
| 3 | Good | 黄色 | 3/4 |
| 4-5 | Strong | 绿色 | 4/4 |

---

## 11. Reusable PasswordField Component

`PasswordField.swift` 提供可复用的密码输入区域，各场景使用方式：

| 场景 | showStrength | showConfirm | placeholder |
|------|-------------|-------------|-------------|
| Setup | true | true | "Master password" |
| Unlock | false | false | "Master password" |
| Change (current) | false | false | "Current password" |
| Change (new) | true | true | "New password" |
| Touch ID confirm | false | false | "Master password" |

---

## Crypto Summary

| 组件 | 算法/参数 |
|------|----------|
| Key Derivation | PBKDF2-HMAC-SHA256, 100,000 iterations, 32B salt |
| Encryption | AES-256-GCM (CryptoKit) |
| Key Storage | macOS Keychain (Touch ID protected) |
| Verification | 加密固定字符串 `"vault-verification-token"` 作为密码校验 |
