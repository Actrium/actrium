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
                // Check explicit actor isolation and cross-thread transport ownership.
                .enableExperimentalFeature("StrictConcurrency"),
            ]
        ),
        .testTarget(
            name: "ActriumIPCTests",
            dependencies: ["ActriumIPC"]
        ),
    ]
)
