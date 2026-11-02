// The Mac's side of pasting (ADR-005 C3): who is in front, bringing them back, whether Accessibility is allowed, and
// pressing ⌘V, which only Accessibility allows. PasteFlow decides when.

import AppKit
import ApplicationServices
import ClipsCore

enum MacPaste {
    static func flow() -> PasteFlow {
        PasteFlow(mac: PasteFlow.Mac(
            front: { NSWorkspace.shared.frontmostApplication?.processIdentifier },
            activate: { pid in NSRunningApplication(processIdentifier: pid)?.activate() },
            trusted: { AXIsProcessTrusted() },
            pressPaste: pressCommandV,
            after: { seconds, block in DispatchQueue.main.asyncAfter(deadline: .now() + seconds, execute: block) }
        ))
    }

    /// Accessibility for Clips: asks macOS, which shows its prompt the first time.
    static func askForAccessibility() {
        let prompt = kAXTrustedCheckOptionPrompt.takeUnretainedValue() as String
        _ = AXIsProcessTrustedWithOptions([prompt: true] as CFDictionary)
    }

    /// ⌘V, as if pressed. The V key's code on an ANSI keyboard.
    private static func pressCommandV() {
        let source = CGEventSource(stateID: .combinedSessionState)
        for down in [true, false] {
            let event = CGEvent(keyboardEventSource: source, virtualKey: 9, keyDown: down)
            event?.flags = .maskCommand
            event?.post(tap: .cgSessionEventTap)
        }
    }
}
