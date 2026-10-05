//! Win32 layered-window helper, and the keyboard shortcut that releases a
//! pinned widget.
//!
//! Gives the overlay a uniformly translucent window **without** relying on the
//! GPU surface exposing an alpha mode — DX12 reports `AlphaMode::Ignore` and
//! therefore drops per-pixel alpha, whereas `SetLayeredWindowAttributes`
//! composites at the DWM level and works with any rendering backend.

/// The window alpha for a given slider value.
///
/// Split out from the Win32 call so the mapping is testable: it is the only
/// place a rounding mistake can make the widget either fully opaque or, at the
/// bottom of the slider, almost invisible. The window takes a byte, the setting
/// is a ratio in `0.05..=1.0`, and the two ends have to land exactly.
#[cfg(target_os = "windows")]
fn alpha_byte(alpha: f32) -> u8 {
    (alpha.clamp(0.05, 1.0) * 255.0).round() as u8
}

/// Applies `WS_EX_LAYERED` + uniform alpha to the window identified by `hwnd`
/// (the raw id reported by `iced::window::raw_id`).
///
/// Returns `true` when the window system accepted the attributes.
#[cfg(target_os = "windows")]
pub fn apply(hwnd: u64, alpha: f32) -> bool {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GWL_EXSTYLE, GetWindowLongPtrW, LWA_ALPHA, SetLayeredWindowAttributes, SetWindowLongPtrW, WS_EX_LAYERED,
    };

    if hwnd == 0 {
        return false;
    }

    let hwnd = hwnd as *mut core::ffi::c_void;
    let alpha_byte = alpha_byte(alpha);

    // SAFETY: `hwnd` comes from iced/winit for our own window; the calls are the
    // documented Win32 pattern for enabling uniform-alpha layered windows.
    unsafe {
        let ex_style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
        if ex_style & (WS_EX_LAYERED as isize) == 0 {
            SetWindowLongPtrW(hwnd, GWL_EXSTYLE, ex_style | (WS_EX_LAYERED as isize));
        }
        SetLayeredWindowAttributes(hwnd, 0, alpha_byte, LWA_ALPHA) != 0
    }
}

/// No-op on non-Windows platforms.
#[cfg(not(target_os = "windows"))]
pub fn apply(_hwnd: u64, _alpha: f32) -> bool {
    false
}

/// Whether mouse pass-through can actually be applied here.
///
/// Only Windows is implemented; macOS (`setIgnoresMouseEvents`) and X11 (input
/// shape) would each need their own branch, and Wayland has no protocol for it
/// at all. Where it is unavailable, pin mode still locks the position and the
/// right-click menu remains reachable.
#[cfg(target_os = "windows")]
pub const fn click_through_supported() -> bool {
    true
}

/// See the Windows variant.
#[cfg(not(target_os = "windows"))]
pub const fn click_through_supported() -> bool {
    false
}

/// Whether the reach keys are held down right now, as `(ctrl, alt)`.
///
/// Returns the two separately rather than their combination so the rule joining
/// them ([`reach_keys_held`]) is a pure function a test can drive. This is the
/// only part of the reach mechanism that cannot be tested on the machine this was
/// written on: synthetic input never populates the async key state there, from
/// either `keybd_event` or `SendInput`, even though the same synthetic keys *do*
/// reach `RegisterHotKey` — so the real function cannot be made to return `true`
/// here at all.
///
/// Deliberately **not** a registered shortcut: a `RegisterHotKey` combination can
/// be taken by another program — on this machine, sixteen of twenty-one plausible
/// ones already were — and an escape hatch a third party can own is not one.
#[cfg(target_os = "windows")]
pub fn reach_keys_down() -> (bool, bool) {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, VK_CONTROL, VK_MENU};

    /// The high bit of `GetAsyncKeyState`'s result means "down right now".
    const DOWN: i16 = i16::MIN;

    // SAFETY: `GetAsyncKeyState` reads global key state and has no preconditions.
    let ctrl = unsafe { GetAsyncKeyState(VK_CONTROL as i32) } & DOWN != 0;
    let alt = unsafe { GetAsyncKeyState(VK_MENU as i32) } & DOWN != 0;
    (ctrl, alt)
}

/// See the Windows variant.
#[cfg(not(target_os = "windows"))]
pub fn reach_keys_down() -> (bool, bool) {
    (false, false)
}

/// Whether `Ctrl` and `Alt` are both down.
///
/// **Both, not either.** One of them alone is held constantly by ordinary work —
/// `Ctrl` for shortcuts, `Alt` for menus — so a single key would have the widget
/// grabbing the mouse at random.
pub const fn reach_keys_held(ctrl: bool, alt: bool) -> bool {
    ctrl && alt
}

/// Whether the window should be ignoring the mouse right now.
///
/// Three facts, and the third is what makes the reach gesture usable at all:
///
/// - `wanted` — what the settings say.
/// - `reach_held` — the user is holding `Ctrl` and `Alt`.
/// - `ui_open` — the widget's own menu or settings panel is on screen.
///
/// **`ui_open` is not a refinement; it is the difference between working and
/// infuriating.** The natural gesture is hold → right-click → *let go* → click the
/// menu item. Without this, letting go restores click-through at the exact moment
/// the menu appears, so the menu cannot be clicked — an escape hatch that opens a
/// door and shuts it in the same motion.
///
/// The setting is *suspended*, never overridden: drop all three and the widget goes
/// straight back to ignoring the mouse, so a pin is paused rather than undone.
pub const fn should_ignore_mouse(wanted: bool, reach_held: bool, ui_open: bool) -> bool {
    wanted && !reach_held && !ui_open
}

/// Whether the widget has something of its own on screen that needs the mouse.
static UI_OPEN: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// What the *settings* want, which the watcher then combines with the modifiers.
static WANT_TRANSPARENT: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Applies the click-through setting, and keeps applying it while `Ctrl` and
/// `Alt` are held.
///
/// **The single entry point for click-through, and the only writer of
/// `WS_EX_TRANSPARENT`.** Two writers would fight: the settings say "ignore the
/// mouse", the held modifiers say "do not", and whichever ran last would win —
/// which is a widget that is clickable or not depending on tick order.
///
/// The held-modifier escape is what makes a pinned widget reachable by *mouse*
/// rather than only by key: hold `Ctrl+Alt`, right-click the widget, use the menu
/// it already has. It needs no shortcut registration at all, so unlike a hotkey it
/// cannot be taken by another program.
///
/// Returns whether the state is being watched — `false` means it was applied once
/// and will not follow the modifiers, which is what a platform without
/// click-through gets anyway.
pub fn set_click_through(hwnd: u64, wanted: bool, ui_open: bool) -> bool {
    WANT_TRANSPARENT.store(wanted, std::sync::atomic::Ordering::SeqCst);
    UI_OPEN.store(ui_open, std::sync::atomic::Ordering::SeqCst);
    if hwnd == 0 {
        return false;
    }

    // Applied immediately rather than waiting for the watcher's next pass, so a
    // toggle in the settings panel takes effect as the user clicks it instead of
    // up to a frame later — and so opening the widget's own menu makes it
    // clickable in the same frame it appears.
    let (ctrl, alt) = reach_keys_down();
    let transparent = should_ignore_mouse(wanted, reach_keys_held(ctrl, alt), ui_open);
    raw_set_click_through(hwnd, transparent);

    #[cfg(target_os = "windows")]
    {
        start_reach_watcher(hwnd)
    }
    #[cfg(not(target_os = "windows"))]
    {
        false
    }
}

/// Watches the reach modifiers and keeps the window's hit-testing in step.
///
/// A thread of its own, polling every 16 ms, because the widget's own tick runs
/// once a second at best: a user who holds `Ctrl+Alt` and clicks does both inside
/// a few hundred milliseconds, and an escape that arrives after the click is not
/// an escape.
#[cfg(target_os = "windows")]
fn start_reach_watcher(hwnd: u64) -> bool {
    use std::sync::OnceLock;

    static STARTED: OnceLock<()> = OnceLock::new();
    if STARTED.get().is_some() {
        return true;
    }
    let _ = STARTED.set(());

    std::thread::Builder::new()
        .name("wattseal-overlay-reach".to_string())
        .spawn(move || {
            let mut transparent: Option<bool> = None;
            loop {
                let wanted = WANT_TRANSPARENT.load(std::sync::atomic::Ordering::SeqCst);
                let ui_open = UI_OPEN.load(std::sync::atomic::Ordering::SeqCst);

                if !wanted || ui_open {
                    // Nothing to watch for: either the window is not ignoring the
                    // mouse, or the widget has its own menu up and needs it. Waking
                    // sixty times a second to establish that would be the whole cost
                    // of the feature with none of the benefit. `set_click_through`
                    // applies the change immediately, so slowing down here cannot
                    // make turning click-through *on* feel late.
                    if transparent != Some(false) {
                        raw_set_click_through(hwnd, false);
                        transparent = Some(false);
                    }
                    std::thread::sleep(std::time::Duration::from_millis(250));
                    continue;
                }

                // One decision, taken in a pure function a test can drive.
                let (ctrl, alt) = reach_keys_down();
                let wanted = should_ignore_mouse(true, reach_keys_held(ctrl, alt), ui_open);

                // Only touched when it actually changed. `SetWindowLongPtrW` per
                // frame would be sixty needless syscalls a second for a value that
                // changes twice per gesture.
                if transparent != Some(wanted) {
                    raw_set_click_through(hwnd, wanted);
                    transparent = Some(wanted);
                }

                std::thread::sleep(std::time::Duration::from_millis(16));
            }
        })
        .is_ok()
}

/// Sets or clears `WS_EX_TRANSPARENT`, and nothing else.
fn raw_set_click_through(hwnd: u64, transparent: bool) {
    #[cfg(target_os = "windows")]
    {
        use windows_sys::Win32::UI::WindowsAndMessaging::{
            GWL_EXSTYLE, GetWindowLongPtrW, SetWindowLongPtrW, WS_EX_NOACTIVATE, WS_EX_TRANSPARENT,
        };

        if hwnd == 0 {
            return;
        }

        let hwnd = hwnd as *mut core::ffi::c_void;
        let flags = (WS_EX_TRANSPARENT | WS_EX_NOACTIVATE) as isize;

        // SAFETY: `hwnd` is our own window and these are the documented extended
        // styles for a click-through overlay.
        unsafe {
            let mut ex_style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
            if transparent {
                ex_style |= flags;
            } else {
                ex_style &= !flags;
            }
            SetWindowLongPtrW(hwnd, GWL_EXSTYLE, ex_style);
        }
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = (hwnd, transparent);
    }
}

/// A keyboard shortcut the window system delivers whatever has focus.
///
/// **Not every combination can be one.** Registering a bare letter would take
/// that letter away from every other program on the machine, and one with no
/// modifier at all is a keylogger rather than a shortcut. `usable()` is the
/// rule, and the capture UI refuses anything that fails it — see
/// [`Hotkey::refusal`] for the reason to show the user.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Hotkey {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    /// The virtual-key code. Letters are their ASCII uppercase (`A` is `0x41`),
    /// digits are their ASCII value, and F1–F12 are `0x70..=0x7B` — which is what
    /// Windows itself uses for `RegisterHotKey`.
    pub vk: u32,
}

/// The shortcut a fresh install gets, and the one a config file with no
/// `escape_hotkey` keeps using.
pub const DEFAULT_HOTKEY: Hotkey = Hotkey {
    ctrl: true,
    alt: true,
    shift: false,
    vk: 0x4F, // 'O'
};

impl Hotkey {
    /// The modifier flags `RegisterHotKey` wants.
    #[cfg(target_os = "windows")]
    fn modifiers(self) -> u32 {
        use windows_sys::Win32::UI::Input::KeyboardAndMouse::{MOD_ALT, MOD_CONTROL, MOD_NOREPEAT, MOD_SHIFT};

        // `MOD_NOREPEAT` is not optional. Without it a held key unpins, repins and
        // unpins again at the keyboard's repeat rate, and the user cannot tell
        // which state they are in when they let go.
        let mut flags = MOD_NOREPEAT;
        if self.ctrl {
            flags |= MOD_CONTROL;
        }
        if self.alt {
            flags |= MOD_ALT;
        }
        if self.shift {
            flags |= MOD_SHIFT;
        }
        flags
    }

    /// Whether this is something a person could reasonably press, and that we are
    /// willing to take from the rest of the machine.
    pub fn usable(self) -> bool {
        // A modifier alone is not a shortcut, and neither is a key with nothing
        // held down: both would swallow input in every other program.
        (self.ctrl || self.alt) && self.key_name().is_some()
    }

    /// Why this combination is refused, or `None` when it is fine.
    ///
    /// A reason rather than a bool because the capture UI has to say something,
    /// and "invalid shortcut" tells the user nothing about which part is wrong.
    pub fn refusal(self) -> Option<&'static str> {
        if self.key_name().is_none() {
            return Some("that key cannot be used");
        }
        if !self.ctrl && !self.alt {
            return Some("hold Ctrl or Alt as well");
        }
        None
    }

    /// The key on its own, as a person writes it.
    pub fn key_name(self) -> Option<&'static str> {
        match self.vk {
            0x41..=0x5A => LETTERS.iter().find(|(code, _)| *code == self.vk).map(|(_, name)| *name),
            0x30..=0x39 => DIGITS.iter().find(|(code, _)| *code == self.vk).map(|(_, name)| *name),
            0x70..=0x7B => FUNCTIONS
                .iter()
                .find(|(code, _)| *code == self.vk)
                .map(|(_, name)| *name),
            _ => None,
        }
    }

    /// The whole shortcut, as a person writes it: `Ctrl+Alt+O`.
    ///
    /// Allocates, which the earlier fixed-label version deliberately did not.
    /// That was over-cautious: this is called while drawing one settings row and
    /// one hint line, a handful of times per second, and a shortcut the user
    /// cannot change is worth less than the microseconds.
    pub fn label(self) -> String {
        let mut parts: Vec<&str> = Vec::with_capacity(4);
        if self.ctrl {
            parts.push("Ctrl");
        }
        if self.alt {
            parts.push("Alt");
        }
        if self.shift {
            parts.push("Shift");
        }
        if let Some(key) = self.key_name() {
            parts.push(key);
        }
        parts.join("+")
    }

    /// The config-file form: `ctrl+alt+o`, `ctrl+shift+f5`.
    ///
    /// Lower case and unspaced on purpose — it is meant to be typed by hand into
    /// `overlay_config.json`, and [`Hotkey::parse`] accepts any case so a user who
    /// writes `Ctrl+Alt+O` there is not punished for it.
    pub fn to_config(self) -> String {
        let mut parts: Vec<String> = Vec::with_capacity(4);
        if self.ctrl {
            parts.push("ctrl".to_string());
        }
        if self.alt {
            parts.push("alt".to_string());
        }
        if self.shift {
            parts.push("shift".to_string());
        }
        if let Some(key) = self.key_name() {
            parts.push(key.to_lowercase());
        }
        parts.join("+")
    }

    /// Reads back what [`Hotkey::to_config`] writes, and is forgiving about how
    /// it is written.
    ///
    /// Accepts any order and any case, because a file a person edits by hand is
    /// not a serialisation format — and a config that silently stops working
    /// because `Alt+Ctrl+O` was typed instead of `Ctrl+Alt+O` is a bad trade for
    /// a little strictness.
    pub fn parse(text: &str) -> Option<Self> {
        let mut hotkey = Hotkey {
            ctrl: false,
            alt: false,
            shift: false,
            vk: 0,
        };

        for token in text.split('+') {
            let token = token.trim();
            if token.is_empty() {
                continue;
            }
            match token.to_ascii_lowercase().as_str() {
                "ctrl" | "control" => hotkey.ctrl = true,
                "alt" => hotkey.alt = true,
                "shift" => hotkey.shift = true,
                "win" | "super" | "meta" => return None,
                _ => {
                    let upper = token.to_ascii_uppercase();
                    let code = LETTERS
                        .iter()
                        .chain(DIGITS.iter())
                        .chain(FUNCTIONS.iter())
                        .find(|(_, name)| *name == upper)
                        .map(|(code, _)| *code)?;
                    if hotkey.vk != 0 {
                        // Two keys and no way to say which one is the shortcut.
                        return None;
                    }
                    hotkey.vk = code;
                }
            }
        }

        hotkey.usable().then_some(hotkey)
    }
}

impl std::fmt::Display for Hotkey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.label())
    }
}

/// The keys that can be the *last* part of a shortcut, with the names a person
/// writes. The virtual-key codes are Windows' own.
const LETTERS: [(u32, &str); 26] = [
    (0x41, "A"),
    (0x42, "B"),
    (0x43, "C"),
    (0x44, "D"),
    (0x45, "E"),
    (0x46, "F"),
    (0x47, "G"),
    (0x48, "H"),
    (0x49, "I"),
    (0x4A, "J"),
    (0x4B, "K"),
    (0x4C, "L"),
    (0x4D, "M"),
    (0x4E, "N"),
    (0x4F, "O"),
    (0x50, "P"),
    (0x51, "Q"),
    (0x52, "R"),
    (0x53, "S"),
    (0x54, "T"),
    (0x55, "U"),
    (0x56, "V"),
    (0x57, "W"),
    (0x58, "X"),
    (0x59, "Y"),
    (0x5A, "Z"),
];

const DIGITS: [(u32, &str); 10] = [
    (0x30, "0"),
    (0x31, "1"),
    (0x32, "2"),
    (0x33, "3"),
    (0x34, "4"),
    (0x35, "5"),
    (0x36, "6"),
    (0x37, "7"),
    (0x38, "8"),
    (0x39, "9"),
];

const FUNCTIONS: [(u32, &str); 12] = [
    (0x70, "F1"),
    (0x71, "F2"),
    (0x72, "F3"),
    (0x73, "F4"),
    (0x74, "F5"),
    (0x75, "F6"),
    (0x76, "F7"),
    (0x77, "F8"),
    (0x78, "F9"),
    (0x79, "F10"),
    (0x7A, "F11"),
    (0x7B, "F12"),
];

/// The line the settings panel shows under the click-through toggle.
///
/// The wording is about the *setting* rather than about being stuck, because it
/// is shown while the user is still deciding — telling someone how to undo a
/// choice before they make it is the only time the advice can be acted on. The
/// stuck widget shows [`escape_hint`] instead.
pub fn escape_setting_hint(language: crate::language::AppLanguage, hotkey: Hotkey) -> String {
    use crate::language::AppLanguage;

    let key = hotkey.label();
    match language {
        AppLanguage::German => format!("{key} löst einen Fix"),
        AppLanguage::French => format!("{key} libère un widget épinglé"),
        AppLanguage::Chinese => format!("按 {key} 可以解开固定"),
        AppLanguage::Romanian => format!("{key} eliberează un widget fixat"),
        _ => format!("{key} releases a pinned widget"),
    }
}

/// The line a stuck widget shows when the shortcut was registered.
///
/// The one sentence the user can still act on, because a widget that ignores the
/// mouse can still be read.
pub fn escape_hint(language: crate::language::AppLanguage, hotkey: Hotkey) -> String {
    use crate::language::AppLanguage;

    let key = hotkey.label();
    match language {
        AppLanguage::German => format!("Angeheftet · {key} löst den Fix"),
        AppLanguage::French => format!("Épinglé · {key} libère"),
        AppLanguage::Chinese => format!("已固定 · 按 {key} 解开"),
        AppLanguage::Romanian => format!("Fixat · {key} eliberează"),
        _ => format!("Pinned · {key} releases"),
    }
}

/// The identifier the shortcut is registered under. Arbitrary; it only has to be
/// the same one on the way in and on the way out.
#[cfg(target_os = "windows")]
const ESCAPE_HOTKEY_ID: i32 = 0x5711;

/// Binds the escape shortcut, replacing whatever was bound before.
///
/// Returns whether the new one took. **False is an ordinary outcome, not an
/// error**: another program may already own the combination, which is exactly
/// what happened on the machine this was developed on — sixteen of twenty-one
/// plausible candidates were taken, including the original hard-coded
/// `Ctrl+Alt+O`. That is why the key is configurable rather than fixed.
///
/// **The dedicated thread is not a detail.** `WM_HOTKEY` is delivered to the
/// *thread* that called `RegisterHotKey`, and `iced` drains the main thread's
/// queue in its event loop — so a shortcut registered on the main thread and read
/// back with `PeekMessage` is one whose message has already been taken by the time
/// the widget's tick runs. That version was written first and did not work on a
/// real machine: the key was pressed and nothing happened. A null window on a
/// thread of its own gives the message a queue nothing else reads.
#[cfg(target_os = "windows")]
pub fn set_escape_hotkey(hotkey: Hotkey) -> bool {
    use std::sync::{OnceLock, mpsc};

    static COMMANDS: OnceLock<mpsc::Sender<Hotkey>> = OnceLock::new();

    if !hotkey.usable() {
        return false;
    }

    let sender = COMMANDS.get_or_init(|| {
        let (commands, orders) = mpsc::channel::<Hotkey>();
        std::thread::Builder::new()
            .name("wattseal-overlay-hotkey".to_string())
            .spawn(move || run_hotkey_thread(&orders))
            .ok();
        commands
    });

    // **Cleared before the request, not after.** `SETTLED` starts `true` so that a
    // widget which never asks for a shortcut never waits — but that also lets the
    // wait below be satisfied by the *previous* answer, before the thread has even
    // looked at this one. It did exactly that on a real machine: every bind
    // reported failure, `escape_hotkey_ready` stayed false, `poll_escape_hotkey`
    // short-circuited on it, and the key did nothing at all.
    clear_settled();

    if sender.send(hotkey).is_err() {
        return false;
    }

    // The thread answers by setting this, and the answer has to be waited for:
    // the caller shows a different hint depending on it, and "we do not know yet"
    // is not one of the answers the user can be given.
    let deadline = std::time::Instant::now() + std::time::Duration::from_millis(600);
    while std::time::Instant::now() < deadline && !shortcut_settled() {
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    shortcut_registered()
}

/// The thread that owns the shortcut's message queue for the life of the process.
///
/// It pumps messages itself rather than blocking in `GetMessageW`, because it also
/// has to notice a request to bind a different key — the user changing the
/// shortcut in the settings panel, which must take effect without a restart.
#[cfg(target_os = "windows")]
fn run_hotkey_thread(orders: &std::sync::mpsc::Receiver<Hotkey>) {
    use windows_sys::Win32::UI::{
        Input::KeyboardAndMouse::{RegisterHotKey, UnregisterHotKey},
        WindowsAndMessaging::{MSG, PM_REMOVE, PeekMessageW, WM_HOTKEY},
    };

    let mut bound = false;
    let mut message: MSG = unsafe { std::mem::zeroed() };

    loop {
        // Everything the queue has, so a press cannot sit behind another message.
        while unsafe { PeekMessageW(&mut message as *mut MSG, std::ptr::null_mut(), 0, 0, PM_REMOVE) } != 0 {
            if message.message == WM_HOTKEY && message.wParam as i32 == ESCAPE_HOTKEY_ID {
                PRESSED.store(true, std::sync::atomic::Ordering::SeqCst);
            }
        }

        match orders.recv_timeout(std::time::Duration::from_millis(25)) {
            Ok(hotkey) => {
                // Unregistered before re-registering: `RegisterHotKey` fails on a
                // combination this process already holds, so binding a new key over
                // an old one would report failure while the old one kept working.
                if bound {
                    unsafe { UnregisterHotKey(std::ptr::null_mut(), ESCAPE_HOTKEY_ID) };
                }

                // SAFETY: a null window and this thread's identifier; nothing else
                // in the process registers under this id.
                bound =
                    unsafe { RegisterHotKey(std::ptr::null_mut(), ESCAPE_HOTKEY_ID, hotkey.modifiers(), hotkey.vk) }
                        != 0;

                set_registered(bound);
                // Last, so the widget's wait can only end once the answer it is
                // about to read is the one for its own request.
                SETTLED.store(true, std::sync::atomic::Ordering::SeqCst);
            }
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
            // The sender is gone, which means the process is going.
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => return,
        }
    }
}

/// No-op on non-Windows platforms.
#[cfg(not(target_os = "windows"))]
pub fn set_escape_hotkey(_hotkey: Hotkey) -> bool {
    false
}

// Everything below is used only by the Windows implementation above, and is
// gated to match. Ungated it compiles here and warns as dead code everywhere
// else — which the first macOS run reported, six times, as the only thing it had
// to say about this file.

/// A press the shortcut thread has seen and the widget has not yet acted on.
#[cfg(target_os = "windows")]
static PRESSED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Whether the shortcut could be registered at all.
#[cfg(target_os = "windows")]
static REGISTERED_OK: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Whether the thread has answered the most recent bind request.
#[cfg(target_os = "windows")]
static SETTLED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(true);

#[cfg(target_os = "windows")]
fn set_registered(ok: bool) {
    REGISTERED_OK.store(ok, std::sync::atomic::Ordering::SeqCst);
}

#[cfg(target_os = "windows")]
fn shortcut_registered() -> bool {
    REGISTERED_OK.load(std::sync::atomic::Ordering::SeqCst)
}

#[cfg(target_os = "windows")]
fn shortcut_settled() -> bool {
    SETTLED.load(std::sync::atomic::Ordering::SeqCst)
}

/// Clears the "settled" flag before a bind request is sent, so the wait in
/// [`set_escape_hotkey`] cannot be satisfied by the *previous* answer.
#[cfg(target_os = "windows")]
fn clear_settled() {
    SETTLED.store(false, std::sync::atomic::Ordering::SeqCst);
}

/// Whether the shortcut was pressed since the last time this was called.
///
/// Takes the press rather than only looking at it, so three presses inside one
/// tick are still one toggle — the same intent as `MOD_NOREPEAT`, that a held key
/// is one press and not a stream of them.
#[cfg(target_os = "windows")]
pub fn poll_escape_hotkey() -> bool {
    PRESSED.swap(false, std::sync::atomic::Ordering::SeqCst)
}

/// See the Windows variant.
#[cfg(not(target_os = "windows"))]
pub fn poll_escape_hotkey() -> bool {
    // No shortcut can be registered, so no press can arrive.
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_language_quotes_the_key_that_is_bound() {
        // The label is built from the hotkey rather than written into the strings,
        // so a user who changes the shortcut sees the new one everywhere. This
        // pins that — the alternative is a panel that still says `Ctrl+Alt+O`
        // after the user bound `Ctrl+Shift+F5`.
        for language in crate::language::AppLanguage::all() {
            let hint = escape_hint(*language, DEFAULT_HOTKEY);
            assert!(hint.contains("Ctrl+Alt+O"), "{language:?} says {hint:?}");

            let setting = escape_setting_hint(*language, DEFAULT_HOTKEY);
            assert!(
                setting.contains("Ctrl+Alt+O"),
                "{language:?} says {setting:?} in the panel"
            );
        }

        // And a changed key really does travel into both strings.
        let custom = Hotkey::parse("ctrl+shift+f5").unwrap();
        assert!(escape_hint(crate::language::AppLanguage::English, custom).contains("Ctrl+Shift+F5"));
        assert!(escape_setting_hint(crate::language::AppLanguage::Chinese, custom).contains("Ctrl+Shift+F5"));
    }

    #[test]
    fn a_shortcut_survives_being_written_and_read_back() {
        // The config file is the only place a custom shortcut lives, so this round
        // trip is the whole persistence story.
        for text in ["ctrl+alt+o", "ctrl+shift+f5", "alt+1", "ctrl+alt+shift+f12", "ctrl+9"] {
            let hotkey = Hotkey::parse(text).unwrap_or_else(|| panic!("{text} did not parse"));
            let written = hotkey.to_config();
            let read_back = Hotkey::parse(&written).unwrap_or_else(|| panic!("{written} did not parse"));
            assert_eq!(hotkey, read_back, "{text} -> {written} did not survive");
        }
    }

    #[test]
    fn a_hand_edited_file_is_read_however_it_was_typed() {
        // A config file is not a serialisation format. Someone who writes
        // `Alt+Ctrl+O` or `CTRL + ALT + O` meant the same thing, and a widget that
        // silently ignores their shortcut because of the order is a widget they
        // cannot fix without being told the rule.
        let canonical = Hotkey::parse("ctrl+alt+o").unwrap();
        for text in [
            "Alt+Ctrl+O",
            "CTRL+ALT+O",
            "ctrl + alt + o",
            "Control+Alt+O",
            "  ctrl+alt+o  ",
        ] {
            assert_eq!(Hotkey::parse(text), Some(canonical), "{text:?} was not understood");
        }
    }

    #[test]
    fn a_shortcut_that_would_swallow_ordinary_typing_is_refused() {
        // The rule that keeps this feature from being a keylogger. A bare letter,
        // or a bare function key, would be taken from every other program on the
        // machine for as long as the widget runs.
        for text in ["o", "f5", "1", "shift+o", "win+o", "ctrl", "ctrl+alt"] {
            assert_eq!(Hotkey::parse(text), None, "{text:?} was accepted as a shortcut");
        }

        // And the refusal explains which half is wrong, because "invalid" does not
        // tell the user what to change.
        let bare_letter = Hotkey {
            ctrl: false,
            alt: false,
            shift: false,
            vk: 0x4F,
        };
        assert_eq!(bare_letter.refusal(), Some("hold Ctrl or Alt as well"));

        let no_key = Hotkey {
            ctrl: true,
            alt: true,
            shift: false,
            vk: 0x10,
        };
        assert_eq!(no_key.refusal(), Some("that key cannot be used"));
    }

    #[test]
    fn the_label_is_written_the_way_a_person_writes_it() {
        assert_eq!(DEFAULT_HOTKEY.label(), "Ctrl+Alt+O");
        assert_eq!(Hotkey::parse("ctrl+shift+f5").unwrap().label(), "Ctrl+Shift+F5");
        assert_eq!(Hotkey::parse("alt+1").unwrap().label(), "Alt+1");

        // Modifiers in a fixed order, so the same shortcut is never written two
        // ways — including the way the user typed it in.
        assert_eq!(Hotkey::parse("shift+ctrl+alt+p").unwrap().label(), "Ctrl+Alt+Shift+P");
    }

    #[test]
    fn only_the_keys_that_exist_are_offered() {
        // The table is the contract with the capture UI: what the user presses has
        // to be in it, or their key is silently not accepted.
        assert_eq!(LETTERS.len(), 26);
        assert_eq!(DIGITS.len(), 10);
        assert_eq!(FUNCTIONS.len(), 12);
        assert_eq!(LETTERS[0], (0x41, "A"));
        assert_eq!(LETTERS[25], (0x5A, "Z"));
        assert_eq!(FUNCTIONS[0], (0x70, "F1"));
        assert_eq!(FUNCTIONS[11], (0x7B, "F12"));

        // No gaps and no duplicates: every code in each range resolves, and no two
        // entries claim the same one.
        let mut codes: Vec<u32> = LETTERS
            .iter()
            .chain(DIGITS.iter())
            .chain(FUNCTIONS.iter())
            .map(|(code, _)| *code)
            .collect();
        let before = codes.len();
        codes.sort_unstable();
        codes.dedup();
        assert_eq!(codes.len(), before, "two keys share a virtual-key code");
    }

    #[test]
    fn the_settings_are_the_only_thing_that_says_ignore_the_mouse() {
        // Two writers of `WS_EX_TRANSPARENT` would fight: the settings say "ignore
        // the mouse", a held modifier says "do not", and whichever ran last wins —
        // a widget that is clickable or not depending on thread timing. So there is
        // exactly one place the style is written, and this is it.
        let source = include_str!("winlayer.rs");
        // Cut before the tests, or this would count its own assertion text —
        // `include_str!` sees the whole file, and the first version of this test
        // failed on exactly that.
        let production = source.split("#[cfg(test)]").next().expect("the file has a beginning");

        let writers = production.matches("WS_EX_TRANSPARENT | WS_EX_NOACTIVATE").count();
        assert_eq!(
            writers, 1,
            "expected exactly one place to compose the click-through style, found {writers}"
        );

        // And the watcher is the thing that follows the modifiers, so a config
        // change on its own cannot leave the window stuck ignoring the mouse.
        assert!(
            production.contains("fn raw_set_click_through"),
            "the single writer of the click-through style is gone"
        );
    }

    #[test]
    fn the_reach_rule_needs_both_keys_and_gives_the_setting_back() {
        // The whole rule, in a few lines, because the platform read underneath it
        // cannot be tested here. Synthetic input never reaches `GetAsyncKeyState`
        // on this machine — proven with both `keybd_event` and `SendInput`, while
        // the same keys *do* reach `RegisterHotKey` — so `reach_keys_down` is the
        // one line of this feature that ships unverified, and everything decidable
        // is pinned here instead.
        //
        // **Both keys, not either.** `Ctrl` is held constantly for shortcuts and
        // `Alt` for menus, so a single key would have the widget grabbing the mouse
        // several times an hour for no reason the user could connect to anything.
        assert!(reach_keys_held(true, true));
        assert!(!reach_keys_held(true, false), "Ctrl alone must not release the mouse");
        assert!(!reach_keys_held(false, true), "Alt alone must not release the mouse");
        assert!(!reach_keys_held(false, false));

        // And the setting is *suspended*, not overridden: let go and it is back.
        assert!(
            should_ignore_mouse(true, false, false),
            "a pinned widget stopped ignoring the mouse"
        );
        assert!(
            !should_ignore_mouse(true, true, false),
            "the reach keys did not reach it"
        );
        assert!(
            !should_ignore_mouse(false, false, false),
            "the mouse was captured with pin mode off"
        );
        assert!(
            !should_ignore_mouse(false, true, false),
            "the reach keys captured the mouse on their own"
        );

        // **And the widget's own menu keeps the mouse whatever the keys do.** The
        // natural gesture is hold → right-click → let go → click the menu item, so
        // restoring click-through the moment the keys are released makes the menu
        // unclickable — an escape hatch that shuts the door as it opens.
        assert!(
            !should_ignore_mouse(true, false, true),
            "releasing the keys made the widget's own menu unclickable"
        );
        assert!(
            !should_ignore_mouse(true, true, true),
            "the menu was click-through while the keys were still held"
        );
        // The menu opening is not a reason to capture the mouse when nothing asked
        // the widget to ignore it in the first place.
        assert!(!should_ignore_mouse(false, false, true));
    }

    #[test]
    fn the_reach_modifiers_are_not_a_registered_shortcut() {
        // **Why this mechanism exists at all.** `RegisterHotKey` combinations can
        // be owned by another program — sixteen of twenty-one plausible ones on the
        // machine this was written on, including the original hard-coded
        // `Ctrl+Alt+O`. Reading the keys directly cannot be taken away, so the
        // reach gesture has to stay on `GetAsyncKeyState` rather than being folded
        // into the shortcut list.
        let source = include_str!("winlayer.rs");
        let reach = source
            .split("pub fn reach_keys_down")
            .nth(1)
            .expect("the reach check is gone");
        let body = reach.split("/// See the Windows variant.").next().unwrap();
        assert!(
            body.contains("GetAsyncKeyState"),
            "the reach gesture no longer reads the key state directly, so another program can own it"
        );
        assert!(
            !body.contains("RegisterHotKey"),
            "the reach gesture became a registered shortcut, which is exactly what it exists to avoid"
        );
    }

    #[test]
    fn a_bind_request_is_never_answered_by_the_previous_bind() {
        // **The bug that made the shortcut do nothing at all.**
        //
        // `SETTLED` starts `true` so a widget that never asks for a shortcut never
        // waits on one. The wait after a bind therefore has to *clear* it before
        // sending the request — otherwise it is satisfied instantly by the answer
        // to the previous request, `registered` is read before the thread has
        // looked at anything, and the widget concludes its key was refused.
        //
        // Every bind reported failure, `escape_hotkey_ready` stayed false,
        // `poll_escape_hotkey` short-circuited on it, and the key did nothing.
        // Unit tests could not see it — `RegisterHotKey` needs a real desktop and
        // a real thread — so this pins the ordering in the source instead, which is
        // the part that was wrong.
        let source = include_str!("winlayer.rs");
        let body = source
            .split("pub fn set_escape_hotkey")
            .nth(1)
            .expect("set_escape_hotkey is gone");

        let cleared = body.find("clear_settled();").expect(
            "set_escape_hotkey no longer clears the settled flag, so every bind will be answered by the one before it",
        );
        let sent = body.find("sender.send(hotkey)").expect("the request is never sent");
        assert!(
            cleared < sent,
            "the settled flag must be cleared *before* the request, not after"
        );
    }

    #[test]
    fn the_default_is_one_the_rules_allow() {
        // A default that `usable()` rejects would ship a widget with no escape
        // hatch and no way to notice.
        assert!(DEFAULT_HOTKEY.usable());
        assert_eq!(DEFAULT_HOTKEY.refusal(), None);
        assert_eq!(Hotkey::parse(&DEFAULT_HOTKEY.to_config()), Some(DEFAULT_HOTKEY));
    }

    #[test]
    fn a_bind_is_only_attempted_for_a_usable_shortcut() {
        // Called with rubbish, this must answer false rather than take a bare key
        // away from the rest of the machine.
        let bare = Hotkey {
            ctrl: false,
            alt: false,
            shift: false,
            vk: 0x4F,
        };
        assert!(!set_escape_hotkey(bare));
    }

    #[test]
    fn the_two_platform_agree_on_which_one_supports_click_through() {
        // Pin mode's second half is advertised from this constant, and the
        // settings panel offers a toggle only when it is true. Getting it wrong
        // hands the user a switch that cannot be thrown.
        assert_eq!(super::click_through_supported(), cfg!(target_os = "windows"));
    }

    #[test]
    fn a_missing_window_handle_is_never_treated_as_a_window() {
        // `iced::window::raw_id` yields nothing before the window exists, and
        // the id arrives as a plain integer, so zero is reachable. Passing it to
        // `SetLayeredWindowAttributes` would ask Win32 about the desktop.
        assert!(!super::apply(0, 0.8));
        assert!(!super::set_click_through(0, true, false));
        assert!(!super::set_click_through(0, false, false));
        // The menu being open must not make a null handle look like a window
        // either — the same guard, reached through the other argument.
        assert!(!super::set_click_through(0, true, true));
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn the_slider_ends_land_exactly_on_the_window_alphas() {
        // A byte is all the layered window takes, so the two ends of the slider
        // have to be exactly opaque and exactly as transparent as it allows.
        // Rounding the wrong way here is invisible in the settings panel and
        // obvious on screen: a widget that is a little too transparent, or one
        // that never gets fully opaque.
        assert_eq!(super::alpha_byte(1.0), 255);
        assert_eq!(super::alpha_byte(0.0), 13, "the bottom of the slider is 0.05, not 0");
        assert_eq!(super::alpha_byte(-5.0), 13);
        assert_eq!(super::alpha_byte(9.0), 255);

        // Monotone, and never wraps: the cast to a byte is the thing that could.
        let mut previous = 0;
        for step in 0..=100 {
            let byte = super::alpha_byte(step as f32 / 100.0);
            assert!(byte >= previous, "{step}% is darker than the step before it");
            previous = byte;
        }
    }
}
