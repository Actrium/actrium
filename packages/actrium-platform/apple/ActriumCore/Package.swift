// swift-tools-version: 5.10
import PackageDescription

let package = Package(
    name: "ActriumCore",
    platforms: [
        .macOS(.v14),
        .iOS(.v17),
    ],
    products: [
        .library(
            name: "ActriumCore",
            targets: ["ActriumCore"]
        ),
    ],
    targets: [
        .target(
            name: "ActriumCore",
            dependencies: [
                .target(name: "actrium_coreFFI"),
            ],
            path: "Sources/ActriumCore"
        ),
        .binaryTarget(
            name: "actrium_coreFFI",
            path: "../../../../crates/actrium-core/ActriumCore.xcframework"
        ),
        .testTarget(
            name: "ActriumCoreTests",
            dependencies: ["ActriumCore"],
            path: "Tests/ActriumCoreTests"
        ),
    ]
)
