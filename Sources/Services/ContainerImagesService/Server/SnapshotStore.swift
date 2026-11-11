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

import ContainerAPIClient
import ContainerPersistence
import ContainerResource
import Containerization
import ContainerizationEXT4
import ContainerizationError
import ContainerizationExtras
import ContainerizationOCI
import ContainerizationOS
import Darwin
import Foundation
import Logging
import TerminalProgress

public actor SnapshotStore {
    private static let snapshotFileName = "snapshot"
    private static let snapshotInfoFileName = "snapshot-info"
    private static let layerSignatureFileName = "layer-signature.json"
    private static let ingestDirName = "ingest"

    struct LayerSignature: Codable, Equatable {
        let platform: String
        let layers: [String]
    }

    /// Return the Unpacker to use for a given image.
    /// If the given platform for the image cannot be unpacked return `nil`.
    public typealias UnpackStrategy = @Sendable (Containerization.Image, Platform) async throws -> Unpacker?

    public static func defaultUnpackStrategy(initImage: String) -> UnpackStrategy {
        { image, platform in
            guard platform.os == "linux" else {
                return nil
            }
            let capacityInBytes: UInt64
            if image.reference == initImage {
                capacityInBytes = 512.mib()
            } else {
                capacityInBytes = 512.gib()
            }
            return EXT4Unpacker(capacityInBytes: capacityInBytes, journal: .init(defaultMode: .ordered))
        }
    }

    let path: URL
    let fm = FileManager.default
    let ingestDir: URL
    let unpackStrategy: UnpackStrategy
    let log: Logger?
    let initImage: String?

    public init(path: URL, unpackStrategy: @escaping UnpackStrategy, log: Logger?, initImage: String? = nil) throws {
        let root = path.appendingPathComponent("snapshots")
        self.path = root
        self.ingestDir = self.path.appendingPathComponent(Self.ingestDirName)
        self.unpackStrategy = unpackStrategy
        self.log = log
        self.initImage = initImage
        try PathUtils.createDirectoryExcludingFromBackup(root, fileManager: self.fm)
        try self.fm.createDirectory(at: self.ingestDir, withIntermediateDirectories: true)
    }

    public func unpack(image: Containerization.Image, platform: Platform? = nil, progressUpdate: ProgressUpdateHandler?) async throws {
        var toUnpack: [Descriptor] = []
        if let platform {
            let desc = try await image.descriptor(for: platform)
            toUnpack = [desc]
        } else {
            // The index can list platforms whose content is not in the store, for example when the
            // image was loaded from an archive saved for a single platform, or pulled for one.
            // There is nothing to unpack for those.
            for desc in try await image.unpackableDescriptors() {
                guard let platform = desc.platform, try await image.hasManifest(for: platform) else {
                    self.log?.info(
                        "skipping unpack of a platform with no content",
                        metadata: [
                            "image": "\(image.reference)",
                            "platform": "\(desc.platform?.description ?? "unknown")",
                            "digest": "\(desc.digest)",
                        ])
                    continue
                }
                toUnpack.append(desc)
            }
        }

        let taskManager = ProgressTaskCoordinator()
        var taskUpdateProgress: ProgressUpdateHandler?

        for desc in toUnpack {
            try Task.checkCancellation()
            let snapshotDir = try self.snapshotDir(desc)
            guard !self.fm.fileExists(atPath: snapshotDir.absolutePath()) else {
                // We have already unpacked this image + platform. Skip
                continue
            }
            guard let platform = desc.platform else {
                throw ContainerizationError(.internalError, message: "missing platform for descriptor \(desc.digest)")
            }
            guard let unpacker = try await self.unpackStrategy(image, platform) else {
                self.log?.warning("no unpacker configured, skipping unpack for \(image.reference) for platform \(platform.description)")
                continue
            }
            let currentSubTask = await taskManager.startTask()
            if let progressUpdate {
                let _taskUpdateProgress = ProgressTaskCoordinator.handler(for: currentSubTask, from: progressUpdate)
                await _taskUpdateProgress([
                    .setSubDescription("for platform \(platform.description)")
                ])
                taskUpdateProgress = _taskUpdateProgress
            }

            let tempDir = try self.tempUnpackDir()

            let tempSnapshotPath = tempDir.appendingPathComponent(Self.snapshotFileName, isDirectory: false)
            let infoPath = tempDir.appendingPathComponent(Self.snapshotInfoFileName, isDirectory: false)
            do {
                let canReuse = unpacker is EXT4Unpacker && self.initImage != nil && image.reference != self.initImage
                let signature = canReuse ? try await self.layerSignature(for: image, platform: platform) : nil
                let reused =
                    if let signature {
                        try self.reuseSnapshot(matching: signature, at: tempSnapshotPath)
                    } else {
                        false
                    }
                if reused {
                    await taskUpdateProgress?([.setSubDescription("reused identical image layers")])
                } else if unpacker is EXT4Unpacker {
                    await taskUpdateProgress?([.setSubDescription("extracting image layers in a single pass")])
                }
                // EXT4Unpacker scans every compressed layer once to calculate a
                // total before reading it again to extract. Avoid that full extra
                // pass for the default image format; the task remains indeterminate.
                let progress =
                    unpacker is EXT4Unpacker
                    ? nil : ContainerizationProgressAdapter.handler(from: taskUpdateProgress)
                let mount =
                    if reused {
                        Mount.block(format: "ext4", source: tempSnapshotPath.absolutePath(), destination: "/", options: [])
                    } else {
                        try await unpacker.unpack(image, for: platform, at: tempSnapshotPath, progress: progress)
                    }
                let fs = Filesystem.block(
                    format: mount.type,
                    source: try self.snapshotPath(desc).absolutePath(),
                    destination: mount.destination,
                    options: mount.options
                )
                let snapshotInfo = try JSONEncoder().encode(fs)
                self.fm.createFile(atPath: infoPath.absolutePath(), contents: snapshotInfo)
                if let signature {
                    let signaturePath = tempDir.appendingPathComponent(Self.layerSignatureFileName, isDirectory: false)
                    try JSONEncoder().encode(signature).write(to: signaturePath, options: .atomic)
                }
            } catch {
                try? self.fm.removeItem(at: tempDir)
                throw error
            }
            do {
                try fm.moveItem(at: tempDir, to: snapshotDir)
            } catch let err as NSError {
                guard err.code == NSFileWriteFileExistsError else {
                    throw err
                }
                try? self.fm.removeItem(at: tempDir)
            }
        }
        await taskManager.finish()
    }

    public func delete(for image: Containerization.Image, platform: Platform? = nil) async throws {
        var toDelete: [Descriptor] = []
        if let platform {
            let desc = try await image.descriptor(for: platform)
            toDelete.append(desc)
        } else {
            toDelete = try await image.unpackableDescriptors()
        }
        for desc in toDelete {
            let p = try self.snapshotDir(desc)
            guard self.fm.fileExists(atPath: p.absolutePath()) else {
                continue
            }
            try self.fm.removeItem(at: p)
        }
    }

    public func get(for image: Containerization.Image, platform: Platform) async throws -> Filesystem {
        let desc = try await image.descriptor(for: platform)
        let infoPath = try snapshotInfoPath(desc)
        let fsPath = try snapshotPath(desc)

        guard self.fm.fileExists(atPath: infoPath.absolutePath()),
            self.fm.fileExists(atPath: fsPath.absolutePath())
        else {
            throw ContainerizationError(.notFound, message: "image snapshot for \(image.reference) with platform \(platform.description)")
        }
        let decoder = JSONDecoder()
        let data = try Data(contentsOf: infoPath)
        let fs = try decoder.decode(Filesystem.self, from: data)
        return fs
    }

    public func clean(keepingSnapshotsFor images: [Containerization.Image] = []) async throws -> UInt64 {
        var toKeep: [String] = [Self.ingestDirName]
        for image in images {
            for manifest in try await image.index().manifests {
                guard let platform = manifest.platform else {
                    continue
                }
                let desc = try await image.descriptor(for: platform)
                toKeep.append(try desc.digest.validatedDigestEncoding())
            }
        }
        let all = try self.fm.contentsOfDirectory(at: self.path, includingPropertiesForKeys: [.totalFileAllocatedSizeKey]).map {
            $0.lastPathComponent
        }
        let delete = Set(all).subtracting(Set(toKeep))
        var deletedBytes: UInt64 = 0
        for dir in delete {
            let unpackedPath = self.path.appending(path: dir, directoryHint: .isDirectory)
            guard self.fm.fileExists(atPath: unpackedPath.absolutePath()) else {
                continue
            }
            deletedBytes += self.fm.allocatedSize(of: unpackedPath)
            try self.fm.removeItem(at: unpackedPath)
        }
        return deletedBytes
    }

    private func snapshotDir(_ desc: Descriptor) throws -> URL {
        let p = self.path.appendingPathComponent(try desc.digest.validatedDigestEncoding(), isDirectory: true)
        return p
    }

    private func snapshotPath(_ desc: Descriptor) throws -> URL {
        let p = try self.snapshotDir(desc)
            .appendingPathComponent(Self.snapshotFileName, isDirectory: false)
        return p
    }

    private func snapshotInfoPath(_ desc: Descriptor) throws -> URL {
        let p = try self.snapshotDir(desc)
            .appendingPathComponent(Self.snapshotInfoFileName, isDirectory: false)
        return p
    }

    private func tempUnpackDir() throws -> URL {
        let uniqueDirectoryURL = ingestDir.appendingPathComponent(UUID().uuidString, isDirectory: true)
        try self.fm.createDirectory(at: uniqueDirectoryURL, withIntermediateDirectories: true, attributes: nil)
        return uniqueDirectoryURL
    }

    private func layerSignature(for image: Containerization.Image, platform: Platform) async throws -> LayerSignature {
        let manifest = try await image.manifest(for: platform)
        return LayerSignature(
            platform: platform.description,
            layers: manifest.layers.map { "\($0.mediaType):\($0.digest)" }
        )
    }

    /// Reuse only a complete, byte-for-byte identical layer stack. The ext4
    /// formatter cannot append to an existing filesystem, so a shared prefix
    /// alone is not yet reusable. clonefile avoids copying the sparse 512 GiB
    /// image byte by byte; if the volume does not support clones, unpack anew.
    func reuseSnapshot(matching signature: LayerSignature, at destination: URL, minimumBytes: UInt64 = 512.gib()) throws -> Bool {
        let entries = try self.fm.contentsOfDirectory(at: self.path, includingPropertiesForKeys: nil)
        for entry in entries where entry.lastPathComponent != Self.ingestDirName {
            let signaturePath = entry.appendingPathComponent(Self.layerSignatureFileName, isDirectory: false)
            let source = entry.appendingPathComponent(Self.snapshotFileName, isDirectory: false)
            guard self.fm.fileExists(atPath: source.absolutePath()),
                let data = try? Data(contentsOf: signaturePath),
                (try? JSONDecoder().decode(LayerSignature.self, from: data)) == signature,
                let size = try? self.fm.attributesOfItem(atPath: source.absolutePath())[.size] as? NSNumber,
                size.uint64Value >= minimumBytes
            else {
                continue
            }
            let result = source.path.withCString { sourcePath in
                destination.path.withCString { destinationPath in
                    clonefile(sourcePath, destinationPath, 0)
                }
            }
            if result == 0 {
                return true
            }
            // An optimization must not prevent the ordinary unpack path.
            try? self.fm.removeItem(at: destination)
            self.log?.warning("could not clone image snapshot, unpacking normally (errno \(errno))")
        }
        return false
    }

    /// Get the disk size for a specific snapshot descriptor
    public func getSnapshotSize(descriptor: Descriptor) throws -> UInt64 {
        let snapshotPath = try self.snapshotDir(descriptor)
        guard self.fm.fileExists(atPath: snapshotPath.path) else {
            return 0
        }
        return self.fm.allocatedSize(of: snapshotPath)
    }

    /// Returns (trimmed digest, size) pairs for every unpackable snapshot owned by the image.
    public func getSnapshotSizes(for image: Containerization.Image) async throws -> [(digest: String, size: UInt64)] {
        var results: [(digest: String, size: UInt64)] = []
        for descriptor in try await image.unpackableDescriptors() {
            let size = try self.getSnapshotSize(descriptor: descriptor)
            guard size > 0 else { continue }
            results.append((try descriptor.digest.validatedDigestEncoding(), size))
        }
        return results
    }

    /// Total allocated bytes across all snapshot storage (including orphans).
    public func totalAllocatedSize() -> UInt64 {
        self.fm.allocatedSize(of: self.path)
    }
}

extension Containerization.Image {
    /// Whether the manifest for the platform is in the content store.
    fileprivate func hasManifest(for platform: Platform) async throws -> Bool {
        do {
            _ = try await self.manifest(for: platform)
            return true
        } catch let error as ContainerizationError where error.code == .notFound {
            return false
        }
    }

    fileprivate func unpackableDescriptors() async throws -> [Descriptor] {
        let index = try await self.index()
        return index.manifests.filter { desc in
            guard desc.platform != nil else {
                return false
            }
            if let referenceType = desc.annotations?["vnd.docker.reference.type"], referenceType == "attestation-manifest" {
                return false
            }
            return true
        }
    }
}
