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
    @State private var editingCredential: Credential?
    @State private var showingChangePassword = false
    @State private var touchIDEnabled = false
    @State private var showingTouchIDPasswordSheet = false
    @State private var errorMessage: String?

    private var filteredCredentials: [Credential] {
        if searchText.isEmpty { return vault.credentials }
        return vault.credentials.filter {
            $0.domain.localizedCaseInsensitiveContains(searchText) ||
            $0.username.localizedCaseInsensitiveContains(searchText)
        }
    }

    private let timeoutOptions = [0, 1, 5, 10, 15]

    var body: some View {
        VStack(spacing: 0) {
            // Toolbar
            HStack {
                TextField("Search credentials...", text: $searchText)
                    .textFieldStyle(.roundedBorder)
                    .frame(maxWidth: 200)

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

            if filteredCredentials.isEmpty {
                VStack(spacing: 8) {
                    Spacer()
                    Image(systemName: "key")
                        .font(.system(size: 28))
                        .foregroundColor(.secondary)
                    Text(searchText.isEmpty ? "No saved credentials" : "No matches found")
                        .foregroundColor(.secondary)
                    Spacer()
                }
                .frame(maxWidth: .infinity, maxHeight: .infinity)
            } else {
                List {
                    ForEach(filteredCredentials) { credential in
                        CredentialRow(
                            credential: credential,
                            onEdit: { editingCredential = credential },
                            onDelete: { deleteCredential(credential) }
                        )
                    }
                }
                .listStyle(.inset)
            }

            Divider()

            // Settings footer
            HStack(spacing: 16) {
                Picker("Auto-lock:", selection: $vault.autoLockTimeout) {
                    ForEach(timeoutOptions, id: \.self) { minutes in
                        if minutes == 0 {
                            Text("Never").tag(0)
                        } else {
                            Text("\(minutes) min").tag(minutes)
                        }
                    }
                }
                .frame(width: 170)

                if vault.isBiometricsAvailable {
                    Toggle("Touch ID", isOn: $touchIDEnabled)
                        .onChange(of: touchIDEnabled) { newValue in
                            handleTouchIDToggle(newValue)
                        }
                }

                Spacer()

                Button("Change Password...") {
                    showingChangePassword = true
                }
                .buttonStyle(.link)
            }
            .padding(.horizontal, 16)
            .padding(.vertical, 8)
        }
        .onAppear {
            touchIDEnabled = vault.isTouchIDEnabled
        }
        .sheet(isPresented: $showingAddSheet) {
            CredentialFormSheet(vault: vault)
        }
        .sheet(item: $editingCredential) { credential in
            CredentialFormSheet(vault: vault, editing: credential)
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

    private func deleteCredential(_ credential: Credential) {
        try? vault.deleteCredential(id: credential.id)
    }

    private func handleTouchIDToggle(_ enabled: Bool) {
        if enabled {
            // Enabling Touch ID requires password verification
            showingTouchIDPasswordSheet = true
        } else {
            // Disabling Touch ID can be done directly
            vault.disableTouchID()
        }
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

            SecureField("Master password", text: $password)
                .textFieldStyle(.roundedBorder)
                .frame(width: 280)
                .onSubmit { confirm() }

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

// MARK: - Credential Row

private struct CredentialRow: View {
    let credential: Credential
    let onEdit: () -> Void
    let onDelete: () -> Void

    @State private var showPassword = false

    var body: some View {
        HStack {
            VStack(alignment: .leading, spacing: 2) {
                Text(credential.domain)
                    .font(.system(.body, design: .default, weight: .medium))
                Text(credential.username)
                    .font(.caption)
                    .foregroundColor(.secondary)
            }

            Spacer()

            Text(showPassword ? credential.password : String(repeating: "\u{2022}", count: 8))
                .font(.system(.caption, design: .monospaced))
                .foregroundColor(.secondary)

            Button {
                showPassword.toggle()
            } label: {
                Image(systemName: showPassword ? "eye.slash" : "eye")
                    .font(.caption)
            }
            .buttonStyle(.borderless)

            Button {
                NSPasteboard.general.clearContents()
                NSPasteboard.general.setString(credential.password, forType: .string)
            } label: {
                Image(systemName: "doc.on.doc")
                    .font(.caption)
            }
            .buttonStyle(.borderless)
            .help("Copy password")

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

// MARK: - Credential Form Sheet

private struct CredentialFormSheet: View {
    @ObservedObject var vault: VaultManager
    @Environment(\.dismiss) private var dismiss

    let editing: Credential?

    @State private var domain: String
    @State private var username: String
    @State private var password: String
    @State private var notes: String
    @State private var errorMessage: String?

    init(vault: VaultManager, editing: Credential? = nil) {
        self.vault = vault
        self.editing = editing
        _domain = State(initialValue: editing?.domain ?? "")
        _username = State(initialValue: editing?.username ?? "")
        _password = State(initialValue: editing?.password ?? "")
        _notes = State(initialValue: editing?.notes ?? "")
    }

    private var isValid: Bool {
        !domain.isEmpty && !password.isEmpty
    }

    var body: some View {
        VStack(spacing: 16) {
            Text(editing == nil ? "Add Credential" : "Edit Credential")
                .font(.headline)

            Form {
                TextField("Domain:", text: $domain)
                TextField("Username:", text: $username)
                SecureField("Password:", text: $password)
                TextField("Notes:", text: $notes)
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
                updated.domain = domain
                updated.username = username
                updated.password = password
                updated.notes = notes.isEmpty ? nil : notes
                try vault.updateCredential(updated)
            } else {
                try vault.addCredential(
                    domain: domain,
                    username: username,
                    password: password,
                    notes: notes.isEmpty ? nil : notes
                )
            }
            dismiss()
        } catch {
            errorMessage = error.localizedDescription
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
    @State private var errorMessage: String?

    private var isValid: Bool {
        !currentPassword.isEmpty && newPassword.count >= 8 && newPassword == confirmPassword
    }

    var body: some View {
        VStack(spacing: 16) {
            Text("Change Master Password")
                .font(.headline)

            Form {
                SecureField("Current password:", text: $currentPassword)
                SecureField("New password:", text: $newPassword)
                SecureField("Confirm new password:", text: $confirmPassword)
            }
            .frame(width: 340)

            if !confirmPassword.isEmpty && newPassword != confirmPassword {
                Text("Passwords do not match")
                    .font(.caption)
                    .foregroundColor(.red)
            }

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
                try await vault.changeMasterPassword(current: currentPassword, new: newPassword)
                dismiss()
            } catch {
                errorMessage = error.localizedDescription
            }
        }
    }
}
