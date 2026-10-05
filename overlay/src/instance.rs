//! Keeps one overlay process at a time.

use std::{
    fs::{File, OpenOptions},
    path::Path,
    time::{Duration, Instant},
};

use fs2::FileExt;

use crate::config::OverlayConfig;

/// How often a newcomer retries while a previous instance lets go.
const RETRY: Duration = Duration::from_millis(100);

/// Holds the lock that marks this process as *the* overlay.
///
/// The OS releases the lock when the file is dropped or the process ends, so a
/// crash cannot leave an overlay that can never be started again.
pub struct InstanceGuard {
    _file: File,
}

/// What happened when this process asked to be the overlay.
pub enum Claim {
    /// This process holds the lock and is the only overlay.
    Held(InstanceGuard),
    /// Another overlay is running; this process has nothing to do.
    Taken,
    /// The lock could not be used at all.
    Unavailable,
}

/// How long to give a previous instance to release the lock.
///
/// The overlay that is being hidden only finds out on its next poll — the
/// interval the config asks for — so a hide-then-show can arrive before the old
/// process has let go. Waiting one poll interval covers that handover. A genuine
/// duplicate only pays this wait before leaving, and it draws nothing while it
/// waits.
pub fn handover_wait() -> Duration {
    let refresh = OverlayConfig::load().map_or(1, |config| config.refresh_secs.clamp(1, 5));

    Duration::from_secs(u64::from(refresh) + 1)
}

impl InstanceGuard {
    /// Tries to become the overlay, waiting up to `wait` for a previous instance
    /// to let go of the lock.
    ///
    /// The tray, the dashboard footer and `--overlay` each start the process on
    /// their own, and no two of them can see the others' handle. Without the
    /// lock they stack two widgets on the same spot, both of them writing the
    /// same config file.
    pub fn claim(wait: Duration) -> Claim {
        Self::claim_at(&OverlayConfig::lock_path(), wait)
    }

    /// [`InstanceGuard::claim`] against an explicit lock path.
    ///
    /// Split out so the three outcomes can be driven from a test — including the
    /// one that needs a path nothing can open, which no amount of waiting or
    /// retrying would ever change.
    fn claim_at(path: &Path, wait: Duration) -> Claim {
        let deadline = Instant::now() + wait;

        loop {
            let file = match OpenOptions::new().write(true).create(true).truncate(false).open(path) {
                Ok(file) => file,
                Err(err) => {
                    // Not knowing whether we are alone is no reason to show
                    // nothing: two widgets read worse than one, but no widget at
                    // all is a broken feature.
                    log::warn!("overlay: single-instance lock unavailable: {err}");
                    return Claim::Unavailable;
                }
            };

            match file.try_lock_exclusive() {
                Ok(()) => return Claim::Held(Self { _file: file }),
                Err(_) if Instant::now() < deadline => std::thread::sleep(RETRY),
                Err(_) => return Claim::Taken,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A lock path no other test in this run can collide with.
    fn scratch(name: &str) -> std::path::PathBuf {
        use std::sync::atomic::{AtomicU64, Ordering};
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let unique = NEXT.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!("wattseal-lock-{name}-{}-{unique}", std::process::id()))
    }

    /// Deletes the scratch file, and **reports** a failure rather than swallowing
    /// it.
    ///
    /// This one swallowed, and the cost was a zero-byte file left in the user's
    /// temp folder on every single run for as long as it went unnoticed — the
    /// same failure mode as the seventeen thousand leaked files earlier in this
    /// project. A cleanup that fails quietly is indistinguishable from a cleanup
    /// that never existed.
    fn remove(path: &Path) {
        match std::fs::remove_file(path) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => panic!("could not remove {path:?}: {error}"),
        }
    }

    #[test]
    fn only_one_instance_can_hold_the_lock() {
        let path = scratch("single");
        let Claim::Held(guard) = InstanceGuard::claim_at(&path, Duration::ZERO) else {
            // Nothing to assert where the filesystem has no locking at all.
            remove(&path);
            return;
        };

        assert!(matches!(InstanceGuard::claim_at(&path, Duration::ZERO), Claim::Taken));

        // The lock is handed over, rather than needing the holder to exit.
        drop(guard);
        assert!(matches!(InstanceGuard::claim_at(&path, Duration::ZERO), Claim::Held(_)));
        remove(&path);
    }

    #[test]
    fn a_newcomer_waits_for_the_instance_it_is_replacing() {
        // Hide-then-show can arrive before the widget being hidden has noticed,
        // so the newcomer waits rather than concluding it is a duplicate. Without
        // this, showing the overlay right after hiding it did nothing until the
        // next time the user tried.
        let path = scratch("handover");
        let Claim::Held(guard) = InstanceGuard::claim_at(&path, Duration::ZERO) else {
            remove(&path);
            return;
        };

        let releaser = std::thread::spawn(move || {
            std::thread::sleep(RETRY * 3);
            drop(guard);
        });

        let claimed = InstanceGuard::claim_at(&path, RETRY * 20);
        assert!(
            matches!(claimed, Claim::Held(_)),
            "the newcomer gave up while the previous instance was still letting go"
        );
        releaser.join().unwrap();
        // Dropped before the delete, not after: a held claim keeps the file open,
        // and Windows refuses to remove an open file. Cleaning up "at the end"
        // is exactly the promise a later edit forgets to keep.
        drop(claimed);
        remove(&path);
    }

    #[test]
    fn an_unusable_lock_shows_the_widget_rather_than_nothing() {
        // Two overlapping widgets read worse than one, but no widget at all is a
        // broken feature — so a lock file that cannot be opened, for any reason,
        // is a reason to run anyway.
        let unreachable = scratch("unusable").join("no-such-directory").join("lock");

        // Retried for the whole wait, and then reported as unavailable rather
        // than as "another copy is running" — which would leave the user with no
        // widget and no reason why.
        assert!(matches!(
            InstanceGuard::claim_at(&unreachable, RETRY * 2),
            Claim::Unavailable
        ));
    }
}
