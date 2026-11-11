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

/// A conservative fast path for `.dockerignore` files that exclude everything
/// and then opt specific root paths back in. Complex negations fall back to the
/// shim's full matcher so this optimization cannot change build results.
struct DockerignoreRootAllowlist {
    let names: Set<String>

    init?(_ data: Data) {
        guard let contents = String(data: data, encoding: .utf8) else { return nil }
        var excludesAll = false
        var included = Set<String>()

        for rawLine in contents.components(separatedBy: .newlines) {
            let line = rawLine.trimmingCharacters(in: .whitespacesAndNewlines)
            if line.isEmpty || line.hasPrefix("#") { continue }
            if line == "*" { excludesAll = true }
            guard line.hasPrefix("!") else { continue }

            var pattern = String(line.dropFirst())
            while pattern.hasPrefix("./") || pattern.hasPrefix("/") {
                pattern.removeFirst(pattern.hasPrefix("./") ? 2 : 1)
            }
            let components = pattern.split(separator: "/").map(String.init)
            guard !components.contains(".") && !components.contains(".."),
                let first = components.first,
                first != ".",
                first != "..",
                !first.contains(where: { "*?[]\\".contains($0) })
            else { return nil }
            included.insert(first)
        }

        guard excludesAll else { return nil }
        names = included
    }
}
