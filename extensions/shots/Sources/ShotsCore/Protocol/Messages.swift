// Samo's extension protocol (ADR-002, protocol 1 with ADR-004's additions), as Shots speaks it: one JSON object per
// line, `type` naming the message. These mirror `crates/samo-ext-protocol` field for field.

import Foundation

public let protocolVersion = 1

/// Samo to Shots.
public enum HostMsg: Equatable {
    case hello(protocolVersion: Int, samoVersion: String, dataDir: String)
    case setting(key: String, value: Bool)
    case action(id: String, row: String?)
    case choice(key: String, value: String)
    case visible(page: Bool)
    case shutdown

    /// One line from Samo; `nil` for anything Shots doesn't understand (it is ignored, never fatal).
    public static func decode(_ line: String) -> HostMsg? {
        guard let data = line.data(using: .utf8),
              let object = try? JSONSerialization.jsonObject(with: data),
              let dict = object as? [String: Any],
              let type = dict["type"] as? String
        else { return nil }
        let text = { (key: String) in dict[key] as? String }
        switch type {
        case "hello":
            return .hello(
                protocolVersion: dict["protocol"] as? Int ?? 0,
                samoVersion: text("samo_version") ?? "",
                dataDir: text("data_dir") ?? ""
            )
        case "setting":
            guard let key = text("key"), let value = dict["value"] as? Bool else { return nil }
            return .setting(key: key, value: value)
        case "action":
            guard let id = text("id") else { return nil }
            return .action(id: id, row: text("row"))
        case "choice":
            guard let key = text("key"), let value = text("value") else { return nil }
            return .choice(key: key, value: value)
        case "visible":
            return .visible(page: dict["page"] as? Bool ?? false)
        case "shutdown":
            return .shutdown
        default:
            return nil
        }
    }
}

public enum Lamp: String {
    case working
    case needsYou = "needs_you"
    case done
    case off
    case error
}

public struct RowAction: Equatable {
    public var id: String
    public var label: String
    public init(id: String, label: String) {
        self.id = id
        self.label = label
    }
}

public struct RowItem: Equatable {
    public var id: String?
    public var lamp: Lamp
    public var title: String
    public var detail: String
    public var trailing: String
    public var tag: String?
    public var actions: [RowAction]

    public init(
        id: String? = nil, lamp: Lamp, title: String, detail: String = "", trailing: String = "",
        tag: String? = nil, actions: [RowAction] = []
    ) {
        self.id = id
        self.lamp = lamp
        self.title = title
        self.detail = detail
        self.trailing = trailing
        self.tag = tag
        self.actions = actions
    }
}

public struct ChoiceOption: Equatable {
    public var id: String
    public var label: String
    public init(id: String, label: String) {
        self.id = id
        self.label = label
    }
}

public enum ButtonKind: String {
    case plain = "default"
    case link
}

public enum NoteKind: String {
    case plain
    case error
}

/// What Shots' page in Samo is built from; Samo draws each with its own parts.
public enum Widget: Equatable {
    case section(title: String, about: String?)
    case rows([RowItem])
    case toggle(key: String, label: String, on: Bool, note: String?)
    case button(action: String, label: String, kind: ButtonKind)
    case note(text: String, kind: NoteKind)
    case choice(key: String, label: String, options: [ChoiceOption], selected: String)

    var json: [String: Any] {
        switch self {
        case let .section(title, about):
            var d: [String: Any] = ["widget": "section", "title": title]
            if let about { d["about"] = about }
            return d
        case let .rows(items):
            return ["widget": "rows", "items": items.map(Widget.rowJSON)]
        case let .toggle(key, label, on, note):
            var d: [String: Any] = ["widget": "switch", "key": key, "label": label, "on": on]
            if let note { d["note"] = note }
            return d
        case let .button(action, label, kind):
            return ["widget": "button", "action": action, "label": label, "kind": kind.rawValue]
        case let .note(text, kind):
            return ["widget": "note", "text": text, "kind": kind.rawValue]
        case let .choice(key, label, options, selected):
            return [
                "widget": "choice", "key": key, "label": label, "selected": selected,
                "options": options.map { ["id": $0.id, "label": $0.label] },
            ]
        }
    }

    static func rowJSON(_ item: RowItem) -> [String: Any] {
        var d: [String: Any] = [
            "lamp": item.lamp.rawValue, "title": item.title, "detail": item.detail, "trailing": item.trailing,
        ]
        if let id = item.id { d["id"] = id }
        if let tag = item.tag { d["tag"] = tag }
        if !item.actions.isEmpty {
            d["actions"] = item.actions.map { ["id": $0.id, "label": $0.label] }
        }
        return d
    }
}

/// Shots to Samo.
public enum HelperMsg: Equatable {
    case hello
    case view([Widget])
    case badge(count: Int, summary: String)
    case log(event: String, detail: String)
    case health(ok: Bool, message: String)
    case capturing
    case captured

    var json: [String: Any] {
        switch self {
        case .hello:
            return ["type": "hello", "protocol": protocolVersion]
        case let .view(sections):
            return ["type": "view", "sections": sections.map(\.json)]
        case let .badge(count, summary):
            return ["type": "badge", "count": count, "summary": summary]
        case let .log(event, detail):
            return ["type": "log", "event": event, "detail": detail]
        case let .health(ok, message):
            return ["type": "health", "ok": ok, "message": message]
        case .capturing:
            return ["type": "capturing"]
        case .captured:
            return ["type": "captured"]
        }
    }

    /// One line, without its newline; compact JSON never holds a raw newline.
    public func line() -> String? {
        guard let data = try? JSONSerialization.data(withJSONObject: json, options: [.sortedKeys]) else {
            return nil
        }
        return String(data: data, encoding: .utf8)
    }
}
