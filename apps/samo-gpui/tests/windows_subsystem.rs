//! Samo.exe is a windowed program on Windows: started from Explorer, the Start menu or at sign-in, no console window
//! opens beside it, and none can take Samo down when it is closed (ADR-006).
#![cfg(windows)]

/// The `Subsystem` field of a PE program's optional header: 2 is a windowed program, 3 a console one.
fn subsystem(exe: &[u8]) -> Option<u16> {
    let at = |i: usize| {
        exe.get(i..i + 4)
            .and_then(|b| b.try_into().ok())
            .map(u32::from_le_bytes)
    };
    let pe = usize::try_from(at(0x3C)?).ok()?;
    if exe.get(pe..pe + 4)? != b"PE\0\0" {
        return None;
    }
    // After the 4-byte signature and the 20-byte file header comes the optional header; Subsystem is at 68 in it.
    let field = pe + 4 + 20 + 68;
    exe.get(field..field + 2)
        .map(|b| u16::from_le_bytes([b[0], b[1]]))
}

#[test]
fn samo_is_a_windowed_program() -> Result<(), String> {
    let exe = std::fs::read(env!("CARGO_BIN_EXE_samo-gpui")).map_err(|e| e.to_string())?;
    assert_eq!(subsystem(&exe), Some(2), "not a windowed program");
    Ok(())
}
