// swift-tools-version: 5.10
import PackageDescription

let package = Package(
    name: "ActroCore",
    platforms: [
        .macOS(.v14),
        .iOS(.v17),
    ],
    products: [
        .library(
            name: "ActroCore",
            targets: ["ActroCore"]
        ),
    ],
    targets: [
        .target(
            name: "ActroCore",
            dependencies: [
                .target(name: "actro_coreFFI"),
            ],
            path: "Sources/ActroCore"
        ),
        .binaryTarget(
            name: "actro_coreFFI",
            path: "../../../../crates/actrium-core/ActroCore.xcframework"
        ),
        .testTarget(
            name: "ActroCoreTests",
            dependencies: ["ActroCore"],
            path: "Tests/ActroCoreTests"
        ),
    ]
)
