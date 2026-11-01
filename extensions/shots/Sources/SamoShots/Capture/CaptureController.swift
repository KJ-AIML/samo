// One capture, start to finish (ADR-004 S1, R19): check Screen Recording, freeze every display, let the person
// choose, then copy and save before any window opens, open the editor or a pin, and say what happened.

import AppKit
import ScreenCaptureKit
import ShotsCore
import ShotsUI

final class CaptureController {
    enum Mode {
        case area
        case screen
        case window
    }

    private let permission: Permission
    private let settings: () -> Settings
    private let out: Output
    private let open: (Shot, Settings.Then) -> Void
    private let onPermission: () -> Void
    private let onShot: (Shot, CaptureOutcome) -> Void
    private let clipboard = SystemClipboard()
    private let disk = FileDisk()

    private var overlay: OverlaySession?
    private var busy = false
    private var fromPage = false

    init(
        permission: Permission,
        settings: @escaping () -> Settings,
        out: Output,
        open: @escaping (Shot, Settings.Then) -> Void,
        onPermission: @escaping () -> Void,
        onShot: @escaping (Shot, CaptureOutcome) -> Void
    ) {
        self.permission = permission
        self.settings = settings
        self.out = out
        self.open = open
        self.onPermission = onPermission
        self.onShot = onShot
    }

    /// Starts a capture. From Samo's page, Samo is asked to step aside first, so it isn't in the shot.
    func start(_ mode: Mode, fromPage: Bool) {
        guard !busy else { return }
        guard permission.state == .allowed else {
            permission.request()
            onPermission()
            return
        }
        busy = true
        self.fromPage = fromPage
        if fromPage {
            out.send(.capturing)
            // Samo hides its windows on its next wake, within about a quarter of a second.
            DispatchQueue.main.asyncAfter(deadline: .now() + 0.45) { self.run(mode) }
        } else {
            run(mode)
        }
    }

    private func run(_ mode: Mode) {
        let started = Date()
        Task { @MainActor [weak self] in
            let frozen = await Screenshotter.freezeAll()
            guard let self else { return }
            guard !frozen.isEmpty else {
                self.end(note: "Shots couldn't capture the screen. Is Screen Recording allowed for Shots?",
                         problem: true)
                return
            }
            switch mode {
            case .screen:
                let pointer = NSEvent.mouseLocation
                guard let f = frozen.first(where: { $0.screen.frame.contains(pointer) }) ?? frozen.first else {
                    self.end(note: nil, problem: false)
                    return
                }
                self.done(Shot(image: f.image, scale: f.scale, takenAt: started, origin: f.screen.frame.origin),
                          since: Date())
            case .area, .window:
                let session = OverlaySession(
                    startIn: mode == .window ? .window : .area,
                    onArea: { [weak self] frozen, rect, view in
                        self?.area(frozen, rect, view: view, takenAt: started)
                    },
                    onWindow: { [weak self] window, scale in
                        self?.window(window, scale: scale, takenAt: started)
                    },
                    onCancel: { [weak self] in self?.end(note: nil, problem: false) }
                )
                self.overlay = session
                session.show(frozen)
                let ms = Int(Date().timeIntervalSince(started) * 1000)
                FileHandle.standardError.write(Data("shots: overlay up in \(ms) ms\n".utf8))
            }
        }
    }

    private func area(_ frozen: Frozen, _ rect: CGRect, view: OverlayView, takenAt: Date) {
        let released = Date()
        let pixels = Selection.imageRect(rect, viewHeight: view.bounds.height, scale: frozen.scale)
            .intersection(CGRect(x: 0, y: 0, width: frozen.image.width, height: frozen.image.height))
        overlay?.close()
        guard let crop = frozen.image.cropping(to: pixels) else {
            end(note: "Shots couldn't cut out that area", problem: true)
            return
        }
        let origin = CGPoint(x: frozen.screen.frame.minX + rect.minX, y: frozen.screen.frame.minY + rect.minY)
        done(Shot(image: crop, scale: frozen.scale, takenAt: takenAt, origin: origin), since: released)
    }

    private func window(_ window: SCWindow, scale: CGFloat, takenAt: Date) {
        let clicked = Date()
        overlay?.close()
        Task { @MainActor [weak self] in
            let image = await Screenshotter.capture(window: window, scale: scale)
            guard let self else { return }
            guard let image else {
                self.end(note: "Shots couldn't capture that window", problem: true)
                return
            }
            let primaryHeight = NSScreen.screens.first?.frame.height ?? 0
            let origin = CGPoint(x: window.frame.minX, y: primaryHeight - window.frame.maxY)
            self.done(Shot(image: image, scale: scale, takenAt: takenAt, origin: origin), since: clicked)
        }
    }

    /// Copy and save, then open the editor or a pin (R19). `since` is when the person let go, for the timing log.
    private func done(_ shot: Shot, since: Date) {
        let settings = settings()
        let outcome = AfterCapture.run(shot, settings: settings, clipboard: clipboard, disk: disk, open: open)
        if settings.sound {
            NSSound(named: "Grab")?.play()
        }
        if outcome.then == .edit {
            let ms = Int(Date().timeIntervalSince(since) * 1000)
            FileHandle.standardError.write(Data("shots: editor up in \(ms) ms\n".utf8))
        }
        onShot(shot, outcome)
        end(note: outcome.note, problem: !outcome.problems.isEmpty)
    }

    private func end(note: String?, problem: Bool) {
        overlay?.close()
        overlay = nil
        busy = false
        if fromPage {
            out.send(.captured)
            fromPage = false
        }
        if let note {
            Note.show(note, problem: problem)
        }
    }
}
