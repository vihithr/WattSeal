//! Messages handled by the overlay application.

use crate::config::{Density, FontSize, Layout, Metric};

/// All events handled by [`crate::app::OverlayApp`].
#[derive(Debug, Clone)]
pub enum Message {
    Tick,
    /// The overlay window id (from `window::latest`).
    WindowId(Option<iced::window::Id>),
    /// The raw OS window handle (used by the layered transparency mode).
    RawWindowId(u64),
    /// The size of the monitor the window ended up on, if the OS reported one.
    MonitorSize(Option<iced::Size>),
    StartDrag,
    /// Window moved (persist position).
    Moved(f32, f32),
    ToggleSettings,
    OpenMenu,
    CloseMenu,
    TogglePin,
    TogglePinClickThrough(bool),

    // appearance
    ChangeOpacity(f32),
    SetBgColor(crate::config::BgColor),
    SetTextColor(crate::config::TextColor),
    SetTransparency(crate::config::Transparency),
    ToggleShadow(bool),
    SetLayout(Layout),
    /// Pin the widget's language here, overriding the dashboard's.
    SetLanguage(crate::language::AppLanguage),
    /// Pin the widget's scheme here, overriding the dashboard's.
    SetTheme(crate::theme::ThemeChoice),
    /// Go back to following whatever the dashboard has saved.
    FollowDashboardLanguage,
    FollowDashboardTheme,
    /// Start listening for the combination the user wants as the escape shortcut.
    BeginHotkeyCapture,
    /// Give up on capturing, keeping the shortcut that is already bound.
    CancelHotkeyCapture,
    /// A key arrived while capturing. Carries the modifiers held with it.
    CapturedKey {
        ctrl: bool,
        alt: bool,
        shift: bool,
        vk: u32,
    },
    SetDensity(Density),
    SetFontSize(FontSize),
    ToggleLabels(bool),
    ToggleUnits(bool),
    ToggleAbbreviated(bool),
    SetDecimals(u8),
    SetRefresh(u32),

    // window
    ToggleAlwaysOnTop(bool),
    /// Width (logical px) the widget may grow to.
    SetWidth(f32),

    // content
    ToggleMetric(Metric, bool),
    /// Moves a metric one place earlier or later in the bar.
    MoveMetricUp(Metric),
    MoveMetricDown(Metric),
    SetTopApps(usize),

    /// Quit the overlay.
    Quit,
    /// The OS asked to close the window.
    CloseRequested,
}
