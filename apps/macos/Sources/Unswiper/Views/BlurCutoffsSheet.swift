// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Rob Ranslam

import SwiftUI

/// "Blur Cutoffs…": the blurry-below sharpness score for Low, Normal and High (same texts as Linux).
struct BlurCutoffsSheet: View {
    @EnvironmentObject private var state: AppState
    @Environment(\.dismiss) private var dismiss
    @State private var values = BlurCutoffs.default

    static let explanation = """
    Each photo gets a sharpness score: how strongly its finest detail changes from pixel to pixel \
    (Laplacian variance, measured on a copy at most 1024 pixels wide). Higher means sharper. \
    Clearly blurry photos usually score in single digits; sharp photos usually score in the \
    tens or more, detailed ones in the hundreds or thousands.

    A photo is suggested as blurry when its score is below the number for the chosen \
    sensitivity. Lower numbers flag fewer photos. Low must be below Normal, and Normal below High.

    Photos also count as blurry when they look shaken (score below 4 × the number) or are much \
    softer than the best shot of a burst. Saving re-applies the marks from the saved scores; \
    nothing is scanned again.
    """

    var body: some View {
        VStack(alignment: .leading, spacing: 14) {
            Text("Blur Cutoffs").font(.title2.bold())
            Text(Self.explanation)
                .font(.callout)
                .foregroundStyle(.secondary)
                .fixedSize(horizontal: false, vertical: true)
            Grid(alignment: .leading, horizontalSpacing: 12, verticalSpacing: 8) {
                row(.low, $values.low)
                row(.normal, $values.normal)
                row(.high, $values.high)
            }
            Text(values.rounded.problem
                 ?? "In use now (\(state.blurSensitivity.rawValue)): blurry below \(String(format: "%.1f", values.rounded.value(state.blurSensitivity))).")
                .font(.caption)
                .foregroundStyle(values.rounded.problem == nil ? Color.secondary : Color.red)
            HStack {
                Button("Reset to Defaults") { values = .default }
                    .help("Low 7, Normal 13, High 25")
                Spacer()
                Button("Cancel", role: .cancel) { dismiss() }
                    .keyboardShortcut(.cancelAction)
                Button("Save") {
                    if state.setBlurCutoffs(values) == nil { dismiss() }
                }
                .keyboardShortcut(.defaultAction)
                .disabled(values.rounded.problem != nil)
            }
        }
        .padding(20)
        .frame(width: 480)
        .onAppear { values = state.blurCutoffs }
    }

    private func row(_ s: BlurRules.Sensitivity, _ value: Binding<Double>) -> some View {
        GridRow {
            Text("\(s.label): blurry below")
            TextField(s.label, value: value, format: .number.precision(.fractionLength(0...1)))
                .frame(width: 80)
                .multilineTextAlignment(.trailing)
            Stepper("", value: value, in: 0.1...BlurCutoffs.max, step: 0.5)
                .labelsHidden()
        }
    }
}
