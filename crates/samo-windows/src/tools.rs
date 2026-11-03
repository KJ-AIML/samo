//! Windows' own tools, run by their full path (as the Mac runs `/bin/kill`), so nothing on `PATH` can stand in.

use std::path::PathBuf;

/// `CREATE_NO_WINDOW`: no console window flashes up.
pub(crate) const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// `%SystemRoot%\System32\<name>`.
pub(crate) fn system_tool(name: &str) -> PathBuf {
    std::env::var_os("SystemRoot")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(r"C:\Windows"))
        .join("System32")
        .join(name)
}
