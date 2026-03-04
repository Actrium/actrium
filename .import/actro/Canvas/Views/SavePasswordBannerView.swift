import SwiftUI

/// Chrome-style banner for saving/updating passwords
struct SavePasswordBannerView: View {
    @ObservedObject private var autofillManager = AutofillManager.shared

    var body: some View {
        if let action = autofillManager.pendingSaveAction {
            HStack(spacing: 12) {
                Image(systemName: "key.fill")
                    .font(.system(size: 16))
                    .foregroundColor(.secondary)

                Text(action.isUpdate
                     ? "Update password for \(action.domain)?"
                     : "Save password for \(action.domain)?")
                    .font(.system(size: 13))
                    .lineLimit(1)

                Spacer()

                Button(action.isUpdate ? "Update" : "Save") {
                    if action.isUpdate {
                        autofillManager.updateCredentials()
                    } else {
                        autofillManager.saveCredentials()
                    }
                }
                .buttonStyle(.borderedProminent)
                .controlSize(.small)

                Button("Never") {
                    autofillManager.neverSaveForDomain()
                }
                .buttonStyle(.bordered)
                .controlSize(.small)

                Button {
                    autofillManager.dismissBanner()
                } label: {
                    Image(systemName: "xmark")
                        .font(.system(size: 11, weight: .semibold))
                        .foregroundColor(.secondary)
                }
                .buttonStyle(.plain)
            }
            .padding(.horizontal, 16)
            .padding(.vertical, 10)
            .background(.ultraThinMaterial)
            .overlay(alignment: .bottom) {
                Divider()
            }
            .transition(.move(edge: .top).combined(with: .opacity))
        }
    }
}
