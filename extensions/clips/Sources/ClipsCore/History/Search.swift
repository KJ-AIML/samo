// Finding an item (ADR-005 C2, Phase 1): every word typed, in any order, without minding case or accents, in its
// text or its files' names.

import Foundation

public enum Search {
    static func fold(_ s: String) -> String {
        s.folding(options: [.caseInsensitive, .diacriticInsensitive, .widthInsensitive], locale: nil)
    }

    public static func matches(_ item: Item, _ query: String) -> Bool {
        let words = fold(query).split(whereSeparator: \.isWhitespace)
        guard !words.isEmpty else { return true }
        let haystack = fold(item.kind == .files ? item.files.map { ($0 as NSString).lastPathComponent }
            .joined(separator: " ") + " " + item.text : item.text)
        return words.allSatisfy { haystack.contains($0) }
    }

    public static func filter(_ items: [Item], _ query: String) -> [Item] {
        items.filter { matches($0, query) }
    }
}
