// Global shortcuts through Carbon's `RegisterEventHotKey`, which needs no Accessibility permission. A shortcut
// macOS refuses is reported, never forced (R18).

import Carbon.HIToolbox
import ShotsCore

final class HotKeys {
    static let shared = HotKeys()

    private var refs: [UInt32: EventHotKeyRef] = [:]
    private var handlers: [UInt32: () -> Void] = [:]
    private var installed = false

    /// Registers `shortcut` under `id`. `false` if macOS refused it.
    func register(id: UInt32, _ shortcut: Shortcut, handler: @escaping () -> Void) -> Bool {
        installIfNeeded()
        unregister(id)
        var ref: EventHotKeyRef?
        let hotKey = EventHotKeyID(signature: OSType(0x5348_4F54), id: id)  // 'SHOT'
        let status = RegisterEventHotKey(
            shortcut.keyCode, shortcut.modifiers, hotKey, GetApplicationEventTarget(), 0, &ref
        )
        guard status == noErr, let ref else { return false }
        refs[id] = ref
        handlers[id] = handler
        return true
    }

    func unregister(_ id: UInt32) {
        if let ref = refs.removeValue(forKey: id) {
            UnregisterEventHotKey(ref)
        }
        handlers[id] = nil
    }

    func unregisterAll() {
        for id in Array(refs.keys) {
            unregister(id)
        }
    }

    fileprivate func fire(_ id: UInt32) {
        handlers[id]?()
    }

    private func installIfNeeded() {
        guard !installed else { return }
        installed = true
        var spec = EventTypeSpec(eventClass: OSType(kEventClassKeyboard), eventKind: UInt32(kEventHotKeyPressed))
        InstallEventHandler(GetApplicationEventTarget(), { _, event, _ in
            var hotKey = EventHotKeyID()
            let status = GetEventParameter(
                event, EventParamName(kEventParamDirectObject), EventParamType(typeEventHotKeyID), nil,
                MemoryLayout<EventHotKeyID>.size, nil, &hotKey
            )
            if status == noErr {
                let id = hotKey.id
                DispatchQueue.main.async { HotKeys.shared.fire(id) }
            }
            return noErr
        }, 1, &spec, nil, nil)
    }
}
