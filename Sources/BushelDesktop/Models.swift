//===----------------------------------------------------------------------===//
// Copyright © 2026 Apple Inc. and the container project authors.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//   https://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.
//===----------------------------------------------------------------------===//

import Foundation

enum BushelSection: String, CaseIterable, Identifiable {
    case containers = "Containers"
    case images = "Images"
    case volumes = "Volumes"

    var id: Self { self }

    var symbol: String {
        switch self {
        case .containers: "shippingbox.fill"
        case .images: "square.3.layers.3d"
        case .volumes: "externaldrive.fill"
        }
    }
}

struct ContainerItem: Decodable, Identifiable, Sendable {
    struct Configuration: Decodable, Sendable {
        struct Image: Decodable, Sendable { let reference: String }
        struct Resources: Decodable, Sendable {
            let cpus: Int?
            let memoryInBytes: UInt64?
        }
        struct PublishedPort: Decodable, Sendable {
            let hostPort: UInt16
            let containerPort: UInt16
            let proto: String
        }

        let image: Image
        let publishedPorts: [PublishedPort]?
        let resources: Resources?
        let creationDate: Date?
    }

    struct Status: Decodable, Sendable {
        let state: String
        let startedDate: Date?
    }

    let id: String
    let configuration: Configuration
    let status: Status

    var name: String { id }
    var shortID: String { String(id.prefix(12)) }
    var image: String { configuration.image.reference }
    var isRunning: Bool { status.state == "running" }
    var ports: String {
        guard let ports = configuration.publishedPorts, !ports.isEmpty else { return "—" }
        return ports.map { "\($0.hostPort):\($0.containerPort)/\($0.proto)" }.joined(separator: ", ")
    }
}

struct ImageItem: Decodable, Identifiable, Sendable {
    struct Configuration: Decodable, Sendable {
        struct Descriptor: Decodable, Sendable {
            let digest: String
            let size: UInt64?
        }

        let creationDate: Date?
        let name: String
        let descriptor: Descriptor
    }

    struct Variant: Decodable, Sendable {
        struct Platform: Decodable, Sendable {
            let os: String?
            let architecture: String?
        }

        let platform: Platform?
        let size: UInt64?
    }

    let id: String
    let configuration: Configuration
    let variants: [Variant]?

    var name: String { configuration.name }
    var shortID: String { String(id.prefix(12)) }
    var size: UInt64 {
        let variantSize = variants?.compactMap(\.size).reduce(0, +) ?? 0
        return variantSize > 0 ? variantSize : configuration.descriptor.size ?? 0
    }
    var platform: String {
        let values =
            variants?.compactMap { variant -> String? in
                guard let platform = variant.platform else { return nil }
                return [platform.os, platform.architecture].compactMap { $0 }.joined(separator: "/")
            } ?? []
        return values.isEmpty ? "—" : values.joined(separator: ", ")
    }
}

struct VolumeItem: Decodable, Identifiable, Sendable {
    struct Configuration: Decodable, Sendable {
        let name: String
        let driver: String
        let format: String
        let source: String
        let creationDate: Date?
        let labels: [String: String]?
        let sizeInBytes: UInt64?
    }

    let id: String
    let configuration: Configuration

    var name: String { configuration.name }
    var kind: String {
        configuration.labels?["com.apple.container.resource.anonymous"] == nil ? "Named" : "Anonymous"
    }
}

struct CommandResult: Sendable {
    let output: Data
    let error: Data
    let status: Int32
}

enum BushelCommandError: LocalizedError {
    case failed(String)

    var errorDescription: String? {
        switch self {
        case .failed(let message): message
        }
    }
}

extension ByteCountFormatter {
    static func bushelString(_ bytes: UInt64) -> String {
        string(fromByteCount: Int64(clamping: bytes), countStyle: .file)
    }
}

extension Date {
    var bushelRelative: String {
        RelativeDateTimeFormatter().localizedString(for: self, relativeTo: Date())
    }
}
