// swift-tools-version:5.9

import PackageDescription

let package = Package(
    name: "orange-ios-secret-store",
    platforms: [.iOS(.v13)],
    products: [
        .library(
            name: "orange-ios-secret-store",
            type: .static,
            targets: ["orange-ios-secret-store"]
        )
    ],
    dependencies: [
        .package(name: "Tauri", path: "../.tauri/tauri-api"),
        .package(path: "../secret-store-core"),
    ],
    targets: [
        .target(
            name: "orange-ios-secret-store",
            dependencies: [
                .byName(name: "Tauri"),
                .product(
                    name: "OrangeSecretStoreCore",
                    package: "secret-store-core"
                ),
            ],
            path: "Sources"
        )
    ]
)
