// Freezing the screen (ADR-004 S1): every display captured at the shortcut through ScreenCaptureKit, before the
// overlay appears; and one window, live, when chosen.

import AppKit
import ScreenCaptureKit
import ShotsCore

/// One display's frozen image.
struct Frozen {
    let screen: NSScreen
    let image: CGImage
    /// Pixels per point.
    let scale: CGFloat
}

enum Screenshotter {
    static func displayID(_ screen: NSScreen) -> CGDirectDisplayID? {
        (screen.deviceDescription[NSDeviceDescriptionKey("NSScreenNumber")] as? NSNumber)?.uint32Value
    }

    /// Every display, as it is now.
    static func freezeAll() async -> [Frozen] {
        guard let content = try? await SCShareableContent.excludingDesktopWindows(false, onScreenWindowsOnly: true)
        else { return [] }
        var frozen: [Frozen] = []
        for screen in NSScreen.screens {
            guard let id = displayID(screen), let display = content.displays.first(where: { $0.displayID == id })
            else { continue }
            let filter = SCContentFilter(display: display, excludingWindows: [])
            let config = SCStreamConfiguration()
            let scale = CGFloat(filter.pointPixelScale)
            config.width = Int(filter.contentRect.width * scale)
            config.height = Int(filter.contentRect.height * scale)
            config.showsCursor = false
            config.captureResolution = .best
            if let image = try? await SCScreenshotManager.captureImage(contentFilter: filter, configuration: config) {
                frozen.append(Frozen(screen: screen, image: image, scale: scale))
            }
        }
        return frozen
    }

    /// The windows a person would pick from: normal, on screen, not Shots' own.
    static func windows() async -> [SCWindow] {
        guard let content = try? await SCShareableContent.excludingDesktopWindows(true, onScreenWindowsOnly: true)
        else { return [] }
        let me = ProcessInfo.processInfo.processIdentifier
        return content.windows.filter {
            $0.windowLayer == 0 && $0.isOnScreen && $0.frame.width > 20 && $0.frame.height > 20
                && $0.owningApplication?.processID != me
        }
    }

    /// One window alone, with its shadow, as it is now.
    static func capture(window: SCWindow, scale: CGFloat) async -> CGImage? {
        let filter = SCContentFilter(desktopIndependentWindow: window)
        let config = SCStreamConfiguration()
        config.width = Int(filter.contentRect.width * CGFloat(filter.pointPixelScale))
        config.height = Int(filter.contentRect.height * CGFloat(filter.pointPixelScale))
        config.showsCursor = false
        config.captureResolution = .best
        config.ignoreShadowsSingleWindow = false
        return try? await SCScreenshotManager.captureImage(contentFilter: filter, configuration: config)
    }
}

/// The clipboard, for this Mac only: Universal Clipboard doesn't send it to your other devices (R16).
