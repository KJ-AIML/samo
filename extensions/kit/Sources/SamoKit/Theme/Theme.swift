// Samo's palette (`apps/samo-gpui/src/theme.rs`), ported with the same mixes, so an extension's windows look like
// Samo.

import AppKit

public struct RGBA: Equatable {
    public var r: Double
    public var g: Double
    public var b: Double
    public var a: Double

    public init(r: Double, g: Double, b: Double, a: Double) {
        self.r = r
        self.g = g
        self.b = b
        self.a = a
    }

    /// An AppKit colour, in sRGB.
    public init?(_ color: NSColor) {
        guard let c = color.usingColorSpace(.sRGB) else { return nil }
        self.init(r: Double(c.redComponent), g: Double(c.greenComponent), b: Double(c.blueComponent),
                  a: Double(c.alphaComponent))
    }

    public static func hex(_ v: UInt32) -> RGBA {
        RGBA(r: Double((v >> 16) & 0xFF) / 255, g: Double((v >> 8) & 0xFF) / 255, b: Double(v & 0xFF) / 255, a: 1)
    }

    /// `color-mix(in srgb, self p, other)` for two opaque colours.
    func mix(_ other: RGBA, _ p: Double) -> RGBA {
        RGBA(r: r * p + other.r * (1 - p), g: g * p + other.g * (1 - p), b: b * p + other.b * (1 - p), a: 1)
    }

    /// `color-mix(in srgb, self p, transparent)`.
    func fade(_ p: Double) -> RGBA {
        RGBA(r: r, g: g, b: b, a: p)
    }

    public var color: NSColor {
        NSColor(srgbRed: r, green: g, blue: b, alpha: a)
    }
}

private let blue = RGBA.hex(0x243EB5)
private let blueLight = RGBA.hex(0xABC0EF)
private let ink = RGBA.hex(0x242A33)
private let white = RGBA.hex(0xFFFFFF)
private let orange = RGBA.hex(0xF15B2A)
private let orangeInk = RGBA.hex(0xA83B17)
private let black = RGBA.hex(0x000000)

/// The tokens an extension draws with.
public struct Palette: Equatable {
    public var card: RGBA
    public var shell: RGBA
    public var text: RGBA
    public var text2: RGBA
    public var hairline: RGBA
    public var hairlineStrong: RGBA
    public var accent: RGBA
    public var accentText: RGBA
    public var accentWash: RGBA
    public var signalText: RGBA
    public var primaryBg: RGBA
    public var primaryFg: RGBA
    /// The readout's dark glass, in both themes; labels over the screen sit on it.
    public var glass: RGBA
    public var glassLine: RGBA
    public var glassText: RGBA
    public var glassText2: RGBA
    public var phosphor: RGBA

    public static let light = Palette(
        card: white,
        shell: ink.mix(white, 0.06),
        text: ink,
        text2: ink.mix(blueLight, 0.72),
        hairline: ink.fade(0.09),
        hairlineStrong: ink.fade(0.18),
        accent: blue,
        accentText: blue,
        accentWash: blue.mix(white, 0.07),
        signalText: orangeInk,
        primaryBg: ink,
        primaryFg: white,
        glass: ink.mix(black, 0.72),
        glassLine: white.fade(0.14),
        glassText: white,
        glassText2: white.mix(ink.mix(black, 0.72), 0.62),
        phosphor: blueLight
    )

    public static let dark: Palette = {
        let card = ink.mix(black, 0.68)
        let text = white.mix(ink, 0.92)
        return Palette(
            card: card,
            shell: ink.mix(black, 0.54),
            text: text,
            text2: white.mix(ink, 0.62),
            hairline: white.fade(0.09),
            hairlineStrong: white.fade(0.17),
            accent: blue.mix(blueLight, 0.38),
            accentText: blueLight,
            accentWash: blue.mix(card, 0.24),
            signalText: orange.mix(white, 0.70),
            primaryBg: text,
            primaryFg: ink,
            glass: ink.mix(black, 0.18),
            glassLine: white.fade(0.12),
            glassText: white,
            glassText2: white.mix(ink.mix(black, 0.18), 0.58),
            phosphor: blueLight
        )
    }()

    /// The palette for an appearance.
    public static func current(_ appearance: NSAppearance?) -> Palette {
        let dark = appearance?.bestMatch(from: [.darkAqua, .aqua]) == .darkAqua
        return dark ? .dark : .light
    }
}

/// Samo's sizes, in points.
public enum Metrics {
    public static let radiusChip: CGFloat = 6
    public static let radiusControl: CGFloat = 8
    public static let radiusCard: CGFloat = 12
    public static let textMeta: CGFloat = 12.5
    public static let textBody: CGFloat = 14
    public static let textLabel: CGFloat = 11
}
