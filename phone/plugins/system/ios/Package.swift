// swift-tools-version:5.9

import PackageDescription

let package = Package(
  name: "tauri-plugin-nocturne-system",
  platforms: [
    .macOS(.v14),
    .iOS(.v17),
  ],
  products: [
    .library(
      name: "tauri-plugin-nocturne-system",
      type: .static,
      targets: ["tauri-plugin-nocturne-system"])
  ],
  dependencies: [
    .package(name: "Tauri", path: "../.tauri/tauri-api")
  ],
  targets: [
    .target(
      name: "tauri-plugin-nocturne-system",
      dependencies: [
        .byName(name: "Tauri")
      ],
      path: "Sources")
  ]
)
