// `samo-shots`: Shots' helper (ADR-004). Samo runs `samo-shots serve`, which starts the worker as macOS's own
// "Shots" app; `on`, `off` and `uninstall` have nothing outside Shots' folder to undo.

import AppKit
import ShotsCore

signal(SIGPIPE, SIG_IGN)
let arguments = CommandLine.arguments
let executable = Bundle.main.executablePath ?? arguments.first ?? "samo-shots"

switch arguments.dropFirst().first {
case "serve":
    exit(Spawn.relay(executable: executable))
case "--worker":
    WorkerApp.run(shared: arguments.contains("--shared"), executable: executable)
case "on", "off", "uninstall":
    // Shortcuts and windows end with the worker; saved shots stay where you saved them.
    exit(0)
default:
    FileHandle.standardError.write(Data("usage: samo-shots serve|on|off|uninstall\n".utf8))
    exit(2)
}
