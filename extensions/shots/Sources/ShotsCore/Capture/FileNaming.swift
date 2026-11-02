// What a saved shot is called: `Shot 2026-11-01 at 21.35.12.png`, with ` (2)` if that name is taken.

import Foundation

public enum FileNaming {
    public static func name(at date: Date, format: Settings.Format, timeZone: TimeZone = .current) -> String {
        let f = DateFormatter()
        f.locale = Locale(identifier: "en_US_POSIX")
        f.timeZone = timeZone
        f.dateFormat = "yyyy-MM-dd 'at' HH.mm.ss"
        return "Shot \(f.string(from: date)).\(format == .png ? "png" : "jpg")"
    }

    /// `name` in `folder`, or `name (2)`, `(3)` … if taken.
    public static func unique(_ name: String, in folder: URL, exists: (URL) -> Bool) -> URL {
        let first = folder.appendingPathComponent(name)
        guard exists(first) else { return first }
        let ext = (name as NSString).pathExtension
        let stem = (name as NSString).deletingPathExtension
        for n in 2...999 {
            let candidate = folder.appendingPathComponent("\(stem) (\(n)).\(ext)")
            if !exists(candidate) { return candidate }
        }
        return folder.appendingPathComponent("\(stem) \(UUID().uuidString).\(ext)")
    }
}
