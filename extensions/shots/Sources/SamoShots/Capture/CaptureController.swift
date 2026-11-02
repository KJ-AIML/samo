// One capture, start to finish (ADR-004 S1, S4, R19): check Screen Recording, freeze every display, let the person
// choose, then copy and save before any window opens, open the editor or a pin, and say what happened. Reading
// text, picking a colour and measuring use the same frozen screen and save nothing. Scrolling capture chooses its
// area on the frozen screen, then watches it live while you scroll.

import AppKit
import ScreenCaptureKit
import SamoKit
import ShotsCore
import ShotsUI

final class CaptureController {
    enum Mode {
        case area
        case screen
        case window
        case text
        case colour
        case ruler
        case scroll
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
    private var scrolling: ScrollSession?
    private var busy = false
    private let watchdog = Watchdog()
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
        let token = waitForScreen()
        Task { @MainActor [weak self] in
            let frozen = await Screenshotter.freezeAll()
            guard let self, self.watchdog.answered(token) else { return }
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
            case .area, .window, .text, .colour, .ruler, .scroll:
                let start: OverlayMode = [Mode.window: .window, .text: .text, .colour: .colour, .ruler: .ruler,
                                          .scroll: .scroll][mode] ?? .area
                let session = OverlaySession(
                    startIn: start,
                    onArea: { [weak self] frozen, rect, view, overlayMode in
                        if overlayMode == .text {
                            self?.read(frozen, rect, view: view)
                        } else if overlayMode == .scroll {
                            self?.scroll(frozen, rect, takenAt: started)
                        } else {
                            self?.area(frozen, rect, view: view, takenAt: started)
                        }
                    },
                    onWindow: { [weak self] window, scale in
                        self?.window(window, scale: scale, takenAt: started)
                    },
                    onColour: { [weak self] colour, css in self?.colour(colour, css: css) },
                    onMeasure: { [weak self] size in self?.measured(size) },
                    onCancel: { [weak self] in self?.end(note: nil, problem: false) }
                )
                self.overlay = session
                session.show(frozen)
                let ms = Int(Date().timeIntervalSince(started) * 1000)
                FileHandle.standardError.write(Data("shots: overlay up in \(ms) ms\n".utf8))
            }
        }
    }

    /// Starts waiting on macOS for the screen. If it never answers, the capture ends after `Watchdog.limit` with a
    /// note, so Shots isn't left busy, ignoring every shortcut.
    private func waitForScreen() -> Int {
        let token = watchdog.begin()
        DispatchQueue.main.asyncAfter(deadline: .now() + Watchdog.limit) { [weak self] in
            guard let self, self.watchdog.expired(token) else { return }
            self.end(note: "macOS didn't hand Shots the screen. Try again, or restart Shots from its page in Samo.",
                     problem: true)
        }
        return token
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

    /// T: the selection's text is copied, and a QR code in it shows on the card. Nothing is saved or listed.
    private func read(_ frozen: Frozen, _ rect: CGRect, view: OverlayView) {
        let pixels = Selection.imageRect(rect, viewHeight: view.bounds.height, scale: frozen.scale)
            .intersection(CGRect(x: 0, y: 0, width: frozen.image.width, height: frozen.image.height))
        overlay?.close()
        guard let crop = frozen.image.cropping(to: pixels) else {
            end(note: "Shots couldn't cut out that area", problem: true)
            return
        }
        let pointer = NSEvent.mouseLocation
        let reading = Flag()
        // The first reading on a Mac can take a while (macOS loads its models once): say so.
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.4) {
            if !reading.done { Note.show("Reading text…", problem: false) }
        }
        let clipboard = clipboard
        Task { @MainActor [weak self] in
            let outcome = await Reading.run(crop, clipboard: clipboard)
            reading.done = true
            guard let self else { return }
            if let code = outcome.codes.first {
                ResultCard(.code(code), clipboard: clipboard).show(near: pointer)
            }
            self.end(note: outcome.note, problem: outcome.problem)
        }
    }

    /// S: the area chosen, watch it live while the person scrolls; the stitched picture is then a shot like any
    /// other (copied, saved, opened).
    private func scroll(_ frozen: Frozen, _ rect: CGRect, takenAt: Date) {
        overlay?.close()
        let area = rect.offsetBy(dx: frozen.screen.frame.minX, dy: frozen.screen.frame.minY)
        let session = ScrollSession(screen: frozen.screen, area: area, scale: frozen.scale) { [weak self] image, reason in
            guard let self else { return }
            self.scrolling = nil
            guard let image else {
                self.end(note: "Nothing was captured", problem: true)
                return
            }
            self.done(Shot(image: image, scale: frozen.scale, takenAt: takenAt, origin: area.origin), since: Date())
            if let note = reason.note { Note.show(note, problem: false) }
        }
        scrolling = session
        let token = waitForScreen()
        Task { @MainActor [weak self] in
            let started = await session.start()
            guard let self, self.watchdog.answered(token) else {
                session.cancel()
                return
            }
            if !started {
                self.scrolling = nil
                self.end(note: "Shots couldn't watch that area. Is Screen Recording allowed for Shots?", problem: true)
            }
        }
    }

    /// Off or quitting while scrolling: it stops, and nothing half-made is kept (it isn't a shot until it ends).
    func stop() {
        scrolling?.cancel()
        scrolling = nil
    }

    /// C: the colour under the click, as hex (or `rgb()` with ⇧).
    private func colour(_ colour: Pixel, css: Bool) {
        overlay?.close()
        let text = css ? colour.css : colour.hex
        if clipboard.put(text: text) {
            end(note: "Copied \(text)", problem: false)
        } else {
            end(note: "Couldn't copy the colour", problem: true)
        }
    }

    /// R: a measurement is copied; the ruler stays until Esc.
    private func measured(_ size: String) {
        if clipboard.put(text: size) {
            Note.show("Copied \(size)", problem: false)
        } else {
            Note.show("Couldn't copy the size", problem: true)
        }
    }

    private func window(_ window: SCWindow, scale: CGFloat, takenAt: Date) {
        let clicked = Date()
        overlay?.close()
        let token = waitForScreen()
        Task { @MainActor [weak self] in
            let image = await Screenshotter.capture(window: window, scale: scale)
            guard let self, self.watchdog.answered(token) else { return }
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

/// Whether something finished, shared between a timer and the work it watches.
private final class Flag {
    var done = false
}
