// Shots kept in memory only while a window shows them (R17), and none lost when Shots stops (R19): whatever is
// still open and was never saved is saved as `… (unsaved).png`.

import Foundation

public final class ShotStore {
    private struct Entry {
        let shot: Shot
        var holders: Int
    }

    private var entries: [ObjectIdentifier: Entry] = [:]

    public init() {}

    /// A window now shows `shot`.
    public func retain(_ shot: Shot) {
        let key = ObjectIdentifier(shot)
        entries[key, default: Entry(shot: shot, holders: 0)].holders += 1
    }

    /// A window showing `shot` closed; the last one lets it go.
    public func release(_ shot: Shot) {
        let key = ObjectIdentifier(shot)
        guard var entry = entries[key] else { return }
        entry.holders -= 1
        entries[key] = entry.holders > 0 ? entry : nil
    }

    public var live: Int { entries.count }

    /// Saves every shown shot that was never saved, into the folder, as `<name> (unsaved).png`. Returns where.
    @discardableResult
    public func saveUnsaved(settings: Settings, disk: Disk) -> [URL] {
        var saved: [URL] = []
        for entry in entries.values where entry.shot.savedTo == nil {
            guard let data = Encoding.data(entry.shot.image, format: .png) else { continue }
            let stem = (FileNaming.name(at: entry.shot.takenAt, format: .png) as NSString).deletingPathExtension
            do {
                try disk.makeFolder(settings.folderURL)
                let url = FileNaming.unique("\(stem) (unsaved).png", in: settings.folderURL, exists: disk.exists)
                try disk.write(data, to: url)
                entry.shot.savedTo = url
                saved.append(url)
            } catch {
                continue
            }
        }
        return saved
    }
}

public enum PinMath {
    /// A pin's opacity after a scroll: between 20 % and 100 %.
    public static func opacity(_ current: Double, scrolledBy delta: Double) -> Double {
        min(1, max(0.2, current + delta * 0.02))
    }

    /// A pin's size: the shot at its real size in points, shrunk to fit 80 % of the screen.
    public static func size(pixels: CGSize, scale: CGFloat, screen: CGSize) -> CGSize {
        let points = CGSize(width: pixels.width / max(scale, 1), height: pixels.height / max(scale, 1))
        let fit = min(1, (screen.width * 0.8) / max(points.width, 1), (screen.height * 0.8) / max(points.height, 1))
        return CGSize(width: (points.width * fit).rounded(), height: (points.height * fit).rounded())
    }
}
