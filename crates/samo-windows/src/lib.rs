//! L5 on Windows (ADR-006): the scanner, the Recycle Bin mover, Quit and telemetry, with the same functions as
//! `samo-macos`. Each app takes the platform crate of the OS it is built for; on any other OS this crate is empty.
#![cfg(windows)]
