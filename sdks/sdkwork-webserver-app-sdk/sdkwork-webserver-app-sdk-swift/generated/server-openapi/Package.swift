// swift-tools-version:5.7
import PackageDescription

let package = Package(
    name: "AppSDK",
    platforms: [
        .iOS(.v13),
        .macOS(.v10_15),
    ],
    products: [
        .library(
            name: "AppSDK",
            targets: ["AppSDK"]
        ),
    ],
    dependencies: [
        .package(url: "https://github.com/sdkwork/sdk-common-swift.git", from: "1.0.0")
    ],
    targets: [
        .target(
            name: "AppSDK",
            dependencies: ["SDKworkCommon"],
            path: "Sources"
        )
    ]
)
