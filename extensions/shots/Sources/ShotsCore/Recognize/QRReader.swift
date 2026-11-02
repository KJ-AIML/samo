// Reading QR codes in a picture (ADR-004 S4), on the Mac with Vision. Each code's text, once, in the order found.

import CoreGraphics
import Foundation
import Vision

public enum QRReader {
    /// The QR codes' text in `image`, off the main thread.
    public static func read(_ image: CGImage) async -> [String] {
        await withCheckedContinuation { done in
            DispatchQueue.global(qos: .utility).async {
                done.resume(returning: readNow(image))
            }
        }
    }

    public static func readNow(_ image: CGImage) -> [String] {
        let request = VNDetectBarcodesRequest()
        request.symbologies = [.qr]
        do {
            try VNImageRequestHandler(cgImage: image).perform([request])
        } catch {
            return []
        }
        var codes: [String] = []
        for payload in (request.results ?? []).compactMap(\.payloadStringValue) where !codes.contains(payload) {
            codes.append(payload)
        }
        return codes
    }
}
