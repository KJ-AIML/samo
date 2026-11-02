// Clips' page in Samo (ADR-005 C5), drawn by Samo with its own widgets.

import Foundation
import SamoKit

public enum ClipsPage {
    /// What the first log says if the worker couldn't be made its own app.
    public static let sharedNote = "Clips couldn't start as its own app, so Paste from Other Apps and Accessibility "
        + "would be Samo's, and every extension's"

    /// The page before anything is known.
    public static func starting() -> [Widget] {
        [
            .section(title: "CLIPBOARD", about: nil),
            .note(text: "Clips keeps what you copy on this Mac. Press ⇧⌘C to find it.", kind: .plain),
        ]
    }
}
