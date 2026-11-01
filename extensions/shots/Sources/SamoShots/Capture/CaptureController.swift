// One capture, start to finish (ADR-004 S1, R19): check Screen Recording, freeze every display, let the person
// choose, then copy and save before any window opens, and say what happened.

import AppKit
import ScreenCaptureKit
import ShotsCore

final class CaptureController {
    enum Mode {
        case area
        case screen
        case window
    }

    private let permission: Permission
    private let settings: () -> Settings
    private let out: Output
    private let openPin: (Shot) -> Void
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
        openPin: @escaping (Shot) -> Void,
        onPermission: @escaping () -> Void,
        onShot: @escaping (Shot, CaptureOutcome) -> Void
    ) {
        self.permission = permission
        self.settings = settings
        self.out = out
        self.openPin = openPin
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
        Task { @MainActor in
            let frozen = await Screenshotter.freezeAll()
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
                self.done(Shot(image: f.image, scale: f.scale, takenAt: started))
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
        let pixels = Selection.imageRect(rect, viewHeight: view.bounds.height, scale: frozen.scale)
            .intersection(CGRect(x: 0, y: 0, width: frozen.image.width, height: frozen.image.height))
        overlay?.close()
        guard let crop = frozen.image.cropping(to: pixels) else {
            end(note: "Shots couldn't cut out that area", problem: true)
            return
        }
        done(Shot(image: crop, scale: frozen.scale, takenAt: takenAt))
    }

    private func window(_ window: SCWindow, scale: CGFloat, takenAt: Date) {
        overlay?.close()
        Task { @MainActor in
            guard let image = await Screenshotter.capture(window: window, scale: scale) else {
                self.end(note: "Shots couldn't capture that window", problem: true)
                return
            }
            self.done(Shot(image: image, scale: scale, takenAt: takenAt))
        }
    }

    private func done(_ shot: Shot) {
        let settings = settings()
        let outcome = AfterCapture.run(shot, settings: settings, clipboard: clipboard, disk: disk, openPin: openPin)
        if settings.sound {
            NSSound(named: "Grab")?.play()
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
