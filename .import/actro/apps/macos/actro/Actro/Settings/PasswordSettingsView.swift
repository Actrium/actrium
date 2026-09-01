import SwiftUI

struct PasswordSettingsView: View {
    @ObservedObject private var vault = VaultManager.shared

    var body: some View {
        switch vault.state {
        case .notSetup:
            SetupMasterPasswordView(vault: vault)
        case .locked:
            UnlockVaultView(vault: vault)
        case .unlocked:
            UnlockedVaultView(vault: vault)
        }
    }
}

// MARK: - Unlocked Vault View

private struct UnlockedVaultView: View {
    @ObservedObject var vault: VaultManager

    @State private var searchText = ""
    @State private var showingAddSheet = false
    @State private var editingItem: VaultItem?
    @State private var showingChangePassword = false
    @State private var touchIDEnabled: Bool
    @State private var showingTouchIDPasswordSheet = false
    @State private var editingHint = false
    @State private var hintText: String
    @State private var showingResetConfirmation = false
    @State private var selectedCategory: VaultItemCategory?

    init(vault: VaultManager) {
        self.vault = vault
        _touchIDEnabled = State(initialValue: vault.isTouchIDEnabled)
        _hintText = State(initialValue: vault.passwordHint ?? "")
    }

    private var filteredItems: [VaultItem] {
        var result = vault.items
        if let category = selectedCategory {
            result = result.filter { $0.category == category }
        }
        if !searchText.isEmpty {
            result = result.filter {
                $0.title.localizedCaseInsensitiveContains(searchText)
            }
        }
        return result
    }

    private let timeoutOptions = [0, 1, 5, 10, 15]

    var body: some View {
        VStack(spacing: 0) {
            // Toolbar
            HStack {
                TextField("Search vault...", text: $searchText)
                    .textFieldStyle(.roundedBorder)
                    .frame(maxWidth: 200)

                // Category filter
                Picker("Category", selection: $selectedCategory) {
                    Text("All").tag(VaultItemCategory?.none)
                    ForEach(VaultItemCategory.allCases, id: \.self) { cat in
                        Label(cat.label, systemImage: cat.icon).tag(VaultItemCategory?.some(cat))
                    }
                }
                .frame(width: 120)

                Spacer()

                Button {
                    showingAddSheet = true
                } label: {
                    Label("Add", systemImage: "plus")
                }

                Button {
                    vault.lock()
                } label: {
                    Label("Lock", systemImage: "lock")
                }
            }
            .padding(.horizontal, 16)
            .padding(.vertical, 8)

            Divider()

            if filteredItems.isEmpty {
                VStack(spacing: 8) {
                    Spacer()
                    Image(systemName: "key")
                        .font(.system(size: 28))
                        .foregroundColor(.secondary)
                    Text(searchText.isEmpty && selectedCategory == nil ? "No saved items" : "No matches found")
                        .foregroundColor(.secondary)
                    Spacer()
                }
                .frame(maxWidth: .infinity, maxHeight: .infinity)
            } else {
                List {
                    ForEach(filteredItems) { item in
                        VaultItemRow(
                            item: item,
                            onEdit: { editingItem = item },
                            onDelete: { deleteItem(item) }
                        )
                    }
                }
                .listStyle(.inset)
            }

            Divider()

            // Security section
            securitySection
        }
        .sheet(isPresented: $showingAddSheet) {
            VaultItemFormSheet(vault: vault)
        }
        .sheet(item: $editingItem) { item in
            VaultItemFormSheet(vault: vault, editing: item)
        }
        .sheet(isPresented: $showingChangePassword) {
            ChangePasswordSheet(vault: vault)
        }
        .sheet(isPresented: $showingTouchIDPasswordSheet) {
            TouchIDPasswordSheet(vault: vault) { success in
                if !success {
                    touchIDEnabled = false
                }
            }
        }
    }

    private var securitySection: some View {
        VStack(alignment: .leading, spacing: 8) {
            Text("Security")
                .font(.caption)
                .foregroundColor(.secondary)
                .textCase(.uppercase)

            VStack(spacing: 6) {
                // Auto-lock
                HStack {
                    Text("Auto-lock")
                        .font(.subheadline)
                    Spacer()
                    Picker("", selection: $vault.autoLockTimeout) {
                        ForEach(timeoutOptions, id: \.self) { minutes in
                            if minutes == 0 {
                                Text("Never").tag(0)
                            } else {
                                Text("\(minutes) min").tag(minutes)
                            }
                        }
                    }
                    .frame(width: 100)
                }

                Divider()

                // Touch ID
                if vault.isBiometricsAvailable {
                    HStack {
                        Text("Touch ID")
                            .font(.subheadline)
                        Spacer()
                        Toggle("", isOn: $touchIDEnabled)
                            .labelsHidden()
                            .onChange(of: touchIDEnabled) { newValue in
                                handleTouchIDToggle(newValue)
                            }
                    }

                    Divider()
                }

                // Password hint
                HStack {
                    Text("Password hint")
                        .font(.subheadline)
                    Spacer()
                    if editingHint {
                        TextField("Hint", text: $hintText)
                            .textFieldStyle(.roundedBorder)
                            .frame(width: 150)
                            .onSubmit { saveHint() }
                        Button("Save") { saveHint() }
                            .font(.subheadline)
                            .buttonStyle(.borderless)
                        Button("Cancel") {
                            hintText = vault.passwordHint ?? ""
                            editingHint = false
                        }
                        .font(.subheadline)
                        .buttonStyle(.borderless)
                    } else {
                        Text(vault.passwordHint?.isEmpty == false ? "Set" : "Not set")
                            .font(.subheadline)
                            .foregroundColor(.secondary)
                        Button("Edit") {
                            hintText = vault.passwordHint ?? ""
                            editingHint = true
                        }
                        .font(.subheadline)
                        .buttonStyle(.borderless)
                    }
                }

                Divider()

                // Change password
                HStack {
                    Button("Change Password...") {
                        showingChangePassword = true
                    }
                    .font(.subheadline)
                    .buttonStyle(.link)
                    Spacer()
                }

                Divider()

                // Reset vault
                HStack {
                    Button("Reset Vault...") {
                        showingResetConfirmation = true
                    }
                    .font(.subheadline)
                    .foregroundColor(.red)
                    .buttonStyle(.borderless)
                    Text("All saved items will be permanently deleted.")
                        .font(.caption2)
                        .foregroundColor(.secondary)
                    Spacer()
                }
            }
        }
        .padding(.horizontal, 16)
        .padding(.vertical, 10)
        .alert("Reset Vault?", isPresented: $showingResetConfirmation) {
            Button("Cancel", role: .cancel) {}
            Button("Reset", role: .destructive) {
                vault.resetVault()
            }
        } message: {
            Text("This will permanently delete your master password and all saved items. This action cannot be undone.")
        }
    }

    private func deleteItem(_ item: VaultItem) {
        try? vault.deleteItem(id: item.id)
    }

    private func handleTouchIDToggle(_ enabled: Bool) {
        if enabled {
            showingTouchIDPasswordSheet = true
        } else {
            vault.disableTouchID()
        }
    }

    private func saveHint() {
        vault.setPasswordHint(hintText)
        editingHint = false
    }
}

// MARK: - Touch ID Password Confirmation Sheet

private struct TouchIDPasswordSheet: View {
    @ObservedObject var vault: VaultManager
    @Environment(\.dismiss) private var dismiss

    let onComplete: (Bool) -> Void

    @State private var password = ""
    @State private var errorMessage: String?

    var body: some View {
        VStack(spacing: 16) {
            Text("Enable Touch ID")
                .font(.headline)

            Text("Enter your master password to enable Touch ID unlock.")
                .font(.subheadline)
                .foregroundColor(.secondary)
                .multilineTextAlignment(.center)

            PasswordField(
                password: $password,
                placeholder: "Master password",
                onSubmit: { confirm() }
            )
            .frame(width: 280)

            if let error = errorMessage {
                Text(error)
                    .font(.caption)
                    .foregroundColor(.red)
            }

            HStack {
                Button("Cancel") {
                    onComplete(false)
                    dismiss()
                }
                .keyboardShortcut(.cancelAction)

                Button {
                    confirm()
                } label: {
                    if vault.isProcessing {
                        HStack(spacing: 6) {
                            ProgressView()
                                .controlSize(.small)
                            Text("Verifying...")
                        }
                    } else {
                        Text("Enable")
                    }
                }
                .buttonStyle(.borderedProminent)
                .keyboardShortcut(.defaultAction)
                .disabled(password.isEmpty || vault.isProcessing)
                .opacity(!password.isEmpty && !vault.isProcessing ? 1 : 0.5)
            }
        }
        .padding(20)
        .frame(width: 380)
    }

    private func confirm() {
        errorMessage = nil
        Task {
            do {
                try await vault.enableTouchID(password: password)
                onComplete(true)
                dismiss()
            } catch {
                errorMessage = error.localizedDescription
            }
        }
    }
}

// MARK: - Vault Item Row

private struct VaultItemRow: View {
    let item: VaultItem
    let onEdit: () -> Void
    let onDelete: () -> Void

    @State private var showSensitive = false

    var body: some View {
        HStack(spacing: 8) {
            Image(systemName: item.category.icon)
                .foregroundColor(.accentColor)
                .frame(width: 20)

            VStack(alignment: .leading, spacing: 2) {
                Text(item.title)
                    .font(.system(.body, design: .default, weight: .medium))
                if let subtitle = item.subtitle {
                    Text(subtitle)
                        .font(.caption)
                        .foregroundColor(.secondary)
                }
            }

            Spacer()

            // Show/hide sensitive values
            if item.primarySensitiveValue != nil {
                Text(showSensitive ? (item.primarySensitiveValue ?? "") : String(repeating: "\u{2022}", count: 8))
                    .font(.system(.caption, design: .monospaced))
                    .foregroundColor(.secondary)
                    .lineLimit(1)

                Button {
                    showSensitive.toggle()
                } label: {
                    Image(systemName: showSensitive ? "eye.slash" : "eye")
                        .font(.caption)
                }
                .buttonStyle(.borderless)

                Button {
                    if let value = item.primarySensitiveValue {
                        NSPasteboard.general.clearContents()
                        NSPasteboard.general.setString(value, forType: .string)
                    }
                } label: {
                    Image(systemName: "doc.on.doc")
                        .font(.caption)
                }
                .buttonStyle(.borderless)
                .help("Copy")
            }

            Button { onEdit() } label: {
                Image(systemName: "pencil")
                    .font(.caption)
            }
            .buttonStyle(.borderless)

            Button { onDelete() } label: {
                Image(systemName: "trash")
                    .font(.caption)
                    .foregroundColor(.red)
            }
            .buttonStyle(.borderless)
        }
        .padding(.vertical, 2)
    }
}

// MARK: - Vault Item Form Sheet

private struct VaultItemFormSheet: View {
    @ObservedObject var vault: VaultManager
    @Environment(\.dismiss) private var dismiss

    let editing: VaultItem?

    @State private var title: String
    @State private var category: VaultItemCategory
    @State private var fields: [VaultField]
    @State private var notes: String
    @State private var errorMessage: String?

    init(vault: VaultManager, editing: VaultItem? = nil) {
        self.vault = vault
        self.editing = editing
        _title = State(initialValue: editing?.title ?? "")
        _category = State(initialValue: editing?.category ?? .login)
        _fields = State(initialValue: editing?.fields ?? VaultItemCategory.login.template)
        _notes = State(initialValue: editing?.notes ?? "")
    }

    private var isValid: Bool {
        !title.isEmpty
    }

    var body: some View {
        VStack(spacing: 16) {
            Text(editing == nil ? "Add Item" : "Edit Item")
                .font(.headline)

            Form {
                TextField("Title:", text: $title)

                if editing == nil {
                    Picker("Category:", selection: $category) {
                        ForEach(VaultItemCategory.allCases, id: \.self) { cat in
                            Label(cat.label, systemImage: cat.icon).tag(cat)
                        }
                    }
                    .onChange(of: category) { newCategory in
                        fields = newCategory.template
                    }
                } else {
                    HStack {
                        Text("Category:")
                        Spacer()
                        Label(category.label, systemImage: category.icon)
                            .foregroundColor(.secondary)
                    }
                }

                ForEach($fields) { $field in
                    if field.isSensitive {
                        SensitiveFieldEditor(field: $field)
                    } else {
                        TextField("\(field.label):", text: $field.value)
                    }
                }

                TextField("Notes:", text: $notes, axis: .vertical)
                    .lineLimit(3...6)
            }
            .frame(width: 340)

            if let error = errorMessage {
                Text(error)
                    .font(.caption)
                    .foregroundColor(.red)
            }

            HStack {
                Button("Cancel") { dismiss() }
                    .keyboardShortcut(.cancelAction)

                Button(editing == nil ? "Add" : "Save") { save() }
                    .buttonStyle(.borderedProminent)
                    .keyboardShortcut(.defaultAction)
                    .disabled(!isValid)
            }
        }
        .padding(20)
        .frame(width: 400)
    }

    private func save() {
        errorMessage = nil
        do {
            if let existing = editing {
                var updated = existing
                updated.title = title
                updated.fields = fields
                updated.notes = notes.isEmpty ? nil : notes
                try vault.updateItem(updated)
            } else {
                let item = VaultItem(
                    id: UUID().uuidString,
                    title: title,
                    category: category,
                    fields: fields,
                    notes: notes.isEmpty ? nil : notes,
                    createdAt: Date(),
                    updatedAt: Date()
                )
                try vault.addItem(item)
            }
            dismiss()
        } catch {
            errorMessage = error.localizedDescription
        }
    }
}

// MARK: - Sensitive Field Editor

private struct SensitiveFieldEditor: View {
    @Binding var field: VaultField
    @State private var showValue = false

    var body: some View {
        HStack {
            if showValue {
                if field.label == "Private Key" || field.label == "Content" {
                    VStack(alignment: .leading) {
                        Text("\(field.label):")
                        TextEditor(text: $field.value)
                            .font(.system(.body, design: .monospaced))
                            .frame(height: 80)
                    }
                } else {
                    TextField("\(field.label):", text: $field.value)
                }
            } else {
                SecureField("\(field.label):", text: $field.value)
            }
            Button {
                showValue.toggle()
            } label: {
                Image(systemName: showValue ? "eye.slash" : "eye")
                    .font(.caption)
            }
            .buttonStyle(.borderless)
        }
    }
}

// MARK: - Change Password Sheet

private struct ChangePasswordSheet: View {
    @ObservedObject var vault: VaultManager
    @Environment(\.dismiss) private var dismiss

    @State private var currentPassword = ""
    @State private var newPassword = ""
    @State private var confirmPassword = ""
    @State private var hint = ""
    @State private var errorMessage: String?

    private var isValid: Bool {
        !currentPassword.isEmpty && newPassword.count >= 5 && newPassword == confirmPassword
    }

    var body: some View {
        VStack(spacing: 16) {
            Text("Change Master Password")
                .font(.headline)

            VStack(alignment: .leading, spacing: 12) {
                PasswordField(
                    password: $currentPassword,
                    placeholder: "Current password",
                    hintTooltip: vault.passwordHint
                )

                PasswordField(
                    password: $newPassword,
                    placeholder: "New password",
                    showStrength: true,
                    confirmPassword: $confirmPassword,
                    showConfirm: true,
                    confirmPlaceholder: "Confirm new password"
                )

                TextField("Password hint (optional)", text: $hint)
                    .textFieldStyle(.roundedBorder)
            }
            .frame(width: 300)

            if let error = errorMessage {
                Text(error)
                    .font(.caption)
                    .foregroundColor(.red)
            }

            HStack {
                Button("Cancel") { dismiss() }
                    .keyboardShortcut(.cancelAction)

                Button {
                    changePassword()
                } label: {
                    if vault.isProcessing {
                        HStack(spacing: 6) {
                            ProgressView()
                                .controlSize(.small)
                            Text("Changing...")
                        }
                    } else {
                        Text("Change Password")
                    }
                }
                .buttonStyle(.borderedProminent)
                .keyboardShortcut(.defaultAction)
                .disabled(!isValid || vault.isProcessing)
                .opacity(isValid && !vault.isProcessing ? 1 : 0.5)
            }
        }
        .padding(20)
        .frame(width: 400)
    }

    private func changePassword() {
        errorMessage = nil
        Task {
            do {
                try await vault.changeMasterPassword(
                    current: currentPassword,
                    new: newPassword,
                    hint: hint.isEmpty ? nil : hint
                )
                dismiss()
            } catch {
                errorMessage = error.localizedDescription
            }
        }
    }
}
