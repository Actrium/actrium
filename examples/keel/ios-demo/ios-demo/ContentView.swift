//
//  ContentView.swift
//  ios-demo
//
//  Main UI for testing keel sandbox across different engine configurations
//

import SwiftUI
import UIKit

// MARK: - Configuration Display

struct ConfigurationBanner: View {
    var body: some View {
        HStack(spacing: 12) {
            VStack(alignment: .leading, spacing: 2) {
                Text("Mode")
                    .font(.caption2)
                    .foregroundColor(.secondary)
                Text(KeelConfiguration.modeName())
                    .font(.caption)
                    .fontWeight(.semibold)
                    .foregroundColor(.primary)
            }

            Divider()
                .frame(height: 24)

            VStack(alignment: .leading, spacing: 2) {
                Text("Sandbox")
                    .font(.caption2)
                    .foregroundColor(.secondary)
                Text(KeelConfiguration.sandboxEngineName())
                    .font(.caption)
                    .fontWeight(.semibold)
                    .foregroundColor(.primary)
            }

            Spacer()

            Circle()
                .fill(Color.green)
                .frame(width: 8, height: 8)
            Text("Running")
                .font(.caption2)
                .foregroundColor(.secondary)
        }
        .padding(.horizontal, 16)
        .padding(.vertical, 10)
        .background(Color(.systemGray6))
    }
}

// MARK: - React Native Container

struct ReactNativeContainerView: UIViewRepresentable {
    func makeUIView(context: Context) -> UIView {
        let container = UIView()
        let env = ProcessInfo.processInfo.environment
        var initialProperties: [String: Any] = [:]

        if env["KEEL_IOS_E2E"] == "1" {
            initialProperties["keelE2E"] = true
        }

        if let sandboxTarget = env["KEEL_SANDBOX_TARGET"], !sandboxTarget.isEmpty {
            initialProperties["keelSandbox"] = sandboxTarget
        }

        let factory = ReactNativeFactory.sharedInstance()
        let rootView = factory.createRootView(
            withModuleName: "KeelDemo",
            initialProperties: initialProperties.isEmpty ? nil : initialProperties
        )
        rootView.translatesAutoresizingMaskIntoConstraints = false
        container.addSubview(rootView)

        NSLayoutConstraint.activate([
            rootView.topAnchor.constraint(equalTo: container.topAnchor),
            rootView.bottomAnchor.constraint(equalTo: container.bottomAnchor),
            rootView.leadingAnchor.constraint(equalTo: container.leadingAnchor),
            rootView.trailingAnchor.constraint(equalTo: container.trailingAnchor)
        ])

        return container
    }

    func updateUIView(_ uiView: UIView, context: Context) {}
}

// MARK: - Main Content View

struct ContentView: View {
    var body: some View {
        VStack(spacing: 0) {
            // Configuration header
            ConfigurationBanner()

            // React Native view (includes RN PerformanceDashboard at bottom)
            ReactNativeContainerView()
                .frame(maxWidth: .infinity, maxHeight: .infinity)
        }
        .ignoresSafeArea(.container, edges: .bottom)
    }
}

#Preview {
    ContentView()
}
