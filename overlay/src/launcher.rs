//! Starting WattSeal, and stopping when it stops.
//!
//! The widget is a standalone program, which leaves the user with an ordering
//! problem: it shows what the collector writes, so somebody has to start the
//! collector first. That is a fine thing to ask of a service and a poor thing to
//! ask of a person who double-clicked one icon.
//!
//! So this widget starts WattSeal itself when there is nothing to read. Two rules
//! keep that from being rude:
//!
//! 1. **Never start a second copy.** The check is the data, not the process list:
//!    a database whose newest sample is recent is a database somebody is already
//!    writing to. WattSeal has its own single-instance lock and a second copy
//!    exits with an error, which would look like the widget had crashed.
//! 2. **Never write anything.** Starting a program is not writing to its
//!    database, and nothing here opens the file for anything but reading.
//!
//! The child is *not* waited on as a job. It is detached: WattSeal is the user's
//! program and outlives the widget in the normal case — the widget is closed far
//! more often than the collector is.

use std::{
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
};

/// The collector program, next to this executable.
///
/// A path rather than a bare name on purpose. Starting `"WattSeal"` would let
/// `PATH` decide which WattSeal runs, and the one that matters is the one whose
/// database this widget reads.
///
/// **The Windows arm needs its own `cfg`, not just the other one.** Leaving it
/// ungated defines the name twice on every other platform — which is how this
/// shipped: it compiles here and nowhere else, because this is the only platform
/// it has ever been built on.
#[cfg(target_os = "windows")]
const WATTSEAL_EXE: &str = "WattSeal.exe";
#[cfg(not(target_os = "windows"))]
const WATTSEAL_EXE: &str = "WattSeal";

/// A database is treated as live if its newest sample is younger than this.
///
/// The collector's own default interval is one second, so three is three missed
/// samples: long enough that a busy machine is not mistaken for a dead one,
/// short enough that a user who has just started WattSeal does not stare at a
/// placeholder while it warms up.
const FRESH_SECONDS: i64 = 3;

/// The handle on a WattSeal this widget started.
#[derive(Debug)]
pub struct Supervised {
    child: Child,
}

impl Supervised {
    /// Whether the child is still running.
    ///
    /// `try_wait` rather than a check that can block: this is called from the
    /// widget's tick, and a tick that waits on a process is a tick the window
    /// stops repainting during.
    pub fn is_running(&mut self) -> bool {
        !matches!(self.child.try_wait(), Ok(Some(_)))
    }
}

/// Why a launch was skipped, in words a caller can show.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Skipped {
    /// The database has a recent sample, so WattSeal is already running.
    AlreadyRunning,
    /// `launch_wattseal` is off, so the user said not to.
    Disabled,
    /// There is no WattSeal beside this executable to start.
    NotInstalled,
}

/// Decides and performs the launch, returning the child when one was started.
///
/// `database` is the widget's path to `power_monitoring.db`; it is only ever read.
pub fn start_if_needed(
    configured: bool,
    executable_dir: &Path,
    database: &Path,
) -> Result<Option<Supervised>, Skipped> {
    if !configured {
        return Err(Skipped::Disabled);
    }
    if is_live(database) {
        return Err(Skipped::AlreadyRunning);
    }

    let program = executable_dir.join(WATTSEAL_EXE);
    if !program.is_file() {
        return Err(Skipped::NotInstalled);
    }

    let child = Command::new(&program)
        .current_dir(executable_dir)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|_| Skipped::NotInstalled)?;

    Ok(Some(Supervised { child }))
}

/// Whether the database's newest sample is recent enough to call it live.
///
/// `false` for a missing or unreadable file, which is the answer that lets the
/// launch happen — and the right one: "cannot tell" must not mean "assume it is
/// running and show nothing".
fn is_live(database: &Path) -> bool {
    let Some(conn) = crate::source::open_read_only(database) else {
        return false;
    };
    let Ok(Some(newest)) = conn.query_row("SELECT MAX(\"timestamp\") FROM \"total_data\"", [], |row| {
        row.get::<_, Option<i64>>(0)
    }) else {
        return false;
    };

    let Ok(now_ms) = std::time::SystemTime::now().duration_since(std::time::SystemTime::UNIX_EPOCH) else {
        return false;
    };

    let age_seconds = (now_ms.as_millis() as i64 - newest) / 1000;
    age_seconds < FRESH_SECONDS
}

/// The directory this executable lives in.
pub fn executable_dir() -> Option<PathBuf> {
    std::env::current_exe().ok()?.parent().map(PathBuf::from)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_missing_database_is_not_live() {
        // "Cannot tell" must never mean "assume it is running and show nothing",
        // or a user who has not installed WattSeal yet gets a widget that
        // refuses to start it and gives no reason.
        let path = std::env::temp_dir().join("wattseal-launch-test-no-such-file.db");
        let _ = std::fs::remove_file(&path);
        assert!(!is_live(&path));
    }

    #[test]
    fn a_database_with_a_fresh_sample_is_live() {
        // **The single-instance guard.** If this ever reads false, a second copy
        // of WattSeal gets started — and each would exit on the upstream
        // single-instance lock, or race it for the lock.
        //
        // Tested against the real function on a real file, on purpose. An earlier
        // version of this file re-derived the age inline and compared the numbers
        // itself, which tested nothing: removing the `AlreadyRunning` branch from
        // `start_if_needed` left every test in the file green.
        let path = fixture("fresh", now_ms());
        assert!(
            is_live(&path),
            "a sample written a moment ago counted as a stopped collector"
        );
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn a_database_that_exists_but_was_never_sampled_is_not_live() {
        // Created but empty: the collector has not written its first row. The
        // mere existence of a file must not conclude a collector is running.
        let path = fixture("empty", -1);
        assert!(!is_live(&path), "an empty database counted as a running collector");
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn an_old_sample_is_not_live() {
        let path = fixture("old", 1_000);
        assert!(!is_live(&path), "a sample from 1970 counted as live");
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn a_live_database_stops_a_second_wattseal_from_being_started() {
        // The guard above, through the entry point that actually launches.
        let path = fixture("end-to-end", now_ms());
        assert_eq!(
            start_if_needed(true, &std::env::temp_dir(), &path).err(),
            Some(Skipped::AlreadyRunning),
            "a live database did not stop a second WattSeal"
        );
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn launching_is_skipped_when_the_user_said_not_to() {
        // The one case that must never launch, whatever the data says: the
        // configuration says WattSeal is started some other way.
        let dir = std::env::temp_dir();
        let result = start_if_needed(false, &dir, &dir.join("nothing.db"));
        assert_eq!(result.err(), Some(Skipped::Disabled));
    }

    #[test]
    fn launching_is_skipped_when_there_is_nothing_to_launch() {
        let dir = std::env::temp_dir().join("wattseal-launch-empty");
        std::fs::create_dir_all(&dir).unwrap();
        let result = start_if_needed(true, &dir, &dir.join("nothing.db"));
        assert_eq!(
            result.err(),
            Some(Skipped::NotInstalled),
            "started something out of an empty directory"
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    /// A database file with one `total_data` row stamped at `timestamp`, or no
    /// rows at all when `timestamp` is negative.
    fn fixture(name: &str, timestamp: i64) -> PathBuf {
        let path = std::env::temp_dir().join(format!("wattseal-launch-{}-{name}.db", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let conn = rusqlite::Connection::open(&path).unwrap();
        conn.execute_batch(
            "CREATE TABLE total_data (
                 \"timestamp\" INTEGER NOT NULL,
                 \"duration_ms\" INTEGER NOT NULL,
                 \"total_energy_uj\" INTEGER
             )",
        )
        .unwrap();
        if timestamp >= 0 {
            conn.execute("INSERT INTO total_data VALUES (?1, 1000, 0)", [timestamp])
                .unwrap();
        }
        drop(conn);
        path
    }

    fn now_ms() -> i64 {
        std::time::SystemTime::now()
            .duration_since(std::time::SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64
    }
}
