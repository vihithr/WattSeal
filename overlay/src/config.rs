//! Persisted configuration for the standalone overlay.
//!
//! Stored in its own `overlay_config.json` next to the executable so the
//! overlay never touches the main application's database schema.
//!
//! Everything in here is a setting only the widget has an opinion about. The
//! two the dashboard already owns — language and theme — are read from
//! `ui_settings` instead, so the widget follows the application rather than
//! keeping a second copy of them that can disagree.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::{language::AppLanguage, theme::ThemeChoice};

const CONFIG_FILENAME: &str = "overlay_config.json";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Metric {
    Total,
    Cpu,
    Gpu,
    Ram,
    Disk,
    Network,
    /// Top-N most power-hungry processes (count configurable).
    TopApps,
}

impl Metric {
    pub const DEFAULT_ORDER: &[Metric] = &[Metric::Total, Metric::Cpu, Metric::Gpu, Metric::Ram, Metric::TopApps];

    pub const ALL: &[Metric] = &[
        Metric::Total,
        Metric::Cpu,
        Metric::Gpu,
        Metric::Ram,
        Metric::Disk,
        Metric::Network,
        Metric::TopApps,
    ];

    pub fn id(self) -> &'static str {
        match self {
            Metric::Total => "total",
            Metric::Cpu => "cpu",
            Metric::Gpu => "gpu",
            Metric::Ram => "ram",
            Metric::Disk => "disk",
            Metric::Network => "network",
            Metric::TopApps => "top_apps",
        }
    }

    /// Single-letter label used by the abbreviated mode in the Latin-script
    /// languages; `translations::metric_short_name` owns the Chinese ones.
    pub fn short_label(self) -> &'static str {
        match self {
            Metric::Total => "T",
            Metric::Cpu => "C",
            Metric::Gpu => "G",
            Metric::Ram => "R",
            Metric::Disk => "D",
            Metric::Network => "N",
            Metric::TopApps => "Top",
        }
    }

    /// Whether this metric expands into multiple rows (Top-N apps).
    pub fn is_multi(self) -> bool {
        self == Metric::TopApps
    }
}

/// Widget orientation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Layout {
    #[default]
    Vertical,
    /// All metrics on a single compact line (MSI-Afterburner OSD style).
    Horizontal,
}

impl Layout {
    pub const ALL: &[Layout] = &[Layout::Vertical, Layout::Horizontal];
}

/// How tall a font's line box is relative to its glyph size.
///
/// Generous on purpose, like the width estimate: slack in the height costs a few
/// pixels, a clipped row costs a reading.
const LINE_HEIGHT_RATIO: f32 = 1.35;

/// Layout density (padding / spacing).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Density {
    Ultra,
    #[default]
    Compact,
    Normal,
}

impl Density {
    pub const ALL: &[Density] = &[Density::Ultra, Density::Compact, Density::Normal];

    pub fn padding(self) -> f32 {
        match self {
            Density::Ultra => 4.0,
            Density::Compact => 6.0,
            Density::Normal => 8.0,
        }
    }

    pub fn spacing(self) -> f32 {
        match self {
            Density::Ultra => 3.0,
            Density::Compact => 5.0,
            Density::Normal => 7.0,
        }
    }

    /// Height of one row at a given text size.
    ///
    /// The density is only a floor: the row still has to hold the value font's line
    /// box, which is taller than the glyph size. Ignoring that made a tall enough
    /// stack clip — the shortfall accumulates, so the bottom row is the one cut.
    pub fn row_height(self, font: FontSize) -> f32 {
        let floor: f32 = match self {
            Density::Ultra => 15.0,
            Density::Compact => 18.0,
            Density::Normal => 21.0,
        };

        floor.max((font.value() * LINE_HEIGHT_RATIO).ceil())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum FontSize {
    #[default]
    Small,
    Medium,
    Large,
}

impl FontSize {
    pub const ALL: &[FontSize] = &[FontSize::Small, FontSize::Medium, FontSize::Large];

    pub fn label(self) -> f32 {
        match self {
            FontSize::Small => 10.5,
            FontSize::Medium => 12.0,
            FontSize::Large => 13.5,
        }
    }

    pub fn value(self) -> f32 {
        match self {
            FontSize::Small => 12.5,
            FontSize::Medium => 14.5,
            FontSize::Large => 16.5,
        }
    }
}

/// Selectable card background color.
///
/// Alpha cannot be controlled per-element on Windows (the swapchain only offers
/// `Opaque`, so translucency is applied to the whole layered window), but the
/// *hue* is free — this is how you tune the card's look.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum BgColor {
    #[default]
    Auto,
    Slate,
    Graphite,
    Navy,
    Plum,
    Forest,
    Sand,
    White,
}

impl BgColor {
    pub const ALL: &[BgColor] = &[
        BgColor::Auto,
        BgColor::Slate,
        BgColor::Graphite,
        BgColor::Navy,
        BgColor::Plum,
        BgColor::Forest,
        BgColor::Sand,
        BgColor::White,
    ];

    /// The RGB triple, or `None` to keep the theme's color.
    pub fn rgb(self) -> Option<(f32, f32, f32)> {
        Some(match self {
            BgColor::Auto => return None,
            BgColor::Slate => (0.106, 0.118, 0.153),
            BgColor::Graphite => (0.13, 0.13, 0.14),
            BgColor::Navy => (0.07, 0.11, 0.22),
            BgColor::Plum => (0.17, 0.09, 0.20),
            BgColor::Forest => (0.07, 0.16, 0.12),
            BgColor::Sand => (0.76, 0.71, 0.60),
            BgColor::White => (0.96, 0.97, 0.99),
        })
    }
}

/// Selectable text color. Separate from the background so contrast stays
/// tunable even though alpha cannot be.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum TextColor {
    #[default]
    Auto,
    White,
    Silver,
    Cyan,
    Green,
    Amber,
    Red,
    Ink,
}

impl TextColor {
    pub const ALL: &[TextColor] = &[
        TextColor::Auto,
        TextColor::White,
        TextColor::Silver,
        TextColor::Cyan,
        TextColor::Green,
        TextColor::Amber,
        TextColor::Red,
        TextColor::Ink,
    ];

    /// The RGB triple, or `None` to keep the theme's color.
    pub fn rgb(self) -> Option<(f32, f32, f32)> {
        Some(match self {
            TextColor::Auto => return None,
            TextColor::White => (0.97, 0.98, 1.0),
            TextColor::Silver => (0.72, 0.76, 0.82),
            TextColor::Cyan => (0.20, 0.88, 0.95),
            TextColor::Green => (0.45, 0.92, 0.45),
            TextColor::Amber => (0.98, 0.76, 0.25),
            TextColor::Red => (0.98, 0.45, 0.45),
            TextColor::Ink => (0.06, 0.08, 0.12),
        })
    }
}

/// How the window achieves translucency.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Transparency {
    /// Per-pixel alpha through the window surface (best quality). Needs a
    /// backend that exposes alpha — Vulkan does, DX12 usually does not.
    #[default]
    Auto,
    /// Win32 layered window with uniform alpha (GPU-independent fallback: the
    /// whole window, text included, is composited at one alpha).
    Layered,
    Off,
}

impl Transparency {
    pub const ALL: &[Transparency] = &[Transparency::Auto, Transparency::Layered, Transparency::Off];

    /// Whether to composite through a Win32 layered window. `Auto` picks the
    /// layered path on Windows because DX12 drops surface alpha, and the
    /// per-pixel path elsewhere.
    pub fn uses_layered(self) -> bool {
        // Kept as an explicit `match` rather than `matches!`: the `Auto` arm is
        // `cfg`-dependent, so collapsing it would silently become wrong on the
        // other platform.
        match self {
            Transparency::Auto => cfg!(target_os = "windows"),
            Transparency::Layered => true,
            Transparency::Off => false,
        }
    }

    /// Whether the OS window is created with per-pixel alpha.
    pub fn transparent_window(self) -> bool {
        match self {
            Transparency::Auto => !cfg!(target_os = "windows"),
            _ => false,
        }
    }

    pub fn enabled(self) -> bool {
        !matches!(self, Transparency::Off)
    }
}

/// All persisted overlay preferences.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OverlayConfig {
    // appearance
    #[serde(default = "default_opacity")]
    pub opacity: f32,
    /// Card background swatch (`Auto` follows the theme).
    #[serde(default)]
    pub bg_color: BgColor,
    /// Text swatch (`Auto` follows the theme).
    #[serde(default)]
    pub text_color: TextColor,
    #[serde(default)]
    pub transparency: Transparency,
    /// Drop shadow under the card. Ignored where the mode cannot blend one —
    /// see [`Self::draws_shadow`].
    #[serde(default = "default_true")]
    pub shadow: bool,
    #[serde(default)]
    pub layout: Layout,
    #[serde(default)]
    pub density: Density,
    #[serde(default)]
    pub font_size: FontSize,
    #[serde(default = "default_true")]
    pub show_labels: bool,
    #[serde(default = "default_true")]
    pub show_units: bool,
    /// Collapse metric labels to a single letter (`Total` -> `T`, `CPU` -> `C`).
    #[serde(default)]
    pub abbreviated: bool,
    #[serde(default = "default_decimals")]
    pub decimals: u8,
    #[serde(default = "default_refresh")]
    pub refresh_secs: u32,

    // window
    #[serde(default = "default_true")]
    pub always_on_top: bool,
    /// Widest the widget may get, in logical pixels. The content is measured and
    /// the window stays at that width when it is narrower.
    #[serde(default = "default_width")]
    pub width: f32,
    /// Whether the overlay should be running. The main window flips this to
    /// `false` to close the overlay; the overlay only ever reads it, so a
    /// standalone `--overlay` run is unaffected by a stale value.
    #[serde(default = "default_true")]
    pub overlay_requested: bool,
    /// "Pin mode": the overlay stops responding to the mouse so it cannot be
    /// moved by accident. See `pin_click_through` for how far that goes.
    #[serde(default)]
    pub pin_mode: bool,
    /// When set, pin mode also makes the window transparent to the mouse, so
    /// every click lands on whatever is underneath. That is the strict overlay
    /// behaviour, but it means the right-click menu can no longer be reached and
    /// the overlay has to be released from the tray. Pinning always locks the
    /// position, whether or not this is set.
    #[serde(default = "default_true")]
    pub pin_click_through: bool,
    #[serde(default)]
    pub blur: bool,
    #[serde(default)]
    pub position: Option<(f32, f32)>,

    // content
    #[serde(default = "default_metrics")]
    pub metrics: Vec<Metric>,
    #[serde(default = "default_top_apps")]
    pub top_apps: usize,

    // ownership of the two settings that are not really ours
    /// Language override, or `None` to follow whatever the dashboard has saved.
    ///
    /// **This exists because the dashboard is optional.** It used to own the
    /// language outright: the overlay had no picker and no key, because there was
    /// always a dashboard to change it in. As a standalone program that
    /// assumption broke — a user who never opens the dashboard would be stuck in
    /// English with no way out, which is not a setting, it is a wall. `None` is
    /// the default and still means "follow the dashboard", so both ways of
    /// working stay true.
    #[serde(default)]
    pub language: Option<AppLanguage>,
    /// Theme override, or `None` to follow the dashboard's. Same reasoning as
    /// `language`, and it also decides what `bg_color`/`text_color` resolve
    /// `auto` to.
    #[serde(default)]
    pub theme: Option<ThemeChoice>,
    /// Whether to start WattSeal when the database has nothing recent in it.
    ///
    /// On by default because the alternative is a widget that shows nothing and
    /// gives no reason. Turn it off if WattSeal is started some other way — a
    /// service, a scheduled task — and starting a second copy would be wrong.
    #[serde(default = "default_true")]
    pub launch_wattseal: bool,
    /// The shortcut that releases a pinned, click-through widget, written
    /// `ctrl+alt+o`.
    ///
    /// **Configurable because a fixed key cannot be relied on.** The first version
    /// hard-coded `Ctrl+Alt+O`, and on the machine this was written on that
    /// combination was already owned by another program — as were fifteen of
    /// twenty other plausible candidates. `RegisterHotKey` reports that as a plain
    /// failure, so a fixed default is an escape hatch that may not exist, which is
    /// the one thing an escape hatch cannot be.
    ///
    /// Unset means [`crate::winlayer::DEFAULT_HOTKEY`]. An unreadable value is
    /// treated the same way rather than failing the whole config: a shortcut is
    /// not worth losing every other setting over.
    #[serde(default)]
    pub escape_hotkey: Option<String>,
}

fn default_opacity() -> f32 {
    0.80
}
fn default_width() -> f32 {
    140.0
}
fn default_true() -> bool {
    true
}
fn default_decimals() -> u8 {
    1
}
fn default_refresh() -> u32 {
    1
}
fn default_top_apps() -> usize {
    3
}
fn default_metrics() -> Vec<Metric> {
    Metric::DEFAULT_ORDER.to_vec()
}

/// Identifies this process and this save, so two writers never share a staging
/// file.
///
/// Where this writer stages its copy before renaming it onto `path`.
///
/// The name carries the writer's identity because there is more than one writer.
/// The tray toggles the pin from the *main* process (`overlay::toggle_pin`) while
/// the overlay process may be writing a setting of its own at the same moment, and
/// a shared staging name let them destroy each other: one renamed the file away,
/// and the other's rename then failed, silently dropping whatever the user had
/// just done.
///
/// It only has to be unique among writers that are *concurrently* writing, so the
/// process id and a per-process counter are enough — a historical name does not
/// collide with anything.
fn staging_path(path: &Path) -> PathBuf {
    path.with_file_name(format!("{CONFIG_FILENAME}.{}.tmp", writer_id()))
}

/// Moves an unreadable config aside instead of letting it be overwritten.
///
/// A missing file is nothing to preserve, and a readable one is not at risk.
/// Only the third case matters: a file that exists and does not parse, which is
/// what a hand-edit with a typo looks like. Copy rather than move, because the
/// write is about to replace the original anyway and a failure here must not stop
/// it — this is a courtesy, not a gate.
fn keep_if_unreadable(path: &Path) {
    let Ok(contents) = std::fs::read_to_string(path) else {
        return;
    };
    if serde_json::from_str::<OverlayConfig>(&contents).is_ok() {
        return;
    }

    let kept = path.with_extension("json.unreadable");
    // Only the first time: once kept, the next save finds a readable file and
    // leaves the copy alone, so a user who never looks at it accumulates one
    // file rather than one per save.
    if !kept.exists() {
        let _ = std::fs::write(&kept, &contents);
    }
}

/// The process id separates the main process from the overlay; the counter
/// separates two threads of the same process, which is enough because a rename
/// only ever has to beat a *concurrent* writer, not a historical one.
fn writer_id() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let sequence = NEXT.fetch_add(1, Ordering::Relaxed);
    format!("{}-{sequence}", std::process::id())
}

// ---- test-only location override ----
//
// The tray, the dashboard and the overlay agree on one file, so its location
// cannot be a parameter without threading it through the public API — which
// would put a testing seam into the product for no product reason. It is
// overridden here instead, under `#[cfg(test)]`, and the tests that use it hold
// `TEST_PATH_LOCK` so only one of them runs at a time.
//
// The lock is needed even though each test gets a file of its own: **the path is
// a process global**, so a second test entering early would replace it, and the
// first test would then read and write the second one's file. Distinct
// directories fix nothing on their own. Taking the lock away makes
// `closing_is_what_releases_a_pinned_click_through_widget` fail on the first
// run, so this is load-bearing rather than decorative.

#[cfg(test)]
static TEST_PATH: std::sync::Mutex<Option<PathBuf>> = std::sync::Mutex::new(None);

#[cfg(test)]
fn test_path_override() -> Option<PathBuf> {
    TEST_PATH.lock().ok().and_then(|path| path.as_ref().cloned())
}

/// Points the shared config at `path` until the returned guard is dropped.
///
/// The guard serialises these tests against each other. The reason is the path,
/// not the file: `TEST_PATH` is a process global, so two tests running side by
/// side would leave one of them reading and writing the other's file. That is
/// why the lock is returned rather than taken inside — the caller has to hold it
/// for the whole test, which `#[must_use]` on a value it cannot drop early
/// expresses for it.
#[cfg(test)]
pub(crate) fn override_path(path: PathBuf) -> PathOverride {
    static TEST_PATH_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let lock = TEST_PATH_LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    *TEST_PATH.lock().unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(path);
    PathOverride { _lock: lock }
}

/// Restores the real location when dropped.
#[cfg(test)]
pub(crate) struct PathOverride {
    _lock: std::sync::MutexGuard<'static, ()>,
}

#[cfg(test)]
impl Drop for PathOverride {
    fn drop(&mut self) {
        *TEST_PATH.lock().unwrap_or_else(|poisoned| poisoned.into_inner()) = None;
    }
}

impl Default for OverlayConfig {
    fn default() -> Self {
        Self {
            opacity: default_opacity(),
            bg_color: BgColor::default(),
            text_color: TextColor::default(),
            transparency: Transparency::default(),
            shadow: true,
            layout: Layout::default(),
            density: Density::default(),
            font_size: FontSize::default(),
            show_labels: true,
            show_units: true,
            abbreviated: false,
            decimals: default_decimals(),
            refresh_secs: default_refresh(),
            always_on_top: true,
            width: default_width(),
            // `false`, where the *key's* serde default is `true`.
            //
            // This default is only ever reached when there is no readable file,
            // and `is_requested` answers "no file" with `false`. Having the two
            // disagree meant that pinning from the tray on a fresh install —
            // `toggle_pin` on a missing file — invented a config claiming the
            // overlay was running, and the dashboard's footer then offered to
            // hide a widget that was never opened.
            //
            // A file that *has* the flag missing is a different question: that is
            // a file written before the flag existed, where "running" was the
            // only behaviour there was, so it keeps `default_true` above.
            overlay_requested: false,
            pin_mode: false,
            pin_click_through: true,
            blur: false,
            position: None,
            metrics: default_metrics(),
            top_apps: default_top_apps(),
            // `None` for both: "follow the dashboard" is what a widget that has
            // never been configured should do, and it is also what keeps the
            // dashboard authoritative for anyone who opens it.
            language: None,
            theme: None,
            launch_wattseal: true,
            escape_hotkey: None,
        }
    }
}

impl OverlayConfig {
    /// Config file path (next to the executable).
    pub fn path() -> PathBuf {
        #[cfg(test)]
        if let Some(path) = test_path_override() {
            return path;
        }

        std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|dir| dir.join(CONFIG_FILENAME)))
            .unwrap_or_else(|| PathBuf::from(CONFIG_FILENAME))
    }

    /// Path of the single-instance lock, beside the config file.
    ///
    /// Next to the executable like the config, so two installations in different
    /// folders get their own lock instead of excluding one another — and
    /// `overlay_config.json*` in `.gitignore` already covers this name.
    pub fn lock_path() -> PathBuf {
        Self::path().with_file_name(format!("{CONFIG_FILENAME}.lock"))
    }

    pub fn load() -> Option<Self> {
        Self::load_from(&Self::path())
    }

    /// [`OverlayConfig::load`] against an explicit path.
    pub fn load_from(path: &Path) -> Option<Self> {
        let contents = std::fs::read_to_string(path).ok()?;
        serde_json::from_str(&contents).ok()
    }

    pub fn save(&self) -> bool {
        self.save_to(&Self::path())
    }

    /// [`OverlayConfig::save`] against an explicit path.
    ///
    /// The overlay is the only writer of this file, but not the only reader: the
    /// tray and the dashboard poll it, so the write goes through a staging file
    /// rather than truncating the real one in place.
    pub fn save_to(&self, path: &Path) -> bool {
        let Ok(json) = serde_json::to_string_pretty(self) else {
            return false;
        };

        // Before overwriting something that cannot be read back. A reader that
        // falls back to the defaults — which every one of them does — would
        // otherwise replace a hand-edited file with a typo in it, silently, and
        // the documentation tells people this file is safe to edit by hand.
        // Keeping the unreadable one costs nothing and turns a lost afternoon of
        // tuning into a rename.
        keep_if_unreadable(path);

        // Written beside the real file and renamed onto it: a poll landing
        // mid-write would read half a document, parse as nothing, and fall back
        // to the defaults.
        let staging = staging_path(path);

        if std::fs::write(&staging, json).is_err() {
            return false;
        }

        std::fs::rename(&staging, path).is_ok()
    }

    /// Whether the card actually draws its drop shadow.
    ///
    /// A shadow is per-pixel alpha, so it can only be blended where the window
    /// surface carries some. The layered path composites the whole window at one
    /// constant alpha, which flattens the soft edge into a dark ring around the
    /// card; with transparency off there is nothing behind the window to blend
    /// into either. In both modes the setting is dropped rather than honoured
    /// badly.
    pub fn draws_shadow(&self) -> bool {
        self.shadow && self.transparency.transparent_window()
    }

    /// Clamps `top_apps` into the supported 1..=8 range.
    pub fn top_apps(&self) -> usize {
        self.top_apps.clamp(1, 8)
    }

    /// Number of rows the vertical layout renders, given `app_rows` process rows
    /// actually available.
    ///
    /// The observed count rather than the configured one. Before the collector's
    /// first process sample there are none, and reserving the configured three
    /// made the widget sit two rows taller than its own content until the sample
    /// landed — which is the same class of bug as fitting the width to numbers
    /// that are not there yet.
    fn content_rows(&self, app_rows: usize) -> usize {
        let mut rows = 0usize;
        for metric in &self.metrics {
            rows += if metric.is_multi() { app_rows } else { 1 };
        }
        rows.max(1)
    }

    /// Window height that fits the metrics currently enabled.
    ///
    /// `app_rows` is how many process rows there are to draw right now, rather
    /// than how many the user asked for — see the note on `content_rows` above.
    pub fn fitted_height(&self, app_rows: usize) -> f32 {
        let pad = self.density.padding();
        let spacing = self.density.spacing();
        let row = self.density.row_height(self.font_size);

        // No header is drawn in metrics mode, so it must not be counted here —
        // doing so used to leave a visible empty strip under the text.
        let content = match self.layout {
            Layout::Horizontal => row,
            Layout::Vertical => {
                let rows = self.content_rows(app_rows) as f32;
                rows * row + (rows - 1.0).max(0.0) * spacing
            }
        };
        // A single horizontal line needs far less room than a stacked list.
        let floor = match self.layout {
            Layout::Horizontal => 16.0,
            Layout::Vertical => 32.0,
        };
        (pad * 2.0 + content).ceil().max(floor)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A private directory under the temp directory, counted rather than stamped.
    ///
    /// Counted because the Windows clock reads the same value for a whole ~15 ms
    /// tick, so two tests starting in the same tick were handed the same name.
    /// Private because the staging files sit *beside* the config and the temp
    /// directory is shared: scanning it for leftovers would report what another
    /// test in this binary happens to be writing at the time.
    fn unique_dir(name: &str) -> PathBuf {
        use std::sync::atomic::{AtomicU64, Ordering};
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let unique = NEXT.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("wattseal-config-{name}-{}-{unique}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("the scratch directory can be made");
        dir
    }

    /// Removes a scratch directory, and **fails the test** if it cannot.
    ///
    /// Not `let _ =`: a cleanup that fails quietly is indistinguishable from a
    /// cleanup that never existed, which is how two directories sat in the
    /// user's temp folder on every run while the suite reported itself green.
    fn cleanup(dir: &Path) {
        match std::fs::remove_dir_all(dir) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => panic!("could not remove {dir:?}: {error}"),
        }
    }
    #[test]
    fn unknown_keys_are_ignored_so_the_file_stays_hand_editable() {
        let config: OverlayConfig =
            serde_json::from_str(r#"{"layout":"horizontal","nonsense":1}"#).expect("valid json");

        assert_eq!(config.layout, Layout::Horizontal);
        assert_eq!(config.decimals, default_decimals());
    }

    #[test]
    fn the_documented_keys_are_exactly_the_keys_that_exist() {
        // The config file is something users are told they can hand-edit, so a
        // field that exists but is undocumented is a setting nobody can find,
        // and a documented key that no longer exists is one that silently stops
        // doing anything. Both are the kind of drift nobody notices, because the
        // code and the documentation each look fine on their own.
        //
        // The reference is the serialised form, not the struct's field names:
        // that is what a hand-edited file actually has to get right.
        //
        // One trap: cargo rebuilds on the doc's timestamp, so an edit that lands
        // with an *older* one (a restore from a copy, a checkout that preserves
        // times) leaves this asserting against a stale copy of the document.
        // If this test ever fails on a key you did not just rename, the first
        // thing to check is whether the build is actually current.
        let documented: Vec<&str> = include_str!("../../doc/overlay.md")
            .lines()
            .filter_map(|line| line.strip_prefix("| `"))
            .filter_map(|rest| rest.split('`').next())
            .filter(|key| !key.is_empty() && key.chars().all(|c| c.is_ascii_lowercase() || c == '_'))
            .collect();

        let mut written: Vec<String> = serde_json::to_value(OverlayConfig::default())
            .expect("the default config serialises")
            .as_object()
            .expect("a struct serialises to an object")
            .keys()
            .cloned()
            .collect();
        written.sort();

        let mut expected = documented.clone();
        expected.sort();

        assert_eq!(
            expected, written,
            "doc/overlay.md and the config file disagree about the keys"
        );
        assert_eq!(
            documented.len(),
            expected.len(),
            "a key is documented twice, so the table above is not one key per row"
        );
    }

    #[test]
    fn the_option_counts_the_readme_advertises_are_the_ones_there_are() {
        // "three densities, three text sizes" is as much a promise as the metric
        // list is: add a fourth and the sentence is wrong, drop one and it is
        // wrong the other way.
        //
        // The numbers are spelled out in prose, so they are parsed as words. That
        // is deliberate rather than lazy — it means rewriting the sentence to
        // "3 densities" fails loudly here instead of quietly comparing nothing.
        fn words(text: &str) -> Option<usize> {
            const ONES: [&str; 12] = [
                "one", "two", "three", "four", "five", "six", "seven", "eight", "nine", "ten", "eleven", "twelve",
            ];
            let claimed = text.split_whitespace().next()?;
            ONES.iter().position(|w| *w == claimed).map(|n| n + 1)
        }

        let readme = include_str!("../../README.md");
        let line = readme
            .lines()
            .find(|line| line.contains("three densities") || line.contains("densities,"))
            .expect("the README still describes styling the widget");

        let count_before = |noun: &str| -> usize {
            let at = line
                .find(noun)
                .unwrap_or_else(|| panic!("the README no longer mentions {noun}"));
            line[..at]
                .split_whitespace()
                .rev()
                .take(2)
                .collect::<Vec<_>>()
                .iter()
                .rev()
                .find_map(|w| words(w))
                .unwrap_or_else(|| panic!("the README does not say how many {noun} there are, before '{noun}'"))
        };

        assert_eq!(
            count_before("densities"),
            Density::ALL.len(),
            "README's density count is wrong"
        );
        assert_eq!(
            count_before("text sizes"),
            FontSize::ALL.len(),
            "README's text size count is wrong"
        );
        assert_eq!(
            Layout::ALL.len(),
            2,
            "the README offers a vertical and a horizontal layout"
        );
    }

    #[test]
    fn every_metric_the_readme_advertises_is_one_the_widget_can_show() {
        // The README is how a user finds out what they can put on the bar. A
        // metric that exists but is not listed is one nobody will ever try; a
        // listed metric that does not exist is one they go looking for.
        //
        // The two are written differently on purpose — the README is prose
        // ("CPU", "top apps"), the enum is data ("cpu", "top_apps") — so the
        // comparison normalises rather than insists they match character for
        // character.
        let readme = include_str!("../../README.md");
        let line = readme
            .lines()
            .find(|line| line.contains("Choose which metrics appear"))
            .expect("the README still describes choosing metrics");

        let listed: Vec<String> = line
            .split_once('(')
            .and_then(|(_, rest)| rest.split_once(')'))
            .map(|(inside, _)| inside)
            .expect("the sentence names the metrics in parentheses")
            .split(',')
            .map(|name| name.trim().to_lowercase().replace(' ', "_"))
            .collect();

        let mut actual: Vec<String> = Metric::ALL.iter().map(|m| m.id().to_string()).collect();
        actual.sort();
        let mut expected = listed.clone();
        expected.sort();

        assert_eq!(
            expected, actual,
            "README.md and the widget disagree about which metrics there are"
        );
    }

    #[test]
    fn there_is_exactly_one_opacity_knob() {
        // The panel comment and `doc/overlay.md` both say the widget has a single
        // opacity setting — and the stepper row's own comment used to say "the two
        // opacity settings", which the code has never had. Over-counting in a
        // comment is its own kind of wrong: it reads as though there were room for
        // a second one.
        //
        // Pinned where the count is actually observable: the keys a config file
        // ends up with. Adding a second opacity knob without updating the docs and
        // the comments fails here.
        let json = serde_json::to_string(&OverlayConfig::default()).expect("the config serialises");
        let value: serde_json::Value = serde_json::from_str(&json).expect("and parses back");
        let object = value.as_object().expect("the config is a JSON object");

        let opacity_keys: Vec<&String> = object.keys().filter(|key| key.contains("opacity")).collect();
        assert_eq!(
            opacity_keys,
            vec!["opacity"],
            "the config has {} opacity setting(s)",
            opacity_keys.len()
        );
    }

    #[test]
    fn each_transparency_setting_says_plainly_what_it_does() {
        // These three answers decide whether the window gets `WS_EX_LAYERED`,
        // whether it is created with a per-pixel alpha surface, and whether
        // either happens at all. `Auto` was tested; the two explicit choices had
        // no test at all, so "pick Layered" was never checked to mean layered.
        let on_windows = cfg!(target_os = "windows");

        assert_eq!(
            Transparency::Auto.uses_layered(),
            on_windows,
            "Auto should pick this platform's path"
        );
        assert!(
            Transparency::Layered.uses_layered(),
            "asking for Layered did not give a layered window"
        );
        assert!(
            !Transparency::Off.uses_layered(),
            "Off should not give a layered window"
        );

        // Only `Auto` ever asks for the per-pixel surface — the other two would
        // contradict the mode above.
        assert_eq!(Transparency::Auto.transparent_window(), !on_windows);
        assert!(
            !Transparency::Layered.transparent_window(),
            "Layered must not also ask for per-pixel alpha"
        );
        assert!(!Transparency::Off.transparent_window());

        // Turning transparency off switches it off on every platform; leaving it
        // on says so, whatever the mode.
        for mode in Transparency::ALL {
            assert_eq!(
                mode.enabled(),
                !matches!(mode, Transparency::Off),
                "{mode:?} is off but says it is on"
            );
        }
    }

    #[test]
    fn every_colour_the_settings_offer_is_one_the_renderer_accepts() {
        // These triples go straight to `Color::from_rgb`, whose components are
        // `0..=1`. A transposed digit gives a colour outside that range and the
        // widget draws something the user never picked — and **nothing reports
        // an error**, because the value is a perfectly ordinary `f32`. This is
        // the cheapest kind of typo to make and the easiest to miss.
        for swatch in BgColor::ALL {
            let Some((r, g, b)) = swatch.rgb() else { continue };
            for (component, value) in [('r', r), ('g', g), ('b', b)] {
                assert!(
                    (0.0..=1.0).contains(&value),
                    "background {swatch:?} has {component} = {value}, outside 0..=1"
                );
            }
        }
        for swatch in TextColor::ALL {
            let Some((r, g, b)) = swatch.rgb() else { continue };
            for (component, value) in [('r', r), ('g', g), ('b', b)] {
                assert!(
                    (0.0..=1.0).contains(&value),
                    "text {swatch:?} has {component} = {value}, outside 0..=1"
                );
            }
        }
    }

    #[test]
    fn no_document_has_a_character_somebody_mangled() {
        // U+FFFD is what a decoder leaves behind when it cannot read a byte
        // sequence. In a document it is never intentional, and here it has not
        // been: three of them appeared because a scripted edit wrote Chinese
        // text back through an encoding that mangled it, and each one landed
        // silently in a sentence that still read fine around the hole.
        //
        // The docs in this fork are full of Chinese, Romanian (`Română`,
        // `Français`) and arrows (`→`), so this is a live hazard rather than a
        // theoretical one. `include_str!` rather than a runtime read, so a
        // missing file is a compile error instead of a test that passes because
        // it found nothing.
        const REPLACEMENT: char = '\u{fffd}';

        for (name, text) in [
            ("README.md", include_str!("../../README.md")),
            ("doc/overlay.md", include_str!("../../doc/overlay.md")),
            ("FORK_ROADMAP.md", include_str!("../../FORK_ROADMAP.md")),
        ] {
            let Some(offset) = text.find(REPLACEMENT) else { continue };
            let line = text[..offset].lines().count();
            let start = text[..offset].rfind('\n').map_or(offset, |n| n + 1);
            let end = offset + text[offset..].find('\n').unwrap_or(text.len() - offset);
            panic!(
                "{name} line {line} has a mangled character: {}\n    fix it by editing the file with a UTF-8 aware tool, not by writing it back from a script",
                text[start..end].trim()
            );
        }
    }

    #[test]
    fn the_documented_defaults_are_the_defaults_the_file_is_created_with() {
        // Keys matching is not enough. A wrong default in the table is worse
        // than a missing one: the reader changes a value expecting a starting
        // point that is not where the file starts, and has no way to tell.
        //
        // Numbers and booleans are compared as values, so `0.80` and `0.8` are
        // the same and only a real difference is reported.
        let mut documented: Vec<(String, String)> = Vec::new();
        for line in include_str!("../../doc/overlay.md").lines() {
            let Some(rest) = line.strip_prefix("| `") else {
                continue;
            };
            let mut cells = rest.split('|').map(str::trim);
            let Some(key) = cells.next().and_then(|k| k.strip_suffix('`')) else {
                continue;
            };
            if !key.chars().all(|c| c.is_ascii_lowercase() || c == '_') {
                continue;
            }
            if let Some(value) = cells.next().and_then(|v| v.strip_prefix('`')) {
                documented.push((key.to_string(), value.trim_end_matches('`').to_string()));
            }
        }

        let written = serde_json::to_value(OverlayConfig::default()).expect("serialisable");

        for (key, value) in documented {
            let actual = &written[&key];
            let same = match actual {
                // Compared as `f32`, because that is the type they are stored
                // as: widening `0.80f32` to `f64` gives `0.800000011920929`, so a
                // tolerance-based comparison would be reporting the number
                // format rather than a difference a reader could ever notice.
                serde_json::Value::Number(number) => value
                    .parse::<f32>()
                    .is_ok_and(|documented| number.as_f64().is_some_and(|actual| documented == actual as f32)),
                serde_json::Value::Bool(flag) => value == flag.to_string(),
                serde_json::Value::String(text) => value == *text,
                // A list is written the way it is read: `total, cpu, gpu`.
                serde_json::Value::Array(items) => value
                    .split(',')
                    .map(str::trim)
                    .eq(items.iter().filter_map(serde_json::Value::as_str)),
                // `position` is null until the widget has been moved, and the
                // table says "unset" rather than `null`.
                serde_json::Value::Null => value == "unset",
                // No field has a struct-valued default, so there is nothing to compare.
                serde_json::Value::Object(_) => true,
            };
            assert!(
                same,
                "doc/overlay.md says `{key}` defaults to `{value}`, the file defaults to {actual}"
            );
        }
    }

    #[test]
    fn a_hand_edited_file_that_will_not_parse_is_kept_rather_than_overwritten() {
        // The documentation tells people this file is safe to edit by hand. A
        // stray comma would otherwise be answered by silently replacing everything
        // they had tuned with the defaults — the reader falls back, and the next
        // save writes that fallback over the top.
        let dir = unique_dir("unreadable");
        let path = dir.join(CONFIG_FILENAME);
        let hand_edited = r#"{"width": 300,}"#; // the trailing comma
        std::fs::write(&path, hand_edited).unwrap();

        assert!(
            OverlayConfig::load_from(&path).is_none(),
            "the fixture is supposed to be broken"
        );

        let saved = OverlayConfig::default();
        assert!(saved.save_to(&path));
        assert_eq!(OverlayConfig::load_from(&path).unwrap().width, saved.width);

        let kept = path.with_extension("json.unreadable");
        assert!(kept.exists(), "the file the user wrote is gone");
        assert_eq!(
            std::fs::read_to_string(&kept).unwrap(),
            hand_edited,
            "the kept copy is not what they wrote"
        );

        // A readable file is not copied. The kept copy from above is removed first, so
        // what is checked is that *this* save creates nothing — not that an
        // earlier one never did.
        let _ = std::fs::remove_file(&kept);
        assert!(OverlayConfig::load_from(&path).is_some(), "the file is readable again");
        assert!(saved.save_to(&path));
        assert!(!kept.exists(), "a readable file was preserved as though it were broken");

        cleanup(&dir);
    }

    #[test]
    fn no_readable_file_means_the_overlay_is_not_running() {
        // Two ways of asking the same question have to agree, because two
        // processes ask it: `Default` is what a caller gets when there is no
        // file to read, and `is_requested` treats a missing file as "not open".
        //
        // They did not. Pinning from the tray on a fresh install went through
        // `Default`, wrote a file claiming the overlay was running, and the
        // dashboard's footer then offered to hide a widget that was never
        // opened — and the tray's `Toggle Overlay` tried to kill it.
        assert!(!OverlayConfig::default().overlay_requested);

        // A file that predates the flag is a different question, and keeps the
        // behaviour that had only one answer: the overlay runs.
        let legacy: OverlayConfig = serde_json::from_str(r#"{"width":140.0}"#).expect("valid json");
        assert!(
            legacy.overlay_requested,
            "a file written before this flag existed stopped opening the overlay"
        );

        // And the current version always writes the key, so the two paths cannot
        // drift apart in practice.
        let current = serde_json::to_value(OverlayConfig::default()).unwrap();
        assert!(current.get("overlay_requested").is_some());
    }

    #[test]
    fn missing_keys_fall_back_to_their_defaults() {
        let config: OverlayConfig = serde_json::from_str("{}").expect("valid json");

        assert_eq!(config.opacity, default_opacity());
        assert_eq!(config.width, default_width());
        assert!(config.always_on_top);
        assert!(!config.pin_mode);
    }

    #[test]
    fn the_shadow_is_dropped_where_no_surface_alpha_can_carry_it() {
        let mut config = OverlayConfig {
            shadow: true,
            ..OverlayConfig::default()
        };

        // Neither of these composites per pixel: the layered window carries one
        // alpha for everything, and an opaque window has nothing to fade into.
        config.transparency = Transparency::Layered;
        assert!(!config.draws_shadow());

        config.transparency = Transparency::Off;
        assert!(!config.draws_shadow());
    }

    #[test]
    fn the_top_apps_count_is_clamped_to_what_can_be_listed() {
        let mut config = OverlayConfig {
            top_apps: 0,
            ..OverlayConfig::default()
        };
        assert_eq!(config.top_apps(), 1);

        config.top_apps = 99;
        assert_eq!(config.top_apps(), 8);
    }

    #[test]
    fn two_writers_do_not_destroy_each_others_save() {
        // The tray toggles the pin from the main process while the overlay
        // process may be writing a setting of its own. With one shared staging
        // name, whichever renamed first took the file and the other's rename
        // failed — so the setting the user had just made was silently dropped.
        // Its own directory, because the staging files sit beside the config and the
        // temp directory is shared: scanning it for leftovers would report what
        // another test in this binary happens to be writing at the time.
        let dir = unique_dir("two-writers");
        let path = dir.join(CONFIG_FILENAME);

        let mut main_process = OverlayConfig {
            pin_mode: true,
            ..OverlayConfig::default()
        };
        let mut overlay_process = OverlayConfig {
            decimals: 3,
            ..OverlayConfig::default()
        };

        // Two writers, two staging files: the whole failure was that they shared
        // one name, so one renamed it away and the other's rename failed.
        let main_staged = dir.join(format!("{CONFIG_FILENAME}.main.tmp"));
        let overlay_staged = dir.join(format!("{CONFIG_FILENAME}.overlay.tmp"));
        std::fs::write(&main_staged, serde_json::to_string(&main_process).unwrap()).unwrap();
        std::fs::write(&overlay_staged, serde_json::to_string(&overlay_process).unwrap()).unwrap();

        // Either order, both writers still land.
        std::fs::rename(&main_staged, &path).unwrap();
        std::fs::rename(&overlay_staged, &path).unwrap();
        assert!(OverlayConfig::load_from(&path).is_some());

        // And the real thing writes to distinct names, so two of them can run at
        // once without either losing what the user just did.
        main_process.decimals = 0;
        overlay_process.decimals = 2;
        assert!(main_process.save_to(&path));
        assert!(overlay_process.save_to(&path));
        assert_eq!(OverlayConfig::load_from(&path).unwrap().decimals, 2);

        // Each save leaves no staging file behind of its own.
        let leftovers: Vec<PathBuf> = std::fs::read_dir(&dir)
            .unwrap()
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|entry| entry.extension().is_some_and(|ext| ext == "tmp"))
            .collect();
        assert!(leftovers.is_empty(), "staging files left behind: {leftovers:?}");

        cleanup(&dir);
    }

    #[test]
    fn two_writers_never_stage_to_the_same_name() {
        // The actual fix, stated as a property rather than demonstrated: every
        // write stages to a name of its own.
        //
        // Everything else in this area could pass with a shared name. Two
        // *sequential* saves each rename their staging file away before the next
        // one writes, so the collision never happens and both succeed; a test
        // that only checks "two saves both worked" would therefore stay green
        // after a regression straight back to the bug — and the bug it misses is
        // the one that silently drops what the user just did.
        let path = PathBuf::from(CONFIG_FILENAME);
        let first = staging_path(&path);
        let second = staging_path(&path);
        assert_ne!(
            first, second,
            "two writes in the same process staged to one name; across processes the \
             writer id is the only thing separating them"
        );

        // And the name still belongs to the file being written, so two different
        // config files never share one either.
        let elsewhere = staging_path(Path::new("somewhere-else/overlay_config.json"));
        assert_eq!(elsewhere.parent().unwrap(), Path::new("somewhere-else"));
        assert_ne!(elsewhere, first);
    }

    #[test]
    fn a_config_written_before_the_theme_moved_still_loads() {
        // The theme used to be a key here. It is read from `ui_settings` now, and
        // an old file still carries the key — which has to be ignored rather than
        // rejected, or upgrading would reset every other setting too.
        let config: OverlayConfig =
            serde_json::from_str(r#"{"theme":"light","width":200.0,"layout":"horizontal","abbreviated":true}"#)
                .expect("a file with a retired key still loads");

        assert_eq!(config.width, 200.0);
        assert_eq!(config.layout, Layout::Horizontal);
        assert!(config.abbreviated);
    }

    #[test]
    fn a_saved_config_reads_back_unchanged() {
        // The round trip every setting makes on every change the user makes, so
        // a field that fails to serialize is caught here rather than at exit.
        // Every field set to something other than its default, so the round trip cannot
        // pass by accident on fields that happen to serialise trivially.
        let original = OverlayConfig {
            opacity: 0.42,
            bg_color: BgColor::Plum,
            text_color: TextColor::Amber,
            transparency: Transparency::Off,
            shadow: false,
            layout: Layout::Horizontal,
            density: Density::Normal,
            font_size: FontSize::Large,
            show_labels: false,
            show_units: false,
            abbreviated: true,
            decimals: 3,
            refresh_secs: 5,
            always_on_top: false,
            width: 321.0,
            overlay_requested: true,
            pin_mode: true,
            pin_click_through: false,
            blur: true,
            position: Some((12.0, 34.0)),
            metrics: vec![Metric::Gpu, Metric::Disk, Metric::Total],
            top_apps: 6,
            // Set to non-default on purpose, like every other field above: a
            // round trip that passes on `None` and `true` proves nothing about
            // whether these survive being written at all.
            language: Some(AppLanguage::Romanian),
            theme: Some(ThemeChoice::Light),
            launch_wattseal: false,
            escape_hotkey: Some("ctrl+shift+f7".to_string()),
        };

        let json = serde_json::to_string(&original).expect("every field serializes");
        let read_back: OverlayConfig = serde_json::from_str(&json).expect("every field reads back");

        assert_eq!(format!("{original:?}"), format!("{read_back:?}"));
    }

    #[test]
    fn the_fitted_height_matches_the_rows_it_is_built_from() {
        // The height is derived, not a constant, and both layouts need it to hold
        // the content they claim to.
        for layout in [Layout::Vertical, Layout::Horizontal] {
            for density in [Density::Ultra, Density::Compact, Density::Normal] {
                for font in [FontSize::Small, FontSize::Medium, FontSize::Large] {
                    let mut config = OverlayConfig {
                        layout,
                        density,
                        font_size: font,
                        ..OverlayConfig::default()
                    };
                    config.metrics = Metric::ALL.to_vec();
                    config.top_apps = 8;

                    let tall = config.fitted_height(8);
                    assert!(
                        tall >= density.row_height(font),
                        "{layout:?}/{density:?}/{font:?} reserves {tall}px, less than one row"
                    );

                    config.metrics = vec![Metric::Total];
                    let one = config.fitted_height(0);
                    if layout == Layout::Vertical {
                        assert!(
                            tall > one,
                            "{density:?}/{font:?}: eight metrics per row fit no taller than one"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn a_widget_with_nothing_to_show_is_still_a_widget() {
        // Every metric switched off, or switched on but with no sample behind
        // it, has to leave something on screen — not a sliver, and not a window
        // sized for rows that were never drawn.
        let config = OverlayConfig {
            metrics: Vec::new(),
            top_apps: 3,
            ..OverlayConfig::default()
        };
        assert!(config.fitted_height(0) >= 32.0, "the widget collapsed to nothing");
        assert!(config.fitted_height(0) >= config.density.padding() * 2.0);

        // Only the per-app metric, and no process sample yet.
        let config = OverlayConfig {
            metrics: vec![Metric::TopApps],
            top_apps: 3,
            ..OverlayConfig::default()
        };
        let empty = config.fitted_height(0);
        let with_three = config.fitted_height(3);
        assert!(
            with_three > empty,
            "three process rows fit in the same height as none: {with_three} vs {empty}"
        );

        // The reservation tracks what is there, not what was configured — the
        // widget used to sit two rows taller than its own content until the
        // collector's first process sample arrived.
        assert!(config.fitted_height(1) < config.fitted_height(3));

        // Zero and one app row are the same height on purpose. The floor keeps
        // the widget from collapsing to nothing, and at zero there is nothing to
        // draw, so reserving a second row would be reserving for nothing.
        assert_eq!(config.fitted_height(0), config.fitted_height(1));
    }
}
