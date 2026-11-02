// `samo-clips`: Clips' helper (ADR-005). Samo runs `samo-clips serve`, which starts the worker as macOS's own
// "Clips" app; `on`, `off` and `uninstall` have nothing outside Clips' folder to undo (Samo deletes the folder on
// uninstall).

import AppKit
import SamoKit

signal(SIGPIPE, SIG_IGN)
let arguments = CommandLine.arguments
let executable = Bundle.main.executablePath ?? arguments.first ?? "samo-clips"

switch arguments.dropFirst().first {
case "serve":
    exit(Spawn.relay(executable: executable))
case "--worker":
    WorkerApp.run(shared: arguments.contains("--shared"), executable: executable)
case "on", "off", "uninstall":
    exit(0)
default:
    FileHandle.standardError.write(Data("usage: samo-clips serve|on|off|uninstall\n".utf8))
    exit(2)
}
