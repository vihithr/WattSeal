//! WattSeal Overlay — a standalone power widget.
//!
//! Reads `power_monitoring.db` from its own directory and shows what it finds.
//! It links none of WattSeal's code.
//!
//! Three things are worth knowing before reading any further:
//!
//! - **It never writes to the database.** Not a migration, not a `CREATE TABLE`,
//!   not a journal-mode change. A widget that only reads is still a second writer
//!   on the lock.
//! - **It reads by column name.** With no shared structs to read a row into,
//!   every query names the columns it wants. A column that is not there is left
//!   out, so an unfamiliar database produces fewer numbers rather than numbers
//!   that mean something else. See `overlay::source`.
//! - **It starts WattSeal when there is nothing to read.** See `overlay::launcher`.
//!   It launches, supervises and leaves it running; it never writes to it.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::process::ExitCode;

fn main() -> ExitCode {
    // A file left by an earlier bad morning would send someone chasing a crash
    // that stopped happening days ago — and it is the *only* thing a
    // console-less build can tell them, so a stale one is worse than none.
    overlay::clear_startup_error();

    match overlay::run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            // **A release build on Windows has no console.** The
            // `windows_subsystem` attribute below detaches it from one, so
            // `eprintln!` goes nowhere — the error would be lost exactly when the
            // user cannot see a window either, which is the only time it matters.
            // Hence a file, and hence *next to the executable*, which is the one
            // place a user who just double-clicked an icon will look.
            let report = format!("wattseal-overlay failed to start: {error}\n");
            let _ = std::fs::write(overlay::startup_error_path(), &report);

            // Still printed: on Linux and macOS there is a console, and in a
            // debug build there is one on Windows too. The file is written in
            // every case, so a user who reports "it does nothing" has something
            // to send.
            eprint!("{report}");
            ExitCode::FAILURE
        }
    }
}
