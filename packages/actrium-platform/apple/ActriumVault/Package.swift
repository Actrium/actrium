// swift-tools-version: 5.10
import PackageDescription

let package = Package(
    name: "ActriumVault",
    platforms: [
        .macOS(.v14),
        .iOS(.v17),
    ],
    products: [
        .library(
            name: "ActriumVault",
            targets: ["ActriumVault"]
        ),
    ],
    targets: [
        .target(
            name: "ActriumVault",
            dependencies: [
                .target(name: "actrium_vaultFFI"),
            ],
            path: "Sources/ActriumVault"
        ),
        .binaryTarget(
            name: "actrium_vaultFFI",
            path: "../../../../crates/actrium-vault/ActriumVault.xcframework"
        ),
    ]
)
