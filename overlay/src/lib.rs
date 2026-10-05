//! WattSeal always-on-top overlay widget.
//!
//! A **standalone** program: it reads `power_monitoring.db` and shows what it
//! finds, and it links none of WattSeal's code. That is the whole point —
//! something that only touches the data never has to be rebased when upstream
//! changes, and it runs against the *official* WattSeal rather than a build of
//! this fork. What it costs is given up in [`source`]: with no shared structs
//! to read a row into, a schema this build does not recognise yields **fewer
//! numbers**, never a number that means something else.

pub mod app;
pub mod config;
mod instance;
pub mod language;
pub mod launcher;
pub mod message;
pub mod source;

pub use language::AppLanguage;
pub mod theme;
pub mod translations;
pub mod winlayer;

pub use source::Availability;
pub use winlayer::click_through_supported;

/// Points the process at the directory the executable lives in.
///
/// A windowless widget started from a shortcut, a task scheduler or a shell has
/// no reason to be in any particular working directory, and everything it reads
/// or writes — the database, the config — is named relative to the executable so
/// that a zip can be unpacked anywhere and still work. Best effort: a failure
/// here leaves the process where it was, which is survivable.
fn switch_to_executable_directory() {
    match std::env::current_exe().and_then(|exe| {
        exe.parent()
            .ok_or_else(|| std::io::Error::other("the executable has no parent directory"))
            .map(std::path::PathBuf::from)
    }) {
        Ok(directory) => {
            if let Err(error) = std::env::set_current_dir(&directory) {
                log::warn!("overlay: could not switch to {}: {error}", directory.display());
            }
        }
        Err(error) => log::warn!("overlay: could not locate the executable directory: {error}"),
    }
}

/// Runs the overlay window.
///
/// Transparency is delegated to the platform: [`config::Transparency::Auto`]
/// resolves to a Win32 layered window on Windows (where the GPU surface exposes
/// no alpha-capable composite mode) and to per-pixel surface alpha elsewhere.
///
/// Returns as soon as another overlay already holds the single-instance lock:
/// nothing failed, this process simply has nothing to add.
pub fn run() -> iced::Result {
    // Resolve the database — and the config file — against the executable's
    // directory instead of whatever directory this process happened to start
    // in.
    //
    // Copied rather than imported: it is a few lines, and importing it is what
    // this program used to do before it stopped sharing code with WattSeal.
    // Reaching into another program's helper for this is not worth linking to a
    // tree whose releases have nothing to do with this one.
    switch_to_executable_directory();
    init_logging();

    // The tray, the dashboard footer and `--overlay` each start this process and
    // none of them can see the others' handle, so a second copy steps aside here
    // rather than stacking a widget on top of the first.
    let _instance = match instance::InstanceGuard::claim(instance::handover_wait()) {
        instance::Claim::Held(guard) => guard,
        instance::Claim::Taken => {
            log::info!("overlay: another instance is already running; leaving");
            return Ok(());
        }
        instance::Claim::Unavailable => return app::run(),
    };

    app::run()
}

/// Whether the overlay is currently asked to be running.
///
/// The overlay and its callers talk through the config file rather than through
/// IPC, so this is the read side of that channel.
pub fn is_requested() -> bool {
    config::OverlayConfig::load()
        .map(|config| config.overlay_requested)
        .unwrap_or(false)
}

/// Asks the overlay to open.
///
/// Callers say what they want instead of editing the file, which keeps the
/// layout of that file in one place — the tray, the dashboard and the overlay
/// itself all go through here.
///
/// Pin mode is left alone: it changes when something pins, and by nothing else,
/// so a widget pinned before it was opened comes back pinned. Clearing it here
/// meant the tray's pin entry silently did nothing whenever the overlay happened
/// to be closed, which read as "pin does not work".
pub fn request_open() {
    let mut config = config::OverlayConfig::load().unwrap_or_default();

    // Nothing to write when it is already open.
    if config.overlay_requested {
        return;
    }

    config.overlay_requested = true;
    config.save();
}

/// Asks the overlay to close, and clears pin mode with it.
///
/// Closing is the escape hatch a click-through widget needs: it is how the
/// dashboard's `Hide overlay` releases one where there is no tray. Opening
/// deliberately leaves the pin alone, so a widget pinned from the tray while it
/// was closed comes back pinned.
pub fn request_close() {
    let mut config = config::OverlayConfig::load().unwrap_or_default();

    if !config.overlay_requested && !config.pin_mode {
        return;
    }

    config.overlay_requested = false;
    config.pin_mode = false;
    config.save();
}

/// Flips "pin mode" and returns the new state.
///
/// The overlay polls the file, so this needs no IPC. It is also how a pinned,
/// click-through overlay is released, since that window ignores the mouse.
pub fn toggle_pin() -> bool {
    let mut config = config::OverlayConfig::load().unwrap_or_default();
    config.pin_mode = !config.pin_mode;
    config.save();
    config.pin_mode
}

/// Where the opt-in log is written: `overlay.log` next to the executable.
static LOG_PATH: std::sync::OnceLock<std::path::PathBuf> = std::sync::OnceLock::new();

/// Installs a tiny file logger, but only when `WATTSEAL_OVERLAY_LOG` is set.
///
/// The renderer diagnostics (selected adapter, surface format and **alpha mode**)
/// are the only way to explain why a window renders opaque on a machine we cannot
/// inspect, which is the kind of report an overlay attracts. It is opt-in so a
/// normal run never writes to the user's disk.
fn init_logging() {
    use std::io::Write;

    if std::env::var_os("WATTSEAL_OVERLAY_LOG").is_none() {
        return;
    }
    LOG_PATH
        .set(config::OverlayConfig::path().with_file_name("overlay.log"))
        .ok();

    struct FileLogger;

    impl log::Log for FileLogger {
        fn enabled(&self, _metadata: &log::Metadata) -> bool {
            true
        }

        fn log(&self, record: &log::Record) {
            let Some(path) = LOG_PATH.get() else {
                return;
            };
            if let Ok(mut file) = std::fs::OpenOptions::new().create(true).append(true).open(path) {
                let _ = writeln!(file, "[{}] {}", record.level(), record.args());
            }
        }

        fn flush(&self) {}
    }

    static LOGGER: FileLogger = FileLogger;

    let _ = log::set_logger(&LOGGER);
    log::set_max_level(log::LevelFilter::Info);
}

/// Where a failure to start is reported: `startup_error.txt`, next to the
/// executable.
///
/// Separate from the opt-in `overlay.log` on purpose. That one is diagnostics a
/// user turns on deliberately, and it is off by default so a normal run never
/// writes to disk — but the one failure worth catching on every machine is the
/// one that happens *before* a window exists, and that is exactly the moment the
/// user has nothing on screen to tell them anything went wrong.
///
/// Plain text with no timestamps and no rotation: it exists until someone reads
/// it, and it is overwritten rather than appended so it always describes the run
/// that failed rather than accumulating every bad morning since.
pub fn startup_error_path() -> std::path::PathBuf {
    config::OverlayConfig::path().with_file_name("startup_error.txt")
}

/// Removes any report left by an earlier run.
///
/// A file that survived a crash would send someone chasing a failure that stopped
/// happening days ago, and on a build with no console this one file is the *only*
/// thing the widget can say about itself. A stale one is worse than none, so it
/// goes before the run that might replace it.
///
/// Best-effort: a file held open by something else, or in a directory the user
/// has made read-only, is not a reason to refuse to start.
pub fn clear_startup_error() {
    let _ = std::fs::remove_file(startup_error_path());
}

/// The contract between the three processes, which is nothing but this file.
///
/// There is no IPC: the tray and the dashboard call these, and the overlay
/// polls the file they write. So the invariants below are the whole feature —
/// in particular "closing releases a pinned, click-through widget", which on a
/// system with no tray is the only way out of one.
///
/// Each test gets a file of its own, and they are serialised by the guard
/// `scratch_config` returns — because **the config path is a process global**,
/// not because the files collide. Two of these running side by side would leave
/// one reading and writing the other's file.
#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use config::PathOverride;

    use super::*;

    #[test]
    fn a_release_build_must_not_open_a_console_window() {
        // **The one thing a user notices before anything else works.** A Windows
        // binary that is a console subsystem gets a black CMD window the moment
        // it starts, and it stays there for as long as the widget runs — a
        // permanent black rectangle on top of a program whose entire purpose is
        // to be a discreet layer above everything else.
        //
        // `src/main.rs` is included rather than read at runtime so this cannot be
        // satisfied by a file that happens to exist. The `not(debug_assertions)`
        // half matters as much as the first: a *debug* build keeps the console,
        // because that is the build a developer runs and a panic nobody can see
        // is worse than an ugly window. WattSeal's own entry point makes the
        // same trade in the same way.
        let source = include_str!("main.rs");
        assert!(
            source.contains("#![cfg_attr(not(debug_assertions), windows_subsystem = \"windows\")]"),
            "src/main.rs no longer detaches the release build from a console window"
        );

        // And the console has to stop existing *before* the failure it would hide:
        // a console-less binary cannot report a start-up error by printing it.
        assert!(
            source.contains("startup_error_path()"),
            "src/main.rs lost the file it writes start-up failures to; with no console there is nowhere else"
        );
    }

    #[test]
    fn a_report_from_an_earlier_run_does_not_outlive_it() {
        // The whole point of the file is that it describes the run that failed.
        // Left in place it describes whichever morning happened to fail, and
        // sends someone chasing a crash that stopped happening days ago — on a
        // build where this one file is the only thing the widget can say.
        let (_override, dir) = scratch_config();
        std::fs::write(startup_error_path(), "a crash from last week").unwrap();
        assert!(startup_error_path().exists());

        clear_startup_error();
        assert!(
            !startup_error_path().exists(),
            "a stale report survived a successful start"
        );

        // And clearing a file that is not there is not an error — the normal
        // case, where the run before this one also succeeded.
        clear_startup_error();
        remove(&dir);
    }

    #[test]
    fn a_start_up_failure_is_reported_next_to_the_executable() {
        // Not in `%TEMP%`, not in a log folder: a user who just double-clicked an
        // icon is standing in the directory it is in, and a report they cannot
        // find is the same as no report.
        let path = startup_error_path();
        assert_eq!(path.file_name().unwrap(), "startup_error.txt");
        assert_eq!(path.parent(), config::OverlayConfig::path().parent());

        // Not appended to: it is overwritten every run, so it describes the run
        // that failed rather than accumulating every bad morning since.
        assert!(
            path != config::OverlayConfig::path(),
            "the error report would overwrite the settings"
        );
    }

    /// Points the shared config at a file of its own, and returns the guard
    /// that serialises these tests and restores the real location afterwards.
    ///
    /// The per-test directory is a counter, so a failure that leaves one behind
    /// is named after the test that left it rather than colliding with the next.
    fn scratch_config() -> (PathOverride, PathBuf) {
        use std::sync::atomic::{AtomicU64, Ordering};
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let unique = NEXT.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("wattseal-contract-{}-{unique}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(config::OverlayConfig::path().file_name().unwrap());
        (config::override_path(path.clone()), path)
    }

    /// Deletes the scratch directory, and **reports** a failure rather than
    /// swallowing it.
    ///
    /// The ignored `Result` this replaces was hiding a directory left in the
    /// user's temp folder on every single run: cleanup that fails quietly is
    /// indistinguishable from cleanup that never existed, which is how the
    /// seventeen thousand leaked files earlier in this project went unnoticed.
    fn remove(path: &Path) {
        let dir = path.parent().expect("the scratch path has a directory");
        if let Err(error) = std::fs::remove_dir_all(dir) {
            panic!("could not remove {dir:?}: {error}");
        }
    }

    #[test]
    fn closing_is_what_releases_a_pinned_click_through_widget() {
        // The invariant the whole pin mode rests on: a widget that ignores the
        // mouse and cannot be dragged cannot be clicked out of, so closing has to
        // clear the pin or there is no way back on a system with no tray.
        let (guard, path) = scratch_config();
        let saved = config::OverlayConfig {
            overlay_requested: true,
            pin_mode: true,
            ..config::OverlayConfig::default()
        };
        assert!(saved.save_to(&path));

        request_close();

        let read_back = config::OverlayConfig::load_from(&path).unwrap();
        assert!(!read_back.overlay_requested, "the overlay would stay up");
        assert!(
            !read_back.pin_mode,
            "a pinned, click-through widget would be unescapable"
        );
        drop(guard);
        remove(&path);
    }

    #[test]
    fn opening_leaves_the_pin_alone_so_a_pinned_widget_comes_back_pinned() {
        // The mirror image, and the reason `request_open` does not touch the pin:
        // clearing it meant the tray's `Pin / Unpin` entry did nothing whenever
        // the overlay happened to be closed, which read as "pin does not work".
        let (guard, path) = scratch_config();
        let saved = config::OverlayConfig {
            overlay_requested: false,
            pin_mode: true,
            ..config::OverlayConfig::default()
        };
        assert!(saved.save_to(&path));

        request_open();

        let read_back = config::OverlayConfig::load_from(&path).unwrap();
        assert!(read_back.overlay_requested);
        assert!(read_back.pin_mode, "opening cleared a pin nothing had changed");
        drop(guard);
        remove(&path);
    }

    #[test]
    fn the_trays_pin_entry_flips_the_same_flag_the_widget_reads() {
        // `toggle_pin` is the only way out of a pinned widget when there is no
        // dashboard to close it from, so the value it returns and the value it
        // writes have to be the same one.
        let (guard, path) = scratch_config();

        assert!(toggle_pin());
        assert!(config::OverlayConfig::load_from(&path).unwrap().pin_mode);
        assert!(!toggle_pin());
        assert!(!config::OverlayConfig::load_from(&path).unwrap().pin_mode);
        drop(guard);
        remove(&path);
    }

    #[test]
    fn asking_for_what_is_stored_changes_it_back() {
        // `request_open` on a file that already says "open", then `request_close`.
        // The second one genuinely has work to do, so it genuinely writes.
        let (guard, path) = scratch_config();
        let saved = config::OverlayConfig {
            overlay_requested: true,
            pin_mode: false,
            ..config::OverlayConfig::default()
        };
        assert!(saved.save_to(&path));

        request_open();
        request_close();

        let read_back = config::OverlayConfig::load_from(&path).expect("the file is still there");
        assert!(!read_back.overlay_requested);
        assert!(!read_back.pin_mode);
        drop(guard);
        remove(&path);
    }

    #[test]
    fn asking_for_what_is_already_stored_does_not_touch_the_file() {
        // Both entry points are called on every toggle, and one of them is
        // called from the dashboard's own close path. Rewriting the file when
        // there is nothing to change is how a poll elsewhere lands on a
        // half-written document — so each one, told what it is already being
        // told, has to be a no-op.
        //
        // This used to be one test that *claimed* to check this and did not: it
        // stored `overlay_requested: true` and then closed it, so the close had
        // real work to do and really wrote — and the assertion it ended on,
        // `!path.exists() || load_from(path).is_some()`, is true no matter what
        // happened, because a readable file satisfies the second half and an
        // absent one satisfies the first.
        //
        // The comparison is the modification time, not the bytes — see below.
        for (requested, pinned, closing) in [(false, false, true), (true, false, false)] {
            let (guard, path) = scratch_config();
            let stored = config::OverlayConfig {
                overlay_requested: requested,
                pin_mode: pinned,
                ..config::OverlayConfig::default()
            };
            assert!(stored.save_to(&path));
            // Bytes alone cannot see a rewrite: the JSON is pretty-printed
            // deterministically, so rewriting an unchanged config produces an
            // *identical* file and the comparison comes back equal — which is
            // exactly how this test passed while the mutation below went
            // unnoticed. The staging file is renamed over the target, so what
            // actually changes is the file's identity, and the one portable
            // thing that shows it is the timestamp.
            std::thread::sleep(std::time::Duration::from_millis(20));
            let before = std::fs::metadata(&path)
                .and_then(|m| m.modified())
                .expect("the fixture wrote a file");

            if closing {
                request_close()
            } else {
                request_open()
            }

            let after = std::fs::metadata(&path)
                .and_then(|m| m.modified())
                .expect("the file is still there");
            assert_eq!(
                after,
                before,
                "asking to {} when it is already stored rewrote the file",
                if closing { "close" } else { "open" }
            );
            drop(guard);
            remove(&path);
        }
    }

    #[test]
    fn closing_something_that_was_never_opened_creates_no_file() {
        // The dashboard's close path runs even on a machine where the widget was
        // never started. Writing a config file there invents a "running" record
        // out of nothing.
        let (guard, path) = scratch_config();
        assert!(!path.exists(), "the fixture did not create a file");

        request_close();

        assert!(!path.exists(), "closing an unopened overlay wrote a file at {path:?}");
        drop(guard);
        remove(&path);
    }

    #[test]
    fn a_fresh_install_is_closed_and_can_still_be_pinned() {
        // No file at all: a standalone `--overlay` run starts from defaults, and
        // the tray must still be able to pin it before anything has been saved.
        let (guard, path) = scratch_config();
        assert!(!path.exists());

        assert!(!is_requested(), "a missing file must not read as 'open'");
        assert!(toggle_pin());
        assert!(!is_requested(), "pinning must not open the overlay");

        drop(guard);
        remove(&path);
    }

    #[test]
    fn the_handover_wait_follows_the_configured_poll_interval() {
        // A hide-then-show can arrive before the widget being hidden has noticed,
        // so a newcomer waits about one poll interval before concluding it is a
        // real duplicate. An interval of zero or an absurd one must not turn that
        // into "wait forever" or "do not wait at all".
        let (guard, path) = scratch_config();

        for (refresh, expected) in [(0, 2), (1, 2), (3, 4), (5, 6), (60, 6), (999, 6)] {
            let saved = config::OverlayConfig {
                refresh_secs: refresh,
                ..config::OverlayConfig::default()
            };
            assert!(saved.save_to(&path));

            assert_eq!(
                instance::handover_wait(),
                std::time::Duration::from_secs(expected),
                "a refresh interval of {refresh}s waits the wrong time"
            );
        }

        drop(guard);
        remove(&path);
    }
}
