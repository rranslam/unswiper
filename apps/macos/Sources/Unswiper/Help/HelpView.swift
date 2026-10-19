// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Rob Ranslam

import SwiftUI

struct HelpView: View {
    @ObservedObject private var router = HelpRouter.shared
    @State private var search = ""

    private var filtered: [HelpTopic] {
        let q = search.trimmingCharacters(in: .whitespaces)
        guard !q.isEmpty else { return HelpLibrary.topics }
        return HelpLibrary.topics.filter {
            $0.title.localizedCaseInsensitiveContains(q) || $0.markdown.localizedCaseInsensitiveContains(q)
        }
    }

    var body: some View {
        NavigationSplitView {
            List(filtered, selection: $router.selection) { topic in
                Label(topic.title, systemImage: icon(for: topic.id)).tag(topic.id)
            }
            .navigationSplitViewColumnWidth(min: 200, ideal: 230)
            .overlay {
                if filtered.isEmpty {
                    Text("No topics match \u{201C}\(search)\u{201D}").foregroundStyle(.secondary).padding()
                }
            }
        } detail: {
            if let id = router.selection, let topic = HelpLibrary.topics.first(where: { $0.id == id }) {
                ScrollView {
                    MarkdownDocument(markdown: topic.markdown, highlight: search)
                        .frame(maxWidth: 680, alignment: .leading)
                        .padding(28)
                        .frame(maxWidth: .infinity, alignment: .leading)
                }
                .id(topic.id)
            } else {
                Text("Choose a topic on the left.").foregroundStyle(.secondary)
            }
        }
        .searchable(text: $search, placement: .sidebar, prompt: "Search Help")
        .onChange(of: search) { _, _ in
            if let sel = router.selection, !filtered.contains(where: { $0.id == sel }) {
                router.selection = filtered.first?.id
            }
        }
        .frame(minWidth: 760, minHeight: 500)
    }

    private func icon(for id: String) -> String {
        switch id {
        case let s where s.contains("getting-started"): return "star"
        case let s where s.contains("visits"): return "rectangle.stack"
        case let s where s.contains("reducing"): return "checkmark.circle"
        case let s where s.contains("review-commit"): return "tray.and.arrow.down"
        case let s where s.contains("restore"): return "arrow.uturn.backward"
        case let s where s.contains("live-photos"): return "livephoto"
        case let s where s.contains("keyboard"): return "keyboard"
        case let s where s.contains("install"): return "arrow.down.app"
        case let s where s.contains("privacy"): return "lock"
        case let s where s.contains("troubleshooting"): return "wrench.and.screwdriver"
        case let s where s.contains("about"): return "info.circle"
        case let s where s.contains("google-photos"): return "icloud.and.arrow.down"
        default: return "doc.text"
        }
    }
}

/// Minimal block-level Markdown renderer: headings, paragraphs, bullet and
/// numbered lists, fenced code. Inline styling via AttributedString(markdown:).
struct MarkdownDocument: View {
    let markdown: String
    var highlight: String = ""

    private enum Block: Hashable {
        case heading(Int, String)
        case paragraph(String)
        case bullet(String)
        case numbered(String, String)
        case code(String)
    }

    private var blocks: [Block] {
        var out: [Block] = []
        var para: [String] = []
        var code: [String]? = nil
        func flush() { if !para.isEmpty { out.append(.paragraph(para.joined(separator: " "))); para = [] } }
        for raw in markdown.components(separatedBy: "\n") {
            let line = raw.trimmingCharacters(in: .whitespaces)
            if line.hasPrefix("```") {
                if let c = code { out.append(.code(c.joined(separator: "\n"))); code = nil } else { flush(); code = [] }
                continue
            }
            if code != nil { code!.append(raw); continue }
            if line.isEmpty { flush(); continue }
            if line.hasPrefix("#") {
                let level = line.prefix { $0 == "#" }.count
                flush(); out.append(.heading(level, String(line.dropFirst(level)).trimmingCharacters(in: .whitespaces)))
            } else if line.hasPrefix("- ") || line.hasPrefix("* ") {
                flush(); out.append(.bullet(String(line.dropFirst(2))))
            } else if let dot = line.firstIndex(of: "."), line[..<dot].allSatisfy(\.isNumber), !line[..<dot].isEmpty,
                      line[line.index(after: dot)...].hasPrefix(" ") {
                flush(); out.append(.numbered(String(line[..<dot]), String(line[line.index(dot, offsetBy: 2)...])))
            } else {
                para.append(line)
            }
        }
        flush()
        if let c = code { out.append(.code(c.joined(separator: "\n"))) }
        return out
    }

    private func inline(_ s: String) -> AttributedString {
        var a = (try? AttributedString(markdown: s, options: .init(interpretedSyntax: .inlineOnlyPreservingWhitespace)))
            ?? AttributedString(s)
        let q = highlight.trimmingCharacters(in: .whitespaces)
        if !q.isEmpty {
            var searchStart = a.startIndex
            while searchStart < a.endIndex,
                  let r = a[searchStart...].range(of: q, options: .caseInsensitive) {
                a[r].backgroundColor = .yellow.opacity(0.4)
                searchStart = r.upperBound
            }
        }
        return a
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            ForEach(Array(blocks.enumerated()), id: \.offset) { _, block in
                switch block {
                case .heading(let level, let text):
                    Text(inline(text))
                        .font(level == 1 ? .largeTitle.bold() : level == 2 ? .title2.bold() : .headline)
                        .padding(.top, level == 1 ? 0 : 8)
                case .paragraph(let text):
                    Text(inline(text)).fixedSize(horizontal: false, vertical: true)
                case .bullet(let text):
                    HStack(alignment: .firstTextBaseline, spacing: 8) {
                        Text("•")
                        Text(inline(text)).fixedSize(horizontal: false, vertical: true)
                    }.padding(.leading, 8)
                case .numbered(let n, let text):
                    HStack(alignment: .firstTextBaseline, spacing: 8) {
                        Text("\(n).").monospacedDigit().frame(minWidth: 18, alignment: .trailing)
                        Text(inline(text)).fixedSize(horizontal: false, vertical: true)
                    }.padding(.leading, 8)
                case .code(let text):
                    Text(text).font(.system(.body, design: .monospaced))
                        .padding(10).frame(maxWidth: .infinity, alignment: .leading)
                        .background(.quaternary, in: RoundedRectangle(cornerRadius: 6))
                }
            }
        }
        .textSelection(.enabled)
        .font(.body)
        .lineSpacing(3)
    }
}
