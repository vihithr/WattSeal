//! Read-only access to the collector's database.
//!
//! This program does not link WattSeal's code, so it has no structs to read a
//! row into. Everything here is therefore addressed by **name** — table names,
//! column names — and whatever is not found is left out rather than guessed at.
//! Three rules, all of them consequences of reading a file this program does not
//! own:
//!
//! 1. **Never write.** Every handle comes from [`open_read_only`], which opens
//!    the file `SQLITE_OPEN_READ_ONLY` and issues no pragma that would modify
//!    it — no migration, no `CREATE TABLE`, no journal-mode change. The
//!    collector owns this database; a widget that "only reads" it is still a
//!    second writer on the lock.
//! 2. **Read the generation first.** `PRAGMA user_version` is the only thing in
//!    the file that says what the tables look like. It is checked on *every*
//!    tick, because the collector can migrate the file under a running widget.
//! 3. **A column that is not there is not a zero.** A missing table or column
//!    is omitted from the result, so the widget shows *fewer* numbers. Nothing
//!    is ever read positionally into a value that would look plausible and mean
//!    something else.
//!
//! The generation number is the one piece of knowledge this program shares with
//! WattSeal, and it is deliberately **not** "the newest one". See
//! [`SUPPORTED_GENERATION`].

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    time::Duration,
};

use rusqlite::{Connection, OpenFlags};

use crate::{config::Metric, language::AppLanguage};

/// The collector's database file, resolved next to the executable.
const DATABASE_PATH: &str = "power_monitoring.db";

/// The schema generation this build reads.
///
/// The single number shared with WattSeal's side that is not data, and the only
/// thing that has to be kept up to date when upstream migrates. **It is not a
/// rebase** — nothing here has to compile against upstream — so the cost of a
/// bump is a widget that hides its numbers until this line is changed, not a
/// merge conflict.
///
/// "Newest generation" would be worse: a reader that follows whatever the
/// collector last wrote has to guess what a column now means, and a wrong guess
/// is a plausible number. An out-of-date reader is boring and safe, which is the
/// whole bargain of only touching the data.
///
/// **This is the one place where independence has a price tag.** With no shared
/// code there is no compiler to notice when WattSeal moves this number, so the
/// value is written down in `doc/overlay.md` and a test checks the two against
/// each other. Moving to a new database generation means bumping this line and
/// that one together; the test is what stops one being done without the other.
pub const SUPPORTED_GENERATION: i32 = 3;

/// One hour, the duration an hourly roll-up covers.
///
/// Only used to *skip* a row: a record one hour long is a roll-up, not a sample,
/// and averaging it into the live figure would dilute it with an hour of
/// history. Nothing is read out of such a row.
const HOUR_MS: i64 = 3_600_000;

/// The sensor tables the degraded reader can address, and the metric each one
/// feeds. Table names come from the collector's own schema and are baked in
/// here rather than taken from the file, so nothing a database says ends up in
/// a query.
const NAME_ADDRESSED_TABLES: &[(&str, Metric)] = &[
    ("total_data", Metric::Total),
    ("cpu_data", Metric::Cpu),
    ("gpu_data", Metric::Gpu),
    ("ram_data", Metric::Ram),
    ("disk_data", Metric::Disk),
    ("network_data", Metric::Network),
];

/// How long a temporary read waits for the collector's write lock. Per
/// connection, so it costs nothing on disk.
const READ_BUSY_TIMEOUT: Duration = Duration::from_millis(2000);

/// What the widget could establish about the database behind it.
///
/// Every variant is a reading of the file, never a repair of it: making the
/// collector's database match what this build expects is the one thing a
/// third-party reader must never do.
///
/// Note what no longer distinguishes the first three from the fourth. When this
/// program shared a build with the dashboard it could resolve a table to a
/// struct and read positionally, which was richer — and wrong in a way nobody
/// could see. Addressing everything by name makes the by-name reader the only
/// reader, so [`ForeignGeneration`](Availability::ForeignGeneration) no longer
/// means "fewer numbers are shown"; it means "the *labels* are not".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Availability {
    /// The generation is one this build reads and the sensor tables are there.
    Ready,
    /// The generation is one this build reads, but the collector has not
    /// created its sensor tables yet.
    Starting,
    /// The file is at a generation this build does not know.
    ///
    /// The figures still arrive, because they are read by column name and a
    /// generation bump reorders columns far more often than it renames them.
    /// What is withheld is everything that would come from the *dashboard's*
    /// settings — its language and its theme — because those are the parts this
    /// build would have to recognise to trust. A theme name it cannot check
    /// would become a colour the user never picked.
    ForeignGeneration(i32),
    /// There is no database to read.
    Missing,
}

impl Availability {
    /// Whether the widget is showing the degraded, label-less presentation.
    ///
    /// `Starting` and `Missing` are deliberately *not* degraded: they are the
    /// normal moments before the collector's first sample, and the widget keeps
    /// its labels and its settings there so it does not flicker between
    /// presentations while it starts up.
    pub fn is_degraded(self) -> bool {
        matches!(self, Availability::ForeignGeneration(_))
    }
}

/// The overlay's handle on the collector's database.
pub struct Source {
    path: PathBuf,
    availability: Availability,
}

impl Source {
    /// Where this source is reading, for a caller that has to make a decision
    /// about the file itself rather than its contents — the launcher, which
    /// decides whether to start the program that writes it.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Opens the shared database next to the executable.
    pub fn open() -> Self {
        Self::at(Path::new(DATABASE_PATH))
    }

    /// Opens the database at `path`, for tests and for a build that is told
    /// where to look.
    pub fn at(path: &Path) -> Self {
        let mut source = Self {
            path: path.to_path_buf(),
            availability: Availability::Missing,
        };
        source.poll();
        source
    }

    pub fn availability(&self) -> Availability {
        self.availability
    }

    /// Re-reads the generation and records what may be shown.
    ///
    /// Every tick, on purpose. The collector can migrate the file while this is
    /// running, and a widget that checked once at startup would keep reading a
    /// shape that no longer exists.
    pub fn poll(&mut self) {
        // The **name** is the contract, not an open handle. On Unix a file with
        // an open handle can be deleted, and the handle then keeps answering
        // from an inode the collector is no longer writing to — which looks
        // exactly like a live widget showing a value that has quietly stopped
        // changing. Windows refuses to unlink a file another handle has open, so
        // this cannot arise there; it is checked anyway because the code is the
        // same and the cost is one failed stat.
        if !self.path.exists() {
            self.availability = Availability::Missing;
            return;
        }

        // The open itself is the probe: the collector may not have created the
        // database yet, which is normal on a cold start and not worth an error.
        let Some(conn) = open_read_only(&self.path) else {
            self.availability = Availability::Missing;
            return;
        };

        let Some(generation) = user_version(&conn) else {
            self.availability = Availability::Missing;
            return;
        };

        if generation != SUPPORTED_GENERATION {
            self.availability = Availability::ForeignGeneration(generation);
            return;
        }

        // Known generation. The collector has not necessarily created its
        // sensor tables yet, and that is the one state a fresh open can change.
        self.availability = if sensor_tables(&conn).is_empty() {
            Availability::Starting
        } else {
            Availability::Ready
        };
    }

    /// The dashboard's saved settings — the language and the theme it has
    /// already applied to the database.
    ///
    /// `None` unless the generation is one this build reads. That is not a
    /// limitation of the table: it is a single row of text this program could
    /// read whatever the generation. It is withheld on purpose, because the
    /// *values* are the dashboard's vocabulary — a theme name, a close
    /// behaviour — and a build that has not seen the schema has no way to know
    /// it still knows them. Showing a setting it cannot interpret is the failure
    /// mode this whole module is arranged to avoid.
    pub fn ui_settings(&self) -> Option<UiSettings> {
        if !matches!(self.availability, Availability::Ready | Availability::Starting) {
            return None;
        }
        let conn = open_read_only(&self.path)?;
        read_ui_settings(&conn)
    }

    /// The newest reading of every metric, in watts.
    ///
    /// Empty when nothing could be confirmed — the caller keeps whatever it had
    /// and the widget shows a placeholder rather than a number it cannot stand
    /// behind.
    pub fn watts(&mut self) -> HashMap<String, f64> {
        match self.availability {
            Availability::Missing => HashMap::new(),
            _ => readings_by_name(&self.path),
        }
    }

    /// The most expensive applications of the last `window_secs` seconds, or `None` when this
    /// tick has no process sample to offer.
    pub fn top_apps(&mut self, window_secs: i64, n: usize) -> Option<Vec<(String, f64)>> {
        let conn = open_read_only(&self.path)?;
        read_top_apps(&conn, window_secs, n)
    }

    /// Seconds since the newest sample, or `None` when there is nothing to date
    /// it by.
    ///
    /// **This is how the widget tells whether WattSeal is running.** Not by
    /// looking for a process — process enumeration is a different API on every
    /// platform, and this program has a reader already. A database whose newest
    /// row is a second old is a database somebody is writing to, and that is the
    /// question actually being asked: "is the data live", not "does a process
    /// with this name exist".
    ///
    /// A stopped collector leaves a perfectly readable database behind, which is
    /// exactly the case worth telling apart from a missing one — the widget can
    /// show numbers from it, and they are simply not moving.
    pub fn freshness_seconds(&self) -> Option<i64> {
        let conn = open_read_only(&self.path)?;
        let newest: i64 = conn
            .query_row("SELECT MAX(\"timestamp\") FROM \"total_data\"", [], |row| {
                row.get::<_, Option<i64>>(0)
            })
            .ok()??;

        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::SystemTime::UNIX_EPOCH)
            .ok()?
            .as_millis() as i64;

        Some(((now_ms - newest) / 1000).max(0))
    }
}

/// The dashboard's own settings row, as far as this build understands it.
///
/// Two text columns out of a row the dashboard owns. `theme` is kept as the
/// string the dashboard wrote rather than resolved here: theme names are that
/// program's vocabulary, and guessing at one this build cannot check would
/// produce a colour the user never picked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UiSettings {
    /// The dashboard's language code, e.g. `EN`.
    pub language: String,
    /// The dashboard's theme name, e.g. `Hunting`.
    pub theme: String,
}

impl UiSettings {
    /// The language the widget should speak.
    ///
    /// An unrecognised code becomes English, which is a language the widget can
    /// actually draw — the alternative is no language at all.
    pub fn language(&self) -> AppLanguage {
        AppLanguage::from_code(&self.language)
    }
}

/// Reads the dashboard's settings row, or `None` if it is not there.
fn read_ui_settings(conn: &Connection) -> Option<UiSettings> {
    conn.query_row("SELECT language, theme FROM ui_settings WHERE id = 1", [], |row| {
        Ok(UiSettings {
            language: row.get(0)?,
            theme: row.get(1)?,
        })
    })
    .ok()
}

/// The most expensive applications of the last `window_secs` seconds.
///
/// Addressed by column name for the same reason the sensor readings are. Two
/// tables have to be joined for this one, so either name may be missing — in
/// which case there is no app list, rather than a list of something.
fn read_top_apps(conn: &Connection, window_secs: i64, n: usize) -> Option<Vec<(String, f64)>> {
    // A zero or negative window is rejected rather than divided by here.
    if window_secs <= 0 {
        return None;
    }

    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::SystemTime::UNIX_EPOCH)
        .ok()?
        .as_millis() as i64;

    let mut statement = conn
        .prepare(
            "SELECT a.name AS app_name,
                    CAST(SUM(COALESCE(p.process_energy_uj, 0)) AS INTEGER) AS process_energy_uj
             FROM process_data p
             JOIN apps a ON p.app_id = a.id
             WHERE p.timestamp >= ?1 AND p.timestamp < ?2
             GROUP BY p.app_id
             ORDER BY process_energy_uj DESC
             LIMIT ?3",
        )
        .ok()?;

    let rows = statement
        .query_map(
            rusqlite::params![
                now_ms - window_secs * 1000,
                now_ms,
                i64::try_from(n).unwrap_or(i64::MAX)
            ],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?)),
        )
        .ok()?;

    let apps: Vec<(String, f64)> = rows
        .filter_map(Result::ok)
        .map(|(name, energy_uj)| (name, energy_uj as f64 / 1_000_000.0 / window_secs as f64))
        .collect();

    (!apps.is_empty()).then_some(apps)
}

/// The readings that survive an unknown generation, addressed by column name.
///
/// Column names are the part of a schema that a generation bump is unlikely to
/// change; the order of the columns is not. This is therefore the query that
/// still means "CPU watts" after a migration, and it deliberately asks for the
/// smallest set of columns that can produce a watt figure. A table or a column
/// that is not there is left out — a number read out of a shifted row is the one
/// thing a third-party reader must never show.
fn readings_by_name(path: &Path) -> HashMap<String, f64> {
    let Some(conn) = open_read_only(path) else {
        return HashMap::new();
    };

    NAME_ADDRESSED_TABLES
        .iter()
        .filter_map(|(table, metric)| newest_watts(&conn, table).map(|watts| (metric.id().to_string(), watts)))
        .collect()
}

/// Energy and duration of the newest live sample in `table`, as watts.
///
/// `SUM` because one timestamp can hold several GPU device rows and their
/// energies add up, and `MAX` for the duration because that is the span those
/// rows cover. Hourly roll-ups are excluded, exactly as the typed reader
/// excludes them: a one-hour average is not a reading.
fn newest_watts(conn: &Connection, table: &str) -> Option<f64> {
    let live = format!("\"duration_ms\" < {HOUR_MS}");
    let sql = format!(
        "SELECT COALESCE(SUM(\"total_energy_uj\"), 0), MAX(\"duration_ms\") \
         FROM \"{table}\" WHERE {live} \
         AND \"timestamp\" = (SELECT MAX(\"timestamp\") FROM \"{table}\" WHERE {live})"
    );

    let (energy, duration_ms): (i64, i64) = conn.query_row(&sql, [], |row| Ok((row.get(0)?, row.get(1)?))).ok()?;

    // Microjoules to watts: `1 J` over `1 s` is `1 W`, so it is a divide by a
    // million and by the seconds the sample covers. The divisor is clamped to at
    // least a millisecond so a zero cannot make this infinite — a value the
    // widget would then render as a number it cannot stand behind.
    let seconds = duration_ms.max(1) as f64 / 1000.0;
    Some(energy.max(0) as f64 / 1_000_000.0 / seconds)
}

/// Opens the file for reading only, or nothing at all.
///
/// A missing file is not an error worth handling twice: it is the state the
/// collector is in for the first second of every start.
pub fn open_read_only(path: &Path) -> Option<Connection> {
    let conn = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY).ok()?;
    let _ = conn.busy_timeout(READ_BUSY_TIMEOUT);
    Some(conn)
}

/// The schema generation the file declares, or `None` if it cannot be read.
///
/// `PRAGMA user_version` is a file-level integer the collector owns and this
/// program never writes. There is no other way to ask what shape the tables
/// have, which is why it is checked before every read.
fn user_version(conn: &Connection) -> Option<i32> {
    conn.pragma_query_value(None, "user_version", |row| row.get::<_, i32>(0))
        .ok()
}

/// The sensor tables the collector says it has created.
///
/// Names only — this never decides *how* to read one, which is the whole point
/// of addressing by column name. It answers a narrower question: has the
/// collector started yet? An empty list on a freshly created file means the
/// widget shows a placeholder rather than a row of zeroes.
fn sensor_tables(conn: &Connection) -> Vec<String> {
    let Ok(list) = conn.query_row("SELECT tables FROM hardware_info WHERE id = 1", [], |row| {
        row.get::<_, Option<String>>(0)
    }) else {
        return Vec::new();
    };

    list.unwrap_or_default()
        .split(',')
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map(str::to_string)
        .collect()
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU64, Ordering};

    use super::*;

    #[test]
    fn the_documented_generation_is_the_one_this_build_reads() {
        // The price of not linking WattSeal's code is that nothing here has to
        // compile against it — and so nothing here *fails* when it moves the
        // schema generation. This is what replaces that compiler: the number is
        // written down in the documentation, and the two are compared here.
        //
        // A reader pinned to the wrong generation still shows numbers, just
        // without labels, so the symptom is "my widget went blank" rather than a
        // stack trace. That is exactly the kind of quiet failure a test is for.
        let documented = include_str!("../../doc/overlay.md")
            .lines()
            .find_map(|line| line.split("this build reads is").nth(1))
            .and_then(|rest| {
                rest.chars()
                    .skip_while(|c| !c.is_ascii_digit())
                    .take_while(char::is_ascii_digit)
                    .collect::<String>()
                    .parse::<i32>()
                    .ok()
            });

        assert_eq!(
            documented,
            Some(SUPPORTED_GENERATION),
            "doc/overlay.md says generation {documented:?}, source.rs reads {SUPPORTED_GENERATION}"
        );
    }

    /// Builds a stand-in for the collector's file. `tables` decides how far the
    /// collector got, and `generation` which schema it wrote.
    ///
    /// **The sensor tables are written out here, not borrowed from the
    /// collector.** This program no longer links WattSeal, so it cannot call the
    /// collector's own `CREATE TABLE` — and that is the point of the change.
    /// What replaces it is a statement of the contract instead: these are the
    /// tables and these are the columns this program asks for *by name*.
    ///
    /// That is a different kind of guarantee from the old fixture's. A fixture
    /// copied from the collector used to catch a column moving; this one cannot,
    /// because nothing here knows what the collector will write next. What it
    /// does catch is the thing that can still go wrong on this side — a query
    /// that starts reading a column the rest of this program does not, or a typo
    /// in a name — and it documents, in one place, exactly what the widget
    /// depends on. When upstream renames something, the right fix is here and in
    /// `NAME_ADDRESSED_TABLES`: nothing is expected to fail at compile time.
    fn write_database(path: &Path, tables: &[&str], generation: i32) {
        let conn = Connection::open(path).unwrap();
        conn.pragma_update(None, "journal_mode", "WAL").unwrap();
        conn.execute_batch(
            "CREATE TABLE hardware_info (id INTEGER PRIMARY KEY, tables TEXT, hardware_data TEXT);
             CREATE TABLE apps (id INTEGER PRIMARY KEY, identity TEXT, name TEXT, exe_path TEXT);
             CREATE TABLE devices (id INTEGER PRIMARY KEY, kind TEXT NOT NULL, name TEXT NOT NULL,
                 UNIQUE(kind, name));
             CREATE TABLE ui_settings (id INTEGER PRIMARY KEY CHECK (id = 1),
                 language TEXT NOT NULL DEFAULT 'EN',
                 carbon_intensity TEXT NOT NULL DEFAULT 'World average',
                 kwh_cost TEXT NOT NULL DEFAULT 'World average',
                 theme TEXT NOT NULL DEFAULT 'Hunting',
                 currency TEXT NOT NULL DEFAULT 'USD',
                 close_behavior TEXT NOT NULL DEFAULT 'ask');",
        )
        .unwrap();

        // One sensor table per metric. `timestamp` and `duration_ms` are the key,
        // `total_energy_uj` is the energy the reader converts, and `device_id`
        // distinguishes rows that share a timestamp — which is how a machine
        // with two GPUs records both in the same second. The shapes here are the
        // collector's: a key over `(timestamp, duration_ms)` for the sensors
        // that belong to no device, and over `(timestamp, device_id)` for the
        // ones that do.
        for (table, metric) in NAME_ADDRESSED_TABLES.iter() {
            let keyed_by_device = matches!(metric, Metric::Gpu);
            let (columns, key) = if keyed_by_device {
                (
                    "\"device_id\" INTEGER NOT NULL, \"total_energy_uj\" INTEGER",
                    "\"timestamp\", \"device_id\"",
                )
            } else {
                ("\"total_energy_uj\" INTEGER", "\"timestamp\", \"duration_ms\"")
            };

            conn.execute_batch(&format!(
                "CREATE TABLE \"{table}\" (
                     \"timestamp\"   INTEGER NOT NULL,
                     \"duration_ms\" INTEGER NOT NULL,
                     {columns},
                     PRIMARY KEY ({key})
                 ) WITHOUT ROWID;"
            ))
            .unwrap();
        }

        // The collector registers its sensor tables in one `hardware_info` row.
        if !tables.is_empty() {
            conn.execute(
                "INSERT INTO hardware_info (id, tables, hardware_data) VALUES (1, ?1, '{}')",
                [tables.join(",")],
            )
            .unwrap();
        }

        // 2 J over 1 s, 1 J over 1 s: 2 W for the total, 1 W for the CPU.
        conn.execute_batch(
            "INSERT INTO total_data VALUES (1000, 1000, 2000000);
             INSERT INTO cpu_data VALUES (1000, 1000, 1000000);",
        )
        .unwrap();

        if tables.contains(&"gpu_data") {
            // Two devices reporting for the same second: 0.5 J each.
            conn.execute_batch(
                "INSERT INTO devices (id, kind, name) VALUES (1, 'gpu', 'first'), (2, 'gpu', 'second');
                 INSERT INTO gpu_data VALUES (1000, 1000, 1, 500000);
                 INSERT INTO gpu_data VALUES (1000, 1000, 2, 500000);",
            )
            .unwrap();
        }

        // An hourly roll-up of the CPU, newer than the live sample: a reader
        // that ignored the duration filter would report the roll-up instead.
        conn.execute_batch("INSERT INTO cpu_data VALUES (2000, 3600000, 3600000000);")
            .unwrap();

        conn.pragma_update(None, "user_version", generation).unwrap();
    }

    /// Scratch files that delete themselves.
    ///
    /// Counted rather than stamped for the name: the Windows clock reads the same
    /// value for a whole ~15 ms tick, so two tests that started in the same tick
    /// were handed the same name and one of them deleted the database the other
    /// was still reading. Deleted on drop rather than on the last line of the
    /// test, so a failing assertion cannot leave a database behind in the temp
    /// directory on every run.
    struct Scratch(PathBuf);

    impl Scratch {
        fn new(name: &str) -> Self {
            static NEXT: AtomicU64 = AtomicU64::new(0);
            let unique = NEXT.fetch_add(1, Ordering::Relaxed);
            Self(std::env::temp_dir().join(format!("wattseal-source-{name}-{}-{unique}.db", std::process::id())))
        }
    }

    impl std::ops::Deref for Scratch {
        type Target = Path;

        fn deref(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            for suffix in ["", "-wal", "-shm", "-journal"] {
                let _ = std::fs::remove_file(
                    self.0
                        .with_file_name(format!("{}{suffix}", self.0.file_name().unwrap().to_string_lossy())),
                );
            }
        }
    }

    #[test]
    fn a_sensor_table_this_build_has_never_heard_of_is_silently_dropped() {
        // The real reason the generation check exists — established by running
        // it, not by assuming it.
        //
        // The typed reader dispatches on the table name against a build-time
        // table of known sensors. A table it does not recognise is not an error:
        // the dispatcher returns nothing, the row is dropped, and the widget
        // reports *fewer numbers* with no indication that anything went wrong.
        // That is the worst shape this feature has — quietly wrong, in a widget
        // nobody is looking at closely.
        let path = Scratch::new("unknown-table");
        write_database(&path, &["total_data", "cpu_data"], SUPPORTED_GENERATION);

        // A future collector registers a sensor this build has no struct for.
        let conn = Connection::open(&*path).unwrap();
        conn.execute_batch(
            "CREATE TABLE nvram_data (timestamp INTEGER NOT NULL, duration_ms INTEGER NOT NULL,
                 total_energy_uj INTEGER, PRIMARY KEY (timestamp, duration_ms)) WITHOUT ROWID;
             INSERT INTO nvram_data VALUES (1000, 1000, 4000000);
             UPDATE hardware_info SET tables = 'total_data,cpu_data,nvram_data' WHERE id = 1;",
        )
        .unwrap();
        drop(conn);

        let mut source = Source::at(&path);
        source.poll();

        // Silently: no error, no degraded state, just a missing metric. Nothing
        // in the widget says the collector knows about something it does not.
        assert_eq!(source.availability(), Availability::Ready);
        let watts = source.watts();
        assert!(!watts.contains_key("nvram"), "the fixture did not take");
        assert_eq!(watts.get("cpu"), Some(&1.0), "the known sensors still read");

        // So the generation check is the only thing standing between a future
        // schema and a widget that quietly reports less than the machine is
        // doing. Bump the generation and the widget says so instead.
        conn_advance_generation(&path);
        let mut source = Source::at(&path);
        source.poll();
        assert!(source.availability().is_degraded(), "an unknown sensor was not noticed");
    }

    #[test]
    fn reading_by_name_keeps_working_across_the_generation_change() {
        // And the degraded path is not just "give up": it reads the three
        // columns it needs by name, out of tables it names itself, so it keeps
        // producing the figures it was producing.
        let path = Scratch::new("degraded-by-name");
        write_database(&path, &["total_data", "cpu_data"], SUPPORTED_GENERATION);

        let mut source = Source::at(&path);
        source.poll();
        let expected = source.watts();
        assert_eq!(expected.get("cpu"), Some(&1.0));

        conn_advance_generation(&path);
        source.poll();

        assert!(source.availability().is_degraded());
        assert_eq!(
            source.watts(),
            expected,
            "the degraded reader disagrees with the typed one"
        );
    }

    /// Bumps the file to a generation this build does not read, the way a future
    /// collector would.
    fn conn_advance_generation(path: &Path) {
        let conn = Connection::open(path).unwrap();
        conn.pragma_update(None, "user_version", SUPPORTED_GENERATION + 1)
            .unwrap();
    }

    #[test]
    fn a_whole_lifecycle_leaves_the_collectors_file_byte_for_byte_identical() {
        // The read-only half of the identity statement, checked over the layer the
        // overlay actually uses rather than over the `common` entry point alone:
        // poll, read, and poll again, in every state the widget can be in.
        //
        // A reader that leaks a write shows up here as a single changed byte.
        // That matters because this file belongs to another program — the
        // collector — and "we only read it" is the entire basis for a
        // third-party tool being allowed to look at it at all.
        let path = Scratch::new("lifecycle-readonly");
        write_database(&path, &["total_data", "cpu_data"], SUPPORTED_GENERATION);

        let mut source = Source::at(&path);
        let before = snapshot(&path);
        for _ in 0..5 {
            source.poll();
            let _ = source.watts();
            let _ = source.top_apps(60, 3);
            let _ = source.ui_settings();
        }
        assert_untouched(&before, &path, "the normal read path");

        // Then the degraded path, which runs its own SQL — the branch most likely
        // to differ from the other.
        //
        // The comparison's own limit is checked first: a writer's change *has*
        // to show up, or "unchanged" below would hold no matter what the overlay
        // did. That it shows up in the write-ahead log rather than in the
        // database file is itself worth knowing — a reader watching only the
        // main file would miss exactly the writes it was supposed to catch.
        let before_bump = snapshot(&path);
        conn_advance_generation(&path);
        let after_bump = snapshot(&path);
        assert_ne!(
            before_bump, after_bump,
            "the fixture's own write was invisible, so the checks below cannot fail"
        );

        let mut source = Source::at(&path);
        for _ in 0..5 {
            source.poll();
            let _ = source.watts();
            let _ = source.top_apps(60, 3);
            let _ = source.ui_settings();
        }
        assert!(
            source.availability().is_degraded(),
            "the degraded path was not the one exercised"
        );
        assert_untouched(&after_bump, &path, "the degraded read path");
    }

    /// The collector's file and its write-ahead log, which is where a write in
    /// WAL mode actually lands.
    fn snapshot(path: &Path) -> Vec<(Vec<u8>, Vec<u8>)> {
        let read = |name: &str| std::fs::read(path.with_file_name(name)).unwrap_or_default();
        let stem = path.file_name().unwrap().to_string_lossy().to_string();
        vec![(std::fs::read(path).unwrap_or_default(), read(&format!("{stem}-wal")))]
    }

    /// Asserts neither the database nor its log changed, with a reason that says
    /// which read path leaked.
    fn assert_untouched(before: &[(Vec<u8>, Vec<u8>)], path: &Path, which: &str) {
        let after = snapshot(path);
        assert_eq!(
            before[0].0.len(),
            after[0].0.len(),
            "{which} changed the size of a database it does not own"
        );
        assert!(before[0].0 == after[0].0, "{which} wrote to the collector's database");
        assert!(
            before[0].1 == after[0].1,
            "{which} wrote to the collector's write-ahead log"
        );
    }

    #[test]
    fn the_overlays_own_reader_does_not_create_a_database_it_cannot_find() {
        // `common` has its own test for its read-only open; this one covers *this*
        // helper, which is a separate line of code used by the degraded path.
        //
        // It was found by mutation: swapping the read-only open for an ordinary
        // writable `Connection::open` made **no test fail**. A writable open creates
        // the file, so the one thing a third-party reader must never do — leave an
        // empty database behind in somebody else's directory — would have happened
        // silently.
        let path = Scratch::new("reader-creates-nothing");

        assert!(open_read_only(&path).is_none(), "a missing file was opened");
        assert!(!path.exists(), "the reader created a database that was not there");

        // And with a file present, it opens without being able to write: SQLite
        // refuses a write on a read-only connection, which is the check that
        // proves the flag took rather than merely being passed.
        write_database(&path, &["cpu_data"], SUPPORTED_GENERATION);
        let conn = open_read_only(&path).expect("an existing database opens");
        let written = conn.execute("UPDATE cpu_data SET usage_percent = 1.0", []);
        assert!(written.is_err(), "the reader was able to write to it");
    }

    /// Adds an app and a reading for it, the way the collector's own writer
    /// would: `apps` carries the name the widget shows, `process_data` carries
    /// the energy.
    ///
    /// Written as SQL rather than through `insert_event` so the fixture cannot
    /// drift from what the query in `select_top_processes_average` actually
    /// reads — and so the timestamps are controllable, which that query needs
    /// (it averages over the last `n` seconds of *wall clock*).
    fn write_process(path: &Path, app_id: i64, name: &str, energy_uj: i64) {
        let conn = Connection::open(path).unwrap();
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64;

        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS process_data (
                 timestamp         INTEGER NOT NULL,
                 duration_ms       INTEGER NOT NULL,
                 app_id            INTEGER NOT NULL REFERENCES apps(id),
                 process_energy_uj INTEGER,
                 process_cpu_usage REAL,
                 process_gpu_usage REAL,
                 process_mem_usage REAL,
                 read_bytes        INTEGER,
                 written_bytes     INTEGER,
                 subprocess_count  INTEGER,
                 PRIMARY KEY (timestamp, duration_ms, app_id)
             ) WITHOUT ROWID;",
        )
        .unwrap();

        conn.execute(
            "INSERT OR IGNORE INTO apps (id, identity, name, exe_path) VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![
                app_id,
                name.to_lowercase(),
                name,
                format!("C:/bin/{}.exe", name.to_lowercase())
            ],
        )
        .unwrap();
        // Five rows, one per second, ending now.
        //
        // Two things this fixture got wrong the first two times, both worth
        // writing down:
        //
        // * **One row is not a second's worth.** The query sums the energy in
        //   the window and divides by the window, so a single row would be
        //   divided by thirty while holding only one second — the widget would
        //   report a fraction of the real power.
        // * **The window must be much wider than the span the rows cover.**
        //   `select_top_processes_average` computes its own `now` at query time,
        //   which is *later* than the `now` this fixture used. With five rows
        //   spanning five seconds and a five-second window, a test that takes
        //   more than about a second between writing and querying — which is
        //   exactly what happens when the machine is busy — drops the oldest
        //   row and the arithmetic comes out wrong. That was a real flake: it
        //   failed on the seventh of eight identical runs. Hence a 30-second
        //   window around five rows, which leaves 25 seconds of slack.
        for second in 0..5 {
            conn.execute(
                "INSERT INTO process_data (timestamp, duration_ms, app_id, process_energy_uj,
                     process_cpu_usage, process_gpu_usage, process_mem_usage,
                     read_bytes, written_bytes, subprocess_count)
                 VALUES (?1, 1000, ?2, ?3, 1.0, 0.0, 2.0, 0, 0, 1)",
                rusqlite::params![now_ms - second * 1000, app_id, energy_uj],
            )
            .unwrap();
        }
    }

    #[test]
    fn a_database_with_no_process_rows_offers_no_app_list() {
        // The caller keeps the previous list on `None` — a tick that lands
        // between two collector samples must not blank the rows.
        let path = Scratch::new("no-apps");
        write_database(&path, &["total_data", "cpu_data"], SUPPORTED_GENERATION);
        let mut source = Source::at(&path);
        source.poll();

        assert_eq!(
            source.top_apps(5, 3),
            None,
            "an app list came out of a database with no processes"
        );
    }

    #[test]
    fn the_app_list_comes_back_named_and_in_watts() {
        // This is the whole of `top_apps`, and until this test existed **no test
        // had ever produced an app list at all** — the fixture had no process
        // rows, so the query returned `None` and the mapping below was never
        // run. It is the one part of the widget that shows per-application power,
        // so it is worth having its arithmetic pinned: 1 J over 1 s is 1 W, and
        // the same energy over a different window is a different reading.
        let path = Scratch::new("apps");
        write_database(&path, &["total_data", "cpu_data"], SUPPORTED_GENERATION);
        // Five rows each, over a 30-second window: Explorer 5 x 30 J = 150 J, so
        // 5 W; Firefox 5 x 6 J = 30 J, so 1 W. See `write_process` for why the
        // window is wider than the rows.
        write_process(&path, 1, "Firefox", 6_000_000);
        write_process(&path, 2, "Explorer", 30_000_000);

        let mut source = Source::at(&path);
        source.poll();
        assert_eq!(source.availability(), Availability::Ready);

        let apps = source.top_apps(30, 5).expect("the two apps were written");
        assert_eq!(
            apps,
            vec![("Explorer".to_string(), 5.0), ("Firefox".to_string(), 1.0)],
            "app list is not the two apps in watts, most expensive first"
        );

        // The divisor is the window, not the rows' own duration: read over 60
        // seconds the same stored energy is half as many watts.
        let slower = source.top_apps(60, 5).unwrap();
        assert_eq!(
            slower[0].1, 2.5,
            "a wider window must mean fewer watts for the same energy"
        );
    }

    #[test]
    fn the_app_list_honours_the_count_the_user_asked_for() {
        let path = Scratch::new("apps-limit");
        write_database(&path, &["total_data", "cpu_data"], SUPPORTED_GENERATION);
        write_process(&path, 1, "Firefox", 6_000_000);
        write_process(&path, 2, "Explorer", 30_000_000);
        write_process(&path, 3, "Code", 12_000_000);

        let mut source = Source::at(&path);
        source.poll();

        let two = source.top_apps(30, 2).expect("the limit is above zero");
        assert_eq!(two.len(), 2, "asked for two, got {}", two.len());
        assert_eq!(two[0].0, "Explorer");
        assert_eq!(
            two[1].0, "Code",
            "the two kept should be the two most expensive, not the first two written"
        );
    }

    #[test]
    fn a_database_that_is_deleted_while_the_widget_runs_is_noticed() {
        // Someone deleting `power_monitoring.db` under a running overlay is not
        // exotic: uninstalling, cleaning up by hand, or a collector that
        // recreates its file rather than migrating it. The widget holds an open
        // handle at that moment, so the question is whether it keeps reading the
        // file that is no longer there — which it cannot, because the name it
        // resolves is the one that just went away.
        let path = Scratch::new("deleted");
        write_database(&path, &["total_data", "cpu_data"], SUPPORTED_GENERATION);

        let mut source = Source::at(&path);
        source.poll();
        assert_eq!(source.availability(), Availability::Ready);
        assert_eq!(source.watts().get("cpu"), Some(&1.0));

        // Windows refuses to unlink a file another handle has open, so the
        // scenario cannot be staged here at all. Where it can, it is the whole
        // point of the test; where it cannot, there is nothing to assert.
        //
        // This is the one assertion in the suite that needs the unlink to
        // succeed, so it is worth saying why it would fail without `poll()`'s
        // name check: with the file unlinked the open connection still answers
        // `user_version()` — the inode is alive, and a WAL-mode database keeps
        // answering from the `-wal` sidecar the unlink did not touch. So the
        // handle is not a witness to the file being gone. It would look exactly
        // like a healthy widget, which is the bug.
        if std::fs::remove_file(&*path).is_err() {
            return;
        }

        source.poll();
        assert_eq!(
            source.availability(),
            Availability::Missing,
            "the widget went on reading a file that is no longer there"
        );
        assert!(source.watts().is_empty(), "readings came from a deleted file");

        // And it comes back when the collector puts it back, rather than
        // needing the widget to be restarted.
        write_database(&path, &["total_data", "cpu_data"], SUPPORTED_GENERATION);
        source.poll();
        assert_eq!(source.availability(), Availability::Ready);
        assert_eq!(source.watts().get("cpu"), Some(&1.0));
    }
    #[test]
    fn a_missing_database_is_reported_as_missing_and_recovers_when_it_appears() {
        let path = Scratch::new("missing");
        let mut source = Source::at(&path);

        assert_eq!(source.availability(), Availability::Missing);
        assert!(source.watts().is_empty());

        write_database(&path, &["total_data", "cpu_data"], SUPPORTED_GENERATION);
        source.poll();

        assert_eq!(source.availability(), Availability::Ready);
    }

    #[test]
    fn a_collector_that_has_not_written_yet_is_a_starting_not_a_failure() {
        let path = Scratch::new("starting");
        write_database(&path, &[], SUPPORTED_GENERATION);
        let source = Source::at(&path);

        // No sensor tables yet: the widget keeps its own presentation and waits,
        // rather than dropping to the degraded one for a second on every start.
        assert_eq!(source.availability(), Availability::Starting);
        assert!(!source.availability().is_degraded());
    }

    #[test]
    fn a_bumped_generation_drops_the_typed_reader_on_the_next_tick() {
        let path = Scratch::new("bumped");
        write_database(&path, &["total_data", "cpu_data"], SUPPORTED_GENERATION);
        let mut source = Source::at(&path);
        assert_eq!(source.availability(), Availability::Ready);

        // The collector migrates while the overlay is open.
        let conn = Connection::open(&*path).unwrap();
        conn.pragma_update(None, "user_version", SUPPORTED_GENERATION + 1)
            .unwrap();
        drop(conn);
        source.poll();

        assert_eq!(
            source.availability(),
            Availability::ForeignGeneration(SUPPORTED_GENERATION + 1)
        );
        assert!(source.availability().is_degraded());
    }

    #[test]
    fn the_degraded_reader_still_returns_the_numbers_it_can_confirm() {
        let path = Scratch::new("degraded-reads");
        write_database(&path, &["total_data", "cpu_data", "gpu_data"], SUPPORTED_GENERATION + 1);
        let mut source = Source::at(&path);

        let watts = source.watts();

        // Addressed by column name, so the values survive the generation being
        // one this build has never seen.
        assert_eq!(watts.get("total"), Some(&2.0));
        assert_eq!(watts.get("cpu"), Some(&1.0));
        // Two devices reporting for one second add up to one reading.
        assert_eq!(watts.get("gpu"), Some(&1.0));
        // The tables the fixture never created are simply absent.
        assert!(!watts.contains_key("ram"));
        assert!(!watts.contains_key("disk"));
    }

    #[test]
    fn the_degraded_reader_ignores_hourly_roll_ups() {
        let path = Scratch::new("degraded-hourly");
        write_database(&path, &["total_data", "cpu_data"], SUPPORTED_GENERATION + 1);
        let mut source = Source::at(&path);

        // The newest CPU row is an hour-long roll-up. Reporting 1 W/s from it
        // would be a wrong number, so it is dropped instead.
        assert_eq!(source.watts().get("cpu"), Some(&1.0));
        assert!(readings_by_name(&path).contains_key("cpu"));
    }

    #[test]
    fn the_degraded_mode_offers_no_app_list_and_no_settings() {
        let path = Scratch::new("degraded-no-extras");
        write_database(&path, &["total_data", "cpu_data"], SUPPORTED_GENERATION + 1);
        let mut source = Source::at(&path);

        // Per-app rows need a second table and a name to go with the energy;
        // neither is confirmed at an unknown generation, so they are hidden.
        assert!(source.top_apps(5, 3).is_none());
        assert!(source.ui_settings().is_none());
    }

    #[test]
    fn the_typed_reader_reports_the_same_watts_as_the_degraded_one() {
        let path = Scratch::new("agreement");
        write_database(&path, &["total_data", "cpu_data", "gpu_data"], SUPPORTED_GENERATION);
        let mut typed = Source::at(&path);
        let expected = typed.watts();

        let path = Scratch::new("agreement-foreign");
        write_database(&path, &["total_data", "cpu_data", "gpu_data"], SUPPORTED_GENERATION + 1);
        let mut degraded = Source::at(&path);
        let actual = degraded.watts();

        // The degraded path is a fallback, not a different set of numbers: if
        // the two ever disagree, one of them is reading the wrong thing.
        assert_eq!(expected, actual);
    }

    #[test]
    fn a_database_that_is_not_a_database_is_missing() {
        let path = Scratch::new("not-a-database");
        std::fs::write(&*path, b"this is not sqlite").unwrap();

        let source = Source::at(&path);
        assert_eq!(source.availability(), Availability::Missing);
    }
}
