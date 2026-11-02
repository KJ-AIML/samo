// The protocol's pipes: lines in from Samo on a background thread, lines out to Samo in order. A write to a
// closed pipe fails quietly (Samo has gone; stdin's end will stop the extension).

import Foundation

/// Where an extension's messages go.
public protocol Output: AnyObject {
    func send(_ msg: HelperMsg)
}

/// Writes messages to a file handle (stdout), one line each, in order.
public final class LineWriter: Output {
    private let handle: FileHandle
    private let queue = DispatchQueue(label: "dev.talvyn.samo.ext.out")

    public init(handle: FileHandle) {
        self.handle = handle
    }

    public func send(_ msg: HelperMsg) {
        guard let line = msg.line(), let data = (line + "\n").data(using: .utf8) else { return }
        queue.async { [handle] in
            try? handle.write(contentsOf: data)
        }
    }
}

/// Collects messages, for tests.
public final class MemoryOutput: Output {
    public private(set) var sent: [HelperMsg] = []
    public init() {}
    public func send(_ msg: HelperMsg) {
        sent.append(msg)
    }
}

/// Reads lines from a file handle (stdin) on its own thread. A line longer than 64 KB is dropped, as Samo drops its
/// own (R8).
public enum LineReader {
    public static let maxLine = 64 * 1024

    public static func start(handle: FileHandle, onLine: @escaping (String) -> Void, onEnd: @escaping () -> Void) {
        let thread = Thread {
            var pending = Data()
            var dropping = false
            while true {
                let chunk = handle.availableData
                if chunk.isEmpty {
                    onEnd()
                    return
                }
                pending.append(chunk)
                while let newline = pending.firstIndex(of: 0x0A) {
                    let line = pending[pending.startIndex..<newline]
                    pending.removeSubrange(pending.startIndex...newline)
                    if dropping {
                        dropping = false
                    } else if let text = String(data: line, encoding: .utf8) {
                        onLine(text)
                    }
                }
                if pending.count > maxLine {
                    pending.removeAll()
                    dropping = true
                }
            }
        }
        thread.name = "samo-ext-in"
        thread.start()
    }
}
