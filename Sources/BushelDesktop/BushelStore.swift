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

@preconcurrency import Foundation
import SwiftUI

final class BushelCommandRunner: @unchecked Sendable {
    private final class DataBuffer: @unchecked Sendable {
        private let lock = NSLock()
        private var data = Data()

        func append(_ chunk: Data) {
            lock.lock()
            data.append(chunk)
            lock.unlock()
        }

        func snapshot() -> Data {
            lock.lock()
            defer { lock.unlock() }
            return data
        }
    }

    func run(executable: String, arguments: [String]) async throws -> CommandResult {
        try await withCheckedThrowingContinuation { continuation in
            let process = Process()
            let output = Pipe()
            let errorPipe = Pipe()
            let outputBuffer = DataBuffer()
            let errorBuffer = DataBuffer()

            if executable.contains("/") {
                process.executableURL = URL(fileURLWithPath: executable)
                process.arguments = arguments
            } else {
                process.executableURL = URL(fileURLWithPath: "/usr/bin/env")
                process.arguments = [executable] + arguments
            }
            process.standardOutput = output
            process.standardError = errorPipe
            output.fileHandleForReading.readabilityHandler = { handle in
                let chunk = handle.availableData
                if !chunk.isEmpty { outputBuffer.append(chunk) }
            }
            errorPipe.fileHandleForReading.readabilityHandler = { handle in
                let chunk = handle.availableData
                if !chunk.isEmpty { errorBuffer.append(chunk) }
            }
            process.terminationHandler = { completed in
                output.fileHandleForReading.readabilityHandler = nil
                errorPipe.fileHandleForReading.readabilityHandler = nil
                outputBuffer.append(output.fileHandleForReading.readDataToEndOfFile())
                errorBuffer.append(errorPipe.fileHandleForReading.readDataToEndOfFile())
                let result = CommandResult(
                    output: outputBuffer.snapshot(),
                    error: errorBuffer.snapshot(),
                    status: completed.terminationStatus
                )
                continuation.resume(returning: result)
            }

            do {
                try process.run()
            } catch {
                output.fileHandleForReading.readabilityHandler = nil
                errorPipe.fileHandleForReading.readabilityHandler = nil
                continuation.resume(throwing: error)
            }
        }
    }
}

@MainActor
final class BushelStore: ObservableObject {
    @Published var selection: BushelSection = .containers
    @Published var search = ""
    @Published var onlyRunning = false
    @Published private(set) var containers: [ContainerItem] = []
    @Published private(set) var images: [ImageItem] = []
    @Published private(set) var volumes: [VolumeItem] = []
    @Published private(set) var isLoading = false
    @Published private(set) var engineRunning = false
    @Published private(set) var errorMessage: String?
    @Published var cliExecutable: String {
        didSet { UserDefaults.standard.set(cliExecutable, forKey: "BushelCLIExecutable") }
    }

    private let runner = BushelCommandRunner()

    init() {
        cliExecutable =
            UserDefaults.standard.string(forKey: "BushelCLIExecutable")
            ?? ProcessInfo.processInfo.environment["BUSHEL_CLI_PATH"]
            ?? Self.defaultCLIExecutable

        if ProcessInfo.processInfo.environment["BUSHEL_GUI_DEMO"] == "1" {
            loadDemoContent()
        } else {
            Task { await refresh() }
        }
    }

    var filteredContainers: [ContainerItem] {
        containers.filter { item in
            (!onlyRunning || item.isRunning) && matches([item.name, item.image, item.shortID])
        }
    }

    var filteredImages: [ImageItem] {
        images.filter { matches([$0.name, $0.shortID, $0.platform]) }
    }

    var filteredVolumes: [VolumeItem] {
        volumes.filter { matches([$0.name, $0.configuration.driver, $0.configuration.source]) }
    }

    var runningCount: Int { containers.filter(\.isRunning).count }
    var allocatedMemory: UInt64 {
        containers.filter(\.isRunning).compactMap { $0.configuration.resources?.memoryInBytes }.reduce(0, +)
    }
    var allocatedCPUs: Int {
        containers.filter(\.isRunning).compactMap { $0.configuration.resources?.cpus }.reduce(0, +)
    }

    func refresh() async {
        isLoading = true
        defer { isLoading = false }

        do {
            let status = try await runner.run(executable: cliExecutable, arguments: ["system", "status"])
            engineRunning = status.status == 0
            guard engineRunning else {
                throw BushelCommandError.failed(message(from: status, fallback: "Bushel engine is stopped"))
            }

            async let containerResult = runner.run(executable: cliExecutable, arguments: ["ps", "--all", "--format", "json"])
            async let imageResult = runner.run(executable: cliExecutable, arguments: ["images", "--format", "json"])
            async let volumeResult = runner.run(executable: cliExecutable, arguments: ["volume", "list", "--format", "json"])

            let (containerOutput, imageOutput, volumeOutput) = try await (containerResult, imageResult, volumeResult)
            containers = try decode([ContainerItem].self, from: containerOutput)
            images = try decode([ImageItem].self, from: imageOutput)
            volumes = try decode([VolumeItem].self, from: volumeOutput)
            errorMessage = nil
        } catch {
            engineRunning = false
            errorMessage = error.localizedDescription
        }
    }

    func startEngine() async {
        await perform(["system", "start"], refreshAfter: true)
    }

    func stopEngine() async {
        await perform(["system", "stop"], refreshAfter: false)
        engineRunning = false
    }

    func toggle(_ container: ContainerItem) async {
        await perform([container.isRunning ? "stop" : "start", container.id])
    }

    func deleteContainer(_ container: ContainerItem) async {
        await perform(["rm", "--force", container.id])
    }

    func deleteImage(_ image: ImageItem) async {
        await perform(["rmi", image.configuration.name])
    }

    func deleteVolume(_ volume: VolumeItem) async {
        await perform(["volume", "delete", volume.id])
    }

    private func perform(_ arguments: [String], refreshAfter: Bool = true) async {
        isLoading = true
        do {
            let result = try await runner.run(executable: cliExecutable, arguments: arguments)
            guard result.status == 0 else {
                throw BushelCommandError.failed(message(from: result, fallback: "Command failed"))
            }
            errorMessage = nil
            if refreshAfter { await refresh() }
        } catch {
            errorMessage = error.localizedDescription
        }
        isLoading = false
    }

    private func decode<T: Decodable>(_ type: T.Type, from result: CommandResult) throws -> T {
        guard result.status == 0 else {
            throw BushelCommandError.failed(message(from: result, fallback: "Unable to load resources"))
        }
        let decoder = JSONDecoder()
        decoder.dateDecodingStrategy = .iso8601
        return try decoder.decode(type, from: result.output)
    }

    private func message(from result: CommandResult, fallback: String) -> String {
        let error = String(decoding: result.error, as: UTF8.self).trimmingCharacters(in: .whitespacesAndNewlines)
        return error.isEmpty ? fallback : error
    }

    private func matches(_ values: [String]) -> Bool {
        search.isEmpty || values.contains { $0.localizedCaseInsensitiveContains(search) }
    }

    private func loadDemoContent() {
        engineRunning = true
        containers = Self.decodeDemo(
            """
            [
              {"id":"bushel-web-1","configuration":{"image":{"reference":"nginx:alpine"},"publishedPorts":[{"hostPort":8080,"containerPort":80,"proto":"tcp"}],"resources":{"cpus":2,"memoryInBytes":1073741824},"creationDate":"2026-10-09T14:20:00Z"},"status":{"state":"running","startedDate":"2026-10-09T14:21:00Z"}},
              {"id":"bushel-api-1","configuration":{"image":{"reference":"ghcr.io/example/api:latest"},"publishedPorts":[{"hostPort":3000,"containerPort":3000,"proto":"tcp"}],"resources":{"cpus":4,"memoryInBytes":2147483648},"creationDate":"2026-10-08T11:00:00Z"},"status":{"state":"running","startedDate":"2026-10-09T10:00:00Z"}},
              {"id":"bushel-postgres-1","configuration":{"image":{"reference":"postgres:17"},"publishedPorts":[{"hostPort":5432,"containerPort":5432,"proto":"tcp"}],"resources":{"cpus":2,"memoryInBytes":1073741824},"creationDate":"2026-10-02T09:00:00Z"},"status":{"state":"stopped","startedDate":null}}
            ]
            """
        )
        images = Self.decodeDemo(
            """
            [
              {"id":"a39deca2d68f","configuration":{"creationDate":"2026-10-08T11:00:00Z","name":"nginx:alpine","descriptor":{"digest":"sha256:a39deca2d68f","size":52428800}},"variants":[{"platform":{"os":"linux","architecture":"arm64"},"size":52428800}]},
              {"id":"c3b03d04a438","configuration":{"creationDate":"2026-10-01T11:00:00Z","name":"postgres:17","descriptor":{"digest":"sha256:c3b03d04a438","size":452984832}},"variants":[{"platform":{"os":"linux","architecture":"arm64"},"size":452984832}]}
            ]
            """
        )
        volumes = Self.decodeDemo(
            """
            [
              {"id":"postgres-data","configuration":{"name":"postgres-data","driver":"local","format":"ext4","source":"/var/lib/bushel/postgres-data","creationDate":"2026-10-01T11:00:00Z","labels":{},"sizeInBytes":10737418240}},
              {"id":"cache","configuration":{"name":"cache","driver":"local","format":"ext4","source":"/var/lib/bushel/cache","creationDate":"2026-10-08T11:00:00Z","labels":{},"sizeInBytes":1073741824}}
            ]
            """
        )
    }

    nonisolated private static func decodeDemo<T: Decodable>(_ json: String) -> T {
        let decoder = JSONDecoder()
        decoder.dateDecodingStrategy = .iso8601
        return try! decoder.decode(T.self, from: Data(json.utf8))
    }

    nonisolated private static var defaultCLIExecutable: String {
        let installedPath = "/usr/local/bin/bushel"
        if FileManager.default.isExecutableFile(atPath: installedPath) {
            return installedPath
        }
        return "bushel"
    }
}
