// swift-tools-version: 5.9
import PackageDescription

let package = Package(
    name: "VisitMediaReduce",
    platforms: [.macOS(.v14)],
    products: [
        .executable(name: "VisitMediaReduce", targets: ["VisitMediaReduce"])
    ],
    targets: [
        .executableTarget(
            name: "VisitMediaReduce",
            path: "Sources/VisitMediaReduce"
        )
    ]
)
