//! Self-contained palette for the overlay.
//!
//! The overlay owns its *colours* — rendering uses `iced`'s built-in [`Theme`]
//! for widget defaults plus closure-based styles for the card and the values,
//! so there is no dependency on the `ui` crate. It does not own its *scheme*:
//! that is read from the same `ui_settings` row the dashboard writes, so
//! switching the dashboard to a light theme takes the widget with it instead of
//! leaving a light dashboard with a dark widget that nobody asked for.

use iced::{Color, Theme};
use serde::{Deserialize, Serialize};

use crate::config::{BgColor, TextColor};

/// The names the dashboard stores for its light themes.
///
/// These are the *English display names* — "Swimming", not `OceanLight` — which
/// is what `ui_settings.theme` actually holds: the dashboard saves
/// `theme_name(English, theme)`, and reads it back the same way. Using the Rust
/// variant names here would have matched nothing at all, and every theme would
/// have come out dark.
///
/// `ui_settings.theme` holds a name rather than a mode, so the dashboard's own
/// `AppTheme::mode()` cannot be called from here. The list is therefore checked
/// against it from the `ui` crate's test suite — a theme added there without
/// updating this fails a build rather than quietly giving the widget the wrong
/// lightness in front of the user.
const LIGHT_THEME_NAMES: &[&str] = &["Swimming", "Splashing"];

/// Overlay color scheme, resolved from the dashboard's theme.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ThemeChoice {
    /// Follow whatever the dashboard has saved, which is what an empty database
    /// and a dashboard set to anything both resolve to.
    #[default]
    Auto,
    Dark,
    Light,
}

impl ThemeChoice {
    /// Every option the picker offers, in the order it shows them.
    pub const ALL: &'static [ThemeChoice] = &[ThemeChoice::Auto, ThemeChoice::Dark, ThemeChoice::Light];

    /// The scheme a theme name from `ui_settings.theme` resolves to.
    ///
    /// Dark is the answer for an unrecognised name: it is the scheme most of
    /// the shipped themes use, and an unreadable name means the dashboard is on
    /// a newer version than this widget was built against.
    pub fn from_dashboard_name(name: &str) -> Self {
        if LIGHT_THEME_NAMES.contains(&name) {
            Self::Light
        } else {
            Self::Dark
        }
    }

    /// The scheme to actually draw in, once "follow the dashboard" is resolved.
    ///
    /// `fallback` is what the dashboard last said, or the default when it has
    /// said nothing. `Auto` never survives to this point: by the time anything is
    /// drawn the choice has been made.
    pub fn resolve(self, fallback: ThemeChoice) -> ThemeChoice {
        match self {
            ThemeChoice::Auto => fallback,
            explicit => explicit,
        }
    }
}

/// Resolved color set for a scheme.
#[derive(Debug, Clone, Copy)]
pub struct Palette {
    /// Base card color (alpha is applied by the configured opacity).
    pub card: Color,
    /// Primary foreground text.
    pub text: Color,
    /// Secondary / label text (already low-alpha).
    pub muted: Color,
    /// Accent (headline value color).
    pub accent: Color,
    /// Success / green value color.
    pub green: Color,
    /// Warning / amber value color.
    pub amber: Color,
    pub border: Color,
    pub shadow: Color,
}

impl Palette {
    /// Card background with the user opacity applied.
    pub fn card_with_alpha(self, alpha: f32) -> Color {
        Color { a: alpha, ..self.card }
    }
}

/// Returns the palette for the given scheme.
///
/// `Auto` has to be resolved before it gets here — see
/// [`ThemeChoice::resolve`]. It falls back to dark rather than being
/// unreachable, because a palette function that panics on a value the type
/// allows is a bug waiting for the day someone adds a variant.
pub fn palette(choice: ThemeChoice) -> Palette {
    match choice {
        // Dark is most of the shipped themes' own answer, so it is the least
        // surprising thing to draw if resolution ever did not happen.
        ThemeChoice::Auto | ThemeChoice::Dark => Palette {
            card: Color::from_rgb(0.106, 0.118, 0.153), // #1b1e27
            text: Color::from_rgb(0.90, 0.94, 1.0),
            muted: Color::from_rgba(0.90, 0.94, 1.0, 0.55),
            accent: Color::from_rgb(0.0, 0.80, 0.90), // #00cce6
            green: Color::from_rgb(0.55, 1.0, 0.40),
            amber: Color::from_rgb(0.95, 0.72, 0.15),
            border: Color::from_rgba(0.0, 0.80, 0.90, 0.35),
            shadow: Color::from_rgba(0.0, 0.0, 0.0, 0.35),
        },
        ThemeChoice::Light => Palette {
            card: Color::from_rgb(0.937, 0.961, 0.988), // #eff5fc
            text: Color::from_rgb(0.08, 0.12, 0.20),
            muted: Color::from_rgba(0.08, 0.12, 0.20, 0.55),
            accent: Color::from_rgb(0.0, 0.62, 0.75), // #009ec0
            green: Color::from_rgb(0.20, 0.62, 0.22),
            amber: Color::from_rgb(0.78, 0.48, 0.0),
            border: Color::from_rgba(0.0, 0.62, 0.75, 0.35),
            shadow: Color::from_rgba(0.0, 0.0, 0.0, 0.18),
        },
    }
}

/// Returns the scheme's palette with the user's color overrides applied.
///
/// Alpha is deliberately *not* part of this: on Windows the whole layered
/// window carries a single alpha, so per-element translucency is impossible.
/// Hue and lightness, however, are fully controllable — which is what these
/// two swatches expose.
pub fn palette_with(choice: ThemeChoice, background: BgColor, foreground: TextColor) -> Palette {
    let mut result = palette(choice);

    if let Some((r, g, b)) = background.rgb() {
        result.card = Color::from_rgb(r, g, b);
        // Keep the drop shadow in the same family as the card.
        result.shadow = Color::from_rgba(r * 0.3, g * 0.3, b * 0.3, 0.35);
    }

    if let Some((r, g, b)) = foreground.rgb() {
        result.text = Color::from_rgb(r, g, b);
        result.muted = Color::from_rgba(r, g, b, 0.55);
    }

    result
}

/// The iced theme handed to widgets (labels, pick-lists, checkboxes…).
///
/// Also expects resolution to have happened; `Auto` draws as dark, which is the
/// same answer [`palette`] gives and the same reason.
pub fn iced_theme(choice: ThemeChoice) -> Theme {
    match choice {
        ThemeChoice::Auto | ThemeChoice::Dark => Theme::Dark,
        ThemeChoice::Light => Theme::Light,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_light_scheme_is_actually_light() {
        // Reading the scheme out of `ui_settings` exists so that switching the
        // dashboard to a light theme takes the widget with it. A copy-paste that
        // left the Light arm looking like the Dark one would put a dark widget on
        // a light dashboard — the exact mismatch this module exists to prevent,
        // and the only test below would not notice, because the other one only
        // ever asks for `ThemeChoice::Dark`.
        let dark = palette(ThemeChoice::Dark);
        let light = palette(ThemeChoice::Light);

        assert!(
            dark.card.r < 0.5 && dark.card.g < 0.5 && dark.card.b < 0.5,
            "the dark card is not dark"
        );
        assert!(
            light.card.r > 0.5 && light.card.g > 0.5 && light.card.b > 0.5,
            "the light card is not light"
        );

        // Text has to contrast with its own card in *both* schemes. Comparing the
        // two sides of mid-grey catches a palette where the foreground drifted
        // towards the card without anyone noticing, which is what "invisible
        // labels" looks like.
        for (name, p) in [("dark", dark), ("light", light)] {
            assert_ne!(
                p.text.r > 0.5,
                p.card.r > 0.5,
                "{name}: text and card sit on the same side of mid-grey, so they cannot be told apart"
            );
            // `muted` is the text's own hue at lower opacity — same RGB, less
            // alpha. (I first wrote this as "between the text and the card",
            // which the code does not do and should not: half the weight comes
            // from opacity, not from drifting the hue towards the background.)
            assert_eq!(
                (p.muted.r, p.muted.g, p.muted.b),
                (p.text.r, p.text.g, p.text.b),
                "{name}: muted is a different hue from the text, so the labels read as a second colour"
            );
            assert!(p.muted.a < p.text.a, "{name}: muted is not the faint one");
        }
    }

    #[test]
    fn the_users_opacity_only_touches_the_alpha_channel() {
        // `card_with_alpha` is one line, and the failure it could have is the
        // kind that never shows up in a screenshot of a dark widget: set the RGB
        // as well and the card silently loses the colour its scheme picked.
        let stock = palette(ThemeChoice::Light).card;
        let faded = palette(ThemeChoice::Light).card_with_alpha(0.42);

        assert_eq!(
            (faded.r, faded.g, faded.b),
            (stock.r, stock.g, stock.b),
            "the opacity changed the colour"
        );
        assert_eq!(faded.a, 0.42, "the opacity was not applied");
    }

    #[test]
    fn a_text_colour_override_reaches_both_the_text_and_the_muted_one() {
        // Half-applied is the bug: the value colour changes and the labels keep
        // the scheme's, which reads as "the override mostly did not work".
        for choice in TextColor::ALL.iter().copied() {
            let applied = palette_with(ThemeChoice::Dark, BgColor::Auto, choice);
            if choice == TextColor::Auto {
                continue;
            }
            let Some((r, g, b)) = choice.rgb() else { continue };

            assert_eq!(
                (applied.text.r, applied.text.g, applied.text.b),
                (r, g, b),
                "{choice:?} did not reach `text`"
            );
            assert_eq!(
                (applied.muted.r, applied.muted.g, applied.muted.b),
                (r, g, b),
                "{choice:?} did not reach `muted`"
            );
            // Same hue, less weight — that is the whole promise of `muted`.
            assert!(
                applied.muted.a < applied.text.a,
                "{choice:?}: muted is not the faint one"
            );
        }
    }

    #[test]
    fn auto_means_leave_the_scheme_alone() {
        // `Auto` is the default, so if it were treated as an override every user
        // who never touched the setting would get a fixed text colour instead of
        // the scheme's.
        for choice in [ThemeChoice::Dark, ThemeChoice::Light] {
            let stock = palette(choice);
            let auto = palette_with(choice, BgColor::Auto, TextColor::Auto);
            assert_eq!(auto.text, stock.text);
            assert_eq!(auto.muted, stock.muted);
            assert_eq!(auto.card, stock.card);
        }
    }

    #[test]
    fn the_widgets_own_theme_follows_the_scheme() {
        // `iced` draws the pick-lists and checkboxes from this. Getting it wrong
        // puts dark dropdowns on a light card — visible on every open of the
        // settings panel, and invisible to every other test here.
        assert!(matches!(iced_theme(ThemeChoice::Dark), Theme::Dark));
        assert!(matches!(iced_theme(ThemeChoice::Light), Theme::Light));
    }

    #[test]
    fn the_stored_theme_names_resolve_to_the_scheme_they_look_like() {
        // These are the strings `ui_settings.theme` actually holds. The `ui`
        // crate's suite checks them against the dashboard's own `mode()`, so a
        // mismatch here fails there rather than in front of a user.
        assert_eq!(ThemeChoice::from_dashboard_name("Swimming"), ThemeChoice::Light);
        assert_eq!(ThemeChoice::from_dashboard_name("Splashing"), ThemeChoice::Light);
        assert_eq!(ThemeChoice::from_dashboard_name("Hunting"), ThemeChoice::Dark);
        assert_eq!(ThemeChoice::from_dashboard_name("Sleeping"), ThemeChoice::Dark);
        assert_eq!(ThemeChoice::from_dashboard_name("Sunbathing"), ThemeChoice::Dark);
        assert_eq!(ThemeChoice::from_dashboard_name("Lounging"), ThemeChoice::Dark);
    }

    #[test]
    fn a_name_this_build_has_never_seen_falls_back_to_dark() {
        // A dashboard newer than this widget stores a name that is not here yet.
        assert_eq!(ThemeChoice::from_dashboard_name("MidnightNeon"), ThemeChoice::Dark);
        assert_eq!(ThemeChoice::from_dashboard_name(""), ThemeChoice::Dark);
    }

    #[test]
    fn an_override_keeps_the_shadow_in_the_cards_own_family() {
        let overridden = palette_with(ThemeChoice::Dark, BgColor::Navy, TextColor::Auto);
        let stock = palette(ThemeChoice::Dark);

        assert_ne!(overridden.card, stock.card, "the override was not applied");

        // The promise `palette_with` makes is "the same family", not "black":
        // a shadow tinted with the card reads as belonging to it on top of
        // whatever the widget happens to be floating over.
        for (shadow, card) in [(overridden.shadow, overridden.card), (overridden.shadow, stock.card)] {
            assert!(
                shadow.r < card.r && shadow.g < card.g && shadow.b < card.b && shadow.r > 0.0,
                "shadow {shadow:?} is not a darkened version of card {card:?}"
            );
        }

        // Black on a navy card would not be "the same family" at all.
        assert!(overridden.shadow.r > stock.shadow.r);
    }
}
