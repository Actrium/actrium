import SwiftUI

/// Chrome-style floating card dialog for saving/updating passwords
struct SavePasswordBannerView: View {
    @ObservedObject private var autofillManager = AutofillManager.shared

    var body: some View {
        if let action = autofillManager.pendingSaveAction {
            VStack(alignment: .leading, spacing: 10) {
                // Header row: title + close button
                HStack {
                    Image(systemName: "key.fill")
                        .font(.system(size: 14))
                        .foregroundColor(.secondary)
                    Text(action.isUpdate ? "Update password?" : "Save password?")
                        .font(.system(size: 13, weight: .semibold))
                    Spacer()
                    Button {
                        autofillManager.dismissBanner()
                    } label: {
                        Image(systemName: "xmark")
                            .font(.system(size: 10, weight: .semibold))
                            .foregroundColor(.secondary)
                    }
                    .buttonStyle(.plain)
                }

                // Domain
                Text("for \(action.domain)")
                    .font(.system(size: 12))
                    .foregroundColor(.secondary)

                // Credential preview
                VStack(alignment: .leading, spacing: 4) {
                    HStack(spacing: 6) {
                        Image(systemName: "person.fill")
                            .font(.system(size: 11))
                            .foregroundColor(.secondary)
                            .frame(width: 14)
                        Text(action.username.isEmpty ? "(no username)" : action.username)
                            .font(.system(size: 12))
                            .lineLimit(1)
                    }
                    HStack(spacing: 6) {
                        Image(systemName: "lock.fill")
                            .font(.system(size: 11))
                            .foregroundColor(.secondary)
                            .frame(width: 14)
                        Text(String(repeating: "•", count: 8))
                            .font(.system(size: 12))
                    }
                }

                // Action buttons
                HStack {
                    Spacer()
                    Button("Never") {
                        autofillManager.neverSaveForDomain()
                    }
                    .buttonStyle(.bordered)
                    .controlSize(.small)

                    Button(action.isUpdate ? "Update" : "Save") {
                        if action.isUpdate {
                            autofillManager.updateCredentials()
                        } else {
                            autofillManager.saveCredentials()
                        }
                    }
                    .buttonStyle(.borderedProminent)
                    .controlSize(.small)
                }
            }
            .padding(16)
            .frame(width: 320)
            .background(.regularMaterial)
            .clipShape(RoundedRectangle(cornerRadius: 12))
            .shadow(color: .black.opacity(0.2), radius: 12, x: 0, y: 4)
            .transition(.move(edge: .top).combined(with: .opacity))
        }
    }
}
