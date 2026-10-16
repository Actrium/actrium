// swift-tools-version: 5.10
import PackageDescription

let package = Package(
    name: "ActroIPC",
    platforms: [.macOS(.v14)],
    products: [
        .library(
            name: "ActroIPC",
            targets: ["ActroIPC"]
        ),
    ],
    dependencies: [
        .package(url: "https://github.com/apple/swift-protobuf.git", exact: "1.38.1"),
    ],
    targets: [
        .target(
            name: "ActroIPC",
            dependencies: [
                .product(name: "SwiftProtobuf", package: "swift-protobuf"),
            ],
            swiftSettings: [
                // Check explicit actor isolation and cross-thread transport ownership.
                .enableExperimentalFeature("StrictConcurrency"),
            ]
        ),
        .testTarget(
            name: "ActroIPCTests",
            dependencies: ["ActroIPC"]
        ),
    ]
)
