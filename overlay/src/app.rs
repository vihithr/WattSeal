//! The overlay iced application: state, update loop and view.
//!
//! Fully self-contained — it reads the shared SQLite database produced by the
//! collector and owns its own state, theme and window. It shares no code with
//! the process that writes that database; see [`crate::source`].

use std::{collections::HashMap, path::PathBuf};

use iced::{
    Alignment, Background, Border, Color, Element, Font, Length, Padding, Shadow, Subscription, Task, Theme, Vector,
    event,
    font::{Family, Weight},
    time::{Duration, every},
    widget::{
        Button, Column, Container, Row, Space, Text, button, checkbox, mouse_area, pick_list, scrollable, slider,
    },
    window,
};

use crate::{
    config::{BgColor, Density, FontSize, Layout, Metric, OverlayConfig, TextColor, Transparency},
    language::AppLanguage,
    message::Message,
    source::{Availability, Source, UiSettings},
    theme::{self, Palette, ThemeChoice},
    translations::{self, Labeled, Language, Localize},
};

/// Monospace bold font for values (clean, aligned digits).
const FONT_VALUE: Font = Font {
    family: Family::Monospace,
    weight: Weight::Bold,
    ..Font::DEFAULT
};

/// The settings panel is laid out in two columns so it stays compact, and the
/// window is sized from what those columns actually hold — see
/// [`OverlayApp::settings_size`].
const SETTINGS_WIDTH: f32 = 560.0;

/// Smallest the panel ever gets. Below this the two columns stop being a
/// saving and the content is what it is.
const MIN_SETTINGS_HEIGHT: f32 = 300.0;

/// Gap kept between the panel and the edge of the monitor, the same one
/// `anchor_point` uses.
const SCREEN_MARGIN: f32 = 16.0;

/// Height of the drag handle above the columns.
const HEADER_HEIGHT: f32 = 8.0;

/// How many lines a hint is allowed to take.
///
/// The hints are full sentences in a column about 250 px wide, so one of them
/// is two lines on every language measured. Counting them as one would make the
/// estimate optimistic in exactly the case it exists for.
const HINT_LINES: f32 = 2.0;

/// Approximate character advance as a fraction of the font size, used by the
/// content-fitted width. The real metrics live in the renderer, so these are
/// deliberately a little generous to avoid clipping.
const LABEL_CHAR_W: f32 = 0.52;
const VALUE_CHAR_W: f32 = 0.62;

/// The width ladder. A measured width is rounded up to a multiple of this, so
/// the widget grows in visible steps instead of inching along every time a
/// digit gets wider. The `Width` setting snaps to the same rungs, and a change
/// smaller than one rung does not move the window at all.
const WIDTH_STEP: f32 = 12.0;

/// Narrowest usable width, matching the OS window minimum so a request is never
/// clamped behind our back.
const MIN_WIDTH: f32 = 24.0;

/// Range the `Width` slider offers.
const MIN_CHOICE_WIDTH: f32 = 60.0;
const MAX_CHOICE_WIDTH: f32 = 1200.0;

/// Gap between a label and its value, and between two entries of the horizontal
/// bar. Matches the row spacing both are built with.
const BAR_INNER_GAP: f32 = 6.0;

/// Padding inside each right-click menu segment, and the gap between segments.
const SEGMENT_PADDING: f32 = 6.0;
const MENU_GAP: f32 = 3.0;

const TOP_NAME_MAX: usize = 14;

/// Advance of the ellipsis, as a fraction of the font size. The renderer has
/// the real metrics; this is the same estimate the rest of the measuring uses.
const ELLIPSIS_W: f32 = 0.52;

const DECIMALS: &[u8] = &[0, 1, 2, 3];
const REFRESH: &[u32] = &[1, 2, 3, 5];
const TOP_APPS: &[usize] = &[1, 2, 3, 4, 5, 6, 8];

/// Advance of one character, as a fraction of the font size.
///
/// CJK glyphs are full-width — close to twice a Latin advance — and both the
/// labels and the `Top apps` process names can contain them. Measuring every
/// character at the Latin factor would under-measure a Chinese label by nearly
/// half, which the horizontal layout and the menu have no slack to absorb.
fn char_advance(character: char, latin_factor: f32) -> f32 {
    let full_width = matches!(
        character as u32,
        0x1100..=0x115F
            | 0x2E80..=0x303E
            | 0x3041..=0x33FF
            | 0x3400..=0x4DBF
            | 0x4E00..=0x9FFF
            | 0xA000..=0xA4CF
            | 0xAC00..=0xD7A3
            | 0xF900..=0xFAFF
            | 0xFE30..=0xFE6F
            | 0xFF00..=0xFF60
            | 0xFFE0..=0xFFE6
    );

    if full_width { latin_factor * 1.9 } else { latin_factor }
}

/// Approximate rendered width of `text` at `size`.
///
/// The renderer owns the real metrics, so this stays an estimate — deliberately
/// on the generous side, since a clipped value is worse than a little slack.
fn text_width(text: &str, size: f32, latin_factor: f32) -> f32 {
    text.chars().map(|c| char_advance(c, latin_factor)).sum::<f32>() * size
}

/// Rounds a measured width up to the next rung of the ladder: a fitted width
/// must never come out narrower than the text it has to hold.
fn width_up(width: f32) -> f32 {
    ((width / WIDTH_STEP).ceil() * WIDTH_STEP).max(MIN_WIDTH)
}

/// Rounds a chosen width to the nearest rung, so the `Width` slider lands on the
/// same ladder the measured widths use.
fn width_near(width: f32) -> f32 {
    ((width / WIDTH_STEP).round() * WIDTH_STEP).max(WIDTH_STEP)
}

/// Swaps `metric` with the neighbour `delta` places away, and reports whether it
/// moved.
///
/// Pure, so the ordering rules are tested rather than clicked: a metric that is
/// switched off has no place in the order, and either end of the list is a wall.
fn move_metric(order: &mut [Metric], metric: Metric, delta: isize) -> bool {
    let Some(index) = order.iter().position(|m| *m == metric) else {
        return false;
    };

    let target = index as isize + delta;
    if target < 0 || target as usize >= order.len() {
        return false;
    }

    order.swap(index, target as usize);
    true
}

struct BarItem {
    /// `None` when labels are switched off, or for an entry that has none.
    label: Option<String>,
    value: String,
    /// This row is a sentence explaining the widget's state, not a reading.
    ///
    /// It changes two things, and both are needed:
    /// - **It may be elided.** Readings are never cut, because a cut number reads
    ///   as a smaller number — but a sentence is already prose, and at the
    ///   default 140 px width a status that overflows is simply clipped by the
    ///   window, which is the same as not saying it.
    /// - **It is drawn in the label style**, muted and at label size. A status is
    ///   not a measurement, and dressing it as one would make it compete with
    ///   the numbers for attention at exactly the moment there are none.
    status: bool,
}

/// The inventory of one settings column: full-height rows, and the shorter
/// explanatory lines that sit under some of them.
///
/// The panel window is sized from these, so they are counted next to the code
/// that builds the column. A row that reaches one and not the other is the bug
/// this exists to make impossible — the window sizes itself without it and cuts
/// it off the bottom.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
struct SettingsColumn {
    rows: u32,
    hints: u32,
}

impl SettingsColumn {
    fn rows(rows: u32) -> Self {
        Self { rows, hints: 0 }
    }

    /// Sets the hint lines that sit under this column's rows.
    fn with_hints(mut self, count: u32) -> Self {
        self.hints = count;
        self
    }

    /// Adds one more full-height row — a control that is only shown sometimes.
    fn with_extra_row(mut self) -> Self {
        self.rows += 1;
        self
    }

    /// Adds one more hint line, which is what a control becomes when the
    /// platform cannot offer it.
    fn with_extra_hint(mut self) -> Self {
        self.hints += 1;
        self
    }

    fn height(self, row: f32, hint: f32, spacing: f32) -> f32 {
        let lines = self.rows + self.hints;
        if lines == 0 {
            return 0.0;
        }
        self.rows as f32 * row + self.hints as f32 * hint * HINT_LINES + (lines - 1) as f32 * spacing
    }
}

pub struct OverlayApp {
    config: OverlayConfig,
    /// Where the shared config lives. `None` is the real location — next to the
    /// executable, which is where the tray and the dashboard look too — and the
    /// three processes only agree because they all resolve it the same way.
    /// Tests point it at a scratch file rather than sharing one.
    config_path: Option<PathBuf>,
    window_id: Option<window::Id>,
    window_raw: Option<u64>,
    power: HashMap<String, f64>,
    top_apps: Vec<(String, f64)>,
    /// Last `overlay_requested` value seen in the shared config. Only a true to
    /// false transition closes the overlay.
    requested: bool,
    show_settings: bool,
    /// True while the bar's content is replaced by the right-click menu.
    show_menu: bool,
    /// Size last requested from the OS, so the auto-fit does not resize — and
    /// flicker — on every tick.
    applied: iced::Size,
    /// The monitor the window is on, once the OS has told us. The settings
    /// panel is never taller than this.
    monitor: Option<iced::Size>,
    /// Language the dashboard is set to; the overlay follows it.
    language: Language,
    /// Scheme the dashboard's theme resolves to. Follows it for the same reason
    /// the language does: one place decides how the application looks.
    theme: ThemeChoice,
    /// Read-only handle on the collector's database, gated on its generation.
    source: Source,
    /// Whether this widget has already tried to start WattSeal. Once is enough:
    /// see [`Self::ensure_wattseal_is_running`].
    launch_attempted: bool,
    /// The WattSeal this widget started, if it did. Held so the handle is not
    /// dropped — dropping a `Child` does not kill it, but keeping it lets the
    /// tick notice a copy that died.
    launched: Option<crate::launcher::Supervised>,
    /// Set when there is no WattSeal beside the executable to start, so the
    /// window can say why it is showing placeholders instead of leaving the user
    /// to work it out.
    wattseal_missing: bool,
    /// Whether the escape shortcut could be registered against our window.
    ///
    /// `false` is an ordinary answer — another program may already own the key —
    /// and the widget stays fully usable; it just has no shortcut, so the hint on
    /// the card is the only way out of a pin.
    escape_hotkey_ready: bool,
    /// Whether the settings panel is waiting for the user to press a shortcut.
    capturing_hotkey: bool,
    /// Why the last captured combination was refused, if it was.
    ///
    /// Kept until the next press so the reason stays on screen while the user
    /// works out what to change — clearing it immediately would leave them
    /// looking at an unchanged row and no explanation.
    hotkey_refusal: Option<String>,
    /// The last click-through state this widget decided on, as
    /// `(ignore the mouse, menu open)`.
    ///
    /// The only record of what was *told to the operating system*, which matters
    /// because deciding and telling are two steps and only one of them used to be
    /// testable — deleting the tell from the menu handler left every test passing.
    click_through_applied: (bool, bool),
    /// Seconds since the newest sample, as of the last tick.
    ///
    /// Read in the tick rather than in `view` because `view` runs on every
    /// repaint and this opens the database — a widget that reopens its own
    /// database to redraw is a widget that redraws slowly, and it also puts a
    /// write-capable handle near the render path, which is a place no such handle
    /// should be.
    freshness: Option<i64>,
    /// When this process started, for the "is it still starting" question.
    started_at: std::time::Instant,
}

impl OverlayApp {
    /// Boots the app: loads config, opens the database, discovers the window id.
    pub fn new() -> (Self, Task<Message>) {
        let config = OverlayConfig::load().unwrap_or_default();
        let source = Source::open();
        let requested = config.overlay_requested;
        let (language, theme) = Self::appearance_of(&config, &source);

        let app = Self {
            config,
            config_path: None,
            requested,
            window_id: None,
            window_raw: None,
            power: HashMap::new(),
            top_apps: Vec::new(),
            show_settings: false,
            show_menu: false,
            applied: iced::Size::ZERO,
            monitor: None,
            language,
            // `Auto` is a *preference*, not something to draw. Resolving it here
            // is what keeps `palette()` from ever being asked about it, which is
            // why that function has a fallback arm at all.
            theme: theme.resolve(ThemeChoice::Dark),
            source,
            launch_attempted: false,
            launched: None,
            wattseal_missing: false,
            escape_hotkey_ready: false,
            capturing_hotkey: false,
            hotkey_refusal: None,
            click_through_applied: (false, false),
            freshness: None,
            started_at: std::time::Instant::now(),
        };

        let task = Task::batch([window::latest().map(Message::WindowId), Task::done(Message::Tick)]);
        (app, task)
    }

    /// How the application currently looks.
    fn appearance_of(config: &OverlayConfig, source: &Source) -> (Language, ThemeChoice) {
        resolve_appearance(config, source.ui_settings().as_ref())
    }

    /// Re-resolves the appearance after the config's overrides change.
    ///
    /// Reads `ui_settings` again rather than reusing the last value, so that
    /// going back to "automatic" lands on what the dashboard says *now* rather
    /// than on a stale answer from when the widget started.
    fn apply_appearance(&mut self) {
        let (language, theme) = Self::appearance_of(&self.config, &self.source);
        self.language = language;
        self.theme = theme.resolve(ThemeChoice::Dark);
    }

    /// Whether the shortcut is holding a press that should release the pin.
    ///
    /// Short-circuits when the shortcut was never registered, so a machine where
    /// another program owns the key does no work per tick for a key that will
    /// never arrive.
    fn poll_escape_hotkey(&mut self) -> bool {
        self.escape_hotkey_ready && crate::winlayer::poll_escape_hotkey()
    }

    /// Whether the widget has locked itself out of its own mouse.
    ///
    /// Pinned locks the position, which is a choice. Pin *and click-through*
    /// takes the mouse as well, which means the menu, the settings panel and the
    /// close button are all unreachable — and with them every in-app way out.
    fn is_stuck_pinned(&self) -> bool {
        self.config.pin_mode && self.config.pin_click_through && crate::winlayer::click_through_supported()
    }

    /// The shortcut in force, from the config file or the default.
    ///
    /// An unreadable value falls back to the default rather than disabling the
    /// escape hatch: a typo in `escape_hotkey` should cost the user their custom
    /// key, not every way out of a pin.
    fn escape_hotkey(&self) -> crate::winlayer::Hotkey {
        self.config
            .escape_hotkey
            .as_deref()
            .and_then(crate::winlayer::Hotkey::parse)
            .unwrap_or(crate::winlayer::DEFAULT_HOTKEY)
    }

    /// Binds whatever the config currently says, and remembers whether it took.
    fn bind_escape_hotkey(&mut self) {
        let hotkey = self.escape_hotkey();
        self.escape_hotkey_ready = crate::winlayer::set_escape_hotkey(hotkey);
    }

    /// Why the widget is showing what it is showing, right now.
    fn status(&self) -> Status {
        status_of(
            self.source.availability(),
            self.config.launch_wattseal,
            self.launched.is_some(),
            self.wattseal_missing,
            self.freshness,
            self.started_at.elapsed().as_secs(),
        )
    }

    /// Starts WattSeal when there is nothing recent to read, once per widget run.
    ///
    /// The "once" is the whole of the state here. WattSeal takes a second or two
    /// to produce its first sample, and a per-tick check would spawn a fresh copy
    /// on every tick until it did — and each of those copies would exit on
    /// WattSeal's own single-instance lock, or worse, race for it.
    ///
    /// A child that dies is *not* retried, for the same reason. If WattSeal
    /// cannot stay up, starting it ten times a second is not a recovery.
    fn ensure_wattseal_is_running(&mut self) {
        if self.launch_attempted {
            return;
        }
        self.launch_attempted = true;

        let Some(directory) = crate::launcher::executable_dir() else {
            return;
        };

        match crate::launcher::start_if_needed(self.config.launch_wattseal, &directory, &self.source.path()) {
            Ok(child) => self.launched = child,
            // `AlreadyRunning` and `Disabled` are the ordinary answers and worth
            // nothing on screen. `NotInstalled` is not: the user will otherwise
            // see a widget full of placeholders and no reason for them.
            Err(crate::launcher::Skipped::NotInstalled) => self.wattseal_missing = true,
            Err(_) => {}
        }
    }

    pub fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Tick => {
                // Honour flags written by the other processes (tray / main
                // window). They only edit the shared config file, and this
                // returns `true` when the overlay has been asked to close.
                if self.sync_external_state() {
                    return iced::exit();
                }
                // Re-check the database generation first: everything below reads
                // the file, and a reader must never outlive the schema it was
                // written against. Re-opening is also what lets the widget pick
                // the collector up when it starts after us.
                self.source.poll();
                self.power = self.source.watts();
                self.refresh_top_apps();
                self.ensure_wattseal_is_running();
                self.freshness = self.source.freshness_seconds();
                if self.poll_escape_hotkey() {
                    // Same effect as the menu's Pin entry, so the pin is left in
                    // the config file and the other process — there is none, but
                    // a later build may add one — sees the same state.
                    self.config.pin_mode = !self.config.pin_mode;
                    self.persist();
                    self.apply_click_through();
                }
                // The dashboard owns the language and the theme unless the user
                // has pinned them here, so re-reading them is what makes the
                // widget follow a change made over there instead of keeping a
                // second, stale copy of both.
                self.apply_appearance();
                // A generation bump under an open panel would leave controls for
                // a database this build can no longer read on screen.
                if self.is_degraded() && self.show_settings {
                    self.show_settings = false;
                }

                // Keep the window fitted to the live content, but only resize
                // when the target moved by a whole rung. Reacting to a couple of
                // pixels is what made the widget jitter while the numbers moved.
                let target = self.fitted_size();
                let width_moved = (target.width - self.applied.width).abs() >= WIDTH_STEP;
                let height_moved = (target.height - self.applied.height).abs() >= 1.0;
                if width_moved || height_moved {
                    return self.resize_task();
                }
                Task::none()
            }
            Message::WindowId(id) => {
                self.window_id = id;
                // Refresh *before* sizing, so the window opens already fitted to
                // the content instead of a placeholder computed from empty data.
                self.source.poll();
                self.power = self.source.watts();
                self.refresh_top_apps();
                self.apply_appearance();
                self.applied = self.fitted_size();
                let raw = id
                    .map(|id| window::raw_id::<Message>(id).map(Message::RawWindowId))
                    .unwrap_or_else(Task::none);
                Task::batch([self.apply_window_settings(), raw])
            }
            Message::RawWindowId(raw) => {
                self.window_raw = Some(raw);
                self.apply_layered();
                self.apply_click_through();
                // Registered on a thread of its own rather than handled by this window:
                // the shortcut has to work on a window the mouse cannot reach,
                // which is the whole reason it exists.
                self.bind_escape_hotkey();
                Task::none()
            }
            Message::MonitorSize(size) => {
                self.monitor = size;
                Task::none()
            }
            Message::StartDrag => self.window_id.map(window::drag::<Message>).unwrap_or_else(Task::none),
            Message::Moved(x, y) => {
                self.config.position = Some((x, y));
                Task::none()
            }

            Message::ToggleSettings => {
                // The degraded mode has no settings panel: every control in it
                // describes how to draw a database this build cannot read.
                if self.is_degraded() {
                    return Task::none();
                }
                self.show_settings = !self.show_settings;
                self.show_menu = false;
                self.apply_click_through();
                self.resize_task().chain(self.keep_on_screen())
            }
            Message::OpenMenu => {
                if self.show_settings {
                    return Task::none();
                }
                self.show_menu = true;
                // Before the menu is drawn, so it is clickable in the same frame
                // it appears rather than a frame later.
                self.apply_click_through();
                self.resize_task().chain(self.keep_on_screen())
            }
            Message::CloseMenu => {
                self.show_menu = false;
                self.apply_click_through();
                self.resize_task()
            }
            Message::TogglePin => {
                self.config.pin_mode = !self.config.pin_mode;
                self.persist();
                // Closed before the style is recomputed, so the re-apply sees the
                // menu gone rather than restoring click-through over a menu that is
                // about to disappear.
                self.show_menu = false;
                self.apply_click_through();
                self.resize_task()
            }
            Message::TogglePinClickThrough(v) => {
                self.config.pin_click_through = v;
                self.persist();
                self.apply_click_through();
                Task::none()
            }

            // appearance
            Message::SetBgColor(v) => {
                self.config.bg_color = v;
                self.persist();
                Task::none()
            }
            Message::SetTextColor(v) => {
                self.config.text_color = v;
                self.persist();
                Task::none()
            }
            Message::ChangeOpacity(v) => {
                self.config.opacity = v.clamp(0.05, 1.0);
                self.persist();
                self.apply_layered();
                Task::none()
            }
            Message::SetTransparency(v) => {
                self.config.transparency = v;
                self.persist();
                self.apply_layered();
                Task::none()
            }
            Message::ToggleShadow(v) => {
                self.config.shadow = v;
                self.persist();
                Task::none()
            }
            Message::SetLayout(v) => {
                self.config.layout = v;
                self.persist();
                self.resize_task()
            }
            Message::SetLanguage(v) => {
                self.config.language = Some(v);
                self.persist();
                // The panel is drawn in the language it is drawn in, so changing
                // it has to redraw everything — including this picker, which is
                // how the user confirms the change took.
                self.apply_appearance();
                self.resize_task()
            }
            Message::SetTheme(v) => {
                self.config.theme = Some(v);
                self.persist();
                self.apply_appearance();
                self.resize_task()
            }
            Message::FollowDashboardLanguage => {
                self.config.language = None;
                self.persist();
                self.apply_appearance();
                self.resize_task()
            }
            Message::FollowDashboardTheme => {
                self.config.theme = None;
                self.persist();
                self.apply_appearance();
                self.resize_task()
            }
            Message::BeginHotkeyCapture => {
                self.capturing_hotkey = true;
                self.hotkey_refusal = None;
                self.resize_task()
            }
            Message::CancelHotkeyCapture => {
                self.capturing_hotkey = false;
                self.hotkey_refusal = None;
                self.resize_task()
            }
            Message::CapturedKey { ctrl, alt, shift, vk } => {
                // The guard lives here rather than in the subscription, because
                // `listen_with` takes a function pointer and cannot see whether
                // the panel is asking. Every keypress arrives; only a capture is
                // listening.
                if !self.capturing_hotkey {
                    return Task::none();
                }

                // Escape is how you get out of capture without binding anything:
                // every other key is a candidate, and a mode with no way to leave
                // it is the same trap this whole feature exists to undo.
                if vk == 0x1B {
                    self.capturing_hotkey = false;
                    self.hotkey_refusal = None;
                    return self.resize_task();
                }

                let hotkey = crate::winlayer::Hotkey { ctrl, alt, shift, vk };

                match hotkey.refusal() {
                    // Kept listening rather than dropping out: the user is mid
                    // gesture, and closing the capture on their first mistake
                    // makes them start again to find out what was wrong.
                    Some(reason) => self.hotkey_refusal = Some(reason.to_string()),
                    None => {
                        self.config.escape_hotkey = Some(hotkey.to_config());
                        self.capturing_hotkey = false;
                        self.hotkey_refusal = None;
                        self.persist();
                        // Bound straight away, so the panel can say whether the
                        // machine accepted it instead of the user finding out the
                        // next time they are stuck.
                        self.bind_escape_hotkey();
                    }
                }
                self.resize_task()
            }
            Message::SetDensity(v) => {
                self.config.density = v;
                self.persist();
                self.resize_task()
            }
            Message::SetFontSize(v) => {
                self.config.font_size = v;
                self.persist();
                self.resize_task()
            }
            Message::ToggleAbbreviated(v) => {
                self.config.abbreviated = v;
                self.persist();
                self.resize_task()
            }
            Message::ToggleLabels(v) => {
                self.config.show_labels = v;
                self.persist();
                Task::none()
            }
            Message::ToggleUnits(v) => {
                self.config.show_units = v;
                self.persist();
                Task::none()
            }
            Message::SetDecimals(v) => {
                self.config.decimals = v.min(3);
                self.persist();
                Task::none()
            }
            Message::SetRefresh(v) => {
                self.config.refresh_secs = v.max(1);
                self.persist();
                Task::none()
            }

            // window
            Message::ToggleAlwaysOnTop(v) => {
                self.config.always_on_top = v;
                self.persist();
                self.set_level(if v {
                    window::Level::AlwaysOnTop
                } else {
                    window::Level::Normal
                })
            }
            Message::SetWidth(v) => {
                // Snap to the ladder, so the number shown next to the slider is
                // the width the window actually takes, and keep it above what a
                // value needs — the slider offers the same floor.
                let floor = self.width_floor();
                self.config.width = width_near(v.clamp(floor, MAX_CHOICE_WIDTH));
                self.persist();
                self.resize_task()
            }
            // content
            Message::ToggleMetric(metric, enabled) => {
                if enabled {
                    if !self.config.metrics.contains(&metric) {
                        self.config.metrics.push(metric);
                    }
                } else {
                    self.config.metrics.retain(|m| *m != metric);
                }
                self.persist();
                self.resize_task()
            }
            Message::MoveMetricUp(metric) => {
                move_metric(&mut self.config.metrics, metric, -1);
                self.persist();
                self.resize_task()
            }
            Message::MoveMetricDown(metric) => {
                move_metric(&mut self.config.metrics, metric, 1);
                self.persist();
                self.resize_task()
            }
            Message::SetTopApps(k) => {
                self.config.top_apps = k.clamp(1, 8);
                self.persist();
                self.resize_task()
            }

            Message::Quit | Message::CloseRequested => {
                // Record that the overlay is no longer wanted, so the main
                // window's footer toggle follows along instead of staying stuck
                // on "Hide overlay" after an exit from the bar's own menu.
                self.config.overlay_requested = false;
                // Closing clears the pin too: that is what makes the dashboard's
                // hide-and-show a way out of a pinned, click-through widget on a
                // system with no tray.
                self.config.pin_mode = false;
                self.persist();
                iced::exit()
            }
        }
    }

    pub fn view(&self) -> Element<'_, Message, Theme> {
        let palette = theme::palette_with(self.theme, self.config.bg_color, self.config.text_color);
        let pad = self.config.density.padding();
        let spacing = self.config.density.spacing();
        let label_size = self.config.font_size.label();
        let value_size = self.config.font_size.value();

        let body: Element<'_, Message, Theme> = if self.show_settings {
            self.view_settings(palette, label_size, spacing)
        } else if self.show_menu {
            self.view_menu(palette, label_size)
        } else {
            self.view_metrics(palette, label_size, value_size, spacing)
        };

        // The header row exists only in settings mode, where it doubles as the
        // drag handle. In metrics mode the whole card is the drag / right-click
        // surface instead — reclaiming the space the old gear/close buttons took.
        let column = if self.show_settings {
            Column::new()
                .spacing(spacing)
                .push(self.view_header(palette))
                .push(body)
        } else {
            // Pin the metrics to the bottom of the card: an oversized window
            // then leaves its slack *above* the text, so the floating grip
            // (anchored to the bottom-right) always lands on the last row.
            // No `spacing` here: the filler already carries the gap, and adding
            // one would make the content need more height than `fitted_height`
            // accounts for.
            Column::new().push(Space::new().height(Length::Fill)).push(body)
        };

        let card = Container::new(column)
            .width(Length::Fill)
            .height(Length::Fill)
            .padding(Padding::from(pad))
            // `draws_shadow` is the verdict already resolved for the active
            // transparency mode, not the raw setting: a surface with no per-pixel
            // alpha would only turn the soft edge into a dark ring.
            .style(card_style(palette, self.card_alpha(), self.config.draws_shadow()));

        // The card is the drag / right-click surface only while the metrics are
        // showing: the settings panel and the menu need clickable widgets. A
        // pinned overlay is never draggable — that is the part of pin mode every
        // platform can honour.
        if self.show_settings || self.show_menu || self.config.pin_mode {
            card.into()
        } else {
            mouse_area(card).on_press(Message::StartDrag).into()
        }
    }

    /// Whether the widget is showing the degraded presentation.
    ///
    /// An unknown schema generation: no translated text, no settings panel and
    /// no labels, because none of that can be confirmed to match what the user
    /// picked in the dashboard. What is left is the numbers and the two things
    /// that keep working without the database at all — pinning and closing.
    fn is_degraded(&self) -> bool {
        self.source.availability().is_degraded()
    }

    /// The menu's segments: what each one says, and what it does.
    ///
    /// Labels and messages together, because the two are always used together —
    /// the width is measured from these and the row is built from them — and
    /// writing them out separately is how a menu ends up measuring four
    /// segments and drawing three.
    ///
    /// The settings entry is simply absent in the degraded mode: a panel full of
    /// controls for a database this build cannot read is worse than no panel.
    fn menu_segments(&self) -> Vec<(&'static str, Message)> {
        let language = self.language;
        let mut segments = vec![(translations::menu_resume(language), Message::CloseMenu)];
        if !self.is_degraded() {
            segments.push((translations::menu_settings(language), Message::ToggleSettings));
        }
        segments.push((
            if self.config.pin_mode {
                translations::menu_unpin(language)
            } else {
                translations::menu_pin(language)
            },
            Message::TogglePin,
        ));
        segments.push((translations::menu_exit(language), Message::Quit));
        segments
    }

    /// Width that fits the menu segments.
    fn menu_width(&self) -> f32 {
        let pad = self.config.density.padding();
        let size = self.config.font_size.label();
        let segments = self.menu_segments();

        let labels: f32 = segments
            .iter()
            .map(|(label, _)| text_width(label, size, LABEL_CHAR_W) + SEGMENT_PADDING * 2.0)
            .sum();
        let gaps = (segments.len() as f32 - 1.0).max(0.0) * MENU_GAP;

        width_up(pad * 2.0 + labels + gaps).max(80.0)
    }

    /// The right-click menu.
    ///
    /// The bar's own content is replaced by its segments instead of raising an
    /// OS popup, so it behaves identically on Windows, Linux and macOS and can
    /// never be clipped by the window's own size.
    fn view_menu(&self, palette: Palette, font: f32) -> Element<'_, Message, Theme> {
        Row::new()
            .spacing(MENU_GAP)
            .align_y(Alignment::Center)
            .extend(self.menu_segments().into_iter().map(|(label, message)| {
                button(Text::new(label).size(font).color(palette.text))
                    .style(flat_button(palette))
                    .padding(Padding::from([1.0, SEGMENT_PADDING]))
                    .on_press(message)
                    .into()
            }))
            .into()
    }

    /// A slim drag handle, shown only in settings mode. Metrics mode needs no
    /// header at all because there the whole card is the drag surface.
    fn view_header(&self, palette: Palette) -> Element<'_, Message, Theme> {
        let bar = Container::new(Space::new().width(Length::Fill))
            .width(Length::Fill)
            .height(Length::Fixed(8.0))
            .style(header_style(palette));
        mouse_area(bar).on_press(Message::StartDrag).into()
    }

    fn view_metrics(
        &self,
        palette: Palette,
        label_size: f32,
        value_size: f32,
        spacing: f32,
    ) -> Element<'_, Message, Theme> {
        // Values follow the configured text color (the separate "Value color"
        // setting was removed in favour of the text swatch).
        let value_color = palette.text;

        // The entries are fitted to the window the app is about to ask for, so
        // what is drawn here and what `fitted_size` measured are the same list.
        let items = self.fitted_items();

        let body: Element<'_, Message, Theme> = match self.config.layout {
            Layout::Vertical => {
                let mut column = Column::new().spacing(spacing);
                if items.is_empty() {
                    column = column.push(Text::new("—").size(label_size).color(palette.muted));
                }
                for item in &items {
                    if item.status {
                        // Muted, and at label size: a sentence about the widget's
                        // state is not a reading, and the value font would give
                        // it more authority than the numbers beneath it. Where
                        // metrics are shown, the status goes above them and the
                        // numbers stay the thing you look at.
                        column = column.push(Text::new(item.value.clone()).size(label_size).color(palette.muted));
                        continue;
                    }
                    column = column.push(self.value_row(
                        item.label.clone(),
                        item.value.clone(),
                        palette,
                        label_size,
                        value_size,
                        value_color,
                    ));
                }
                column.into()
            }
            Layout::Horizontal => {
                let mut row = Row::new().spacing(BAR_INNER_GAP).align_y(Alignment::Center);

                for (index, item) in items.iter().enumerate() {
                    if index > 0 {
                        row = row.push(Text::new("·").size(label_size).color(palette.muted));
                    }
                    if item.status {
                        row = row.push(Text::new(item.value.clone()).size(label_size).color(palette.muted));
                        continue;
                    }
                    if let Some(label) = &item.label {
                        row = row.push(Text::new(label.clone()).size(label_size).color(palette.muted));
                    }
                    row = row.push(
                        Text::new(item.value.clone())
                            .size(value_size)
                            .font(FONT_VALUE)
                            .color(value_color),
                    );
                }

                row.into()
            }
        };

        Container::new(body).width(Length::Fill).into()
    }

    fn value_row(
        &self,
        label: Option<String>,
        value: String,
        palette: Palette,
        label_size: f32,
        value_size: f32,
        value_color: Color,
    ) -> Element<'_, Message, Theme> {
        let mut row = Row::new().spacing(BAR_INNER_GAP).align_y(Alignment::Center);
        match label {
            Some(label) => {
                row = row.push(
                    Text::new(label)
                        .size(label_size)
                        .color(palette.muted)
                        .width(Length::Fill),
                );
            }
            None => {
                // Labels off: the value takes the row so it stays anchored to the
                // right edge, which is where a stack of readings is scanned from.
                row = row.push(Space::new().width(Length::Fill));
            }
        }
        row.push(Text::new(value).size(value_size).font(FONT_VALUE).color(value_color))
            .into()
    }

    fn view_settings(&self, palette: Palette, font: f32, spacing: f32) -> Element<'_, Message, Theme> {
        let language = self.language;
        let bg_dec = (self.config.opacity - 0.05).clamp(0.05, 1.0);
        let bg_inc = (self.config.opacity + 0.05).clamp(0.05, 1.0);

        let opacity_row = Column::new()
            .spacing(2)
            .push(stepper_row(
                translations::label_opacity(language),
                self.config.opacity,
                Message::ChangeOpacity(bg_dec),
                Message::ChangeOpacity(bg_inc),
                font,
                palette,
            ))
            .push(hint(self.opacity_hint(), font, palette));

        // A shadow needs per-pixel alpha to fade into. Where the mode has none the
        // toggle is kept — it is still the user's preference, and it takes effect
        // again in a mode that can render it — but the hint says so whether or
        // not the box is ticked. Tying it to the box meant the explanation
        // arrived only after the user had already discovered that nothing
        // happens, and never at all for an unticked box that could not be ticked.
        let shadow_row = Column::new().spacing(2).push(toggle(
            translations::label_shadow(language),
            self.config.shadow,
            Message::ToggleShadow,
            font,
            palette,
        ));
        let shadow_row = if self.shadow_cannot_render() {
            shadow_row.push(hint(translations::hint_shadow_unavailable(language), font, palette))
        } else {
            shadow_row
        };

        let appearance = Column::new()
            .spacing(spacing)
            .push(section_title(translations::section_appearance(language), font, palette))
            // Language and theme come first in this column, before the things
            // they decide how are read. A row whose label is in a language you
            // cannot read is a row you cannot find, and language is the one
            // setting whose absence makes every other label unreadable.
            .push(picker(
                translations::label_language(language),
                // The shown value is what is *in effect*, not what is stored: an
                // unset override displays as the language the panel is currently
                // drawn in, which is the dashboard's (or the default's), and that
                // is what "Automatic" has to look like.
                labeled_pick(
                    AppLanguage::all(),
                    self.config.language.unwrap_or(language),
                    language,
                    Message::SetLanguage,
                ),
                font,
                palette,
            ))
            .push(if self.config.language.is_some() {
                hint(translations::hint_language_override(language), font, palette)
            } else {
                hint(translations::hint_language_follows_dashboard(language), font, palette)
            })
            .push(picker(
                translations::label_theme(language),
                labeled_pick(
                    ThemeChoice::ALL,
                    self.config.theme.unwrap_or(ThemeChoice::Auto),
                    language,
                    Message::SetTheme,
                ),
                font,
                palette,
            ))
            .push(opacity_row)
            .push(picker(
                translations::label_bg_color(language),
                labeled_pick(BgColor::ALL, self.config.bg_color, language, Message::SetBgColor),
                font,
                palette,
            ))
            .push(picker(
                translations::label_text_color(language),
                labeled_pick(TextColor::ALL, self.config.text_color, language, Message::SetTextColor),
                font,
                palette,
            ))
            .push(picker(
                translations::label_transparency(language),
                labeled_pick(
                    Transparency::ALL,
                    self.config.transparency,
                    language,
                    Message::SetTransparency,
                ),
                font,
                palette,
            ))
            .push(shadow_row)
            .push(picker(
                translations::label_layout(language),
                labeled_pick(Layout::ALL, self.config.layout, language, Message::SetLayout),
                font,
                palette,
            ))
            .push(picker(
                translations::label_density(language),
                labeled_pick(Density::ALL, self.config.density, language, Message::SetDensity),
                font,
                palette,
            ))
            .push(picker(
                translations::label_text_size(language),
                labeled_pick(FontSize::ALL, self.config.font_size, language, Message::SetFontSize),
                font,
                palette,
            ))
            .push(picker(
                translations::label_decimals(language),
                pick_list(DECIMALS, Some(self.config.decimals), Message::SetDecimals),
                font,
                palette,
            ))
            .push(picker(
                translations::label_refresh(language),
                pick_list(REFRESH, Some(self.config.refresh_secs), Message::SetRefresh),
                font,
                palette,
            ))
            .push(
                Row::new()
                    .spacing(12)
                    .align_y(Alignment::Center)
                    .push(
                        checkbox(self.config.show_labels)
                            .label(translations::label_show_labels(language))
                            .text_size(font)
                            .on_toggle(Message::ToggleLabels),
                    )
                    .push(
                        checkbox(self.config.show_units)
                            .label(translations::label_show_units(language))
                            .text_size(font)
                            .on_toggle(Message::ToggleUnits),
                    ),
            )
            .push(
                checkbox(self.config.abbreviated)
                    .label(translations::label_short_labels(language))
                    .text_size(font)
                    .on_toggle(Message::ToggleAbbreviated),
            );

        let mut window_col = Column::new()
            .spacing(spacing)
            .push(section_title(translations::section_window(language), font, palette))
            .push(toggle(
                translations::label_always_on_top(language),
                self.config.always_on_top,
                Message::ToggleAlwaysOnTop,
                font,
                palette,
            ))
            .push(if crate::winlayer::click_through_supported() {
                let hotkey = self.escape_hotkey();
                let escape: String = if self.escape_hotkey_ready {
                    crate::winlayer::escape_setting_hint(language, hotkey)
                } else {
                    translations::hint_escape_unavailable(language).to_string()
                };

                // The rebind row. It sits directly under the toggle that can lock
                // the user out, because that is the only moment the advice can be
                // acted on — a widget that cannot be clicked cannot be told
                // anything afterwards.
                let rebind: Element<'_, Message, Theme> = if self.capturing_hotkey {
                    let prompt = match &self.hotkey_refusal {
                        Some(reason) => translations::hotkey_capture_refused(language, reason),
                        None => translations::hotkey_capture_prompt(language).to_string(),
                    };
                    Column::new()
                        .spacing(2)
                        .push(Text::new(prompt).size(font).color(palette.accent))
                        .push(
                            button(Text::new(translations::hotkey_cancel(language)).size(font))
                                .style(flat_button(palette))
                                .on_press(Message::CancelHotkeyCapture),
                        )
                        .into()
                } else {
                    button(Text::new(translations::hotkey_rebind(language, &hotkey.label())).size(font))
                        .style(flat_button(palette))
                        .on_press(Message::BeginHotkeyCapture)
                        .into()
                };

                Column::new()
                    .spacing(2)
                    .push(toggle(
                        translations::label_pin_click_through(language),
                        self.config.pin_click_through,
                        Message::TogglePinClickThrough,
                        font,
                        palette,
                    ))
                    .push(rebind)
                    // The shortcut is stated here too, so the user can see whether
                    // this machine accepted it without having to get stuck first.
                    .push(hint_owned(escape, font, palette))
                    .into()
            } else {
                hint(translations::hint_pin_unavailable(language), font, palette)
            });
        // The setting is the widest the widget may get: it hugs its numbers and
        // stops here, which is the only way a single line can be capped without
        // wrapping it or cutting it.
        let width_floor = self.width_floor();
        window_col = window_col.push(
            Column::new()
                .spacing(2)
                .push(
                    Text::new(format!(
                        "{}  {} px",
                        translations::label_width(language),
                        self.config.width.max(width_floor).round()
                    ))
                    .size(font)
                    .color(palette.muted),
                )
                .push(slider(
                    width_floor..=MAX_CHOICE_WIDTH,
                    self.config.width.max(width_floor),
                    Message::SetWidth,
                )),
        );

        // Display order first: the metrics that are on, in the order the bar shows
        // them, each with the arrows that move it past its neighbour. The ones that
        // are off follow, and a metric switched on is appended to the end.
        let name_of = |metric: Metric| {
            if metric.is_multi() {
                translations::metric_top_apps_setting(language)
            } else {
                translations::metric_name(language, metric)
            }
        };

        let mut metrics = Column::new().spacing(spacing);
        for &metric in &self.config.metrics {
            metrics = metrics.push(
                Row::new()
                    .spacing(BAR_INNER_GAP)
                    .align_y(Alignment::Center)
                    .push(
                        checkbox(true)
                            .label(name_of(metric))
                            .text_size(font)
                            .on_toggle(move |v| Message::ToggleMetric(metric, v)),
                    )
                    .push(step_button("▲", Message::MoveMetricUp(metric), palette, font))
                    .push(step_button("▼", Message::MoveMetricDown(metric), palette, font)),
            );
        }
        for metric in Metric::ALL.iter().copied().filter(|m| !self.config.metrics.contains(m)) {
            metrics = metrics.push(
                checkbox(false)
                    .label(name_of(metric))
                    .text_size(font)
                    .on_toggle(move |v| Message::ToggleMetric(metric, v)),
            );
        }
        let mut content_col = Column::new()
            .spacing(spacing)
            .push(section_title(translations::section_content(language), font, palette))
            .push(metrics);
        if self.config.metrics.contains(&Metric::TopApps) {
            content_col = content_col.push(picker(
                translations::label_top_count(language),
                pick_list(TOP_APPS, Some(self.config.top_apps()), Message::SetTopApps),
                font,
                palette,
            ));
        }

        let done: Button<'_, Message, Theme> = button(Text::new(translations::button_done(language)).size(font))
            .style(flat_button(palette))
            .on_press(Message::ToggleSettings);

        let quit: Button<'_, Message, Theme> =
            button(Text::new(translations::button_quit_overlay(language)).size(font))
                .style(flat_button(palette))
                .on_press(Message::Quit);

        // Two balanced columns: appearance on the left, window/content on the
        // right. This halves the height, so the panel usually needs no
        // scrolling at all.
        let right = Column::new()
            .spacing(spacing)
            .width(Length::Fill)
            .push(window_col)
            .push(content_col)
            .push(Row::new().spacing(8).align_y(Alignment::Center).push(done).push(quit));

        // The scroll is the guarantee; the measured height is only what keeps
        // it out of the way. A row added to a column, a hint that wraps, a
        // language whose labels are longer — none of them can put a control
        // past the bottom edge with no way to reach it.
        scrollable(
            Row::new()
                .spacing(20)
                .padding(Padding::from([0.0, 16.0]))
                .push(appearance.width(Length::Fill))
                .push(right),
        )
        .into()
    }

    /// Tick + tray polling + window events.
    pub fn subscription(&self) -> Subscription<Message> {
        // Poll quickly until the first sample arrives, so the window settles at
        // its fitted size immediately after opening instead of a second later.
        let interval = if self.power.is_empty() {
            Duration::from_millis(200)
        } else {
            Duration::from_secs(self.config.refresh_secs.max(1) as u64)
        };
        Subscription::batch([
            every(interval).map(|_| Message::Tick),
            event::listen_with(|evt, _status, _id| match evt {
                iced::Event::Window(window::Event::CloseRequested) => Some(Message::CloseRequested),
                iced::Event::Window(window::Event::Moved(point)) => Some(Message::Moved(point.x, point.y)),
                // Handled globally rather than through the card's `mouse_area`,
                // so it works regardless of widget hit-testing.
                iced::Event::Mouse(iced::mouse::Event::ButtonPressed(iced::mouse::Button::Right)) => {
                    Some(Message::OpenMenu)
                }
                // Every keypress is offered, and `update` decides whether it means
                // anything. The check cannot live here: `listen_with` takes a
                // function pointer, so this closure may not capture `self` — and
                // a widget this small is almost never focused, so the cost of
                // being told about keys it ignores is nothing.
                iced::Event::Keyboard(iced::keyboard::Event::KeyPressed { key, modifiers, .. }) => {
                    hotkey_from_key(&key, modifiers.control(), modifiers.alt(), modifiers.shift())
                }
                _ => None,
            }),
        ])
    }

    /// Window title (task switcher only).
    pub fn title(&self) -> String {
        String::from("WattSeal Overlay")
    }

    /// Active iced theme.
    pub fn theme(&self) -> Theme {
        theme::iced_theme(self.theme)
    }

    // ---- helpers ----

    /// Explains how the two opacity settings interact in the active mode.
    fn opacity_hint(&self) -> &'static str {
        let language = self.language;
        let mode = self.config.transparency;
        if mode.uses_layered() {
            translations::hint_opacity_layered(language)
        } else if mode.transparent_window() {
            translations::hint_opacity_surface(language)
        } else {
            translations::hint_opacity_off(language)
        }
    }

    /// Label shown for a metric, honouring the short-label mode.
    fn metric_label(&self, metric: Metric) -> &'static str {
        if self.config.abbreviated {
            translations::metric_short_name(self.language, metric)
        } else {
            translations::metric_name(self.language, metric)
        }
    }

    fn format_value(&self, watts: Option<f64>) -> String {
        match watts {
            Some(w) => {
                let number = format!("{w:.prec$}", prec = self.config.decimals as usize);
                if self.config.show_units {
                    format!("{number}W")
                } else {
                    number
                }
            }
            None => String::from("—"),
        }
    }

    /// Refreshes the per-app rows, keeping the previous ones when this tick has
    /// no process sample to offer.
    ///
    /// A tick that lands between two collector samples comes back with no
    /// processes at all, and assigning that would blink the rows out for a
    /// second — and, in the horizontal layout, would briefly change how many
    /// lines the bar needs.
    fn refresh_top_apps(&mut self) {
        let window = self.config.refresh_secs.max(1) as i64;
        if let Some(apps) = self.source.top_apps(window, self.config.top_apps()) {
            self.top_apps = apps;
        }
    }

    /// The file this process reads and writes.
    ///
    /// One place, so that `persist`, the poll for another process' flags and the
    /// initial load can never end up pointing at different files.
    fn shared_config_path(&self) -> PathBuf {
        self.config_path.clone().unwrap_or_else(OverlayConfig::path)
    }

    fn persist(&self) {
        self.config.save_to(&self.shared_config_path());
    }

    /// Card background alpha: the opacity in per-pixel (surface) mode; fully
    /// opaque in layered/off mode (the window itself carries the alpha there).
    fn card_alpha(&self) -> f32 {
        if self.config.transparency.transparent_window() {
            self.config.opacity
        } else {
            1.0
        }
    }

    /// Color the OS window is cleared with.
    ///
    /// Where the surface has no per-pixel alpha, whatever the card does not paint
    /// shows this color. That is the corner wedges a rounded card leaves, and it
    /// has to be the card's *own* color, overrides included: clearing with the
    /// theme's stock card painted a dark frame around a card set to another
    /// swatch.
    fn window_background(&self) -> Color {
        if self.config.transparency.transparent_window() {
            Color::TRANSPARENT
        } else {
            theme::palette_with(self.theme, self.config.bg_color, self.config.text_color).card
        }
    }

    /// Applies the click-through extended styles for "pin mode".
    fn apply_click_through(&mut self) {
        let state = self.click_through_state();
        // Recorded whether or not there is a window yet, so what the widget
        // *decided* is observable even when nothing can be told to the OS. Without
        // this the decision and its application are two steps a test cannot tell
        // apart: deleting the call from the menu handler left every test green,
        // because the test asked the decision function directly.
        self.click_through_applied = state;

        if let Some(hwnd) = self.window_raw {
            let (through, ui_open) = state;
            crate::winlayer::set_click_through(hwnd, through, ui_open);
        }
    }

    /// What the click-through style should be, as `(ignore the mouse, menu open)`.
    ///
    /// Split out from the call that applies it so the **decision** can be tested.
    /// The Win32 call underneath cannot be driven from a test — synthetic input
    /// never reaches this machine's event loop, so a menu cannot be opened
    /// programmatically here — which leaves this as the only place the wiring can
    /// be wrong. It was: nothing recomputed the style when the menu opened, so
    /// releasing `Ctrl+Alt` to click a menu item restored click-through onto the
    /// menu that had just appeared.
    fn click_through_state(&self) -> (bool, bool) {
        let wanted =
            self.config.pin_mode && self.config.pin_click_through && crate::winlayer::click_through_supported();
        // **The widget's own menu is part of this decision.** Reaching a
        // click-through widget means holding `Ctrl+Alt`, right-clicking, and then
        // *letting go* to click the menu item — so if letting go restored
        // click-through, the menu would become unclickable the instant it appeared.
        // An escape hatch that shuts the door in the same motion.
        let ui_open = self.show_menu || self.show_settings;
        (wanted, ui_open)
    }

    /// Mirrors the externally controlled flags from the shared config file and
    /// reports whether the main window has asked the overlay to close.
    ///
    /// There is no IPC: the tray and the main window only edit
    /// `overlay_config.json`, and this polls it once per tick.
    fn sync_external_state(&mut self) -> bool {
        let Ok(text) = std::fs::read_to_string(self.shared_config_path()) else {
            return false;
        };
        let flags = ExternalFlags::parse(&text);

        if let Some(pinned) = flags.pin_mode
            && pinned != self.config.pin_mode
        {
            self.config.pin_mode = pinned;
            self.apply_click_through();
        }

        let closing = is_closing(self.requested, flags.overlay_requested);
        self.requested = flags.overlay_requested.unwrap_or(true);
        closing
    }

    /// Applies the Win32 layered-window alpha when that mode is selected.
    fn apply_layered(&self) {
        if let (true, Some(hwnd)) = (self.config.transparency.uses_layered(), self.window_raw) {
            crate::winlayer::apply(hwnd, self.config.opacity);
        }
    }

    fn current_height(&self) -> f32 {
        self.config.fitted_height(self.top_apps.len())
    }

    /// Width that fits a set of entries.
    ///
    /// Shared by the two things that have to agree: what the window asks the OS
    /// for, and what the bar is actually able to draw.
    fn entries_width(&self, items: &[BarItem]) -> f32 {
        let label_size = self.config.font_size.label();
        let value_size = self.config.font_size.value();

        let widths: Vec<f32> = items
            .iter()
            .map(|item| {
                let value = text_width(&item.value, value_size, VALUE_CHAR_W);
                match &item.label {
                    Some(label) => text_width(label, label_size, LABEL_CHAR_W) + BAR_INNER_GAP + value,
                    None => value,
                }
            })
            .collect();

        match self.config.layout {
            Layout::Horizontal => {
                // Between two entries: the row spacing, the `·`, and the spacing again.
                let separator = text_width("·", label_size, LABEL_CHAR_W) + BAR_INNER_GAP * 2.0;
                let gaps = widths.len().saturating_sub(1) as f32;
                widths.iter().sum::<f32>() + separator * gaps
            }
            // The stack is as wide as its widest row: one entry per line.
            Layout::Vertical => widths.iter().fold(0.0_f32, |widest, width| widest.max(*width)),
        }
    }

    /// Width the content needs, before the `Width` setting is applied.
    ///
    /// The vertical layout is as wide as its widest row and the horizontal one as
    /// wide as its single line, so the widget hugs its numbers in both cases.
    fn content_width(&self) -> f32 {
        let pad = self.config.density.padding();

        // The grip is an overlay, so it must NOT be measured here — otherwise the
        // bar would reserve room it does not have.
        width_up(pad * 2.0 + self.entries_width(&self.bar_items()))
    }

    /// The entries the bar can actually draw at `cap`.
    ///
    /// The `Width` setting is a maximum, and neither layout can be narrowed
    /// below its content without cutting something. What may be cut is decided
    /// here, in this order:
    ///
    /// 1. **Labels.** A label is the expendable half of a row.
    /// 2. **Characters inside a label.** Enough to keep the row on one line.
    /// 3. **Whole entries, in the horizontal layout only.** A stack loses no
    ///    entry by getting narrower, but a single line does — and a line cut off
    ///    at the right edge reads as a smaller number, which is the one thing
    ///    this widget must never do. Dropping the trailing entries hides readings
    ///    instead of truncating them.
    ///
    /// The values themselves are never touched: [`OverlayApp::width_cap`] never
    /// offers a cap narrower than the widest one.
    fn fitted_items(&self) -> Vec<BarItem> {
        let items = self.bar_items();
        let pad = self.config.density.padding();
        let label_size = self.config.font_size.label();
        let value_size = self.config.font_size.value();
        let available = (self.target_width() - pad * 2.0).max(0.0);

        // Nothing to fit: an empty bar already fits any width.
        if items.is_empty() || self.entries_width(&items) <= available {
            return items;
        }

        match self.config.layout {
            // A stack is as wide as its widest row, so every row gives up just
            // enough of its *own* label for its own value to fit. One long
            // process name costs that one label its tail, not every row on the
            // card — and no row disappears.
            Layout::Vertical => items
                .into_iter()
                .map(|item| {
                    // A status sentence has no label, so the label-elision below
                    // would have nothing to work on and the sentence would run off
                    // the edge of the card. It is elided against the full width
                    // instead — and this is the one place a *value* is ever
                    // shortened, which is what [`BarItem::status`] buys.
                    if item.status {
                        return BarItem {
                            label: None,
                            value: elide(&item.value, label_size, available),
                            status: true,
                        };
                    }

                    let value_w = text_width(&item.value, value_size, VALUE_CHAR_W);
                    let label = item.label.and_then(|label| {
                        let fitted = elide(&label, label_size, available - BAR_INNER_GAP - value_w);
                        (!fitted.is_empty()).then_some(fitted)
                    });
                    BarItem {
                        label,
                        value: item.value,
                        status: false,
                    }
                })
                .collect(),

            // A line either has room for labels or has not, and half a line
            // labelled looks broken. So the labels go, and if the readings still
            // do not fit they are dropped from the end — in the order the user
            // arranged them, so what survives is always the top of the list.
            Layout::Horizontal => {
                let mut bare: Vec<BarItem> = items
                    .iter()
                    .map(|item| BarItem {
                        label: None,
                        value: item.value.clone(),
                        status: item.status,
                    })
                    .collect();
                let fits = (1..=items.len())
                    .rev()
                    .find(|count| self.entries_width(&bare[..*count]) <= available)
                    .unwrap_or(1);
                bare.truncate(fits);
                // Truncating can leave the status sentence as the only thing on
                // the line, and a sentence cut by a *number* drop is worse than
                // one cut by the window — so it is elided to whatever is left.
                let others: f32 = bare
                    .iter()
                    .filter(|item| !item.status)
                    .map(|item| text_width(&item.value, value_size, VALUE_CHAR_W) + BAR_INNER_GAP)
                    .sum();
                if let Some(status) = bare.iter_mut().find(|item| item.status) {
                    status.value = elide(&status.value, label_size, (available - others).max(0.0));
                }
                bare
            }
        }
    }

    /// The width the window should take: what the content needs, capped by the
    /// `Width` setting.
    ///
    /// The setting is a maximum rather than an exact size on purpose. Neither
    /// layout can be narrowed below its content without either wrapping it onto
    /// more lines or cutting it, and both are worse than simply being as wide as
    /// the numbers are — a window padded out to the setting would sit there
    /// mostly empty.
    fn target_width(&self) -> f32 {
        // A widget that is showing *only* a sentence is exempt from the `Width`
        // setting. The cap is there so a long process name cannot push the
        // numbers off the screen — and there are no numbers here to protect.
        // Clipping a sentence to 140 px turns "WattSeal not found — put it next
        // to this file" into "WattSeal not found — p…", which drops the only
        // part that tells the user what to do.
        //
        // The exemption is as narrow as it can be: it needs *every* row to be a
        // status row, so one sentence beside live numbers still respects the cap.
        let content = self.content_width();
        if self.bar_items().iter().all(|item| item.status) && !self.bar_items().is_empty() {
            return content;
        }
        content.min(self.width_cap())
    }

    /// The `Width` setting, never below what a single value needs.
    fn width_cap(&self) -> f32 {
        self.config.width.max(self.width_floor())
    }

    /// Lowest width the `Width` slider offers, snapped to the ladder so the
    /// number shown next to it is a width the window can actually take.
    fn width_floor(&self) -> f32 {
        width_near(self.values_floor().max(MIN_CHOICE_WIDTH))
    }

    /// The entries of the bar, in the order they are shown.
    ///
    /// Both layouts are built from these; only their arrangement differs.
    fn bar_items(&self) -> Vec<BarItem> {
        let status = self.status();

        // A state where the readings cannot be trusted gets a sentence *instead*
        // of the readings. A frozen number is the one thing this widget must
        // never show, because it is indistinguishable from a live one.
        if status.suppresses_metrics() {
            let mut rows: Vec<BarItem> = status
                .message(self.language)
                .map(|message| {
                    vec![BarItem {
                        label: None,
                        value: message.to_string(),
                        status: true,
                    }]
                })
                .unwrap_or_default();
            // The escape hint goes on even here. A widget with no readings *and*
            // no mouse is the case where a user is most stuck and least able to
            // find out why.
            rows.extend(self.stuck_hint());
            return rows;
        }

        // Otherwise the sentence joins the numbers. Only `ForeignGeneration`
        // reaches here with something to say, and the figures underneath it are
        // real.
        let mut items = Vec::new();
        if let Some(message) = status.message(self.language) {
            items.push(BarItem {
                label: None,
                value: message.to_string(),
                status: true,
            });
        }

        // A label is translated text, and in the degraded mode there is no
        // language to confirm it against — so the bar drops to bare numbers.
        let show_labels = self.config.show_labels && !self.is_degraded();

        for metric in self.shown_metrics() {
            if metric.is_multi() {
                for (name, watts) in &self.top_apps {
                    items.push(BarItem {
                        label: Some(truncate(name, TOP_NAME_MAX)),
                        value: self.format_value(Some(*watts)),
                        status: false,
                    });
                }
            } else {
                items.push(BarItem {
                    label: show_labels.then(|| self.metric_label(metric).to_string()),
                    value: self.format_value(self.power.get(metric.id()).copied()),
                    status: false,
                });
            }
        }

        // Last, so it is below the numbers rather than competing with them.
        //
        // **Only when there are no numbers.** This is the whole fix for the
        // regression that shipped: the bar is fitted as one unit, so a hint row
        // makes the horizontal layout drop labels — or whole readings — to find
        // room for something that is not a reading. Pinning is this widget's normal
        // state, so that cost was paid constantly.
        //
        // A hint is worth having exactly where it displaces nothing. Anywhere else
        // it can only ever make the widget worse at its job.
        if items.is_empty() {
            items.extend(self.stuck_hint());
        }

        items
    }

    /// The one row that tells a widget locked out of its own mouse how to get
    /// back.
    ///
    /// **A pinned, click-through widget cannot be clicked.** Its old escape hatch
    /// was the dashboard's hide-and-show, which released the pin from another
    /// process — and there is no other process now. Restarting does not help
    /// either, because the pin is written to the config file.
    ///
    /// So the shortcut comes first when it worked, and the config-file route is
    /// what is left when it did not — another program may already own the key,
    /// and in that case the hint is the only way out, so it has to stand on its
    /// own.
    fn stuck_hint(&self) -> Option<BarItem> {
        if !self.is_stuck_pinned() {
            return None;
        }
        Some(BarItem {
            label: None,
            value: if self.escape_hotkey_ready {
                crate::winlayer::escape_hint(self.language, self.escape_hotkey())
            } else {
                translations::hint_pinned_escape(self.language).to_string()
            },
            status: true,
        })
    }

    /// The metrics the bar draws, in the order they were configured in.
    ///
    /// The per-app rows are left out of the degraded mode: a process row needs a
    /// name that comes out of a second table, and at an unknown generation
    /// neither the name nor the energy can be confirmed. What is left is the
    /// one row per sensor, which is the whole of what this widget is for.
    fn shown_metrics(&self) -> Vec<Metric> {
        let degraded = self.is_degraded();
        self.config
            .metrics
            .iter()
            .copied()
            .filter(|metric| !(degraded && metric.is_multi()))
            .collect()
    }

    /// The rows of the left-hand settings column, in the order the panel builds
    /// them.
    fn appearance_rows(&self) -> SettingsColumn {
        // Section title, the opacity stepper, **two** colour pickers (background
        // and text), the transparency picker, the shadow toggle, five more
        // pickers (layout, density, text size, decimals, refresh), the paired
        // labels-and-units checkbox row and the abbreviated one. That is 13, which
        // is the number below — the comment used to say "three colour pickers"
        // and "the labels checkbox", which adds up to 14 and does not match.
        //
        // The theme is not among them: it is read from `ui_settings` like the
        // language, and there is nothing left to pick. Note that `labels` and
        // `units` share one row, so the panel has one fewer row than it has
        // controls.
        //
        // The opacity hint is always there; the shadow hint only when the mode
        // in use cannot render the shadow the toggle asks for.
        SettingsColumn::rows(13).with_hints(1 + u32::from(self.shadow_cannot_render()))
    }

    /// The rows of the right-hand settings column, in the order the panel builds
    /// them.
    fn rest_rows(&self) -> SettingsColumn {
        // Section title, always-on-top, the pin toggle (or its hint), the width
        // label and its slider, the content section title, one row per known
        // metric, the top-app count and the button row.
        let mut column = SettingsColumn::rows(13);
        if self.config.metrics.contains(&Metric::TopApps) {
            column = column.with_extra_row();
        }
        if !crate::winlayer::click_through_supported() {
            // No toggle to show, so the explanatory line takes its place.
            column = column.with_extra_hint();
        }
        column
    }

    /// Whether the active transparency mode can draw a drop shadow at all.
    ///
    /// A shadow is per-pixel alpha, so it can only be blended where the window
    /// surface carries some. The layered path composites the whole window at one
    /// constant alpha, which flattens the soft edge into a dark ring around the
    /// card; with transparency off there is nothing behind the window to blend
    /// into either.
    ///
    /// This is about the *mode*, not the setting: an unticked box in one of
    /// these modes is just as inert as a ticked one, and is exactly what the
    /// user needs telling about. On Windows the default mode is the layered one,
    /// so the hint is part of the panel's normal appearance there — which is
    /// true, and better said once than left to be discovered.
    fn shadow_cannot_render(&self) -> bool {
        !self.config.transparency.transparent_window()
    }

    /// Size the settings window should take.
    ///
    /// This used to be a fixed 560x452, chosen when the panel held what it holds
    /// now minus the drop-shadow row. Anything that made the columns taller — a
    /// larger text size, the roomier density, a hint that wraps onto a second
    /// line, a translated label that is longer than the English one — simply
    /// pushed the bottom of a column past the bottom of the window, where it
    /// was cut off with no way to scroll to it.
    ///
    /// The height is measured from the rows the panel is about to build, and
    /// capped at what the monitor can show so the panel cannot be pushed off
    /// the bottom edge. The cap is not the safety net, though — the panel
    /// scrolls. This is what makes it open at the right size in the first
    /// place; the scrollbar is what makes a wrong guess harmless.
    fn settings_size(&self) -> iced::Size {
        let pad = self.config.density.padding();
        let spacing = self.config.density.spacing();
        let row = self.config.density.row_height(self.config.font_size);
        let hint = self.config.font_size.label() * 0.85;

        let content = self
            .appearance_rows()
            .height(row, hint, spacing)
            .max(self.rest_rows().height(row, hint, spacing));

        let wanted = (content + pad * 2.0 + HEADER_HEIGHT + spacing)
            .ceil()
            .max(MIN_SETTINGS_HEIGHT);

        // Without a monitor yet (the first frame) the estimate stands on its own;
        // once the size is known the panel is never taller than the screen.
        let ceiling = match self.monitor {
            Some(monitor) => (monitor.height - SCREEN_MARGIN * 2.0).max(MIN_SETTINGS_HEIGHT),
            None => f32::INFINITY,
        };

        iced::Size::new(SETTINGS_WIDTH, wanted.min(ceiling))
    }

    /// Size the window should have right now.
    ///
    /// The width is what the content needs, capped by the `Width` setting, and
    /// the height follows the content as it always did, so the widget hugs its
    /// numbers in both layouts.
    fn fitted_size(&self) -> iced::Size {
        if self.show_settings {
            return self.settings_size();
        }
        // The menu replaces the metrics, so its own width is measured instead.
        if self.show_menu {
            return iced::Size::new(self.menu_width(), self.current_height());
        }

        iced::Size::new(self.target_width(), self.config.fitted_height(self.top_apps.len()))
    }

    /// Narrowest a value can be and still be read.
    ///
    /// The `Width` slider stops here: a lower cap would cut the numbers
    /// themselves, and an unreadable reading is worse than a window wider than
    /// the one that was asked for.
    fn values_floor(&self) -> f32 {
        let pad = self.config.density.padding();
        let value_size = self.config.font_size.value();
        let value_w = |text: String| text_width(&text, value_size, VALUE_CHAR_W);

        let widest = self.shown_metrics().iter().fold(0.0_f32, |widest, metric| {
            let value = if metric.is_multi() {
                self.top_apps.iter().fold(0.0_f32, |widest, (_, watts)| {
                    widest.max(value_w(self.format_value(Some(*watts))))
                })
            } else {
                value_w(self.format_value(self.power.get(metric.id()).copied()))
            };
            widest.max(value)
        });

        width_up(pad * 2.0 + widest)
    }

    fn resize_task(&mut self) -> Task<Message> {
        let size = self.fitted_size();
        self.applied = size;
        match self.window_id {
            Some(id) => window::resize::<Message>(id, size),
            None => Task::none(),
        }
    }

    fn set_level(&self, level: window::Level) -> Task<Message> {
        match self.window_id {
            Some(id) => window::set_level::<Message>(id, level),
            None => Task::none(),
        }
    }

    fn apply_window_settings(&self) -> Task<Message> {
        let Some(id) = self.window_id else {
            return Task::none();
        };
        let level = if self.config.always_on_top {
            window::Level::AlwaysOnTop
        } else {
            window::Level::Normal
        };
        let size = self.fitted_size();
        let mut tasks: Vec<Task<Message>> = vec![
            window::set_level::<Message>(id, level),
            window::resize::<Message>(id, size),
            // The window always hugs its content, so the OS must not offer a
            // manual resize border.
            window::set_resizable::<Message>(id, false),
            // Remembered so the settings panel can be capped at what the screen
            // can actually show instead of being pushed off the bottom of it.
            window::monitor_size(id).map(Message::MonitorSize),
        ];
        if self.config.position.is_none() {
            tasks.push(window::monitor_size(id).and_then(move |monitor| {
                let point = anchor_point(monitor, size.width, size.height);
                window::move_to::<Message>(id, point)
            }));
        }
        Task::batch(tasks)
    }

    /// Nudges the window back inside the monitor after it grew.
    ///
    /// A window grows from its top-left corner, so one docked to the right edge
    /// used to push the settings panel or the menu off the screen.
    fn keep_on_screen(&self) -> Task<Message> {
        let (Some(id), Some((x, y))) = (self.window_id, self.config.position) else {
            return Task::none();
        };

        let size = self.fitted_size();
        window::monitor_size(id).and_then(move |monitor| {
            let target = clamp_point(monitor, size, iced::Point::new(x, y));

            if target == iced::Point::new(x, y) {
                Task::none()
            } else {
                window::move_to::<Message>(id, target)
            }
        })
    }
}

/// How the application currently looks, given the row it saves its choices to.
///
/// Language and theme together because they are read together and because they
/// are the same decision: the widget follows the application instead of keeping
/// its own copy of the two settings the application already owns. `English` and
/// dark when there is nothing to read yet, which is also what a standalone run
/// sees before the collector has started.
pub fn appearance_from(settings: Option<&UiSettings>) -> (Language, ThemeChoice) {
    match settings {
        Some(settings) => (
            Language::from_code(&settings.language),
            ThemeChoice::from_dashboard_name(&settings.theme),
        ),
        None => (Language::default(), ThemeChoice::default()),
    }
}

/// Resolves the language and the scheme, in one place.
///
/// The order is the whole of the override: **the widget's own file wins, then
/// the dashboard's, then a default.** That order exists because the dashboard is
/// optional — a widget that had only ever followed it left a user who never opens
/// WattSeal with no way to change anything, which is not a setting, it is a wall.
///
/// Pure, and taking the dashboard's row as data rather than going to the database
/// for it, because "which of these two wins" is a question about precedence and
/// has no business opening a file to answer.
pub fn resolve_appearance(config: &OverlayConfig, dashboard: Option<&UiSettings>) -> (Language, ThemeChoice) {
    let (language, theme) = appearance_from(dashboard);
    (config.language.unwrap_or(language), config.theme.unwrap_or(theme))
}

/// Why the widget is showing what it is showing.
///
/// A widget whose numbers do not move, with no reason on screen, is a widget
/// nobody can act on: the user cannot tell a stopped collector from a broken
/// widget from a database in the wrong place. Every state here except
/// [`Live`](Status::Live) has a sentence attached to it.
///
/// The split that matters is not how many states there are but whether the
/// *readings* can be trusted:
/// - [`SuppressesMetrics`](Status::SuppressesMetrics) — there is nothing worth
///   showing, so the sentence replaces the metrics.
/// - [`ForeignGeneration`](Status::ForeignGeneration) — the numbers still arrive
///   (they are read by column name, which is the point), so the sentence is
///   *added* rather than substituted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    /// Numbers are arriving.
    Live,
    /// No WattSeal beside this executable to start.
    NotInstalled,
    /// A WattSeal was started and has not produced its first sample yet.
    Starting,
    /// Starting is switched off and there is nothing to read.
    LaunchDisabled,
    /// A database exists, and the collector has stopped writing to it.
    Stalled(u64),
    /// The database is at a generation this build does not read.
    ForeignGeneration(i32),
}

impl Status {
    /// Whether the sentence replaces the metrics rather than joining them.
    ///
    /// The reason for the split is that a stale reading and a current one look
    /// exactly the same on screen. The whole value of this widget is that the
    /// number is *now*, so a number that stopped moving is not shown at all —
    /// it is replaced by a sentence saying it stopped. Withholding it is the
    /// same rule the reader already follows for a column it cannot find: fewer
    /// numbers, never a wrong one.
    pub fn suppresses_metrics(self) -> bool {
        match self {
            Status::Live | Status::ForeignGeneration(_) => false,
            Status::NotInstalled | Status::Starting | Status::LaunchDisabled | Status::Stalled(_) => true,
        }
    }

    /// The one sentence for this state, in `language`.
    ///
    /// **These strings are compiled into this binary, not read from the
    /// database.** That is what makes them safe to show in the degraded
    /// presentation, where the dashboard's language is exactly the thing that
    /// cannot be trusted — and it is why a language the user pinned in this
    /// widget's own file still applies there.
    pub fn message(self, language: Language) -> Option<&'static str> {
        match self {
            Status::Live => None,
            Status::NotInstalled => Some(translations::status_not_installed(language)),
            Status::Starting => Some(translations::status_starting(language)),
            Status::LaunchDisabled => Some(translations::status_launch_disabled(language)),
            Status::Stalled(_) => Some(translations::status_stalled(language)),
            Status::ForeignGeneration(_) => Some(translations::status_foreign_generation(language)),
        }
    }
}

/// Seconds without a new sample after which a collector counts as stopped.
///
/// Long enough that a machine under load is not called broken, short enough that
/// a user who has just closed WattSeal is not left staring at a frozen number.
const STALE_AFTER_SECONDS: u64 = 15;

/// How long after start-up a collector counts as still starting, when the
/// widget did not start it itself.
const STARTING_WINDOW_SECONDS: u64 = 12;

/// Decides what the widget is showing, from what it can see.
///
/// Pure, so every combination of "collector state", "did we start it" and "how
/// long has it been like this" can be checked without a database — the states
/// interact, and a widget whose reason for being blank is decided by four
/// interacting booleans is exactly the thing that gets one branch wrong and
/// nobody notices until a user hits it.
pub fn status_of(
    availability: Availability,
    launch_enabled: bool,
    launched: bool,
    program_missing: bool,
    freshness: Option<i64>,
    app_age_seconds: u64,
) -> Status {
    match availability {
        // Checked first: the numbers are readable here, so the sentence is
        // additional rather than a replacement.
        Availability::ForeignGeneration(generation) => Status::ForeignGeneration(generation),

        Availability::Ready => match freshness {
            Some(seconds) if seconds >= 0 && (seconds as u64) <= STALE_AFTER_SECONDS => Status::Live,
            // We started it, so it is slow rather than stopped.
            _ if launched || app_age_seconds <= STARTING_WINDOW_SECONDS => Status::Starting,
            _ => Status::Stalled(freshness.unwrap_or(i64::MAX).max(0) as u64),
        },

        // Right generation, but the sensor tables are not there yet. That is the
        // collector's first moments — unless we did not start it and it has been
        // a while, which is a collector that never got going.
        Availability::Starting => {
            if launched || app_age_seconds <= STARTING_WINDOW_SECONDS {
                Status::Starting
            } else {
                Status::Stalled(STALE_AFTER_SECONDS)
            }
        }

        Availability::Missing => {
            if program_missing {
                Status::NotInstalled
            } else if !launch_enabled {
                Status::LaunchDisabled
            } else {
                Status::Starting
            }
        }
    }
}

/// The two flags in the shared config file that another process owns.
///
/// Everything else in the file belongs to this one and is ignored here, and a
/// file that cannot be parsed yields two `None`s rather than an error: the
/// overlay is polling a file that another process may be rewriting under it, and
/// the worst a half-written file may do is make this tick see nothing.
///
/// `None` means "not stated", which is not the same as `false`. The file omits
/// keys that were never written, and a missing `overlay_requested` has to read
/// as *open*, or a fresh run would be closed by the absence of a value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
struct ExternalFlags {
    pin_mode: Option<bool>,
    overlay_requested: Option<bool>,
}

impl ExternalFlags {
    fn parse(text: &str) -> Self {
        let Ok(value) = serde_json::from_str::<serde_json::Value>(text) else {
            return Self::default();
        };
        let flag = |name: &str| value.get(name).and_then(serde_json::Value::as_bool);
        Self {
            pin_mode: flag("pin_mode"),
            overlay_requested: flag("overlay_requested"),
        }
    }
}

/// Whether this tick is the one the overlay should close on.
///
/// Only a `true -> false` transition closes it. The alternative — "close
/// whenever the file says false" — kills a fresh `--overlay` run outright when a
/// previous session left `false` behind, which is the normal state of the file
/// after the overlay exits from its own menu.
fn is_closing(previous: bool, now: Option<bool>) -> bool {
    previous && now == Some(false)
}

/// Snaps to the top-right corner of the monitor, keeping a small margin.
fn anchor_point(monitor: iced::Size, width: f32, _height: f32) -> iced::Point {
    const MARGIN: f32 = 16.0;
    iced::Point::new((monitor.width - width - MARGIN).max(MARGIN), MARGIN)
}

/// Pulls a window back so that one of `size` at `point` still fits the monitor,
/// keeping the same margin the anchor uses.
fn clamp_point(monitor: iced::Size, size: iced::Size, point: iced::Point) -> iced::Point {
    const MARGIN: f32 = 16.0;
    let right = (monitor.width - size.width - MARGIN).max(MARGIN);
    let bottom = (monitor.height - size.height - MARGIN).max(MARGIN);

    iced::Point::new(point.x.clamp(MARGIN, right), point.y.clamp(MARGIN, bottom))
}

fn truncate(name: &str, max: usize) -> String {
    if name.chars().count() <= max {
        return name.to_string();
    }
    let mut out: String = name.chars().take(max.saturating_sub(1)).collect();
    out.push('…');
    out
}

/// Shortens `text` until it fits inside `available` at `size`.
///
/// Used on the half of a row that is allowed to give way. Pure and tested, so
/// the shrinking is a rule rather than something to eyeball at a dozen width
/// settings; an empty string is a valid answer and means "drop the label".
fn elide(text: &str, size: f32, available: f32) -> String {
    if available <= 0.0 {
        return String::new();
    }
    if text_width(text, size, LABEL_CHAR_W) <= available {
        return text.to_string();
    }

    // The longest prefix that still fits once the ellipsis is accounted for —
    // the truncation has to be visible, or a label reads as a whole word.
    let chars: Vec<char> = text.chars().collect();
    let fits = |length: usize| {
        let prefix: String = chars[..length].iter().collect();
        text_width(&prefix, size, LABEL_CHAR_W) + ELLIPSIS_W * size <= available
    };

    // The predicate is monotone — a longer prefix is never narrower — so the
    // longest fitting length is a binary search. `best` rather than the bound
    // the search ends on: the loop stops *at* the first length that does not
    // fit, and that length itself is one character too many.
    let mut best = 0;
    let (mut low, mut high) = (0, chars.len());
    while low <= high {
        let middle = low + (high - low) / 2;
        if fits(middle) {
            best = middle;
            low = middle + 1;
        } else if middle == 0 {
            break;
        } else {
            high = middle - 1;
        }
    }

    if best == 0 {
        return String::new();
    }
    chars[..best].iter().collect::<String>() + "…"
}

// ---- style helpers ----

fn with_alpha(color: Color, alpha: f32) -> Color {
    Color { a: alpha, ..color }
}

/// The card's container style.
///
/// `shadow` is the resolved verdict from [`OverlayConfig::draws_shadow`], not the
/// raw setting: the caller knows the transparency mode, and a shadow drawn where
/// the window cannot blend one is worse than no shadow at all.
fn card_style(palette: Palette, opacity: f32, shadow: bool) -> impl Fn(&Theme) -> iced::widget::container::Style {
    move |_theme| iced::widget::container::Style {
        background: Some(Background::Color(palette.card_with_alpha(opacity))),
        border: Border {
            color: palette.border,
            width: 1.0,
            radius: 12.0.into(),
        },
        text_color: Some(palette.text),
        shadow: if shadow {
            Shadow {
                color: palette.shadow,
                offset: Vector::new(0.0, 3.0),
                blur_radius: 14.0,
            }
        } else {
            Shadow::default()
        },
        ..Default::default()
    }
}

fn header_style(palette: Palette) -> impl Fn(&Theme) -> iced::widget::container::Style {
    move |_theme| iced::widget::container::Style {
        background: Some(Background::Color(with_alpha(palette.text, 0.06))),
        border: Border {
            color: with_alpha(palette.text, 0.10),
            width: 1.0,
            radius: 8.0.into(),
        },
        text_color: Some(palette.text),
        ..Default::default()
    }
}

fn flat_button(palette: Palette) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |_theme, status| button::Style {
        background: match status {
            button::Status::Hovered | button::Status::Pressed => {
                Some(Background::Color(with_alpha(palette.text, 0.12)))
            }
            _ => None,
        },
        text_color: palette.muted,
        border: Border::default(),
        shadow: Shadow::default(),
        ..Default::default()
    }
}

fn label<'a>(text: &'a str, font: f32, palette: Palette) -> Element<'a, Message, Theme> {
    Text::new(text)
        .size(font)
        .color(palette.muted)
        .width(Length::Fill)
        .into()
}

fn section_title<'a>(text: &'a str, font: f32, palette: Palette) -> Element<'a, Message, Theme> {
    Text::new(text)
        .size(font)
        .font(Font {
            weight: Weight::Bold,
            ..Font::DEFAULT
        })
        .color(palette.text)
        .into()
}

/// A pick-list whose options and selected value are rendered in `language`.
///
/// `pick_list` displays items through `Display`, so the translation has to
/// travel with each value — the same problem the dashboard solves with its
/// `TranslatedMetricType` wrapper.
fn labeled_pick<'a, T>(
    options: &'a [T],
    selected: T,
    language: Language,
    on_select: impl Fn(T) -> Message + 'a,
) -> Element<'a, Message, Theme>
where
    T: Localize + Clone + PartialEq + 'a,
{
    let choices: Vec<Labeled<T>> = options
        .iter()
        .cloned()
        .map(|value| Labeled::new(value, language))
        .collect();

    pick_list(
        choices,
        Some(Labeled::new(selected, language)),
        move |option: Labeled<T>| on_select(option.value),
    )
    .into()
}

fn picker<'a>(
    title: &'a str,
    list: impl Into<Element<'a, Message, Theme>>,
    font: f32,
    palette: Palette,
) -> Element<'a, Message, Theme> {
    Row::new()
        .spacing(8)
        .align_y(Alignment::Center)
        .push(label(title, font, palette))
        .push(
            Container::new(list.into())
                .width(Length::Fixed(108.0))
                .align_x(Alignment::End),
        )
        .into()
}

fn toggle(
    title: &'static str,
    checked: bool,
    on_toggle: impl Fn(bool) -> Message + 'static,
    font: f32,
    _palette: Palette,
) -> Element<'static, Message, Theme> {
    checkbox(checked)
        .label(title)
        .text_size(font)
        .on_toggle(on_toggle)
        .into()
}

fn step_button<'a>(text: &'a str, message: Message, palette: Palette, font: f32) -> Element<'a, Message, Theme> {
    button(Text::new(text).size(font).font(FONT_VALUE))
        .style(flat_button(palette))
        .on_press(message)
        .padding(Padding::from([1, 8]))
        .into()
}

/// A `label  −  80%  +` row, used for the widget's single opacity setting.
///
/// This used to say "the two opacity settings". There has only ever been one:
/// `OverlayConfig` has one `opacity` field and this row is called once, for it.
/// A comment that over-counts is the same kind of lie as a comment that is
/// simply wrong — it reads as if there were room for a second one.
fn stepper_row<'a>(
    title: &'a str,
    ratio: f32,
    decrease: Message,
    increase: Message,
    font: f32,
    palette: Palette,
) -> Element<'a, Message, Theme> {
    Row::new()
        .spacing(4)
        .align_y(Alignment::Center)
        .push(label(title, font, palette))
        .push(step_button("−", decrease, palette, font))
        .push(
            Text::new(format!("{:.0}%", ratio * 100.0))
                .size(font)
                .font(FONT_VALUE)
                .color(palette.text)
                .width(Length::Fixed(34.0)),
        )
        .push(step_button("+", increase, palette, font))
        .into()
}

/// A small muted explanatory line shown under a setting.
/// Turns a keypress into the message that binds it, or `None` if it is not a key
/// that can end a shortcut.
///
/// A named function rather than a closure inside the subscription because the
/// translation from iced's key vocabulary to Windows virtual-key codes is the
/// part most likely to be subtly wrong — and a wrong code is a shortcut that
/// silently never fires, which is the failure this whole feature exists to make
/// impossible.
///
/// Only letters, digits and F1–F12 are accepted, matching
/// [`crate::winlayer::Hotkey`]. Anything else produces no message at all, so
/// pressing, say, a bare arrow key leaves the capture mode open and waiting
/// rather than binding something the platform cannot register.
fn hotkey_from_key(key: &iced::keyboard::Key, ctrl: bool, alt: bool, shift: bool) -> Option<Message> {
    use iced::keyboard::key::Named;

    let vk = match key {
        iced::keyboard::Key::Character(text) => {
            let mut characters = text.chars();
            let first = characters.next()?;
            // `^`, `%` and friends arrive as characters with a modifier attached.
            // Mapping them would bind a key the user cannot see on their keyboard.
            if characters.next().is_some() {
                return None;
            }
            match first.to_ascii_uppercase() {
                'A'..='Z' => first.to_ascii_uppercase() as u32,
                '0'..='9' => first as u32,
                _ => return None,
            }
        }
        iced::keyboard::Key::Named(Named::F1) => 0x70,
        iced::keyboard::Key::Named(Named::F2) => 0x71,
        iced::keyboard::Key::Named(Named::F3) => 0x72,
        iced::keyboard::Key::Named(Named::F4) => 0x73,
        iced::keyboard::Key::Named(Named::F5) => 0x74,
        iced::keyboard::Key::Named(Named::F6) => 0x75,
        iced::keyboard::Key::Named(Named::F7) => 0x76,
        iced::keyboard::Key::Named(Named::F8) => 0x77,
        iced::keyboard::Key::Named(Named::F9) => 0x78,
        iced::keyboard::Key::Named(Named::F10) => 0x79,
        iced::keyboard::Key::Named(Named::F11) => 0x7A,
        iced::keyboard::Key::Named(Named::F12) => 0x7B,
        // Escape, handled before this is reached, is the only other key that means
        // anything here. Everything else is not a shortcut.
        _ => return None,
    };

    Some(Message::CapturedKey { ctrl, alt, shift, vk })
}

fn hint<'a>(text: &'a str, font: f32, palette: Palette) -> Element<'a, Message, Theme> {
    Text::new(text).size(font * 0.85).color(palette.muted).into()
}

/// [`hint`] for text that was built at draw time — the shortcut's own label.
///
/// Separate because the strings that quote a user-chosen key are `String`s made in
/// this function, and one of them has to be returned inside the `Element` that
/// borrows from `&self`. A `&str` view of a local cannot escape, so this one owns
/// its text instead.
fn hint_owned<'a>(text: String, font: f32, palette: Palette) -> Element<'a, Message, Theme> {
    Text::new(text).size(font * 0.85).color(palette.muted).into()
}

/// Boots the standalone overlay window.
pub fn run() -> iced::Result {
    let config = OverlayConfig::load().unwrap_or_default();
    let level = if config.always_on_top {
        window::Level::AlwaysOnTop
    } else {
        window::Level::Normal
    };
    let pos = match config.position {
        Some((x, y)) => window::Position::Specific(iced::Point::new(x, y)),
        None => window::Position::Centered,
    };

    iced::application(OverlayApp::new, OverlayApp::update, OverlayApp::view)
        .title(OverlayApp::title)
        .settings(iced::Settings {
            id: Some(String::from("wattseal-overlay")),
            fonts: Vec::new(),
            default_font: Font {
                family: Family::SansSerif,
                weight: Weight::Medium,
                ..Font::DEFAULT
            },
            default_text_size: 12.0.into(),
            antialiasing: true,
            vsync: true,
        })
        .window(window::Settings {
            // No icon. The window is undecorated, always on top, and usually
            // click-through; what it looks like in the taskbar is not something
            // a user sees. When this program was still built as part of
            // WattSeal it borrowed the dashboard's icon bytes, which is one more
            // reason it could not be released on its own — and one fewer
            // reason to regret losing them.
            icon: None,
            size: iced::Size::new(config.width, config.fitted_height(0)),
            position: pos,
            // The overlay is always sized to its content; no manual resizing.
            resizable: false,
            decorations: false,
            transparent: config.transparency.transparent_window(),
            blur: config.blur,
            level,
            platform_specific: platform_specific(),
            exit_on_close_request: false,
            ..Default::default()
        })
        .style(|state: &OverlayApp, _theme: &Theme| iced::theme::Style {
            background_color: state.window_background(),
            text_color: Color::WHITE,
        })
        .subscription(OverlayApp::subscription)
        .theme(OverlayApp::theme)
        .exit_on_close_request(false)
        .run()
}

/// Windows-only tweaks: hide from the taskbar and round the corners.
fn platform_specific() -> window::settings::PlatformSpecific {
    #[cfg(target_os = "windows")]
    {
        use window::settings::platform::CornerPreference;
        window::settings::PlatformSpecific {
            skip_taskbar: true,
            drag_and_drop: false,
            undecorated_shadow: false,
            corner_preference: CornerPreference::Round,
        }
    }
    #[cfg(not(target_os = "windows"))]
    {
        window::settings::PlatformSpecific::default()
    }
}

#[cfg(test)]
mod tests {
    use std::{
        path::{Path, PathBuf},
        sync::atomic::{AtomicU64, Ordering},
    };

    use super::*;
    use crate::config::Metric;

    /// Scratch files that delete themselves.
    ///
    /// Every test in this module wants its fixtures gone whether it passed or
    /// panicked. Cleaning up on the last line of a test is a promise every
    /// future edit can forget, and one forgotten line leaves a database behind in
    /// the user's temp directory on every run — which is how a `cargo test` loop
    /// ends up owning thousands of them.
    struct Scratch(Vec<PathBuf>);

    impl Scratch {
        fn new(name: &str) -> Self {
            static NEXT: AtomicU64 = AtomicU64::new(0);
            let unique = NEXT.fetch_add(1, Ordering::Relaxed);
            Self(vec![
                std::env::temp_dir().join(format!("wattseal-{name}-{}-{unique}", std::process::id())),
            ])
        }

        /// Takes over another set of scratch files, so one handle owns them all.
        ///
        /// Needed because a handle that goes out of scope early would delete a
        /// file the app it belongs to is still reading.
        fn absorb(&mut self, other: Scratch) {
            self.0.extend(other.0.iter().cloned());
        }
    }

    impl std::ops::Deref for Scratch {
        type Target = Path;

        fn deref(&self) -> &Path {
            &self.0[0]
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            for path in &self.0 {
                for suffix in ["", "-wal", "-shm", "-journal"] {
                    let _ = std::fs::remove_file(
                        path.with_file_name(format!("{}{suffix}", path.file_name().unwrap().to_string_lossy())),
                    );
                }
            }
        }
    }

    /// An overlay pointed at a scratch config file instead of the real one.
    ///
    /// `update()` persists on almost every message, so without this the tests
    /// below would write a real `overlay_config.json` next to the test binary —
    /// and, running side by side, would read each other's settings back.
    ///
    /// The returned `Scratch` owns the config file *and* the database the app
    /// reads, so keeping it alive keeps both and dropping it cleans up whether
    /// the test passed or not.
    fn app_with_config_file(config: OverlayConfig) -> (Scratch, OverlayApp) {
        let (database, mut app) = app_at(crate::source::SUPPORTED_GENERATION);
        let mut scratch = Scratch::new("app-config");
        app.config_path = Some(scratch.0[0].clone());
        app.config = config;
        scratch.absorb(database);
        (scratch, app)
    }

    /// The config as the other two processes would read it off disk.
    fn shared_file(app: &OverlayApp) -> OverlayConfig {
        OverlayConfig::load_from(&app.shared_config_path()).expect("the overlay wrote its config")
    }

    /// An overlay whose database sits at `generation`, with one live CPU sample
    /// in it. The tables come from the collector's own `CREATE TABLE`, so the
    /// fixture cannot drift away from the schema it stands in for.
    ///
    /// The scratch handle comes **first** in the tuple on purpose. Rust drops
    /// locals in reverse order of declaration, and on Windows a database cannot
    /// be deleted while the app still has it open — so the handle has to be
    /// declared first in order to be dropped last. The other way round leaks
    /// every fixture the suite creates.
    fn app_at(generation: i32) -> (Scratch, OverlayApp) {
        app_with(generation, OverlayConfig::default())
    }

    /// An overlay reading a database that already exists at `path`.
    ///
    /// The caller owns `path` and its cleanup; this only borrows it. Used where
    /// a test has to write to the database between two ticks, which is how the
    /// dashboard changes something while the widget is running.
    fn app_reading(path: &Path, config: OverlayConfig) -> OverlayApp {
        OverlayApp {
            config,
            config_path: None,
            window_id: None,
            window_raw: None,
            power: HashMap::new(),
            top_apps: Vec::new(),
            requested: true,
            show_settings: false,
            show_menu: false,
            applied: iced::Size::ZERO,
            monitor: None,
            language: Language::English,
            theme: ThemeChoice::default(),
            source: Source::at(path),
            // Tests never launch anything: the whole point of a fixture is that
            // it is the only thing touching the filesystem, and a test that
            // spawned WattSeal would outlive the test that spawned it.
            launch_attempted: true,
            launched: None,
            wattseal_missing: false,
            escape_hotkey_ready: false,
            capturing_hotkey: false,
            hotkey_refusal: None,
            click_through_applied: (false, false),
            // A fixture database is written by the test, not by a collector, so
            // it is "live" by the only measure the widget has: its newest sample
            // is the moment the test ran. Without this every test would see a
            // `Stalled` sentence over its numbers and assert against that.
            freshness: Some(0),
            started_at: std::time::Instant::now(),
        }
    }

    fn app_with(generation: i32, config: OverlayConfig) -> (Scratch, OverlayApp) {
        let scratch = Scratch::new("app-fixture.db");
        let path = scratch.0[0].clone();

        let conn = rusqlite::Connection::open(&path).unwrap();
        conn.pragma_update(None, "journal_mode", "WAL").unwrap();
        conn.execute_batch(
            "CREATE TABLE hardware_info (id INTEGER PRIMARY KEY, tables TEXT, hardware_data TEXT);
             INSERT INTO hardware_info VALUES (1, 'cpu_data', '{}');",
        )
        .unwrap();
        conn.execute_batch(
            "CREATE TABLE cpu_data (
                 \"timestamp\"     INTEGER NOT NULL,
                 \"duration_ms\"   INTEGER NOT NULL,
                 \"total_energy_uj\" INTEGER,
                 PRIMARY KEY (\"timestamp\", \"duration_ms\")
             ) WITHOUT ROWID;",
        )
        .unwrap();
        conn.execute_batch("INSERT INTO cpu_data VALUES (1000, 1000, 1000000);")
            .unwrap();
        conn.pragma_update(None, "user_version", generation).unwrap();
        drop(conn);

        let app = OverlayApp {
            config,
            config_path: None,
            window_id: None,
            window_raw: None,
            power: HashMap::new(),
            top_apps: vec![(String::from("firefox"), 4.0)],
            requested: true,
            show_settings: false,
            show_menu: false,
            applied: iced::Size::ZERO,
            monitor: None,
            language: Language::English,
            theme: ThemeChoice::default(),
            source: Source::at(&path),
            launch_attempted: true,
            launched: None,
            wattseal_missing: false,
            escape_hotkey_ready: false,
            capturing_hotkey: false,
            hotkey_refusal: None,
            click_through_applied: (false, false),
            freshness: Some(0),
            started_at: std::time::Instant::now(),
        };
        (scratch, app)
    }

    /// Values in the order the bar draws them.
    fn values_of(items: &[BarItem]) -> Vec<&str> {
        items.iter().map(|item| item.value.as_str()).collect()
    }

    #[test]
    fn a_label_shortens_before_it_is_dropped_and_says_so() {
        // Room to spare: nothing happens.
        assert_eq!(elide("Total", 10.0, 100.0), "Total");

        // No room at all: the label goes rather than push its value off.
        assert_eq!(elide("Total", 10.0, 0.0), "");
        assert_eq!(elide("Total", 10.0, -5.0), "");

        // In between: a visible prefix, and an ellipsis so the reader can see
        // that the label is cut rather than complete.
        let short = elide("Network", 10.0, 20.0);
        assert!(short.ends_with('…'), "{short:?} does not admit to being cut");
        assert!(short.chars().count() < "Network".len());
        assert!(
            text_width(&short, 10.0, LABEL_CHAR_W) <= 20.0,
            "{short:?} was allowed to stay wider than the room it had"
        );
    }

    #[test]
    fn a_narrow_stack_shortens_only_the_rows_that_have_to() {
        let config = OverlayConfig {
            layout: Layout::Vertical,
            width: 96.0,
            ..OverlayConfig::default()
        };
        let (_scratch, app) = app_with(crate::source::SUPPORTED_GENERATION, config);
        let items = app.fitted_items();

        // The one error this widget must never make is a value that looks
        // smaller because it ran off the right edge.
        assert_eq!(values_of(&items), values_of(&app.bar_items()));
        assert!(
            app.entries_width(&items) <= app.target_width(),
            "{}px of content in a {}px window",
            app.entries_width(&items),
            app.target_width()
        );

        // Every row still has a value, and the rows that were left alone kept
        // their full label: one long process name must not cost every other
        // row its name.
        assert!(items.iter().all(|item| !item.value.is_empty()));
        assert!(
            items.iter().any(|item| item.label.is_some()),
            "narrowing the stack dropped every label instead of shortening one"
        );
    }

    #[test]
    fn a_narrow_line_drops_labels_then_readings_never_digits() {
        let config = OverlayConfig {
            layout: Layout::Horizontal,
            width: 60.0,
            ..OverlayConfig::default()
        };
        let (_scratch, app) = app_with(crate::source::SUPPORTED_GENERATION, config);
        let items = app.fitted_items();
        let full = app.bar_items();

        // Half a labelled line looks broken, so the labels all go.
        assert!(items.iter().all(|item| item.label.is_none()));
        assert!(!items.is_empty(), "an empty bar says nothing at all");

        // What survives is the front of the list, whole: hiding a reading beats
        // showing a truncated one.
        assert_eq!(values_of(&items), values_of(&full)[..values_of(&items).len()]);
        assert!(
            values_of(&items).len() < values_of(&full).len(),
            "the fixture was never narrow enough to matter"
        );
    }

    #[test]
    fn no_width_setting_ever_halves_a_reading() {
        // The slider offers 60..=1200 in rungs; the point of this is that none of
        // them, in either layout, produces a value the bar did not intend to
        // draw — which is what a cut-off number looks like.
        for layout in [Layout::Vertical, Layout::Horizontal] {
            for width in (60..=600).step_by(12) {
                let config = OverlayConfig {
                    layout,
                    width: width as f32,
                    ..OverlayConfig::default()
                };
                let (_scratch, app) = app_with(crate::source::SUPPORTED_GENERATION, config);
                let full = app.bar_items();
                let fitted = app.fitted_items();
                let drawn = values_of(&fitted);

                assert_eq!(
                    drawn,
                    values_of(&full)[..drawn.len()],
                    "{layout:?} at {width}px kept readings that are not the front of the list"
                );
                assert!(
                    app.entries_width(&fitted) <= app.target_width() || drawn.len() == 1,
                    "{layout:?} at {width}px still overflows"
                );
            }
        }
    }

    #[test]
    fn the_shared_file_only_ever_tells_the_overlay_two_things() {
        let flags =
            ExternalFlags::parse(r#"{"pin_mode":true,"overlay_requested":false,"opacity":0.5,"metrics":["total"]}"#);
        assert_eq!(
            flags,
            ExternalFlags {
                pin_mode: Some(true),
                overlay_requested: Some(false),
            }
        );

        // Keys this process owns are not another process's business.
        let flags = ExternalFlags::parse(r#"{"opacity":0.5,"metrics":["total"]}"#);
        assert_eq!(flags, ExternalFlags::default());
    }

    #[test]
    fn a_file_mid_rewrite_reads_as_nothing_rather_than_as_a_close() {
        // Another process writes the file by rename, so a poll can still land on
        // a truncated document. Half a document must never read as "close".
        for text in ["", "{", "not json at all", r#"{"pin_mode":"yes"}"#, "[]"] {
            let flags = ExternalFlags::parse(text);
            assert_eq!(
                flags,
                ExternalFlags::default(),
                "{text:?} was read as something other than nothing"
            );
            assert!(
                !is_closing(true, flags.overlay_requested),
                "{text:?} closed the overlay"
            );
        }
    }

    #[test]
    fn only_a_true_to_false_transition_closes_the_overlay() {
        // The whole reason this rule exists: a file left at `false` by the last
        // session must not kill a fresh `--overlay` run on its first tick.
        assert!(!is_closing(true, None), "an absent flag closed a running overlay");
        assert!(!is_closing(true, Some(true)));
        assert!(is_closing(true, Some(false)));

        // Already closed: nothing left to close, and re-reading must not loop.
        assert!(!is_closing(false, Some(false)));
        assert!(!is_closing(false, None));
        assert!(!is_closing(false, Some(true)));
    }

    #[test]
    fn the_widgets_own_setting_wins_over_the_dashboards() {
        // The override is the whole reason this exists, so it is pinned from both
        // sides: pinned here beats the dashboard, and cleared here hands control
        // straight back.
        //
        // The second half is the one that would have been easy to get wrong. A
        // widget that only ever *adds* an override never gives it back, and a
        // user who pinned the language to escape a dashboard they disagree with
        // would then have no way to return to following it.
        let dashboard = UiSettings {
            language: "RO".to_string(),
            theme: "Splashing".to_string(),
        };

        // Nothing pinned: the dashboard decides, as it always did.
        assert_eq!(
            resolve_appearance(&OverlayConfig::default(), Some(&dashboard)),
            (Language::Romanian, ThemeChoice::Light),
            "with nothing pinned the dashboard still decides"
        );

        // Pinned: the widget's file decides, whatever the dashboard says.
        let pinned = OverlayConfig {
            language: Some(Language::Chinese),
            theme: Some(ThemeChoice::Dark),
            ..OverlayConfig::default()
        };
        assert_eq!(
            resolve_appearance(&pinned, Some(&dashboard)),
            (Language::Chinese, ThemeChoice::Dark),
            "the widget's own file did not win"
        );

        // Pinned to exactly what the dashboard said, to prove the assertion
        // above is about *winning* and not about two different values happening
        // to pass.
        let agreeing = OverlayConfig {
            language: Some(Language::Romanian),
            theme: Some(ThemeChoice::Light),
            ..OverlayConfig::default()
        };
        assert_eq!(
            resolve_appearance(&agreeing, Some(&dashboard)),
            (Language::Romanian, ThemeChoice::Light)
        );
    }

    #[test]
    fn a_dashboard_that_saved_nothing_leaves_the_widget_where_it_was() {
        // The standalone case: no dashboard, so `ui_settings` has no row at all.
        // An unset override has to resolve to a real language — the widget has to
        // be able to name its own labels — and to `Auto` rather than to a scheme,
        // because "automatic" is the honest answer to "what does following the
        // dashboard say, when there is no dashboard".
        let config = OverlayConfig::default();
        let (language, theme) = resolve_appearance(&config, None);
        assert_eq!(language, Language::English);
        assert_eq!(theme, ThemeChoice::Auto);

        // `Auto` is a preference, not something to draw. It becomes a scheme at
        // the point of drawing, and until then the widget has no business
        // pretending it knows.
        assert_eq!(theme.resolve(ThemeChoice::Dark), ThemeChoice::Dark);
    }

    #[test]
    fn automatic_is_resolved_never_drawn() {
        // `Auto` reaching `palette()` would mean the resolution was skipped, and
        // the only thing that function can do with it is guess. So both the
        // resolution and the two fallbacks are pinned here rather than left to
        // whichever test happens to touch them.
        assert_eq!(ThemeChoice::Auto.resolve(ThemeChoice::Light), ThemeChoice::Light);
        assert_eq!(ThemeChoice::Auto.resolve(ThemeChoice::Dark), ThemeChoice::Dark);

        // An explicit choice ignores the fallback entirely — in both directions,
        // because only one of them is a bug you would notice.
        assert_eq!(ThemeChoice::Dark.resolve(ThemeChoice::Light), ThemeChoice::Dark);
        assert_eq!(ThemeChoice::Light.resolve(ThemeChoice::Dark), ThemeChoice::Light);
    }

    #[test]
    fn unpinning_hands_the_language_back_to_the_dashboard() {
        // "Following" is a state, not the absence of one, and it has to be
        // reachable. A user who pinned the language because a dashboard kept
        // changing it needs a way back that does not involve editing JSON.
        let dashboard = UiSettings {
            language: "DE".to_string(),
            theme: "Swimming".to_string(),
        };

        let pinned = OverlayConfig {
            language: Some(Language::Chinese),
            theme: Some(ThemeChoice::Dark),
            ..OverlayConfig::default()
        };
        assert_eq!(resolve_appearance(&pinned, Some(&dashboard)).0, Language::Chinese);

        // Clearing is exactly `None`, and that is what the config file records
        // when the row is un-ticked — so the whole "give it back" behaviour is
        // `None` meaning what it says.
        let cleared = OverlayConfig {
            language: None,
            theme: None,
            ..pinned
        };
        assert_eq!(
            resolve_appearance(&cleared, Some(&dashboard)),
            (Language::German, ThemeChoice::Light),
            "clearing the override did not hand control back"
        );
    }

    #[test]
    fn the_language_picker_is_readable_from_any_language() {
        // The one list that must not be translated into the panel's current
        // language: it is how someone who cannot read the panel finds the setting
        // that fixes it. Every row is its own name, so every row is findable.
        let names: Vec<&str> = AppLanguage::all().iter().map(|l| l.native_name()).collect();
        assert_eq!(names, ["English", "Deutsch", "Français", "简体中文", "Română"]);

        // And the widget offers the same number the dashboard can store — a
        // language the widget could read but not offer would be one a user can
        // be given by a dashboard and then cannot choose back.
        assert_eq!(crate::AppLanguage::all().len(), 5);
    }

    #[test]
    fn every_language_the_widget_offers_is_fully_translated() {
        // A language offered but not translated shows a panel half in English,
        // which is worse than not offering it: the user picked something and got
        // a mixture, with nothing saying which parts were meant to be that way.
        let panel_strings: [(&str, fn(Language) -> &'static str); 8] = [
            ("section_appearance", translations::section_appearance),
            ("section_window", translations::section_window),
            ("section_content", translations::section_content),
            ("label_language", translations::label_language),
            ("label_theme", translations::label_theme),
            ("button_done", translations::button_done),
            ("button_quit_overlay", translations::button_quit_overlay),
            ("label_opacity", translations::label_opacity),
        ];

        for language in AppLanguage::all() {
            for (name, string) in panel_strings {
                let rendered = string(*language);
                assert!(
                    !rendered.trim().is_empty(),
                    "{name} is empty for {language:?}, so the panel would show a blank row"
                );
                assert!(
                    !rendered.contains("missing") && !rendered.contains("todo"),
                    "{name} still reads like a placeholder for {language:?}: {rendered:?}"
                );
            }
        }
    }

    #[test]
    fn the_widget_follows_the_language_and_theme_the_dashboard_saved() {
        // The dashboard owns both settings and the widget reads them back. The
        // row is written the way the dashboard writes it — English display name
        // for the theme, ISO code for the language — because the encoding is half
        // of what is being tested.
        //
        // The row in the database has four more columns than this. They are
        // deliberately absent here: the widget reads two of them and has no use
        // for the rest, and carrying fields it never reads is how a reader ends
        // up refusing to show anything because one column it does not care
        // about moved.
        let cases = [
            ("ZH", "Splashing", Language::Chinese, ThemeChoice::Light),
            ("DE", "Hunting", Language::German, ThemeChoice::Dark),
            ("RO", "Lounging", Language::Romanian, ThemeChoice::Dark),
            ("EN", "Swimming", Language::English, ThemeChoice::Light),
        ];

        for (code, theme, language, scheme) in cases {
            let settings = UiSettings {
                language: code.to_string(),
                theme: theme.to_string(),
            };

            assert_eq!(
                appearance_from(Some(&settings)),
                (language, scheme),
                "the dashboard's {code}/{theme} came back as something else"
            );
        }
    }

    #[test]
    fn a_dashboard_that_has_saved_nothing_leaves_the_widget_at_its_defaults() {
        assert_eq!(
            appearance_from(None),
            (Language::default(), ThemeChoice::default()),
            "the standalone run must still have a language and a scheme"
        );
    }

    #[test]
    fn a_change_in_the_dashboard_reaches_a_running_widget() {
        // The reading is per tick, not per launch: switching language in the
        // dashboard while the widget is open has to take effect without a
        // restart, which is the only reason it is worth reading at all.
        let scratch = Scratch::new("follows-dashboard.db");
        let path = scratch.0[0].clone();

        let conn = rusqlite::Connection::open(&path).unwrap();
        conn.pragma_update(None, "journal_mode", "WAL").unwrap();
        conn.execute_batch(
            "CREATE TABLE hardware_info (id INTEGER PRIMARY KEY, tables TEXT, hardware_data TEXT);
             INSERT INTO hardware_info VALUES (1, 'cpu_data', '{}');
             CREATE TABLE ui_settings (id INTEGER PRIMARY KEY CHECK (id = 1),
                 language TEXT NOT NULL DEFAULT 'EN',
                 carbon_intensity TEXT NOT NULL DEFAULT 'World average',
                 kwh_cost TEXT NOT NULL DEFAULT 'World average',
                 theme TEXT NOT NULL DEFAULT 'Hunting',
                 currency TEXT NOT NULL DEFAULT 'USD',
                 close_behavior TEXT NOT NULL DEFAULT 'ask');
             INSERT INTO ui_settings (id, language, theme) VALUES (1, 'EN', 'Hunting');",
        )
        .unwrap();
        conn.execute_batch(
            "CREATE TABLE cpu_data (
                 \"timestamp\"     INTEGER NOT NULL,
                 \"duration_ms\"   INTEGER NOT NULL,
                 \"total_energy_uj\" INTEGER,
                 PRIMARY KEY (\"timestamp\", \"duration_ms\")
             ) WITHOUT ROWID;",
        )
        .unwrap();
        conn.pragma_update(None, "user_version", crate::source::SUPPORTED_GENERATION)
            .unwrap();
        drop(conn);

        let mut app = app_reading(&path, OverlayConfig::default());
        // Its own config file, pointing at nothing. A tick starts by polling for
        // the flags the tray and the dashboard write, and this test is not about
        // that — without this it would read whatever file another test currently
        // has the shared location pointed at, see a close request that was not
        // meant for it, and return before it ever looked at the database.
        app.config_path = Some(Scratch::new("follows-dashboard-config").0[0].clone());

        let _ = app.update(Message::Tick);
        assert_eq!(app.language, Language::English);
        assert_eq!(app.theme, ThemeChoice::Dark);

        // The user switches both in the dashboard.
        let writer = rusqlite::Connection::open(&path).unwrap();
        writer
            .execute(
                "UPDATE ui_settings SET language = 'FR', theme = 'Splashing' WHERE id = 1",
                [],
            )
            .unwrap();
        drop(writer);

        let _ = app.update(Message::Tick);
        assert_eq!(app.language, Language::French, "the widget did not follow the language");
        assert_eq!(app.theme, ThemeChoice::Light, "the widget did not follow the theme");
    }

    #[test]
    fn the_settings_window_is_tall_enough_for_the_rows_it_lists() {
        // The arithmetic behind `settings_size`, re-derived here rather than
        // through `SettingsColumn::height`, so a mistake in the chrome — a
        // forgotten header, padding counted once instead of twice — shows up as
        // a window shorter than the rows it is supposed to hold. That is exactly
        // the failure the fixed height used to have.
        for density in [Density::Ultra, Density::Compact, Density::Normal] {
            for font in [FontSize::Small, FontSize::Medium, FontSize::Large] {
                let config = OverlayConfig {
                    density,
                    font_size: font,
                    metrics: Metric::ALL.to_vec(),
                    top_apps: 8,
                    ..OverlayConfig::default()
                };
                let (_scratch, app) = app_with(crate::source::SUPPORTED_GENERATION, config);

                let row = density.row_height(font);
                let hint = font.label() * 0.85;
                let chrome = 2.0 * density.padding() + HEADER_HEIGHT + density.spacing();

                for column in [app.appearance_rows(), app.rest_rows()] {
                    let needed = column.rows as f32 * row + column.hints as f32 * hint * HINT_LINES;
                    assert!(
                        app.settings_size().height >= needed + chrome,
                        "{density:?}/{font:?}: a column needs {}px of rows but the window is {}px",
                        needed + chrome,
                        app.settings_size().height,
                    );
                }
            }
        }
    }

    #[test]
    fn the_appearance_comments_arithmetic_matches_the_row_count() {
        // The comment above `appearance_rows()` used to claim "three colour
        // pickers", which makes its own sum 14 while the code says 13. A comment
        // that disagrees with the number next to it is worse than no comment: it
        // looks like a checked derivation.
        //
        // What this pins is that adding or removing a row means touching the
        // comment too — the two have to keep agreeing.
        let (_scratch, app) = app_with(crate::source::SUPPORTED_GENERATION, OverlayConfig::default());

        // Count the controls the view actually pushes into the appearance column.
        let pickers = [
            "label_bg_color",
            "label_text_color",
            "label_transparency",
            "label_layout",
            "label_density",
            "label_text_size",
            "label_decimals",
            "label_refresh",
        ];
        assert_eq!(pickers.len(), 8, "eight pick lists are built into this column");

        // title + opacity + 8 pickers + the shadow toggle + the paired
        // labels/units row + abbreviated. `shadow_row` is not a `picker(...)`,
        // which is exactly the row that was easy to forget when counting.
        let accounted_for = 1 + 1 + pickers.len() + 1 + 1 + 1;
        assert_eq!(
            app.appearance_rows().rows as usize,
            accounted_for,
            "the column builds more or fewer controls than the comment lists"
        );
    }

    #[test]
    fn a_widget_with_every_metric_off_is_still_a_widget() {
        // Nothing enabled is a legitimate state — the user can uncheck all of
        // them — and the bar falls back to a placeholder. What it must not do is
        // collapse to a sliver, or size itself for rows it never drew.
        let config = OverlayConfig {
            metrics: Vec::new(),
            ..OverlayConfig::default()
        };
        let (_scratch, app) = app_with(crate::source::SUPPORTED_GENERATION, config);

        assert!(app.bar_items().is_empty());
        let size = app.fitted_size();
        assert!(
            size.height >= 32.0 && size.width >= MIN_WIDTH,
            "the widget collapsed to {size:?}"
        );

        // And the horizontal layout, which fits one line, still has room for it.
        let config = OverlayConfig {
            layout: Layout::Horizontal,
            metrics: Vec::new(),
            ..OverlayConfig::default()
        };
        let (_scratch, app) = app_with(crate::source::SUPPORTED_GENERATION, config);
        assert!(app.fitted_size().height >= 16.0);
    }

    #[test]
    fn every_state_renders_without_falling_over() {
        // Building a view needs no window and no renderer, so this is the only
        // coverage the layout code can get here. It is worth having: the view is
        // a few hundred lines that nothing else in the suite executes, and the
        // states it has to survive are the ones with the fewest things behind
        // them — no metrics at all, a single app row, a label-less degraded bar.
        //
        // What it cannot do is check the result; it catches the crashes.
        //
        // One fixture, varied in place: a database per combination would make
        // this sweep cost more than the rest of the suite put together.
        let (_scratch, mut app) = app_with(crate::source::SUPPORTED_GENERATION, OverlayConfig::default());
        let palette = theme::palette(app.theme);
        let base = OverlayConfig::default();

        for layout in [Layout::Vertical, Layout::Horizontal] {
            for density in Density::ALL {
                for font in FontSize::ALL {
                    for language in crate::AppLanguage::all() {
                        for metrics in [
                            Vec::new(),
                            vec![Metric::Total],
                            vec![Metric::TopApps],
                            Metric::ALL.to_vec(),
                        ] {
                            app.config = OverlayConfig {
                                layout,
                                density: *density,
                                font_size: *font,
                                metrics,
                                ..base.clone()
                            };
                            app.language = *language;

                            for (show_settings, show_menu) in [(false, false), (true, false), (false, true)] {
                                app.show_settings = show_settings;
                                app.show_menu = show_menu;

                                // Each of these builds widgets; none of them may
                                // panic on an empty list or a single row.
                                let _ = app.view();
                                let _ = app.view_menu(palette, font.label());
                                let _ = app.view_metrics(palette, font.label(), font.value(), density.spacing());
                                let _ = app.view_settings(palette, font.label(), density.spacing());
                                let _ = app.settings_size();
                                let _ = app.fitted_size();
                                let _ = app.bar_items();
                                let _ = app.fitted_items();
                                let _ = app.menu_segments();
                            }
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn the_degraded_widget_renders_in_every_layout_too() {
        // The degraded bar has no labels and no app rows, which is a different
        // shape from every other state — and the one most likely to hit an
        // assumption about there being something to draw.
        for layout in [Layout::Vertical, Layout::Horizontal] {
            for generation in [
                crate::source::SUPPORTED_GENERATION + 1,
                crate::source::SUPPORTED_GENERATION,
            ] {
                let (_scratch, mut app) = app_with(generation, OverlayConfig::default());
                app.config.layout = layout;

                let palette = theme::palette(app.theme);
                let _ = app.view();
                let _ = app.view_menu(palette, 12.0);
                assert!(app.fitted_size().height > 0.0);
            }
        }
    }

    #[test]
    fn the_three_processes_agree_through_one_file() {
        // The whole feature end to end, with each side doing what it really
        // does: the tray and the dashboard write, the overlay polls and reacts.
        // The individual steps are covered in `lib.rs`; what this catches is a
        // disagreement *between* them — the dashboard writing `false` and the
        // overlay reading it as something else, or a flag that means one thing to
        // the writer and another to the reader. With no IPC, nothing else would
        // catch that.
        let shared = Scratch::new("app-contract");
        let shared_config = shared.0[0].clone();
        // Serialises this against the other tests that share the config file.
        let _serialise = crate::config::override_path(shared_config.clone());

        let database = Scratch::new("app-contract.db");

        // 1. The user pins from the tray, before there is any overlay at all.
        crate::toggle_pin();
        assert!(OverlayConfig::load_from(&shared_config).unwrap().pin_mode);

        // 2. The dashboard's footer is asked to show it.
        crate::request_open();
        assert!(crate::is_requested());

        // 3. The overlay process starts and polls. The pin it finds is not its
        //    own to clear, so it keeps it — that is what makes a widget pinned
        //    before it was opened come back pinned.
        let mut app = app_reading(&database.0[0], OverlayConfig::default());
        app.config_path = Some(shared_config.clone());
        assert!(!app.sync_external_state());
        assert!(app.config.pin_mode);

        // 4. The widget records its position. Only this process writes it, and
        //    the other two read the file, so it has to survive their writes.
        app.config.position = Some((5.0, 6.0));
        app.persist();
        assert_eq!(
            OverlayConfig::load_from(&shared_config).unwrap().position,
            Some((5.0, 6.0))
        );

        // 5. The user hides it from the dashboard — the only way out of a
        //    pinned, click-through widget on a system with no tray.
        crate::request_close();

        // 6. The overlay's next tick sees it and closes, and the pin goes with
        //    it, or the next widget would come back unclickable.
        assert!(app.sync_external_state(), "the overlay did not notice the close");
        assert!(!app.config.pin_mode, "closing must clear the pin as well");
        assert!(!crate::is_requested());

        // 7. And the widget's own state was not lost on the way through.
        let final_state = OverlayConfig::load_from(&shared_config).unwrap();
        assert!(!final_state.overlay_requested);
        assert!(!final_state.pin_mode);
        assert_eq!(final_state.position, Some((5.0, 6.0)));
    }

    #[test]
    fn the_settings_panel_is_sized_from_its_rows() {
        // A bigger font and a roomier density both make rows taller, and the
        // panel has to follow. This is the whole point of deriving the height
        // instead of pinning it: the panel used to keep a size chosen for the
        // rows it had, and cut off the rows added after it.
        for density in [Density::Ultra, Density::Compact, Density::Normal] {
            let (_scratch, small) = app_with(
                crate::source::SUPPORTED_GENERATION,
                OverlayConfig {
                    density,
                    ..OverlayConfig::default()
                },
            );
            let small_height = small.settings_size().height;

            let (_scratch, large) = app_with(
                crate::source::SUPPORTED_GENERATION,
                OverlayConfig {
                    density,
                    font_size: FontSize::Large,
                    ..OverlayConfig::default()
                },
            );
            assert!(
                large.settings_size().height > small_height,
                "{density:?} did not grow for a larger font"
            );
            assert_eq!(large.settings_size().width, SETTINGS_WIDTH);
        }
    }

    #[test]
    fn the_settings_panel_stops_at_the_monitor() {
        let (_scratch, app) = app_at(crate::source::SUPPORTED_GENERATION);
        let uncapped = app.settings_size().height;
        assert!(uncapped > MIN_SETTINGS_HEIGHT, "the fixture was already at the floor");

        let mut short_screen = app;
        short_screen.monitor = Some(iced::Size::new(1920.0, 500.0));
        assert!(short_screen.settings_size().height <= 500.0 - 2.0 * SCREEN_MARGIN);

        // Even a screen shorter than the floor gets the floor rather than a
        // negative height: the panel scrolls, and it stays on screen.
        let mut tiny = short_screen;
        tiny.monitor = Some(iced::Size::new(800.0, 100.0));
        assert_eq!(tiny.settings_size().height, MIN_SETTINGS_HEIGHT);
    }

    #[test]
    fn the_shadow_hint_is_counted_exactly_when_the_mode_cannot_render_one() {
        // The hint is the row that was added after the panel was given its size,
        // so it is also the one most likely to be miscounted.
        let (_scratch, app) = app_with(
            crate::source::SUPPORTED_GENERATION,
            OverlayConfig {
                transparency: Transparency::Off,
                ..OverlayConfig::default()
            },
        );
        assert!(app.shadow_cannot_render());
        assert_eq!(app.appearance_rows().hints, 2, "the opacity hint plus the shadow one");

        // On a platform whose default window carries no per-pixel alpha the hint
        // is part of the panel's normal appearance; where it does carry alpha,
        // there is nothing to explain.
        let (_scratch, app) = app_at(crate::source::SUPPORTED_GENERATION);
        assert_eq!(app.shadow_cannot_render(), cfg!(target_os = "windows"));
        assert_eq!(
            app.appearance_rows().hints,
            1 + u32::from(cfg!(target_os = "windows")),
            "the shadow hint does not match the mode"
        );
    }

    #[test]
    fn an_unticked_shadow_explains_itself_where_it_cannot_be_drawn() {
        // The control is inert in this mode whether or not it is ticked, so it
        // says so either way — and says nothing at all where it would work.
        for shadow in [false, true] {
            let config = OverlayConfig {
                transparency: Transparency::Off,
                shadow,
                ..OverlayConfig::default()
            };
            let (_scratch, app) = app_with(crate::source::SUPPORTED_GENERATION, config);
            assert!(
                app.shadow_cannot_render(),
                "shadow={shadow} changed nothing about the mode"
            );
            assert!(
                !app.config.draws_shadow(),
                "a shadow was drawn where none can be blended"
            );
        }

        let (_scratch, app) = app_at(crate::source::SUPPORTED_GENERATION);
        assert_eq!(
            app.shadow_cannot_render(),
            cfg!(target_os = "windows"),
            "the platform decides whether the default mode carries per-pixel alpha"
        );

        // The preference itself is never thrown away: switching to a mode that
        // can draw one brings it straight back.
        let config = OverlayConfig {
            transparency: Transparency::Layered,
            shadow: true,
            ..OverlayConfig::default()
        };
        let (_scratch, mut app) = app_with(crate::source::SUPPORTED_GENERATION, config);
        assert!(app.shadow_cannot_render());
        assert!(!app.config.draws_shadow());

        app.config.transparency = Transparency::Auto;
        assert_eq!(
            app.config.draws_shadow(),
            !cfg!(target_os = "windows"),
            "the stored preference did not come back in a mode that can honour it"
        );
    }

    #[test]
    fn an_unknown_generation_shows_numbers_without_labels_or_app_rows() {
        let (_scratch, app) = app_at(crate::source::SUPPORTED_GENERATION);
        assert!(!app.is_degraded());
        // The default set includes `Top apps`, which expands into its own rows.
        assert!(app.bar_items().len() > 1);
        assert!(app.bar_items().iter().all(|item| item.label.is_some()));

        let (_scratch, app) = app_at(crate::source::SUPPORTED_GENERATION + 1);
        assert!(app.is_degraded());

        let items = app.bar_items();
        assert!(
            items.iter().all(|item| item.label.is_none()),
            "a label here would be in a language this build cannot know"
        );

        // The status sentence is first, and it is *added* rather than shown
        // instead. That is the whole point of it at an unknown generation: the
        // figures underneath are still real — they are read by column name — so
        // replacing them with an apology would throw away the only thing the
        // widget could still do.
        assert_eq!(
            items[0].value,
            translations::status_foreign_generation(Language::English),
            "the degraded bar did not say why it is degraded"
        );

        let expected = app.config.metrics.iter().filter(|metric| !metric.is_multi()).count();
        assert!(
            expected > 1,
            "the fixture needs more than one plain metric for this to mean anything"
        );
        assert_eq!(
            items.len(),
            expected + 1,
            "the per-app rows are the one entry that needs a second table, and the status line is one more"
        );
    }

    #[test]
    fn a_pinned_language_still_applies_when_the_database_cannot_be_trusted() {
        // The degraded mode drops the *dashboard's* language because that is read
        // from a database at an unknown generation. A language the user pinned in
        // this widget's own file is not read from anywhere — it came from them.
        //
        // So the one sentence that explains the degraded mode can be translated,
        // and this is what makes that true: without a pinned language there is
        // nothing to translate it into.
        let (_scratch, app) = app_at(crate::source::SUPPORTED_GENERATION + 1);
        assert!(app.is_degraded(), "the fixture is not degraded");

        let pinned = OverlayConfig {
            language: Some(Language::Chinese),
            ..app.config.clone()
        };
        let mut app = app;
        app.config = pinned;
        app.apply_appearance();

        assert_eq!(
            app.status().message(app.language),
            Some(translations::status_foreign_generation(Language::Chinese)),
            "a language the user chose was dropped because the database could not be trusted"
        );
    }

    #[test]
    fn every_reason_for_a_blank_widget_is_distinguishable_from_the_others() {
        // The point of the whole status enum. Before it, four different situations
        // produced the same screen — a row of em dashes — and the user had no way
        // to tell a missing install from a stopped collector from a switch they
        // had flipped themselves.
        //
        // So this pins that the states are *different*, not merely that each one
        // is reachable. Two of them collapsing into the same sentence would put
        // the widget back where it started while still looking finished.
        let seen: Vec<Status> = vec![
            status_of(Availability::Ready, true, true, false, Some(0), 0),
            status_of(Availability::Missing, true, false, true, None, 0),
            status_of(Availability::Missing, false, false, false, None, 0),
            status_of(Availability::Ready, true, false, false, Some(600), 600),
            status_of(Availability::ForeignGeneration(99), true, false, false, Some(0), 0),
        ];

        let mut unique = seen.clone();
        unique.sort_by_key(|status| format!("{status:?}"));
        unique.dedup();
        assert_eq!(unique.len(), seen.len(), "two states report the same thing");

        // And each one says something, in a language, and does not say the same
        // thing as the others.
        let sentences: Vec<&str> = seen.iter().filter_map(|s| s.message(Language::English)).collect();
        assert_eq!(sentences.len(), 4, "Live is the only one with nothing to say");
        let mut unique_sentences = sentences.clone();
        unique_sentences.sort_unstable();
        unique_sentences.dedup();
        assert_eq!(unique_sentences.len(), sentences.len(), "two states share a sentence");
    }

    #[test]
    fn a_fresh_sample_is_the_only_thing_that_counts_as_live() {
        // One second old is live, one second too old is not. The boundary is
        // pinned from both sides because it is the difference between a widget
        // that is working and one that claims the collector died while the user
        // is looking at a busy machine.
        assert_eq!(
            status_of(
                Availability::Ready,
                true,
                false,
                false,
                Some(STALE_AFTER_SECONDS as i64),
                600
            ),
            Status::Live
        );
        assert_eq!(
            status_of(
                Availability::Ready,
                true,
                false,
                false,
                Some(STALE_AFTER_SECONDS as i64 + 1),
                600
            ),
            Status::Stalled(STALE_AFTER_SECONDS + 1)
        );
    }

    #[test]
    fn a_collector_we_started_is_slow_and_one_we_did_not_is_stopped() {
        // **The distinction the sentence depends on.** Same stale database, same
        // moment, different reason — and the user needs a different action for
        // each: wait, versus go and restart it.
        let stale = Some(3600);

        assert_eq!(
            status_of(Availability::Ready, true, true, false, stale, 3600),
            Status::Starting,
            "a collector this widget launched was called stopped before it had a chance"
        );
        assert_eq!(
            status_of(Availability::Ready, true, false, false, stale, 3600),
            Status::Stalled(3600),
            "a collector that stopped on its own was called merely starting"
        );
    }

    #[test]
    fn a_collector_that_never_started_is_not_allowed_to_look_like_it_is_starting_forever() {
        // The window exists for the case where the user double-clicked WattSeal
        // themselves and is watching it warm up. Past it, "starting" stops being
        // true and starts being a way of never having to say anything.
        assert_eq!(
            status_of(
                Availability::Starting,
                false,
                false,
                false,
                None,
                STARTING_WINDOW_SECONDS
            ),
            Status::Starting
        );
        assert_eq!(
            status_of(
                Availability::Starting,
                false,
                false,
                false,
                None,
                STARTING_WINDOW_SECONDS + 1
            ),
            Status::Stalled(STALE_AFTER_SECONDS)
        );
    }

    #[test]
    fn a_missing_wattseal_outranks_a_switch_that_is_off() {
        // Both are true, and only one of them is the thing to go and fix. If the
        // widget reported the switch, a user with no WattSeal installed would be
        // told to change a setting that would not help them.
        assert_eq!(
            status_of(Availability::Missing, false, false, true, None, 0),
            Status::NotInstalled
        );
    }

    #[test]
    fn an_unexpected_generation_is_never_blamed_on_the_collector() {
        // The generation is not something a collector can be blamed for and
        // restarting cannot fix, so it must never come out as `Stalled` — the one
        // status that tells the user to go and do something.
        assert_eq!(
            status_of(Availability::ForeignGeneration(42), true, true, false, Some(9999), 9999),
            Status::ForeignGeneration(42)
        );
    }

    #[test]
    fn a_blank_widget_shows_a_reason_and_never_only_a_dash() {
        // The end-to-end version: each broken state, through `bar_items`, produces
        // words a user can act on rather than the bare em dash that used to be
        // the entire diagnostic.
        let broken = [
            Status::NotInstalled,
            Status::Starting,
            Status::LaunchDisabled,
            Status::Stalled(120),
        ];

        for status in broken {
            assert!(
                status.suppresses_metrics(),
                "{status:?} still leaves metric rows that cannot be trusted"
            );
            let sentence = status.message(Language::English).expect("a broken state said nothing");
            assert!(
                sentence != "—" && !sentence.is_empty(),
                "{status:?} rendered as a placeholder"
            );
        }

        // And the two that keep numbers are the two that say nothing or say why
        // *alongside* the numbers.
        assert!(!Status::Live.suppresses_metrics());
        assert!(!Status::ForeignGeneration(3).suppresses_metrics());
        assert_eq!(Status::Live.message(Language::English), None);
    }

    #[test]
    fn the_reason_is_written_in_every_language_and_is_never_the_same_twice() {
        // Five languages × four reasons. A missing translation here would leave a
        // Chinese user with an empty widget and, again, no way to tell why — the
        // exact failure the status line was added to end.
        for language in AppLanguage::all() {
            let mut sentences: Vec<&str> = [
                Status::NotInstalled,
                Status::Starting,
                Status::LaunchDisabled,
                Status::Stalled(5),
                Status::ForeignGeneration(9),
            ]
            .iter()
            .map(|status| status.message(*language).unwrap())
            .collect();

            assert!(
                sentences.iter().all(|s| !s.trim().is_empty()),
                "{language:?} has an empty status line"
            );

            sentences.sort_unstable();
            let before = sentences.len();
            sentences.dedup();
            assert_eq!(
                sentences.len(),
                before,
                "{language:?} reuses one sentence for two states"
            );
        }
    }

    #[test]
    fn the_reason_survives_the_default_width() {
        // The status line goes through the same fitting machinery as a metric,
        // which caps the widget at 140 px by default. A sentence that elides to
        // "WattSeal no…" has told nobody anything, and the sentence is the whole
        // feature — so the part that survives has to name the problem.
        //
        // This is the failure none of the tests above would have caught: every
        // string was non-empty, every state was distinct, and every mutation
        // died. The sentence could still be useless once narrowed.
        let dir = std::env::temp_dir().join("wattseal-status-width");
        std::fs::create_dir_all(&dir).unwrap();
        let missing = dir.join("no-such-database.db");
        let _ = std::fs::remove_file(&missing);

        let mut app = OverlayApp {
            config: OverlayConfig::default(),
            config_path: None,
            requested: true,
            window_id: None,
            window_raw: None,
            power: HashMap::new(),
            top_apps: Vec::new(),
            show_settings: false,
            show_menu: false,
            applied: iced::Size::ZERO,
            monitor: None,
            language: Language::English,
            theme: ThemeChoice::Dark,
            source: Source::at(&missing),
            launch_attempted: true,
            launched: None,
            wattseal_missing: true,
            escape_hotkey_ready: false,
            capturing_hotkey: false,
            hotkey_refusal: None,
            click_through_applied: (false, false),
            freshness: None,
            started_at: std::time::Instant::now(),
        };

        assert_eq!(app.status(), Status::NotInstalled);

        for language in [
            Language::English,
            Language::German,
            Language::Chinese,
            Language::French,
            Language::Romanian,
        ] {
            app.language = language;
            let fitted = app.fitted_items();
            assert_eq!(fitted.len(), 1, "{language:?} produced more than one status row");

            let shown = &fitted[0].value;
            // "WattSeal" is the one word that is never translated, so it is the
            // one word that has to survive narrowing — in every language.
            assert!(
                shown.contains("WattSeal"),
                "{language:?} narrowed to {shown:?}, which no longer names the program"
            );
            assert!(
                shown.chars().count() >= 10,
                "{language:?} narrowed to {shown:?}, too short to say anything"
            );
            // The action survives too, in every language. Clipping a sentence to
            // 140 px left "WattSeal not found — p…", which names the problem and
            // then says nothing about the fix.
            let full = Status::NotInstalled.message(language).unwrap();
            assert_eq!(
                shown.chars().count(),
                full.chars().count(),
                "{language:?} was cut at the default width: {shown:?}"
            );
        }

        // The exemption is exactly as narrow as it claims: one sentence *beside*
        // live readings still respects the `Width` setting, because there are
        // numbers on the card that the cap exists to protect.
        let mut mixed = app_at(crate::source::SUPPORTED_GENERATION).1;
        mixed.wattseal_missing = true;
        mixed.freshness = Some(0);
        assert_eq!(
            mixed.status(),
            Status::Live,
            "the mixed case was not live, so the exemption below proves nothing"
        );

        let (_scratch, app) = app_with(
            crate::source::SUPPORTED_GENERATION,
            OverlayConfig {
                width: 60.0,
                ..OverlayConfig::default()
            },
        );
        let capped = app.target_width();
        assert!(
            capped <= 60.0_f32.max(app.width_floor()) + 0.5,
            "a bar of readings ignored the Width setting: {capped}"
        );

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_status_beside_readings_is_cut_and_the_readings_are_not() {
        // The other half of the width rule, and the one the exemption does not
        // cover: here there *are* numbers on the card, so the `Width` setting
        // applies and the sentence has to give way.
        //
        // This is the only case where the status elision runs, which is exactly
        // why it needed its own test: with the exemption in place, removing the
        // elision outright left every test green.
        let (_scratch, mut app) = app_with(
            crate::source::SUPPORTED_GENERATION + 1,
            OverlayConfig {
                width: 80.0,
                ..OverlayConfig::default()
            },
        );

        assert_eq!(app.status(), Status::ForeignGeneration(4));
        app.freshness = Some(0);

        let items = app.fitted_items();
        assert!(items[0].status, "the sentence is not the first row");
        assert!(
            items.iter().skip(1).all(|item| !item.status),
            "a second sentence appeared"
        );

        // The numbers keep their values: a reading is never shortened, because a
        // cut number reads as a smaller one.
        let readings: Vec<&str> = items.iter().skip(1).map(|item| item.value.as_str()).collect();
        assert!(
            readings.iter().all(|value| !value.is_empty() && !value.contains('…')),
            "a reading was shortened to {readings:?}"
        );

        // And the sentence is never longer than the whole thing, or it was going
        // to be clipped by the window edge — which is the same as not saying it.
        let full = Status::ForeignGeneration(4).message(Language::English).unwrap();
        assert!(
            items[0].value.chars().count() <= full.chars().count(),
            "the sentence grew: {:?}",
            items[0].value
        );
        assert!(
            app.target_width() <= app.width_cap() + 0.5,
            "a bar with readings ignored the Width setting"
        );
    }

    #[test]
    fn a_widget_that_cannot_be_clicked_says_how_to_release_itself() {
        // The trap this whole line exists for. Pin locks the position, which is a
        // choice; pin *and click-through* also takes the mouse, and with it every
        // in-app way out. The escape used to be the dashboard's hide-and-show,
        // which released the pin from another process — **there is no other
        // process now**, and restarting does nothing because the pin is written to
        // the config file.
        //
        // So the only surface left is one the user can still *read*, and that is
        // what this pins: the widget says so, in words, while it is stuck.
        let (_scratch, mut app) = app_with(
            crate::source::SUPPORTED_GENERATION,
            OverlayConfig {
                pin_mode: true,
                pin_click_through: true,
                ..OverlayConfig::default()
            },
        );
        app.freshness = Some(0);

        if !crate::winlayer::click_through_supported() {
            // Then the widget is not actually stuck, and the line would be a lie.
            assert!(
                !app.is_stuck_pinned(),
                "click-through is not supported but the widget thinks it is"
            );
            return;
        }

        assert!(app.is_stuck_pinned());
        assert!(app.is_stuck_pinned());

        // **But not while there are readings to show.** The bar is fitted as one
        // unit, so a hint row makes the horizontal layout drop labels — or whole
        // readings — to find room for something that is not a reading. Pinning is
        // this widget's normal state, so that cost would be paid constantly.
        //
        // The hint is therefore worth having exactly where it displaces nothing.
        assert!(
            !app.bar_items().iter().any(|item| item.status),
            "the escape hint is competing with the readings for room"
        );

        // With nothing to show it appears, and says the *key* once the shortcut is
        // registered — which needs no editor and no restart, and is the whole
        // reason the shortcut exists.
        app.config.metrics = Vec::new();
        app.escape_hotkey_ready = true;
        let with_key = app.bar_items();
        assert_eq!(with_key.len(), 1, "a widget with no metrics showed more than the hint");
        assert_eq!(
            with_key[0].value,
            crate::winlayer::escape_hint(Language::English, crate::winlayer::DEFAULT_HOTKEY),
            "the stuck widget did not say how to release itself"
        );
        assert!(
            with_key[0].value.contains("Ctrl"),
            "the shortcut line does not name the key: {:?}",
            with_key[0].value
        );

        // Without a registered shortcut it falls back to the config file, which is
        // the only route left when the machine will not give us a key.
        app.escape_hotkey_ready = false;
        assert_eq!(
            app.bar_items()[0].value,
            translations::hint_pinned_escape(Language::English),
            "with no shortcut it should say where the setting lives"
        );

        // Releasing it takes the line away again — otherwise a user who has
        // already fixed it still has a widget telling them something is wrong.
        app.config.pin_mode = false;
        assert!(
            !app.bar_items().iter().any(|item| item.value.contains("pin_mode")),
            "the hint outlived the pin"
        );

        // And a widget that is merely pinned — no click-through — is *not* stuck,
        // so it gets no hint.
        app.config.pin_mode = true;
        app.config.pin_click_through = false;
        assert!(!app.is_stuck_pinned());
    }

    #[test]
    fn the_capture_listens_for_exactly_the_keys_a_shortcut_can_end_with() {
        // The translation from iced's key vocabulary to Windows virtual-key codes.
        // A wrong code here is a shortcut that silently never fires — the exact
        // failure this whole feature exists to remove — so the codes are pinned
        // against the table rather than trusted.
        use iced::keyboard::{Key, key::Named};

        /// Reads the key and modifiers out of whatever [`hotkey_from_key`] made.
        ///
        /// Destructured rather than compared because `Message` is not `PartialEq` —
        /// it carries floats — and pinning the one variant that matters is more
        /// precise than making the whole enum comparable for a test.
        fn captured(message: Option<Message>) -> (bool, bool, bool, u32) {
            match message {
                Some(Message::CapturedKey { ctrl, alt, shift, vk }) => (ctrl, alt, shift, vk),
                other => panic!("expected a captured key, got {other:?}"),
            }
        }

        let letter = captured(hotkey_from_key(&Key::Character("o".into()), true, true, false));
        assert_eq!(letter, (true, true, false, 0x4F));

        // Upper and lower case are the same key; the label upper-cases it later.
        assert_eq!(
            captured(hotkey_from_key(&Key::Character("O".into()), true, false, false)),
            captured(hotkey_from_key(&Key::Character("o".into()), true, false, false))
        );

        // Digits map to their ASCII values, which is what Windows uses.
        assert_eq!(
            captured(hotkey_from_key(&Key::Character("5".into()), true, false, false)),
            (true, false, false, 0x35)
        );

        // F1 is the start of the function range and F12 the end. An off-by-one
        // here would bind F12 to F11.
        assert_eq!(
            captured(hotkey_from_key(&Key::Named(Named::F1), true, false, false)),
            (true, false, false, 0x70)
        );
        assert_eq!(
            captured(hotkey_from_key(&Key::Named(Named::F12), true, false, false)),
            (true, false, false, 0x7B)
        );

        // Everything else yields nothing at all, so the capture stays open and
        // waiting rather than binding a key the platform cannot register.
        for key in [
            Key::Named(Named::Space),
            Key::Named(Named::Enter),
            Key::Named(Named::ArrowUp),
            Key::Named(Named::F13),
            Key::Character("!".into()),
            Key::Character("ab".into()),
            Key::Character("".into()),
        ] {
            assert!(
                hotkey_from_key(&key, true, true, false).is_none(),
                "{key:?} was accepted as a shortcut key"
            );
        }
    }

    #[test]
    fn a_keypress_only_binds_anything_while_the_panel_is_asking() {
        // The subscription cannot filter on this — `listen_with` takes a function
        // pointer and may not capture `self` — so every keypress reaches `update`
        // and the guard lives there. A widget that acted on keys while the user
        // was not rebinding would be reacting to typing it was never meant to see.
        let (_scratch, mut app) = app_at(crate::source::SUPPORTED_GENERATION);
        let before = app.config.escape_hotkey.clone();

        app.capturing_hotkey = false;
        let _ = app.update(Message::CapturedKey {
            ctrl: true,
            alt: true,
            shift: false,
            vk: 0x51,
        });
        assert_eq!(
            app.config.escape_hotkey, before,
            "a keypress bound a shortcut while the panel was not asking"
        );
        assert!(!app.capturing_hotkey);

        // And while it is asking, the same message does bind.
        app.capturing_hotkey = true;
        let _ = app.update(Message::CapturedKey {
            ctrl: true,
            alt: true,
            shift: false,
            vk: 0x51,
        });
        assert_eq!(app.config.escape_hotkey.as_deref(), Some("ctrl+alt+q"));
        assert!(!app.capturing_hotkey, "the capture stayed open after binding");
    }

    #[test]
    fn escape_leaves_the_capture_without_binding_anything() {
        // A capture mode with no stated exit is the same trap as a pinned widget
        // with no way out — and this whole feature exists to remove that trap, so
        // it must not reintroduce it one level up.
        let (_scratch, mut app) = app_at(crate::source::SUPPORTED_GENERATION);
        let before = app.config.escape_hotkey.clone();

        app.capturing_hotkey = true;
        let _ = app.update(Message::CapturedKey {
            ctrl: false,
            alt: false,
            shift: false,
            vk: 0x1B, // Escape
        });
        assert!(!app.capturing_hotkey, "Escape did not leave the capture");
        assert_eq!(app.config.escape_hotkey, before, "Escape bound a shortcut");
    }

    #[test]
    fn a_refused_combination_keeps_listening_and_says_why() {
        // The user is mid-gesture. Closing the capture on their first mistake
        // makes them start again just to find out what was wrong, so it stays open
        // and shows the reason.
        let (_scratch, mut app) = app_at(crate::source::SUPPORTED_GENERATION);
        app.capturing_hotkey = true;

        let _ = app.update(Message::CapturedKey {
            ctrl: false,
            alt: false,
            shift: false,
            vk: 0x51,
        });

        assert!(app.capturing_hotkey, "a refused key closed the capture");
        assert_eq!(app.hotkey_refusal.as_deref(), Some("hold Ctrl or Alt as well"));
        assert!(app.config.escape_hotkey.is_none(), "a refused key was bound anyway");

        // The next press clears it, so a stale reason cannot sit under a
        // combination that has since been accepted.
        let _ = app.update(Message::CapturedKey {
            ctrl: true,
            alt: true,
            shift: false,
            vk: 0x51,
        });
        assert_eq!(app.hotkey_refusal, None);
        assert_eq!(app.config.escape_hotkey.as_deref(), Some("ctrl+alt+q"));
    }

    #[test]
    fn the_shortcut_that_is_bound_is_the_one_in_the_file() {
        // **The whole feature, in one assertion.** Everything else tests the
        // parser, the label, the capture and the fallback — and none of it notices
        // whether the configured key is the one that actually reaches the
        // platform. Replacing the resolution with a hard-coded default left every
        // test in this file green.
        let (_scratch, mut app) = app_at(crate::source::SUPPORTED_GENERATION);

        app.config.escape_hotkey = Some("ctrl+shift+f7".to_string());
        let bound = app.escape_hotkey();
        assert_eq!(bound, crate::winlayer::Hotkey::parse("ctrl+shift+f7").unwrap());
        assert_ne!(
            bound,
            crate::winlayer::DEFAULT_HOTKEY,
            "the fixture's key is the default, so this would prove nothing"
        );

        // And it is the key the panel and the stuck card name, not the default.
        assert_eq!(bound.label(), "Ctrl+Shift+F7");
        assert!(
            crate::winlayer::escape_hint(Language::English, bound).contains("Ctrl+Shift+F7"),
            "the card would name a shortcut the user did not bind"
        );

        // A second value, to prove the first was read rather than coincidentally
        // matching something.
        app.config.escape_hotkey = Some("alt+4".to_string());
        assert_eq!(app.escape_hotkey().label(), "Alt+4");
    }

    #[test]
    fn an_unreadable_shortcut_in_the_file_still_leaves_a_way_out() {
        // A typo must cost the user their custom key, not every way out of a pin.
        let typo = OverlayConfig {
            escape_hotkey: Some("not+a+shortcut".to_string()),
            ..OverlayConfig::default()
        };
        let (_scratch, mut app) = app_at(crate::source::SUPPORTED_GENERATION);
        app.config = typo;

        assert_eq!(
            app.escape_hotkey(),
            crate::winlayer::DEFAULT_HOTKEY,
            "a typo in the config removed the escape hatch entirely"
        );

        // And an explicit `null` is the same thing as absent.
        app.config.escape_hotkey = None;
        assert_eq!(app.escape_hotkey(), crate::winlayer::DEFAULT_HOTKEY);
    }

    #[test]
    fn opening_the_widgets_own_menu_takes_it_out_of_the_mouses_way() {
        // **The bug that would have made the reach gesture infuriating.** The
        // natural way to use it is hold `Ctrl+Alt` → right-click → *let go* →
        // click the menu item. Nothing recomputed the click-through style when the
        // menu opened, so letting go restored it onto a menu that had just
        // appeared, and the menu could not be clicked at all.
        //
        // Driven through the real messages rather than by setting the flags, so
        // this catches a *missing* recompute as well as a wrong one.
        let (_scratch, mut app) = app_with(
            crate::source::SUPPORTED_GENERATION,
            OverlayConfig {
                pin_mode: true,
                pin_click_through: true,
                ..OverlayConfig::default()
            },
        );

        if !crate::winlayer::click_through_supported() {
            // Nowhere has click-through to undo, so there is nothing to prove.
            return;
        }

        // **Asserted on what was *applied*, not on what would be decided.** The two
        // are separate steps, and the first version of this test asked the decision
        // function directly — so deleting the call from the menu handler left it
        // green. `apply_click_through` has to actually run.
        //
        // Established the way the widget establishes it: the first application
        // happens when the OS reports the window, which a fixture has no window to
        // wait for.
        app.apply_click_through();
        assert_eq!(
            app.click_through_applied,
            (true, false),
            "a pinned click-through widget should start ignoring the mouse"
        );

        let _ = app.update(Message::OpenMenu);
        assert_eq!(
            app.click_through_applied,
            (true, true),
            "the menu opened without the widget taking itself out of the mouse's way"
        );

        let _ = app.update(Message::CloseMenu);
        assert_eq!(
            app.click_through_applied,
            (true, false),
            "closing the menu did not give the mouse back"
        );

        // The settings panel is the other surface that needs the mouse, and it is
        // reached the same way round — through the menu — so it gets the same
        // treatment rather than being assumed to follow.
        let _ = app.update(Message::ToggleSettings);
        assert_eq!(
            app.click_through_applied,
            (true, true),
            "the settings panel opened without the widget taking itself out of the mouse's way"
        );
        let _ = app.update(Message::ToggleSettings);
        assert_eq!(app.click_through_applied, (true, false));

        // Pinning *off* from the menu leaves nothing to ignore, menu or no menu —
        // otherwise releasing the pin would leave the widget captured.
        let _ = app.update(Message::OpenMenu);
        let _ = app.update(Message::TogglePin);
        assert!(!app.config.pin_mode);
        assert_eq!(
            app.click_through_applied,
            (false, false),
            "unpinning left the mouse captured"
        );
    }

    #[test]
    fn pinning_does_not_cost_the_widget_its_labels() {
        // **A regression this widget shipped with**, reported as "after pinning,
        // all the displayed text is gone, only the jumping numbers remain".
        //
        // Pinning appends the escape hint to the bar, and the bar is fitted as one
        // *unit*: in the horizontal layout a row that does not fit makes every
        // label give way to make room, so the readings lose their names. The hint
        // is a convenience; the labels are the widget.
        //
        // Checked in both layouts and at three widths, because the failure depends
        // on how much room there is — the default width hides it, which is why the
        // first version of this test passed on broken code.
        for layout in [Layout::Vertical, Layout::Horizontal] {
            for width in [140.0, 220.0, 320.0] {
                let config = |pin_mode| OverlayConfig {
                    layout,
                    width,
                    pin_mode,
                    pin_click_through: true,
                    ..OverlayConfig::default()
                };

                let (_scratch, plain) = app_with(crate::source::SUPPORTED_GENERATION, config(false));
                let before = labelled(&plain.fitted_items());

                let (_scratch, pinned) = app_with(crate::source::SUPPORTED_GENERATION, config(true));
                let after = labelled(&pinned.fitted_items());

                // A bar that already has no names is the designed label-less
                // fallback — the horizontal layout at a narrow width gives up
                // labels on purpose. There is nothing here to regress.
                if before == 0 {
                    continue;
                }

                assert_eq!(
                    before, after,
                    "{layout:?} at {width}px: pinning changed how many readings have names — \
                     the escape hint is crowding them out"
                );
            }
        }
    }

    /// How many rows in a fitted bar are readings with names.
    ///
    /// Counts only *labelled* rows, so the escape hint — which is a status row and
    /// has no label by design — is never mistaken for one of the readings it might
    /// be displacing.
    fn labelled(items: &[BarItem]) -> usize {
        items.iter().filter(|item| item.label.is_some()).count()
    }

    #[test]
    fn the_menu_only_loses_its_settings_entry_while_degraded() {
        // The whole set, in order: the README advertises the menu as
        // `Resume` / `Settings` / `Pin` / `Exit`, and a fifth entry or a
        // reordering would be a doc that no longer matches the widget.
        let (_scratch, normal) = app_at(crate::source::SUPPORTED_GENERATION);
        let labels: Vec<&str> = normal.menu_segments().iter().map(|(label, _)| *label).collect();
        assert_eq!(
            labels,
            vec![
                translations::menu_resume(Language::English),
                translations::menu_settings(Language::English),
                translations::menu_pin(Language::English),
                translations::menu_exit(Language::English),
            ]
        );

        let (_scratch, degraded) = app_at(crate::source::SUPPORTED_GENERATION + 1);
        let labels: Vec<&str> = degraded.menu_segments().iter().map(|(label, _)| *label).collect();
        // Resuming, pinning and exiting all still work: none of them reads the
        // database, which is the only thing the degraded mode gave up.
        assert_eq!(
            labels,
            vec![
                translations::menu_resume(Language::English),
                translations::menu_pin(Language::English),
                translations::menu_exit(Language::English),
            ]
        );
    }

    #[test]
    fn the_wide_characters_in_our_own_labels_measure_wide() {
        // The label `缩写标签（总计→总、CPU→核…）` really exists in the Chinese
        // translations, and its full-width brackets — U+FF08 and U+FF09 — sit in
        // the last range of `char_advance`. No other test reaches that range:
        // everything else covered uses plain ideographs. Drop `0xFF00..=0xFF60`
        // and every Chinese label with a bracket in it is under-measured by a
        // whole character per bracket, which is precisely the slack the
        // horizontal layout does not have.
        for character in ['（', '）', '、', '。', '，', '：', '总', '设', '网'] {
            assert!(
                char_advance(character, 1.0) > 1.0,
                "U+{:04X} is in our labels but measured as a Latin character",
                character as u32
            );
        }

        // And the narrow ones stay narrow — an estimate that inflates is its own
        // kind of wrong, it just wastes space instead of clipping.
        for character in ['a', 'W', '0', '.', ' ', 'ă', 'ț'] {
            assert_eq!(
                char_advance(character, 1.0),
                1.0,
                "U+{:04X} is measured as a wide character",
                character as u32
            );
        }
    }

    #[test]
    fn full_width_glyphs_measure_wider_than_latin_ones() {
        // Measuring CJK at the Latin advance is what clipped the Chinese
        // labels, so the ratio itself is what is worth pinning down.
        let ratio = char_advance('总', 1.0) / char_advance('A', 1.0);
        assert!(ratio >= 1.5, "a full-width glyph measured at only {ratio}x a Latin one");
    }

    #[test]
    fn metrics_move_one_place_and_stop_at_the_ends() {
        let mut order = vec![Metric::Total, Metric::Cpu, Metric::Gpu];

        assert!(move_metric(&mut order, Metric::Cpu, -1));
        assert_eq!(order, vec![Metric::Cpu, Metric::Total, Metric::Gpu]);

        // Neither end of the list gives way, and a metric that is switched off has
        // no place to move from.
        assert!(!move_metric(&mut order, Metric::Cpu, -1));
        assert!(!move_metric(&mut order, Metric::Gpu, 1));
        assert!(!move_metric(&mut order, Metric::Ram, 1));
        assert_eq!(order, vec![Metric::Cpu, Metric::Total, Metric::Gpu]);
    }

    #[test]
    fn a_row_fits_the_font_it_is_drawn_with() {
        // Ultra is the tightest density, so it is the one that clipped: the value
        // font's line box is taller than the row the density used to reserve.
        for density in [Density::Ultra, Density::Compact, Density::Normal] {
            for font in [FontSize::Small, FontSize::Medium, FontSize::Large] {
                let row = density.row_height(font);
                assert!(
                    row >= font.value() * 1.2,
                    "{density:?} at {font:?} reserves {row}px for a {}px font",
                    font.value()
                );
            }
        }

        // The density still shows through where the font leaves room for it.
        assert!(Density::Normal.row_height(FontSize::Small) > Density::Ultra.row_height(FontSize::Small));
    }

    #[test]
    fn a_label_mixing_scripts_adds_up() {
        let latin = text_width("AB", 10.0, 0.5);
        let chinese = text_width("总", 10.0, 0.5);

        assert_eq!(text_width("AB总", 10.0, 0.5), latin + chinese);
        assert_eq!(latin, 10.0);
    }

    #[test]
    fn long_process_names_are_truncated_to_the_limit() {
        assert_eq!(truncate("firefox", TOP_NAME_MAX), "firefox");
        assert_eq!(truncate("a-very-long-process-name", 8), "a-very-…");
        assert_eq!(truncate("a-very-long-process-name", 8).chars().count(), 8);
    }

    #[test]
    fn the_widget_is_anchored_inside_the_monitor() {
        assert_eq!(
            anchor_point(iced::Size::new(1920.0, 1080.0), 200.0, 100.0),
            iced::Point::new(1704.0, 16.0)
        );

        // A widget wider than the monitor still lands inside it.
        let point = anchor_point(iced::Size::new(100.0, 100.0), 200.0, 50.0);
        assert_eq!(point.x, 16.0);
    }

    #[test]
    fn a_window_that_grew_is_nudged_back_inside_the_monitor() {
        let monitor = iced::Size::new(1920.0, 1080.0);
        let docked = iced::Point::new(1704.0, 16.0);

        // The metrics bar keeps its place while it fits where it is.
        assert_eq!(clamp_point(monitor, iced::Size::new(200.0, 40.0), docked), docked);

        // Opening the settings panel there would stick out, so it comes back.
        let nudged = clamp_point(monitor, iced::Size::new(560.0, 452.0), docked);
        assert_eq!(nudged.x, 1920.0 - 560.0 - 16.0);
        assert_eq!(nudged.y, 16.0);
    }

    #[test]
    fn a_measured_width_rounds_up_to_a_rung_of_the_ladder() {
        assert_eq!(width_up(1.0), MIN_WIDTH);
        assert_eq!(width_up(100.0), 108.0);
        // Already on a rung: no extra step is added on top.
        assert_eq!(width_up(108.0), 108.0);
    }

    #[test]
    fn a_chosen_width_snaps_to_the_nearest_rung() {
        assert_eq!(width_near(100.0), 96.0);
        assert_eq!(width_near(110.0), 108.0);
        assert_eq!(width_near(140.0), 144.0);
    }

    #[test]
    fn every_content_change_reaches_the_file_the_other_processes_read() {
        // The overlay, the tray and the dashboard are three processes with no
        // IPC between them. This file is the only channel, so a change that is
        // not persisted is a change that is silently lost on the next launch.
        let (_scratch, mut app) = app_with_config_file(OverlayConfig::default());

        let _ = app.update(Message::ToggleMetric(Metric::Network, true));
        assert_eq!(shared_file(&app).metrics.last(), Some(&Metric::Network));

        // One arrow swaps with the neighbour, it does not jump to the front —
        // which is why the arrow is a pair and not a "first" button.
        let _ = app.update(Message::MoveMetricUp(Metric::Network));
        let metrics = shared_file(&app).metrics;
        let position = metrics.iter().position(|m| *m == Metric::Network).unwrap();
        assert_eq!(position, metrics.len() - 2);
        assert_eq!(metrics[position + 1], Metric::TopApps);

        let _ = app.update(Message::ToggleMetric(Metric::Network, false));
        assert!(!shared_file(&app).metrics.contains(&Metric::Network));

        let _ = app.update(Message::SetTopApps(99));
        assert_eq!(
            shared_file(&app).top_apps,
            8,
            "the count must be clamped before it is stored"
        );

        let _ = app.update(Message::SetDecimals(9));
        assert_eq!(shared_file(&app).decimals, 3);

        let _ = app.update(Message::SetRefresh(0));
        assert_eq!(shared_file(&app).refresh_secs, 1, "a zero interval would busy-loop");

        let _ = app.update(Message::SetWidth(137.0));
        assert_eq!(shared_file(&app).width, width_near(137.0));

        let _ = app.update(Message::ChangeOpacity(9.0));
        assert_eq!(shared_file(&app).opacity, 1.0);

        let _ = app.update(Message::ChangeOpacity(-9.0));
        assert_eq!(shared_file(&app).opacity, 0.05);
    }

    #[test]
    fn pinning_from_the_menu_persists_so_the_tray_can_see_it() {
        // The tray's `Pin / Unpin Overlay` and the widget's own menu are the same
        // flag seen from two processes. One of them writing it in memory only
        // would leave the other's menu lying.
        let (_scratch, mut app) = app_with_config_file(OverlayConfig::default());

        let _ = app.update(Message::TogglePin);
        assert!(shared_file(&app).pin_mode);
        assert!(!app.show_menu, "the metrics must come back after pinning");

        let _ = app.update(Message::TogglePin);
        assert!(!shared_file(&app).pin_mode);
    }

    #[test]
    fn quitting_leaves_the_file_saying_the_overlay_is_not_wanted() {
        // The dashboard's footer toggle reads `overlay_requested`; leaving it
        // `true` would strand it showing `Hide overlay` for a widget that is
        // gone. The pin is cleared too, because that is what makes hide-and-show
        // a way out of a pinned, click-through widget on a system with no tray.
        let (_scratch, mut app) = app_with_config_file(OverlayConfig {
            pin_mode: true,
            ..OverlayConfig::default()
        });

        let _ = app.update(Message::Quit);

        let written = shared_file(&app);
        assert!(!written.overlay_requested, "the dashboard would still offer to hide it");
        assert!(
            !written.pin_mode,
            "a pinned widget with click-through has no way out but this"
        );
    }

    #[test]
    fn another_process_closing_the_file_overlay_asks_for_is_picked_up() {
        // The dashboard and the tray only ever edit the file. This is the whole
        // "hide overlay" path, so it is worth driving end to end.
        let (_scratch, mut app) = app_with_config_file(OverlayConfig::default());
        let path = app.shared_config_path();

        // Nothing in the file yet: the overlay is here on its own account.
        assert!(!app.sync_external_state());

        // The dashboard writes `false`, having pinned it first.
        let written = OverlayConfig {
            overlay_requested: false,
            pin_mode: true,
            ..OverlayConfig::default()
        };
        assert!(written.save_to(&path));

        assert!(app.sync_external_state(), "a close request was missed");
        assert!(app.config.pin_mode, "the pin the tray set was not mirrored");

        // A second tick on the same file must not close again.
        assert!(!app.sync_external_state());
    }
}
