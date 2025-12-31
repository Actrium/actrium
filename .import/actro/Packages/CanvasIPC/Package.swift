// swift-tools-version: 5.10
import PackageDescription

let package = Package(
    name: "CanvasIPC",
    platforms: [.macOS(.v14)],
    products: [
        .library(
            name: "CanvasIPC",
            targets: ["CanvasIPC"]
        ),
    ],
    dependencies: [
        .package(url: "https://github.com/apple/swift-protobuf.git", from: "1.28.0"),
    ],
    targets: [
        .target(
            name: "CanvasIPC",
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
