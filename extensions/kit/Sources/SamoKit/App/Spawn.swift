// How the worker is started (ADR-004, ADR-005: an extension's macOS permissions belong to it alone).
//
// Samo starts `<helper> serve`. That first process starts the same binary again as `<helper> --worker`, with macOS's
// "responsibility disclaim" spawn attribute, so macOS treats the worker as its own app (the Info.plist built into
// the binary names it, e.g. "Shots") and asks for its permissions for it, not for Samo. The worker inherits stdin,
// stdout and stderr, so it talks to Samo directly; the first process only waits for it.
//
// The attribute is private API, looked up by name. If it is missing or refused, the worker starts without it and
// is told so (`--shared`): its permissions would then be Samo's, and its page says so.
//
// This is the only file in an extension that may call `posix_spawn` (R16, checked by the network gate).

import Darwin
import Foundation

public enum Spawn {
    /// `int responsibility_spawnattrs_setdisclaim(posix_spawnattr_t *attrs, int disclaim)`.
    public typealias Disclaim = @convention(c) (UnsafeMutablePointer<posix_spawnattr_t?>, Int32) -> Int32

    /// The disclaim attribute's setter, if this macOS has it.
    public static func disclaimSetter() -> Disclaim? {
        // RTLD_DEFAULT: search every loaded image.
        guard let symbol = dlsym(UnsafeMutableRawPointer(bitPattern: -2), "responsibility_spawnattrs_setdisclaim")
        else { return nil }
        return unsafeBitCast(symbol, to: Disclaim.self)
    }

    /// The worker's arguments: `--shared` when it couldn't be made its own app.
    public static func workerArguments(executable: String, disclaimed: Bool) -> [String] {
        disclaimed ? [executable, "--worker"] : [executable, "--worker", "--shared"]
    }

    /// Starts the worker. Returns its pid and whether it is its own app for macOS's privacy settings.
    public static func startWorker(
        executable: String,
        setter: () -> Disclaim? = disclaimSetter
    ) -> (pid: pid_t, disclaimed: Bool)? {
        if let disclaim = setter(), let pid = spawn(executable, disclaim: disclaim) {
            return (pid, true)
        }
        return spawn(executable, disclaim: nil).map { ($0, false) }
    }

    private static func spawn(_ executable: String, disclaim: Disclaim?) -> pid_t? {
        var attrs: posix_spawnattr_t?
        guard posix_spawnattr_init(&attrs) == 0 else { return nil }
        defer { posix_spawnattr_destroy(&attrs) }
        if let disclaim, disclaim(&attrs, 1) != 0 {
            return nil
        }
        let args = workerArguments(executable: executable, disclaimed: disclaim != nil)
        var argv: [UnsafeMutablePointer<CChar>?] = args.map { strdup($0) }
        argv.append(nil)
        var envp: [UnsafeMutablePointer<CChar>?] = ProcessInfo.processInfo.environment
            .map { strdup("\($0.key)=\($0.value)") }
        envp.append(nil)
        defer {
            argv.forEach { free($0) }
            envp.forEach { free($0) }
        }
        var pid: pid_t = 0
        let started = posix_spawn(&pid, executable, nil, &attrs, argv, envp)
        return started == 0 ? pid : nil
    }

    /// `samo-shots serve`: starts the worker and waits for it, ending as it ends. A worker that exits with
    /// `restartExitCode` (after Screen Recording was allowed) is started again, a few times at most.
    public static func relay(executable: String) -> Int32 {
        for _ in 0..<5 {
            guard let (pid, _) = startWorker(executable: executable) else {
                FileHandle.standardError.write(Data("\((executable as NSString).lastPathComponent): couldn't start the worker\n".utf8))
                return 1
            }
            let code = wait(pid)
            if code != restartExitCode { return code }
        }
        return 1
    }

    private static func wait(_ pid: pid_t) -> Int32 {
        var status: Int32 = 0
        while waitpid(pid, &status, 0) == -1 {
            if errno != EINTR { return 1 }
        }
        return (status & 0x7F) == 0 ? (status >> 8) & 0xFF : 1
    }
}

/// The worker's exit code that asks the first process to start it again (after a permission was granted: macOS
/// applies a grant only to a process started after it), instead of ending.
public let restartExitCode: Int32 = 75
