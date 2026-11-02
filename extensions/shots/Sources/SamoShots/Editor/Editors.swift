// The open editors: one per shot (opening it again brings that one forward), each keeping its shot alive while it
// is open (R17) and handing over its unsaved edits for shutdown (R19). When the last one closes, the app you were
// in gets the keyboard back.

import AppKit
import SamoKit
import ShotsCore
import ShotsUI

final class Editors {
    private let store: ShotStore
    private let settings: () -> Settings
    let dragFolder: URL
    private let saved: (Shot, ExportResult) -> Void
    private let pin: (Shot) -> Void
    private var open: [EditorWindow] = []
    /// The app in front before the first editor opened.
    private var cameFrom: NSRunningApplication?

    init(store: ShotStore, settings: @escaping () -> Settings, dragFolder: URL,
         saved: @escaping (Shot, ExportResult) -> Void, pin: @escaping (Shot) -> Void) {
        self.store = store
        self.settings = settings
        self.dragFolder = dragFolder
        self.saved = saved
        self.pin = pin
    }

    private func hooks() -> EditorHooks {
        EditorHooks(
            settings: settings, clipboard: SystemClipboard(), disk: FileDisk(), dragFolder: dragFolder,
            saved: saved, pin: pin,
            opened: { [weak self] shot, edits in
                self?.store.retain(shot)
                self?.store.setEdits(for: shot, edits)
            },
            closed: { [weak self] editor in self?.closed(editor) }
        )
    }

    var count: Int { open.count }

    /// Opens `shot` in the editor, or brings its editor forward.
    func edit(_ shot: Shot) {
        if let editor = open.first(where: { $0.shot === shot }) {
            editor.bringForward()
            return
        }
        remember()
        let editor = EditorWindow(shot: shot, hooks: hooks())
        open.append(editor)
        editor.show()
    }

    /// Opens a saved shot from its file, or brings forward the editor already showing it.
    func edit(file url: URL) {
        if let editor = open.first(where: { $0.shot.savedTo?.standardizedFileURL == url.standardizedFileURL }) {
            editor.bringForward()
            return
        }
        guard let loaded = Encoding.load(url, fallbackScale: NSScreen.main?.backingScaleFactor ?? 2) else {
            Note.show("Couldn't open that shot", problem: true)
            return
        }
        let shot = Shot(image: loaded.image, scale: loaded.scale, takenAt: Date())
        shot.savedTo = url
        edit(shot)
    }

    /// Closes every editor now, after shutdown has saved their unsaved edits beside their shots.
    func closeAll() {
        for editor in open {
            editor.close()
        }
        open.removeAll()
    }

    private func remember() {
        guard open.isEmpty else { return }
        let front = NSWorkspace.shared.frontmostApplication
        cameFrom = front?.processIdentifier == ProcessInfo.processInfo.processIdentifier ? nil : front
    }

    private func closed(_ editor: EditorWindow) {
        store.setEdits(for: editor.shot, nil)
        store.release(editor.shot)
        open.removeAll { $0 === editor }
        if open.isEmpty, NSApp.isActive, let app = cameFrom, !app.isTerminated {
            app.activate()
        }
        if open.isEmpty { cameFrom = nil }
    }
}
