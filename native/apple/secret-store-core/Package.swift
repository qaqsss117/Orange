// swift-tools-version:5.9

import PackageDescription

let package = Package(
    name: "OrangeSecretStoreCore",
    platforms: [
        .iOS(.v13),
        .macOS(.v10_15),
    ],
    products: [
        .library(
            name: "OrangeSecretStoreCore",
            targets: ["OrangeSecretStoreCore"]
        )
    ],
    targets: [
        .target(name: "OrangeSecretStoreCore"),
        .testTarget(
            name: "OrangeSecretStoreCoreTests",
            dependencies: ["OrangeSecretStoreCore"]
        ),
    ]
)
