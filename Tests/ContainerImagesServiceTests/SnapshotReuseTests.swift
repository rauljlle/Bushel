//===----------------------------------------------------------------------===//
// Copyright © 2025-2026 Apple Inc. and the container project authors.
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
import Testing

@testable import ContainerImagesService

struct SnapshotReuseTests {
    @Test func reusesOnlyMatchingCompleteLayerStack() async throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString, isDirectory: true)
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: root) }

        let store = try SnapshotStore(path: root, unpackStrategy: { _, _ in nil }, log: nil)
        let sourceDir = root.appendingPathComponent("snapshots/sha256-source", isDirectory: true)
        try FileManager.default.createDirectory(at: sourceDir, withIntermediateDirectories: true)
        let source = sourceDir.appendingPathComponent("snapshot")
        try Data("original".utf8).write(to: source)

        let signature = SnapshotStore.LayerSignature(platform: "linux/arm64", layers: ["gzip:sha256:first", "gzip:sha256:second"])
        try JSONEncoder().encode(signature).write(to: sourceDir.appendingPathComponent("layer-signature.json"))

        let different = SnapshotStore.LayerSignature(platform: "linux/arm64", layers: ["gzip:sha256:first", "gzip:sha256:changed"])
        let unused = root.appendingPathComponent("unused")
        #expect(try await store.reuseSnapshot(matching: different, at: unused, minimumBytes: 1) == false)
        #expect(!FileManager.default.fileExists(atPath: unused.path))

        let destination = root.appendingPathComponent("reused")
        #expect(try await store.reuseSnapshot(matching: signature, at: destination, minimumBytes: 1))
        #expect(try Data(contentsOf: destination) == Data("original".utf8))

        try Data("changed".utf8).write(to: destination)
        #expect(try Data(contentsOf: source) == Data("original".utf8))
    }
}
