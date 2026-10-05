//! The languages the widget can speak.
//!
//! This is a **local copy on purpose**. The dashboard has the same list, and
//! when this fork's overlay was still part of the WattSeal build it used to
//! import it from `common` so there was one of them. That import was the last
//! thing tying the widget to WattSeal's source tree, and it is exactly what the
//! standalone build does without.
//!
//! The two lists are not kept in sync by sharing code: they are kept in sync by
//! agreeing on a **two-letter code** in the `ui_settings` table, which is data,
//! not code. An unknown code falls back to English rather than failing — see
//! [`AppLanguage::from_code`].
//!
//! What this costs, stated plainly: a language added to one side and not the
//! other is a silent mismatch. The shared enum made that a compile error; this
//! makes it a test. There is one, and it is the only reason this duplication is
//! acceptable.

use std::fmt::Display;

/// One of the languages the dashboard can store and the widget can read back.
///
/// Declaration order follows upstream, so the two files read alike when someone
/// compares them by eye. It has no effect on anything: the widget is shown the
/// languages by [`AppLanguage::all`], not by this order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub enum AppLanguage {
    #[default]
    English,
    Chinese,
    French,
    German,
    Romanian,
}

impl AppLanguage {
    /// The language's name, as its own speakers write it.
    ///
    /// `Display` renders the same string, but this is what a pick-list needs: a
    /// language has to be shown in *itself* for the list to be readable by
    /// someone who does not know the panel's current language. A list translated
    /// into the language you are trying to leave would be a list you cannot read
    /// to find your way out.
    pub fn native_name(self) -> &'static str {
        match self {
            AppLanguage::English => "English",
            AppLanguage::German => "Deutsch",
            AppLanguage::French => "Français",
            AppLanguage::Chinese => "简体中文",
            AppLanguage::Romanian => "Română",
        }
    }

    /// Returns all available languages.
    ///
    /// **This order, not the declaration order, is what the picker shows.** The
    /// two disagree: upstream changed the order of the *declarations* when it
    /// reordered the picker, and a reader who assumes they are the same thing
    /// will put German second where the dashboard puts it fourth.
    pub const fn all() -> &'static [AppLanguage] {
        &[
            AppLanguage::English,
            AppLanguage::German,
            AppLanguage::French,
            AppLanguage::Chinese,
            AppLanguage::Romanian,
        ]
    }

    /// Returns the ISO language code.
    ///
    /// The lower-case form is what the dashboard writes into `ui_settings`;
    /// [`Self::from_code`] accepts either case, because the file is meant to be
    /// editable by hand.
    pub fn code(self) -> &'static str {
        match self {
            AppLanguage::English => "EN",
            AppLanguage::German => "DE",
            AppLanguage::French => "FR",
            AppLanguage::Chinese => "ZH",
            AppLanguage::Romanian => "RO",
        }
    }

    /// Reads the code back.
    ///
    /// Case-insensitive, and anything unrecognised falls back to English: a
    /// hand-edited settings file should land somewhere sensible rather than
    /// leave a reader with no language at all.
    pub fn from_code(code: &str) -> Self {
        match code.trim().to_ascii_uppercase().as_str() {
            "DE" => Self::German,
            "FR" => Self::French,
            "ZH" => Self::Chinese,
            "RO" => Self::Romanian,
            _ => Self::English,
        }
    }
}

impl Display for AppLanguage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Delegates rather than repeating the list, because two lists would be
        // two chances for a language to be spelled one way in the picker and
        // another in the menu.
        f.write_str(self.native_name())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_native_names_are_what_the_language_picker_shows() {
        // `Display` is what `pick_list` renders for both the options and the
        // selected value, so these five strings are the language names a user
        // actually reads. A typo puts a word nobody can check in front of them.
        let shown: Vec<String> = AppLanguage::all().iter().map(|language| language.to_string()).collect();
        assert_eq!(shown, ["English", "Deutsch", "Français", "简体中文", "Română"]);

        // And each one is distinct, so the picker cannot offer two rows reading
        // the same and leave the user guessing which is which.
        let mut sorted = shown.clone();
        sorted.sort();
        sorted.dedup();
        assert_eq!(sorted.len(), shown.len(), "two languages render the same name");
    }

    #[test]
    fn the_picker_order_is_what_the_picker_would_show() {
        // The declaration order is English, Chinese, French, German, Romanian.
        // The picker order is not the same list, and the difference is the whole
        // reason this constant exists -- so it is pinned separately.
        assert_eq!(
            AppLanguage::all(),
            &[
                AppLanguage::English,
                AppLanguage::German,
                AppLanguage::French,
                AppLanguage::Chinese,
                AppLanguage::Romanian
            ],
            "the picker order changed; the declaration order did not"
        );
    }

    #[test]
    fn every_code_survives_the_round_trip() {
        for language in AppLanguage::all() {
            assert_eq!(AppLanguage::from_code(language.code()), *language);
        }
    }

    #[test]
    fn a_hand_edited_code_still_lands_somewhere() {
        // The file is meant to be editable, so both cases and stray whitespace
        // have to work, and anything else has to fall back rather than fail:
        // the widget shows a name for the language, and "no language" is not one
        // of the answers the picker can display.
        assert_eq!(AppLanguage::from_code("de"), AppLanguage::German);
        assert_eq!(AppLanguage::from_code("  zh  "), AppLanguage::Chinese);
        assert_eq!(AppLanguage::from_code("klingon"), AppLanguage::English);
        assert_eq!(AppLanguage::from_code(""), AppLanguage::English);
        assert_eq!(AppLanguage::default(), AppLanguage::English);
    }
}
