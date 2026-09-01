// swift-tools-version: 6.0
import Foundation
import PackageDescription

// actr-cabi Swift binding — a parallel binding to actr-swift (UniFFI).
// Consumers use `import ActrCabi` (vs `import Actr` for the UniFFI line).
//
// Binary distribution model mirrors bindings/swift/Package.swift:
// - ACTR_CABI_BINARY_PATH override → point at a locally built xcframework
// - dist/ActrCabi.xcframework → CI-built
// - ActrCabi.xcframework → checked-in fallback
let env = ProcessInfo.processInfo.environment
let overrideBinaryPath = env["ACTR_CABI_BINARY_PATH"]
let distBinaryPath = "dist/ActrCabi.xcframework"
let localBinaryPath = "ActrCabi.xcframework"

let manifestDir = URL(fileURLWithPath: #filePath).deletingLastPathComponent().path
let distBinaryAbsolutePath = manifestDir + "/" + distBinaryPath
let localBinaryAbsolutePath = manifestDir + "/" + localBinaryPath

func binaryPathRelativeToPackageRoot(_ path: String) -> String? {
    if path.hasPrefix("/") {
        let prefix = manifestDir.hasSuffix("/") ? manifestDir : "\(manifestDir)/"
        guard path.hasPrefix(prefix) else { return nil }
        return String(path.dropFirst(prefix.count))
    }
    return path
}

let cabiBinaryTarget: Target
if let overrideBinaryPath {
    guard let relative = binaryPathRelativeToPackageRoot(overrideBinaryPath) else {
        fatalError("ACTR_CABI_BINARY_PATH must point inside the bindings/cabi/swift package root.")
    }
    cabiBinaryTarget = .binaryTarget(name: "ActrCabiLib", path: relative)
} else if FileManager.default.fileExists(atPath: distBinaryAbsolutePath) {
    cabiBinaryTarget = .binaryTarget(name: "ActrCabiLib", path: distBinaryPath)
} else if FileManager.default.fileExists(atPath: localBinaryAbsolutePath) {
    cabiBinaryTarget = .binaryTarget(name: "ActrCabiLib", path: localBinaryPath)
} else {
    fatalError("""
    Missing ActrCabi.xcframework. Build it first with:
      ACTR_CABI_WORKLOAD_CRATE=<workload crate> ./build-xcframework.sh
    Then run Swift commands with:
      ACTR_CABI_BINARY_PATH=dist/ActrCabi.xcframework swift build
    """)
}

let package = Package(
    name: "actr-cabi-swift",
    platforms: [
        .iOS(.v15),
        .macOS(.v12),
    ],
    products: [
        .library(name: "ActrCabi", targets: ["ActrCabi"]),
    ],
    targets: [
        cabiBinaryTarget,
        .target(
            name: "ActrCabi",
            dependencies: ["ActrCabiLib"],
            path: "Sources/ActrCabi",
            sources: ["ActrCabi.swift"]
            // No publicHeadersPath — ActrCabi is a pure Swift wrapper. The C
            // headers + modulemap ship inside the ActrCabiLib binaryTarget
            // (ActrCabi.xcframework), imported as `import ActrCabiLib`.
        ),
    ]
)
