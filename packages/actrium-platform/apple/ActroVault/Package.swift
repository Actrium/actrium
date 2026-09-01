// swift-tools-version: 5.10
import PackageDescription

let package = Package(
    name: "ActroVault",
    platforms: [
        .macOS(.v14),
        .iOS(.v17),
    ],
    products: [
        .library(
            name: "ActroVault",
            targets: ["ActroVault"]
        ),
    ],
    targets: [
        .target(
            name: "ActroVault",
            dependencies: [
                .target(name: "actro_vaultFFI"),
            ],
            path: "Sources/ActroVault"
        ),
        .binaryTarget(
            name: "actro_vaultFFI",
            path: "../../../../crates/actrium-vault/ActroVault.xcframework"
        ),
    ]
)
