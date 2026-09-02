// swift-tools-version: 5.10
import PackageDescription

let package = Package(
    name: "ActriumIPC",
    platforms: [.macOS(.v14)],
    products: [
        .library(
            name: "ActriumIPC",
            targets: ["ActriumIPC"]
        ),
    ],
    dependencies: [
        .package(url: "https://github.com/apple/swift-protobuf.git", exact: "1.38.1"),
    ],
    targets: [
        .target(
            name: "ActriumIPC",
            dependencies: [
                .product(name: "SwiftProtobuf", package: "swift-protobuf"),
            ],
            swiftSettings: [
                // Disable MainActor default isolation for this package
                .enableExperimentalFeature("StrictConcurrency"),
            ]
        ),
        .testTarget(
            name: "ActriumIPCTests",
            dependencies: ["ActriumIPC"]
        ),
    ]
)
