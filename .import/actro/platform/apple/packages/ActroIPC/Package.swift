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
        .package(url: "https://github.com/apple/swift-protobuf.git", exact: "1.32.0"),
    ],
    targets: [
        .target(
            name: "ActroIPC",
            dependencies: [
                .product(name: "SwiftProtobuf", package: "swift-protobuf"),
            ],
            swiftSettings: [
                // Disable MainActor default isolation for this package
                .enableExperimentalFeature("StrictConcurrency"),
            ]
        ),
    ]
)
