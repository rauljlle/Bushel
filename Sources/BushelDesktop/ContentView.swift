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

import SwiftUI

private enum BushelPalette {
    static let accent = Color(red: 0.91, green: 0.12, blue: 0.16)
    static let accentDark = Color(red: 0.56, green: 0.035, blue: 0.06)
    static let canvas = Color(red: 0.045, green: 0.055, blue: 0.065)
    static let sidebar = Color(red: 0.025, green: 0.03, blue: 0.036)
    static let panel = Color(red: 0.075, green: 0.09, blue: 0.105)
    static let panelRaised = Color(red: 0.095, green: 0.11, blue: 0.125)
    static let line = Color.white.opacity(0.09)
    static let secondary = Color(red: 0.57, green: 0.63, blue: 0.69)
}

struct ContentView: View {
    @EnvironmentObject private var store: BushelStore
    @State private var settingsPresented = false

    var body: some View {
        GeometryReader { proxy in
            let compact = proxy.size.width < 900

            VStack(spacing: 0) {
                topBar(compact: compact)
                HStack(spacing: 0) {
                    sidebar(compact: compact)
                    Rectangle().fill(BushelPalette.line).frame(width: 1)
                    content(compact: compact)
                }
                statusBar(compact: compact)
            }
            .background(BushelPalette.canvas)
        }
        .sheet(isPresented: $settingsPresented) {
            SettingsView()
                .environmentObject(store)
        }
    }

    private func topBar(compact: Bool) -> some View {
        ZStack {
            LinearGradient(
                colors: [BushelPalette.accentDark, BushelPalette.accent],
                startPoint: .leading,
                endPoint: .trailing
            )
            HStack(spacing: compact ? 10 : 18) {
                Spacer().frame(width: compact ? 52 : 76)
                brand(showName: !compact)
                if !compact {
                    Spacer(minLength: 24)
                }
                HStack(spacing: 10) {
                    Image(systemName: "magnifyingglass")
                        .font(.system(size: 15, weight: .semibold))
                    TextField(compact ? "Search resources" : "Search images, containers and volumes", text: $store.search)
                        .textFieldStyle(.plain)
                        .font(.system(size: 14, weight: .medium))
                    if !compact {
                        Text("⌘K")
                            .font(.system(size: 11, weight: .bold, design: .rounded))
                            .padding(.horizontal, 7)
                            .padding(.vertical, 4)
                            .background(.white.opacity(0.13), in: RoundedRectangle(cornerRadius: 6))
                    }
                }
                .padding(.horizontal, 15)
                .frame(minWidth: 140, maxWidth: compact ? .infinity : 410)
                .frame(height: 38)
                .background(.black.opacity(0.18), in: RoundedRectangle(cornerRadius: 9))
                .layoutPriority(1)

                Button {
                    settingsPresented = true
                } label: {
                    Image(systemName: "gearshape.fill")
                        .font(.system(size: 19, weight: .semibold))
                        .frame(width: 38, height: 38)
                }
                .buttonStyle(.plain)
                .help("Settings")
                Spacer().frame(width: compact ? 8 : 16)
            }
            .foregroundStyle(.white)
        }
        .frame(height: 64)
    }

    private func sidebar(compact: Bool) -> some View {
        VStack(alignment: .leading, spacing: 8) {
            ForEach(BushelSection.allCases) { section in
                Button {
                    withAnimation(.easeOut(duration: 0.16)) { store.selection = section }
                } label: {
                    HStack(spacing: 14) {
                        Image(systemName: section.symbol)
                            .font(.system(size: 17, weight: .semibold))
                            .frame(width: 24)
                        if !compact {
                            Text(section.rawValue)
                                .font(.system(size: 15, weight: store.selection == section ? .semibold : .medium))
                                .lineLimit(1)
                            Spacer()
                        }
                        if !compact, count(for: section) > 0 {
                            Text("\(count(for: section))")
                                .font(.system(size: 11, weight: .bold, design: .rounded))
                                .foregroundStyle(store.selection == section ? .white : BushelPalette.secondary)
                                .padding(.horizontal, 7)
                                .padding(.vertical, 3)
                                .background(.white.opacity(store.selection == section ? 0.14 : 0.06), in: Capsule())
                        }
                    }
                    .foregroundStyle(store.selection == section ? .white : BushelPalette.secondary)
                    .padding(.horizontal, 15)
                    .frame(height: 46)
                    .background(
                        store.selection == section ? BushelPalette.accent.opacity(0.2) : .clear,
                        in: RoundedRectangle(cornerRadius: 9)
                    )
                    .overlay(alignment: .leading) {
                        if store.selection == section {
                            RoundedRectangle(cornerRadius: 2)
                                .fill(BushelPalette.accent)
                                .frame(width: 3, height: 24)
                        }
                    }
                }
                .buttonStyle(.plain)
                .help(section.rawValue)
            }
            Spacer()
            if compact {
                Circle()
                    .fill(store.engineRunning ? Color.green : BushelPalette.secondary)
                    .frame(width: 9, height: 9)
                    .frame(maxWidth: .infinity)
                    .padding(.bottom, 16)
                    .help(store.engineRunning ? "Engine running" : "Engine stopped")
            } else {
                VStack(alignment: .leading, spacing: 8) {
                    HStack(spacing: 8) {
                        Circle()
                            .fill(store.engineRunning ? Color.green : BushelPalette.secondary)
                            .frame(width: 8, height: 8)
                        Text(store.engineRunning ? "Engine running" : "Engine stopped")
                            .font(.system(size: 12, weight: .semibold))
                    }
                    Text("Bushel for macOS")
                        .font(.system(size: 11))
                        .foregroundStyle(BushelPalette.secondary)
                }
                .padding(15)
            }
        }
        .padding(.horizontal, 12)
        .padding(.top, 22)
        .frame(width: compact ? 72 : 238)
        .background(BushelPalette.sidebar)
    }

    @ViewBuilder
    private func content(compact: Bool) -> some View {
        VStack(spacing: 0) {
            HStack {
                VStack(alignment: .leading, spacing: 4) {
                    Text(store.selection.rawValue)
                        .font(.system(size: 28, weight: .bold, design: .rounded))
                    Text(subtitle)
                        .font(.system(size: 13))
                        .foregroundStyle(BushelPalette.secondary)
                }
                Spacer()
                if store.isLoading {
                    ProgressView().controlSize(.small)
                }
                Button {
                    Task { await store.refresh() }
                } label: {
                    Label("Refresh", systemImage: "arrow.clockwise")
                        .font(.system(size: 13, weight: .semibold))
                }
                .buttonStyle(BushelSecondaryButtonStyle())
            }
            .padding(.horizontal, compact ? 18 : 30)
            .padding(.top, 26)
            .padding(.bottom, 20)

            if let message = store.errorMessage {
                errorBanner(message, compact: compact)
            }

            Group {
                switch store.selection {
                case .containers: ContainersView()
                case .images: ImagesView()
                case .volumes: VolumesView()
                }
            }
            .environmentObject(store)
        }
        .background(BushelPalette.canvas)
    }

    private func statusBar(compact: Bool) -> some View {
        HStack(spacing: 18) {
            HStack(spacing: 8) {
                Image(systemName: store.engineRunning ? "checkmark.circle.fill" : "pause.circle.fill")
                Text(store.engineRunning ? "Engine running" : "Engine stopped")
            }
            .foregroundStyle(store.engineRunning ? .white : BushelPalette.secondary)
            .font(.system(size: 12, weight: .semibold))
            Spacer()
            if !compact {
                Text("\(store.runningCount) running")
                Text("\(store.images.count) images")
                Text("\(store.volumes.count) volumes")
            }
        }
        .foregroundStyle(BushelPalette.secondary)
        .padding(.horizontal, 18)
        .frame(height: 34)
        .background(BushelPalette.sidebar)
        .overlay(alignment: .top) { Rectangle().fill(BushelPalette.line).frame(height: 1) }
    }

    private func brand(showName: Bool) -> some View {
        HStack(spacing: 9) {
            Image(systemName: "shippingbox.fill")
                .font(.system(size: 19, weight: .bold))
            if showName {
                Text("bushel")
                    .font(.system(size: 19, weight: .bold, design: .rounded))
            }
        }
    }

    private func count(for section: BushelSection) -> Int {
        switch section {
        case .containers: store.containers.count
        case .images: store.images.count
        case .volumes: store.volumes.count
        }
    }

    private var subtitle: String {
        switch store.selection {
        case .containers: "Run and manage isolated Linux workloads"
        case .images: "Local OCI images available to Bushel"
        case .volumes: "Persistent storage managed by Bushel"
        }
    }

    private func errorBanner(_ message: String, compact: Bool) -> some View {
        let layout =
            compact
            ? AnyLayout(VStackLayout(alignment: .leading, spacing: 10))
            : AnyLayout(HStackLayout(spacing: 12))

        return layout {
            Image(systemName: "exclamationmark.triangle.fill")
                .foregroundStyle(BushelPalette.accent)
            Text(message.components(separatedBy: .newlines).first ?? message)
                .lineLimit(2)
            Spacer()
            if !store.engineRunning {
                Button("Start engine") { Task { await store.startEngine() } }
                    .buttonStyle(BushelRedButtonStyle())
            }
        }
        .font(.system(size: 12, weight: .medium))
        .padding(12)
        .background(BushelPalette.accent.opacity(0.09), in: RoundedRectangle(cornerRadius: 9))
        .overlay(RoundedRectangle(cornerRadius: 9).stroke(BushelPalette.accent.opacity(0.28)))
        .padding(.horizontal, compact ? 18 : 30)
        .padding(.bottom, 14)
    }
}

private struct ContainersView: View {
    @EnvironmentObject private var store: BushelStore

    var body: some View {
        ScrollView {
            VStack(spacing: 18) {
                LazyVGrid(columns: [GridItem(.adaptive(minimum: 210), spacing: 14)], spacing: 14) {
                    MetricCard(title: "Running containers", value: "\(store.runningCount)", detail: "of \(store.containers.count) total", symbol: "play.fill")
                    MetricCard(title: "Allocated CPU", value: "\(store.allocatedCPUs)", detail: "virtual cores", symbol: "cpu")
                    MetricCard(title: "Allocated memory", value: ByteCountFormatter.bushelString(store.allocatedMemory), detail: "running workloads", symbol: "memorychip")
                }
                filterBar
                ResourceTable(
                    columns: ["Name", "Image", "Status", "CPU", "Ports", "Created", "Actions"],
                    widths: [155, 180, 110, 55, 140, 105, 80]
                ) {
                    ForEach(store.filteredContainers) { container in
                        ContainerRow(container: container)
                    }
                }
                if store.filteredContainers.isEmpty { EmptyResources(section: .containers) }
            }
            .padding(.horizontal, 18)
            .padding(.bottom, 30)
        }
    }

    private var filterBar: some View {
        HStack {
            Image(systemName: "line.3.horizontal.decrease")
                .foregroundStyle(BushelPalette.secondary)
            Text("\(store.filteredContainers.count) containers")
                .font(.system(size: 13, weight: .semibold))
            Spacer()
            Toggle("Only running", isOn: $store.onlyRunning)
                .toggleStyle(.switch)
                .controlSize(.small)
                .tint(BushelPalette.accent)
                .foregroundStyle(BushelPalette.secondary)
        }
        .padding(.horizontal, 14)
        .frame(height: 44)
        .background(BushelPalette.panel, in: RoundedRectangle(cornerRadius: 9))
    }
}

private struct ImagesView: View {
    @EnvironmentObject private var store: BushelStore

    var body: some View {
        ScrollView {
            VStack(spacing: 18) {
                ResourceTable(columns: ["Repository", "Image ID", "Platform", "Size", "Created", "Actions"], widths: [260, 140, 150, 100, 120, 70]) {
                    ForEach(store.filteredImages) { image in ImageRow(image: image) }
                }
                if store.filteredImages.isEmpty { EmptyResources(section: .images) }
            }
            .padding(.horizontal, 18)
            .padding(.bottom, 30)
        }
    }
}

private struct VolumesView: View {
    @EnvironmentObject private var store: BushelStore

    var body: some View {
        ScrollView {
            VStack(spacing: 18) {
                ResourceTable(columns: ["Name", "Type", "Driver", "Format", "Size", "Created", "Actions"], widths: [220, 90, 100, 90, 100, 120, 70]) {
                    ForEach(store.filteredVolumes) { volume in VolumeRow(volume: volume) }
                }
                if store.filteredVolumes.isEmpty { EmptyResources(section: .volumes) }
            }
            .padding(.horizontal, 18)
            .padding(.bottom, 30)
        }
    }
}

private struct MetricCard: View {
    let title: String
    let value: String
    let detail: String
    let symbol: String

    var body: some View {
        HStack(spacing: 14) {
            Image(systemName: symbol)
                .font(.system(size: 18, weight: .semibold))
                .foregroundStyle(BushelPalette.accent)
                .frame(width: 38, height: 38)
                .background(BushelPalette.accent.opacity(0.12), in: RoundedRectangle(cornerRadius: 9))
            VStack(alignment: .leading, spacing: 4) {
                Text(title).font(.system(size: 12, weight: .medium)).foregroundStyle(BushelPalette.secondary)
                HStack(alignment: .firstTextBaseline, spacing: 6) {
                    Text(value).font(.system(size: 19, weight: .bold, design: .rounded))
                    Text(detail).font(.system(size: 11)).foregroundStyle(BushelPalette.secondary)
                }
            }
            Spacer()
        }
        .padding(15)
        .frame(maxWidth: .infinity, minHeight: 78)
        .background(BushelPalette.panel, in: RoundedRectangle(cornerRadius: 10))
        .overlay(RoundedRectangle(cornerRadius: 10).stroke(BushelPalette.line))
    }
}

private struct ResourceTable<Rows: View>: View {
    let columns: [String]
    let widths: [CGFloat]
    @ViewBuilder let rows: () -> Rows

    var body: some View {
        ScrollView(.horizontal) {
            VStack(spacing: 0) {
                Grid(horizontalSpacing: 16) {
                    GridRow {
                        ForEach(Array(columns.enumerated()), id: \.offset) { index, title in
                            Text(title)
                                .font(.system(size: 12, weight: .bold))
                                .foregroundStyle(BushelPalette.secondary)
                                .frame(width: widths[index], alignment: .leading)
                                .gridCellColumns(1)
                        }
                    }
                    .padding(.horizontal, 16)
                    .frame(height: 43)
                    Divider().overlay(BushelPalette.line)
                    rows()
                }
            }
            .fixedSize(horizontal: true, vertical: false)
            .background(BushelPalette.panel)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(BushelPalette.panel, in: RoundedRectangle(cornerRadius: 10))
        .clipShape(RoundedRectangle(cornerRadius: 10))
        .overlay(RoundedRectangle(cornerRadius: 10).stroke(BushelPalette.line))
    }

}

private struct ContainerRow: View {
    @EnvironmentObject private var store: BushelStore
    @State private var confirmsDeletion = false
    let container: ContainerItem

    var body: some View {
        GridRow {
            NameCell(symbol: "shippingbox.fill", name: container.name, detail: container.shortID)
                .tableColumn(width: 155)
            TableText(container.image, accent: true).tableColumn(width: 180)
            StatusPill(running: container.isRunning, text: container.status.state.capitalized)
                .tableColumn(width: 110)
            TableText("\(container.configuration.resources?.cpus ?? 0)").tableColumn(width: 55)
            TableText(container.ports, accent: container.ports != "—").tableColumn(width: 140)
            TableText(container.configuration.creationDate?.bushelRelative ?? "—").tableColumn(width: 105)
            HStack(spacing: 14) {
                Button {
                    Task { await store.toggle(container) }
                } label: {
                    Image(systemName: container.isRunning ? "stop.fill" : "play.fill")
                }
                Button {
                    confirmsDeletion = true
                } label: {
                    Image(systemName: "trash")
                }
            }
            .buttonStyle(.plain)
            .foregroundStyle(BushelPalette.secondary)
            .tableColumn(width: 80)
        }
        .resourceRow()
        .confirmationDialog("Delete \(container.name)?", isPresented: $confirmsDeletion) {
            Button("Delete container", role: .destructive) {
                Task { await store.deleteContainer(container) }
            }
            Button("Cancel", role: .cancel) {}
        } message: {
            Text("This permanently removes the container.")
        }
    }
}

private struct ImageRow: View {
    @EnvironmentObject private var store: BushelStore
    @State private var confirmsDeletion = false
    let image: ImageItem

    var body: some View {
        GridRow {
            NameCell(symbol: "square.3.layers.3d", name: image.name, detail: nil)
                .tableColumn(width: 260)
            TableText(image.shortID).tableColumn(width: 140)
            TableText(image.platform).tableColumn(width: 150)
            TableText(ByteCountFormatter.bushelString(image.size)).tableColumn(width: 100)
            TableText(image.configuration.creationDate?.bushelRelative ?? "—").tableColumn(width: 120)
            Button {
                confirmsDeletion = true
            } label: {
                Image(systemName: "trash")
            }
            .buttonStyle(.plain)
            .foregroundStyle(BushelPalette.secondary)
            .tableColumn(width: 70)
        }
        .resourceRow()
        .confirmationDialog("Delete \(image.name)?", isPresented: $confirmsDeletion) {
            Button("Delete image", role: .destructive) {
                Task { await store.deleteImage(image) }
            }
            Button("Cancel", role: .cancel) {}
        } message: {
            Text("Containers using this image may prevent its removal.")
        }
    }
}

private struct VolumeRow: View {
    @EnvironmentObject private var store: BushelStore
    @State private var confirmsDeletion = false
    let volume: VolumeItem

    var body: some View {
        GridRow {
            NameCell(symbol: "externaldrive.fill", name: volume.name, detail: nil)
                .tableColumn(width: 220)
            TableText(volume.kind).tableColumn(width: 90)
            TableText(volume.configuration.driver).tableColumn(width: 100)
            TableText(volume.configuration.format.uppercased()).tableColumn(width: 90)
            TableText(volume.configuration.sizeInBytes.map(ByteCountFormatter.bushelString) ?? "—")
                .tableColumn(width: 100)
            TableText(volume.configuration.creationDate?.bushelRelative ?? "—").tableColumn(width: 120)
            Button {
                confirmsDeletion = true
            } label: {
                Image(systemName: "trash")
            }
            .buttonStyle(.plain)
            .foregroundStyle(BushelPalette.secondary)
            .tableColumn(width: 70)
        }
        .resourceRow()
        .confirmationDialog("Delete \(volume.name)?", isPresented: $confirmsDeletion) {
            Button("Delete volume", role: .destructive) {
                Task { await store.deleteVolume(volume) }
            }
            Button("Cancel", role: .cancel) {}
        } message: {
            Text("This permanently removes the volume and its data.")
        }
    }
}

private struct NameCell: View {
    let symbol: String
    let name: String
    let detail: String?

    var body: some View {
        HStack(spacing: 10) {
            Image(systemName: symbol)
                .foregroundStyle(BushelPalette.accent)
                .frame(width: 24)
            VStack(alignment: .leading, spacing: 3) {
                Text(name).font(.system(size: 13, weight: .semibold)).lineLimit(1)
                if let detail {
                    Text(detail).font(.system(size: 10, design: .monospaced)).foregroundStyle(BushelPalette.secondary)
                }
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }
}

private struct TableText: View {
    let value: String
    let accent: Bool

    init(_ value: String, accent: Bool = false) {
        self.value = value
        self.accent = accent
    }

    var body: some View {
        Text(value)
            .font(.system(size: 12, weight: accent ? .semibold : .regular))
            .foregroundStyle(accent ? BushelPalette.accent : Color.white.opacity(0.84))
            .lineLimit(2)
            .frame(maxWidth: .infinity, alignment: .leading)
    }
}

private struct StatusPill: View {
    let running: Bool
    let text: String

    var body: some View {
        HStack(spacing: 5) {
            Circle().fill(running ? Color.green : BushelPalette.secondary).frame(width: 6, height: 6)
            Text(text).font(.system(size: 11, weight: .semibold))
        }
        .padding(.horizontal, 8)
        .padding(.vertical, 5)
        .background((running ? Color.green : BushelPalette.secondary).opacity(0.12), in: Capsule())
        .frame(maxWidth: .infinity, alignment: .leading)
    }
}

private struct EmptyResources: View {
    let section: BushelSection

    var body: some View {
        VStack(spacing: 12) {
            Image(systemName: section.symbol)
                .font(.system(size: 30, weight: .light))
                .foregroundStyle(BushelPalette.secondary)
            Text("No \(section.rawValue.lowercased()) found")
                .font(.system(size: 14, weight: .semibold))
            Text("Resources will appear here when they are available in Bushel.")
                .font(.system(size: 12))
                .foregroundStyle(BushelPalette.secondary)
        }
        .frame(maxWidth: .infinity)
        .padding(.vertical, 42)
    }
}

struct SettingsView: View {
    @EnvironmentObject private var store: BushelStore

    var body: some View {
        VStack(alignment: .leading, spacing: 22) {
            HStack(spacing: 12) {
                Image(systemName: "gearshape.fill")
                    .font(.system(size: 22))
                    .foregroundStyle(BushelPalette.accent)
                Text("Bushel Settings")
                    .font(.system(size: 20, weight: .bold, design: .rounded))
            }
            VStack(alignment: .leading, spacing: 8) {
                Text("CLI executable").font(.system(size: 12, weight: .semibold))
                TextField("bushel", text: $store.cliExecutable)
                    .textFieldStyle(.roundedBorder)
                Text("Use `bushel` to resolve it from PATH, or provide an absolute path.")
                    .font(.system(size: 11))
                    .foregroundStyle(BushelPalette.secondary)
            }
            Divider()
            HStack {
                VStack(alignment: .leading, spacing: 3) {
                    Text("Container engine").font(.system(size: 13, weight: .semibold))
                    Text(store.engineRunning ? "Running" : "Stopped")
                        .font(.system(size: 11))
                        .foregroundStyle(BushelPalette.secondary)
                }
                Spacer()
                Button(store.engineRunning ? "Stop" : "Start") {
                    Task {
                        if store.engineRunning { await store.stopEngine() } else { await store.startEngine() }
                    }
                }
                .buttonStyle(BushelRedButtonStyle())
            }
            Spacer()
        }
        .padding(28)
        .frame(width: 500, height: 280)
        .background(BushelPalette.canvas)
        .preferredColorScheme(.dark)
    }
}

private struct BushelRedButtonStyle: ButtonStyle {
    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .font(.system(size: 12, weight: .semibold))
            .foregroundStyle(.white)
            .padding(.horizontal, 12)
            .frame(height: 30)
            .background(BushelPalette.accent.opacity(configuration.isPressed ? 0.72 : 1), in: RoundedRectangle(cornerRadius: 7))
    }
}

private struct BushelSecondaryButtonStyle: ButtonStyle {
    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .foregroundStyle(.white.opacity(configuration.isPressed ? 0.65 : 0.9))
            .padding(.horizontal, 12)
            .frame(height: 32)
            .background(BushelPalette.panelRaised, in: RoundedRectangle(cornerRadius: 7))
            .overlay(RoundedRectangle(cornerRadius: 7).stroke(BushelPalette.line))
    }
}

extension View {
    fileprivate func tableColumn(width: CGFloat) -> some View {
        frame(width: width, alignment: .leading)
    }

    fileprivate func resourceRow() -> some View {
        self
            .padding(.horizontal, 16)
            .frame(minHeight: 62)
            .background(BushelPalette.panel)
            .overlay(alignment: .bottom) { Rectangle().fill(BushelPalette.line).frame(height: 1) }
    }
}
