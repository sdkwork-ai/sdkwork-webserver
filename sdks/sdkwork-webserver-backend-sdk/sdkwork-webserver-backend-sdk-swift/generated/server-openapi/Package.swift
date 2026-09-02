// swift-tools-version:5.7
import PackageDescription

let package = Package(
    name: "BackendSDK",
    platforms: [
        .iOS(.v13),
        .macOS(.v10_15),
    ],
    products: [
        .library(
            name: "BackendSDK",
            targets: ["BackendSDK"]
        ),
    ],
    dependencies: [
        .package(url: "https://github.com/sdkwork/sdk-common-swift.git", from: "1.0.0")
    ],
    targets: [
        .target(
            name: "BackendSDK",
            dependencies: ["SDKworkCommon"],
            path: "Sources"
        )
    ]
)
