// Reading the text in a picture (ADR-004 S4), on the Mac with Vision's accurate mode: Thai and English always, and
// the Mac's other languages macOS can read. Nothing leaves the Mac (R16).

import CoreGraphics
import Foundation
import Vision

/// A piece of text found in a picture, and where: Vision's box, from 0 to 1, origin bottom-left.
public struct FoundText: Equatable {
    public var text: String
    public var box: CGRect

    public init(text: String, box: CGRect) {
        self.text = text
        self.box = box
    }
}

public enum TextReader {
    /// Thai and English first, then the Mac's preferred languages that Vision reads (by language when the region
    /// differs: `en-GB` reads as `en-US`).
    public static func languages(preferred: [String], supported: [String]) -> [String] {
        var out: [String] = []
        func add(_ language: String) {
            if supported.contains(language), !out.contains(language) { out.append(language) }
        }
        add("th-TH")
        add("en-US")
        for wanted in preferred {
            if supported.contains(wanted) {
                add(wanted)
                continue
            }
            let base = wanted.split(separator: "-").first.map(String.init) ?? wanted
            if let match = supported.first(where: { $0.split(separator: "-").first.map(String.init) == base }) {
                add(match)
            }
        }
        return out
    }

    /// Lines in reading order: top to bottom, and the pieces of one line left to right, joined by a space.
    public static func lines(_ found: [FoundText]) -> [String] {
        var rows: [[FoundText]] = []
        for piece in found.sorted(by: { $0.box.midY > $1.box.midY }) {
            if let last = rows.last?.last {
                let overlap = min(last.box.maxY, piece.box.maxY) - max(last.box.minY, piece.box.minY)
                if overlap >= 0.5 * min(last.box.height, piece.box.height) {
                    rows[rows.count - 1].append(piece)
                    continue
                }
            }
            rows.append([piece])
        }
        return rows.map { row in
            row.sorted { $0.box.minX < $1.box.minX }.map(\.text).joined(separator: " ")
        }.filter { !$0.trimmingCharacters(in: .whitespaces).isEmpty }
    }

    /// The text in `image`, off the main thread.
    public static func read(_ image: CGImage) async -> [String] {
        await withCheckedContinuation { done in
            DispatchQueue.global(qos: .userInitiated).async {
                done.resume(returning: readNow(image))
            }
        }
    }

    /// The text in `image`, here and now (it takes 60 to 150 ms; never on the main thread).
    public static func readNow(_ image: CGImage) -> [String] {
        // Small selections read better a little bigger.
        let side = min(image.width, image.height)
        let input = side < 64 ? (Encoding.scaled(image, by: 2) ?? image) : image
        let request = VNRecognizeTextRequest()
        request.recognitionLevel = .accurate
        request.usesLanguageCorrection = true
        let supported = (try? request.supportedRecognitionLanguages()) ?? []
        request.recognitionLanguages = languages(preferred: Locale.preferredLanguages, supported: supported)
        do {
            try VNImageRequestHandler(cgImage: input).perform([request])
        } catch {
            return []
        }
        let found = (request.results ?? []).compactMap { observation in
            observation.topCandidates(1).first.map { FoundText(text: $0.string, box: observation.boundingBox) }
        }
        return lines(found)
    }
}
