//! Overlay translations.
//!
//! The overlay is a standalone program that must not depend on `ui` — doing so
//! would drag the whole dashboard into the overlay process — so the handful of
//! strings the overlay itself shows live here. The *language* list is the
//! second copy: it also exists in the dashboard, but importing it from `common`
//! is exactly what would tie this binary to WattSeal's source tree. What keeps
//! the two in agreement is the **code** in the `ui_settings` row, which is data.
//! See [`crate::language`] for what that duplication costs and what holds it.

pub use crate::language::AppLanguage as Language;

/// A value that knows how to label itself in a given language.
pub trait Localize {
    fn localize(&self, language: Language) -> &'static str;
}

/// A setting value paired with the language its label should be shown in.
///
/// `pick_list` renders both the options and the selected value through
/// `Display`, so the translated text has to travel with the value. This is the
/// same approach the dashboard uses for its `TranslatedMetricType`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Labeled<T> {
    pub value: T,
    language: Language,
}

impl<T: Localize> Labeled<T> {
    pub fn new(value: T, language: Language) -> Self {
        Self { value, language }
    }
}

impl<T: Localize> std::fmt::Display for Labeled<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.value.localize(self.language))
    }
}

// Menu

pub fn menu_resume(language: Language) -> &'static str {
    match language {
        Language::English => "Resume",
        Language::German => "Weiter",
        Language::French => "Reprendre",
        Language::Chinese => "继续",
        Language::Romanian => "Reia",
    }
}

pub fn menu_settings(language: Language) -> &'static str {
    match language {
        Language::English => "Settings",
        Language::German => "Einstellungen",
        Language::French => "Paramètres",
        Language::Chinese => "设置",
        Language::Romanian => "Setări",
    }
}

pub fn menu_pin(language: Language) -> &'static str {
    match language {
        Language::English => "Pin",
        Language::German => "Anheften",
        Language::French => "Épingler",
        Language::Chinese => "固定",
        Language::Romanian => "Fixează",
    }
}

pub fn menu_unpin(language: Language) -> &'static str {
    match language {
        Language::English => "Unpin",
        Language::German => "Lösen",
        Language::French => "Détacher",
        Language::Chinese => "取消固定",
        Language::Romanian => "Anulează fixarea",
    }
}

pub fn menu_exit(language: Language) -> &'static str {
    match language {
        Language::English => "Exit",
        Language::German => "Beenden",
        Language::French => "Quitter",
        Language::Chinese => "退出",
        Language::Romanian => "Ieșire",
    }
}

// Section titles

pub fn section_appearance(language: Language) -> &'static str {
    match language {
        Language::English => "Appearance",
        Language::German => "Aussehen",
        Language::French => "Apparence",
        Language::Chinese => "外观",
        Language::Romanian => "Aspect",
    }
}

pub fn section_window(language: Language) -> &'static str {
    match language {
        Language::English => "Window",
        Language::German => "Fenster",
        Language::French => "Fenêtre",
        Language::Chinese => "窗口",
        Language::Romanian => "Fereastră",
    }
}

pub fn section_content(language: Language) -> &'static str {
    match language {
        Language::English => "Content",
        Language::German => "Inhalt",
        Language::French => "Contenu",
        Language::Chinese => "内容",
        Language::Romanian => "Conținut",
    }
}

// Settings

pub fn label_opacity(language: Language) -> &'static str {
    match language {
        Language::English => "Opacity",
        Language::German => "Deckkraft",
        Language::French => "Opacité",
        Language::Chinese => "不透明度",
        Language::Romanian => "Opacitate",
    }
}

pub fn label_bg_color(language: Language) -> &'static str {
    match language {
        Language::English => "Bg color",
        Language::German => "Hintergrund",
        Language::French => "Couleur de fond",
        Language::Chinese => "背景色",
        Language::Romanian => "Culoare fundal",
    }
}

pub fn label_text_color(language: Language) -> &'static str {
    match language {
        Language::English => "Text color",
        Language::German => "Textfarbe",
        Language::French => "Couleur du texte",
        Language::Chinese => "文字颜色",
        Language::Romanian => "Culoare text",
    }
}

pub fn label_transparency(language: Language) -> &'static str {
    match language {
        Language::English => "Transparency",
        Language::German => "Transparenz",
        Language::French => "Transparence",
        Language::Chinese => "透明度",
        Language::Romanian => "Transparență",
    }
}

pub fn label_shadow(language: Language) -> &'static str {
    match language {
        Language::English => "Drop shadow",
        Language::German => "Schlagschatten",
        Language::French => "Ombre portée",
        Language::Chinese => "投影",
        Language::Romanian => "Umbră",
    }
}

pub fn label_layout(language: Language) -> &'static str {
    match language {
        Language::English => "Layout",
        Language::German => "Anordnung",
        Language::French => "Disposition",
        Language::Chinese => "布局",
        Language::Romanian => "Aranjare",
    }
}

pub fn label_density(language: Language) -> &'static str {
    match language {
        Language::English => "Density",
        Language::German => "Dichte",
        Language::French => "Densité",
        Language::Chinese => "密度",
        Language::Romanian => "Densitate",
    }
}

pub fn label_text_size(language: Language) -> &'static str {
    match language {
        Language::English => "Text size",
        Language::German => "Textgröße",
        Language::French => "Taille du texte",
        Language::Chinese => "文字大小",
        Language::Romanian => "Mărime text",
    }
}

pub fn label_decimals(language: Language) -> &'static str {
    match language {
        Language::English => "Decimals",
        Language::German => "Nachkommastellen",
        Language::French => "Décimales",
        Language::Chinese => "小数位",
        Language::Romanian => "Zecimale",
    }
}

pub fn label_refresh(language: Language) -> &'static str {
    match language {
        Language::English => "Refresh",
        Language::German => "Aktualisierung",
        Language::French => "Rafraîchissement",
        Language::Chinese => "刷新",
        Language::Romanian => "Reîmprospătare",
    }
}

pub fn label_show_labels(language: Language) -> &'static str {
    match language {
        Language::English => "Show labels",
        Language::German => "Beschriftungen zeigen",
        Language::French => "Afficher les libellés",
        Language::Chinese => "显示名称",
        Language::Romanian => "Afișează etichetele",
    }
}

pub fn label_show_units(language: Language) -> &'static str {
    match language {
        Language::English => "Show units",
        Language::German => "Einheiten zeigen",
        Language::French => "Afficher les unités",
        Language::Chinese => "显示单位",
        Language::Romanian => "Afișează unitățile",
    }
}

pub fn label_short_labels(language: Language) -> &'static str {
    match language {
        Language::English => "Short labels (Total→T, CPU→C…)",
        Language::German => "Kurze Beschriftungen (Total→T, CPU→C…)",
        Language::French => "Libellés courts (Total→T, CPU→C…)",
        Language::Chinese => "缩写标签（总计→总、CPU→核…）",
        Language::Romanian => "Etichete scurte (Total→T, CPU→C…)",
    }
}

pub fn label_always_on_top(language: Language) -> &'static str {
    match language {
        Language::English => "Always on top",
        Language::German => "Immer im Vordergrund",
        Language::French => "Toujours au-dessus",
        Language::Chinese => "窗口置顶",
        Language::Romanian => "Mereu deasupra",
    }
}

pub fn label_pin_click_through(language: Language) -> &'static str {
    match language {
        Language::English => "Pin makes it click-through",
        Language::German => "Anheften macht klickdurchlässig",
        Language::French => "Épingler rend transparent aux clics",
        Language::Chinese => "固定时鼠标穿透",
        Language::Romanian => "Fixarea permite clicurile prin fereastră",
    }
}

/// Shown before the pixel value, which the caller appends.
pub fn label_width(language: Language) -> &'static str {
    match language {
        Language::English => "Width",
        Language::German => "Breite",
        Language::French => "Largeur",
        Language::Chinese => "宽度",
        Language::Romanian => "Lățime",
    }
}

pub fn label_top_count(language: Language) -> &'static str {
    match language {
        Language::English => "Top count",
        Language::German => "Anzahl Einträge",
        Language::French => "Nombre d'entrées",
        Language::Chinese => "显示数量",
        Language::Romanian => "Număr de intrări",
    }
}

pub fn button_done(language: Language) -> &'static str {
    match language {
        Language::English => "Done",
        Language::German => "Fertig",
        Language::French => "Terminé",
        Language::Chinese => "完成",
        Language::Romanian => "Gata",
    }
}

pub fn button_quit_overlay(language: Language) -> &'static str {
    match language {
        Language::English => "Quit overlay",
        Language::German => "Overlay beenden",
        Language::French => "Quitter la superposition",
        Language::Chinese => "退出悬浮窗",
        Language::Romanian => "Închide suprapunerea",
    }
}

// Hints

pub fn hint_pin_unavailable(language: Language) -> &'static str {
    match language {
        Language::English => "Pin locks the position here: click-through is not available on this platform.",
        Language::German => {
            "Anheften sperrt hier die Position: Klickdurchlässigkeit gibt es auf dieser Plattform nicht."
        }
        Language::French => {
            "Épingler verrouille la position ici : la transparence aux clics n'existe pas sur cette plateforme."
        }
        Language::Chinese => "此平台仅锁定位置：不支持鼠标穿透。",
        Language::Romanian => {
            "Fixarea blochează poziția aici: trecerea clicurilor nu este disponibilă pe această platformă."
        }
    }
}

pub fn hint_shadow_unavailable(language: Language) -> &'static str {
    match language {
        Language::English => {
            "This transparency mode has no per-pixel alpha for a shadow to fade into, so the card \
             stays flat."
        }
        Language::German => {
            "Dieser Transparenzmodus hat kein Alpha pro Pixel, in das ein Schatten auslaufen \
             könnte, die Karte bleibt also flach."
        }
        Language::French => {
            "Ce mode de transparence n'a pas d'alpha par pixel dans lequel une ombre puisse \
             s'estomper : la carte reste plate."
        }
        Language::Chinese => "当前透明模式没有可让阴影淡出的逐像素 alpha，因此卡片保持无阴影。",
        Language::Romanian => {
            "Acest mod de transparență nu are alpha pe pixel în care o umbră să se estompeze, așa \
             că cardul rămâne plat."
        }
    }
}

pub fn hint_opacity_layered(language: Language) -> &'static str {
    match language {
        Language::English => {
            "One alpha covers the whole window, so text fades with the card. This GPU exposes no \
             alpha-capable surface, so only the card's color (not its alpha) is adjustable."
        }
        Language::German => {
            "Ein Alpha gilt für das ganze Fenster, der Text verblasst also mit der Karte. Diese GPU \
             bietet keine alpha-fähige Oberfläche, daher lässt sich nur die Farbe der Karte \
             einstellen, nicht ihr Alpha."
        }
        Language::French => {
            "Un seul alpha couvre toute la fenêtre : le texte s'estompe avec la carte. Ce GPU \
             n'expose aucune surface compatible alpha, seule la couleur de la carte est réglable, \
             pas son alpha."
        }
        Language::Chinese => {
            "整窗共用一个 alpha，文字会随卡片一起变淡。此 GPU 不提供支持 alpha 的绘制表面，\
             因此只能调整卡片颜色，无法调整其透明度。"
        }
        Language::Romanian => {
            "Un singur alpha acoperă toată fereastra, așa că textul se estompează odată cu cardul. \
             Acest GPU nu expune o suprafață compatibilă cu alpha, deci doar culoarea cardului este \
             reglabilă, nu și alpha."
        }
    }
}

pub fn hint_opacity_surface(language: Language) -> &'static str {
    match language {
        Language::English => "Surface: card and text alphas are independent.",
        Language::German => "Oberfläche: die Alphas von Karte und Text sind unabhängig.",
        Language::French => "Surface : les alphas de la carte et du texte sont indépendants.",
        Language::Chinese => "Surface 模式：卡片与文字的透明度相互独立。",
        Language::Romanian => "Suprafață: alpha-urile cardului și ale textului sunt independente.",
    }
}

pub fn hint_opacity_off(language: Language) -> &'static str {
    match language {
        Language::English => "Transparency is off — the overlay is fully opaque.",
        Language::German => "Transparenz ist aus — das Overlay ist vollständig deckend.",
        Language::French => "La transparence est désactivée : la superposition est opaque.",
        Language::Chinese => "透明度已关闭 —— 悬浮窗完全不透明。",
        Language::Romanian => "Transparența este oprită — suprapunerea este complet opacă.",
    }
}

// Metric names

pub fn metric_name(language: Language, metric: crate::config::Metric) -> &'static str {
    use crate::config::Metric;

    match metric {
        Metric::Total => match language {
            Language::English => "Total",
            Language::German => "Gesamt",
            Language::French => "Total",
            Language::Chinese => "总计",
            Language::Romanian => "Total",
        },
        Metric::Cpu => "CPU",
        Metric::Gpu => "GPU",
        Metric::Ram => "RAM",
        Metric::Disk => match language {
            Language::English => "Disk",
            Language::German => "Festplatte",
            Language::French => "Disque",
            Language::Chinese => "磁盘",
            Language::Romanian => "Disc",
        },
        Metric::Network => match language {
            Language::English => "Net",
            Language::German => "Netz",
            Language::French => "Réseau",
            Language::Chinese => "网络",
            Language::Romanian => "Rețea",
        },
        Metric::TopApps => match language {
            Language::English => "Top",
            Language::German => "Top",
            Language::French => "Top",
            Language::Chinese => "应用",
            Language::Romanian => "Top",
        },
    }
}

/// The `Top apps` entry in the Content list, longer than the in-widget label.
pub fn metric_top_apps_setting(language: Language) -> &'static str {
    match language {
        Language::English => "Top apps",
        Language::German => "Top-Apps",
        Language::French => "Applications principales",
        Language::Chinese => "应用排行",
        Language::Romanian => "Aplicații principale",
    }
}

/// The single-glyph label used by the `Short labels` mode.
pub fn metric_short_name(language: Language, metric: crate::config::Metric) -> &'static str {
    use crate::config::Metric;

    match language {
        // One Latin initial, exactly as the `Short labels` setting describes it.
        Language::English | Language::German | Language::French | Language::Romanian => metric.short_label(),
        // A Chinese reader gets more from one character than from a Latin initial.
        Language::Chinese => match metric {
            Metric::Total => "总",
            Metric::Cpu => "核",
            Metric::Gpu => "显",
            Metric::Ram => "存",
            Metric::Disk => "盘",
            Metric::Network => "网",
            Metric::TopApps => "应用",
        },
    }
}

impl Localize for crate::config::Metric {
    fn localize(&self, language: Language) -> &'static str {
        metric_name(language, *self)
    }
}

// Setting values (pick-list options)

impl Localize for crate::config::Layout {
    fn localize(&self, language: Language) -> &'static str {
        use crate::config::Layout;

        match self {
            Layout::Vertical => match language {
                Language::English | Language::French => "Vertical",
                Language::German => "Vertikal",
                Language::Chinese => "纵向",
                Language::Romanian => "Vertical",
            },
            Layout::Horizontal => match language {
                Language::English | Language::French | Language::German => "Horizontal",
                Language::Chinese => "横向",
                Language::Romanian => "Orizontal",
            },
        }
    }
}

impl Localize for crate::config::Density {
    fn localize(&self, language: Language) -> &'static str {
        use crate::config::Density;

        match self {
            Density::Ultra => match language {
                Language::Chinese => "极紧凑",
                _ => "Ultra",
            },
            Density::Compact => match language {
                Language::German => "Kompakt",
                Language::Chinese => "紧凑",
                _ => "Compact",
            },
            Density::Normal => match language {
                Language::Chinese => "普通",
                _ => "Normal",
            },
        }
    }
}

impl Localize for crate::config::FontSize {
    fn localize(&self, language: Language) -> &'static str {
        use crate::config::FontSize;

        match self {
            FontSize::Small => match language {
                Language::English => "Small",
                Language::German => "Klein",
                Language::French => "Petite",
                Language::Chinese => "小",
                Language::Romanian => "Mic",
            },
            FontSize::Medium => match language {
                Language::English => "Medium",
                Language::German => "Mittel",
                Language::French => "Moyenne",
                Language::Chinese => "中",
                Language::Romanian => "Mediu",
            },
            FontSize::Large => match language {
                Language::English => "Large",
                Language::German => "Groß",
                Language::French => "Grande",
                Language::Chinese => "大",
                Language::Romanian => "Mare",
            },
        }
    }
}

impl Localize for crate::config::Transparency {
    fn localize(&self, language: Language) -> &'static str {
        use crate::config::Transparency;

        match self {
            Transparency::Auto => match language {
                Language::German => "Automatisch",
                Language::Chinese => "自动",
                Language::Romanian => "Automat",
                _ => "Auto",
            },
            Transparency::Layered => match language {
                Language::English => "Layered",
                Language::German => "Ebenenfenster",
                Language::French => "Fenêtre en couches",
                Language::Chinese => "分层窗口",
                Language::Romanian => "Fereastră stratificată",
            },
            Transparency::Off => match language {
                Language::English => "Off",
                Language::German => "Aus",
                Language::French => "Désactivée",
                Language::Chinese => "关闭",
                Language::Romanian => "Oprit",
            },
        }
    }
}

impl Localize for crate::language::AppLanguage {
    /// Always the native name, whatever the panel's language is.
    ///
    /// This is the one pick-list on the panel that is deliberately *not*
    /// translated, and the reason is the one that matters when you cannot read
    /// the panel: the list is how you get out of a language you do not read. The
    /// other pickers name colours and layouts; this one names the languages, and
    /// a language is only findable in its own name.
    fn localize(&self, _language: Language) -> &'static str {
        self.native_name()
    }
}

impl Localize for crate::theme::ThemeChoice {
    fn localize(&self, language: Language) -> &'static str {
        use crate::theme::ThemeChoice;

        match self {
            // "Automatic" is the state that follows the dashboard, which is the
            // default — it says what the widget is doing rather than what the
            // dashboard happens to be set to right now.
            ThemeChoice::Auto => match language {
                Language::German => "Automatisch",
                Language::French => "Automatique",
                Language::Chinese => "自动",
                Language::Romanian => "Automat",
                _ => "Automatic",
            },
            ThemeChoice::Dark => match language {
                Language::German => "Dunkel",
                Language::French => "Sombre",
                Language::Chinese => "深色",
                Language::Romanian => "Întunecat",
                _ => "Dark",
            },
            ThemeChoice::Light => match language {
                Language::German => "Hell",
                Language::French => "Clair",
                Language::Chinese => "浅色",
                Language::Romanian => "Luminos",
                _ => "Light",
            },
        }
    }
}

/// The label for the language picker itself.
///
/// Shown in the language the picker is *currently* in, which is the only one
/// guaranteed to be readable: a user who cannot read the panel cannot find the
/// row that fixes it.
pub fn label_language(language: Language) -> &'static str {
    match language {
        Language::German => "Sprache",
        Language::French => "Langue",
        Language::Chinese => "语言",
        Language::Romanian => "Limbă",
        _ => "Language",
    }
}

/// A hint under the language row, saying where the current choice comes from.
///
/// The point is that a language that silently changed because a dashboard opened
/// looks like a bug; saying "following WattSeal" turns the same event into a
/// sentence the user can act on.
pub fn hint_language_follows_dashboard(language: Language) -> &'static str {
    match language {
        Language::German => "Folgt der WattSeal-Einstellung",
        Language::French => "Suit le réglage de WattSeal",
        Language::Chinese => "跟随 WattSeal 的设置",
        Language::Romanian => "Urmește setarea din WattSeal",
        _ => "Following WattSeal's setting",
    }
}

/// The counterpart hint for a language the user has pinned here.
///
/// Saying so matters more than for the automatic case: the user *did* choose
/// this, and the next thing that happens is WattSeal reporting a different
/// language and being ignored. Silently ignoring it would read as a bug.
pub fn hint_language_override(language: Language) -> &'static str {
    match language {
        Language::German => "Überschreibt die WattSeal-Einstellung",
        Language::French => "Remplace le réglage de WattSeal",
        Language::Chinese => "覆盖 WattSeal 的设置",
        Language::Romanian => "Suprascrie setarea din WattSeal",
        _ => "Overrides WattSeal's setting",
    }
}

/// There is no WattSeal beside the executable to start.
///
/// Says what to do about it rather than only what is wrong, because the reader of
/// this line is looking at a widget that shows nothing and has just been told
/// nothing is wrong — the second half is what turns a sentence into a way out.
pub fn status_not_installed(language: Language) -> &'static str {
    match language {
        Language::German => "WattSeal nicht gefunden — neben der Datei ablegen",
        Language::French => "WattSeal introuvable — à placer à côté du fichier",
        Language::Chinese => "没找到 WattSeal — 请放到本程序旁边",
        Language::Romanian => "WattSeal negăsit — puneți-l lângă fișier",
        _ => "WattSeal not found — put it next to this file",
    }
}

/// A collector was started and has not produced its first sample yet.
///
/// Present tense and no progress bar, because there is nothing to report progress
/// about — it either starts or it does not, and the next state says which.
pub fn status_starting(language: Language) -> &'static str {
    match language {
        Language::German => "WattSeal wird gestartet …",
        Language::French => "Démarrage de WattSeal …",
        Language::Chinese => "正在启动 WattSeal …",
        Language::Romanian => "Se pornește WattSeal …",
        _ => "Starting WattSeal …",
    }
}

/// Launching is switched off and there is nothing to read.
///
/// Names the setting, because the setting is why: without saying so the same
/// screen appears for a missing database and for a deliberate choice, and the
/// user has no way to tell which they are looking at.
pub fn status_launch_disabled(language: Language) -> &'static str {
    match language {
        Language::German => "WattSeal läuft nicht — Starten ist in den Einstellungen aus",
        Language::French => "WattSeal n'est pas lancé — démarrage désactivé dans les réglages",
        Language::Chinese => "WattSeal 未运行 — 设置里关掉了自动启动",
        Language::Romanian => "WattSeal nu rulează — pornirea este dezactivată în setări",
        _ => "WattSeal is not running — launching is off in the settings",
    }
}

/// The collector has stopped writing to a database that still exists.
///
/// Deliberately not showing how long: a number that counts up beside a frozen
/// reading is a second thing to keep an eye on, and the sentence has already
/// said the only useful part.
pub fn status_stalled(language: Language) -> &'static str {
    match language {
        Language::German => "WattSeal schreibt keine Werte mehr",
        Language::French => "WattSeal n'écrit plus de mesures",
        Language::Chinese => "WattSeal 已停止写入读数",
        Language::Romanian => "WattSeal nu mai scrie măsurători",
        _ => "WattSeal has stopped writing readings",
    }
}

/// The database is at a generation this build does not read.
///
/// Shown next to numbers that *are* being shown, so it has to read as a note
/// rather than as an error: the figures below it are real, and hiding them would
/// be a bigger lie than saying where they came from.
pub fn status_foreign_generation(language: Language) -> &'static str {
    match language {
        Language::German => "Neuere Datenbank — Zahlen bleiben, Bezeichnungen nicht",
        Language::French => "Base plus récente — chiffres affichés, libellés non",
        Language::Chinese => "数据库较新 — 数字照常，标签不可用",
        Language::Romanian => "Bază mai nouă — cifrele rămân, etichetele nu",
        _ => "Newer database — numbers kept, labels unavailable",
    }
}

/// The shortcut could not be registered — usually because another program has it.
///
/// No key is quoted, because there is none to quote; that is the point of saying
/// this instead of naming one that would do nothing.
pub fn hint_escape_unavailable(language: Language) -> &'static str {
    match language {
        Language::German => "Kein Tastenkürzel verfügbar — im overlay_config.json lösen",
        Language::French => "Aucun raccourci disponible — libérer dans overlay_config.json",
        Language::Chinese => "快捷键不可用 — 在 overlay_config.json 里解开",
        Language::Romanian => "Nicio tastă rapidă — eliberează în overlay_config.json",
        _ => "No shortcut available — release it in overlay_config.json",
    }
}

/// Shown while the widget is pinned with click-through on.
///
/// **The widget cannot be clicked, so this is the only way it can help.** The
/// escape hatch used to be the dashboard's hide-and-show, which released the pin
/// from another process; there is no such process now, and restarting does not
/// help because the pin is written to the config file. So the stuck widget has to
/// say so itself, in words it can still be *read* — the one thing a
/// click-through window can still do.
pub fn hint_pinned_escape(language: Language) -> &'static str {
    match language {
        Language::German => "Angeheftet · in overlay_config.json pin_mode auf false",
        Language::French => "Épinglé · dans overlay_config.json, pin_mode à false",
        Language::Chinese => "已固定 · 在 overlay_config.json 里把 pin_mode 设为 false",
        Language::Romanian => "Fixat · în overlay_config.json, pin_mode pe false",
        _ => "Pinned · set pin_mode false in overlay_config.json",
    }
}

/// The button that starts listening for a new shortcut.
pub fn hotkey_rebind(language: Language, current: &str) -> String {
    match language {
        Language::German => format!("Tastenkürzel ändern ({current})"),
        Language::French => format!("Changer le raccourci ({current})"),
        Language::Chinese => format!("更改快捷键（现在是 {current}）"),
        Language::Romanian => format!("Schimbă scurtătura ({current})"),
        _ => format!("Change shortcut (now {current})"),
    }
}

/// What the panel says while it is waiting for a key.
///
/// Says how to get out as well as how to get in: a capture mode with no stated
/// exit is the same trap this whole feature exists to undo.
pub fn hotkey_capture_prompt(language: Language) -> &'static str {
    match language {
        Language::German => "Neue Tastenkombination drücken · Esc bricht ab",
        Language::French => "Appuyez sur la nouvelle combinaison · Échap pour annuler",
        Language::Chinese => "请按下新的组合键 · 按 Esc 取消",
        Language::Romanian => "Apăsați noua combinație · Esc anulează",
        _ => "Press the new combination · Esc cancels",
    }
}

/// Why the combination just pressed cannot be used.
pub fn hotkey_capture_refused(language: Language, reason: &str) -> String {
    // `reason` comes from the platform and is English; it is quoted rather than
    // translated because only two sentences exist and both name keys, which are
    // not translated either.
    let prefix = match language {
        Language::German => "Nicht möglich:",
        Language::French => "Impossible :",
        Language::Chinese => "不可以：",
        Language::Romanian => "Imposibil:",
        _ => "Cannot use that:",
    };
    format!("{prefix} {reason}")
}

/// The button that leaves capture without binding anything.
pub fn hotkey_cancel(language: Language) -> &'static str {
    match language {
        Language::German => "Abbrechen",
        Language::French => "Annuler",
        Language::Chinese => "取消",
        Language::Romanian => "Anulează",
        _ => "Cancel",
    }
}

/// The label for the theme picker.
pub fn label_theme(language: Language) -> &'static str {
    match language {
        Language::German => "Farbschema",
        Language::French => "Thème",
        Language::Chinese => "主题",
        Language::Romanian => "Temă",
        _ => "Theme",
    }
}

impl Localize for crate::config::BgColor {
    fn localize(&self, language: Language) -> &'static str {
        use crate::config::BgColor;

        match self {
            BgColor::Auto => match language {
                Language::German => "Automatisch",
                Language::Chinese => "自动",
                Language::Romanian => "Automat",
                _ => "Auto",
            },
            BgColor::Slate => match language {
                Language::English => "Slate",
                Language::German => "Schiefer",
                Language::French => "Ardoise",
                Language::Chinese => "石板",
                Language::Romanian => "Ardezie",
            },
            BgColor::Graphite => match language {
                Language::English | Language::French => "Graphite",
                Language::German => "Graphit",
                Language::Chinese => "石墨",
                Language::Romanian => "Grafit",
            },
            BgColor::Navy => match language {
                Language::English => "Navy",
                Language::German => "Marine",
                Language::French => "Marine",
                Language::Chinese => "海军蓝",
                Language::Romanian => "Bleumarin",
            },
            BgColor::Plum => match language {
                Language::English => "Plum",
                Language::German => "Pflaume",
                Language::French => "Prune",
                Language::Chinese => "梅子",
                Language::Romanian => "Prun",
            },
            BgColor::Forest => match language {
                Language::English => "Forest",
                Language::German => "Wald",
                Language::French => "Forêt",
                Language::Chinese => "森林",
                Language::Romanian => "Pădure",
            },
            BgColor::Sand => match language {
                Language::English => "Sand",
                Language::German => "Sand",
                Language::French => "Sable",
                Language::Chinese => "沙色",
                Language::Romanian => "Nisip",
            },
            BgColor::White => match language {
                Language::English => "White",
                Language::German => "Weiß",
                Language::French => "Blanc",
                Language::Chinese => "白色",
                Language::Romanian => "Alb",
            },
        }
    }
}

impl Localize for crate::config::TextColor {
    fn localize(&self, language: Language) -> &'static str {
        use crate::config::TextColor;

        match self {
            TextColor::Auto => match language {
                Language::German => "Automatisch",
                Language::Chinese => "自动",
                Language::Romanian => "Automat",
                _ => "Auto",
            },
            TextColor::White => match language {
                Language::English => "White",
                Language::German => "Weiß",
                Language::French => "Blanc",
                Language::Chinese => "白色",
                Language::Romanian => "Alb",
            },
            TextColor::Silver => match language {
                Language::English => "Silver",
                Language::German => "Silber",
                Language::French => "Argent",
                Language::Chinese => "银色",
                Language::Romanian => "Argintiu",
            },
            TextColor::Cyan => match language {
                Language::English | Language::French | Language::Romanian => "Cyan",
                Language::German => "Türkis",
                Language::Chinese => "青色",
            },
            TextColor::Green => match language {
                Language::English => "Green",
                Language::German => "Grün",
                Language::French => "Vert",
                Language::Chinese => "绿色",
                Language::Romanian => "Verde",
            },
            TextColor::Amber => match language {
                Language::English => "Amber",
                Language::German => "Bernstein",
                Language::French => "Ambre",
                Language::Chinese => "琥珀",
                Language::Romanian => "Chihlimbar",
            },
            TextColor::Red => match language {
                Language::English => "Red",
                Language::German => "Rot",
                Language::French => "Rouge",
                Language::Chinese => "红色",
                Language::Romanian => "Roșu",
            },
            TextColor::Ink => match language {
                Language::English => "Ink",
                Language::German => "Tinte",
                Language::French => "Encre",
                Language::Chinese => "墨黑",
                Language::Romanian => "Cerneală",
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{BgColor, Density, FontSize, Layout, Metric, TextColor, Transparency};

    /// Every language the application ships, so that adding one cannot quietly
    /// leave the checks below half-done. Taken from the shared list rather than
    /// written out again, for the reason in the module docs.
    const LANGUAGES: &[Language] = Language::all();

    #[test]
    fn the_languages_under_test_are_the_ones_the_dashboard_ships() {
        // `all()` is the single source of truth, so this is really a check that
        // the list is not empty and has no duplicates to hide a gap behind it.
        assert!(!LANGUAGES.is_empty());
        let mut codes: Vec<&str> = LANGUAGES.iter().map(|language| language.code()).collect();
        let count = codes.len();
        codes.sort_unstable();
        codes.dedup();
        assert_eq!(codes.len(), count, "two languages share a stored code");
    }

    #[test]
    fn wording_matches_the_dashboard_where_the_setting_already_exists() {
        assert_eq!(menu_settings(Language::German), "Einstellungen");
        assert_eq!(menu_settings(Language::Chinese), "设置");
        assert_eq!(metric_name(Language::German, Metric::Total), "Gesamt");
        assert_eq!(metric_name(Language::Chinese, Metric::Network), "网络");
    }

    #[test]
    fn a_pick_list_option_renders_its_translation() {
        assert_eq!(Labeled::new(BgColor::Navy, Language::Chinese).to_string(), "海军蓝");
        assert_eq!(Labeled::new(FontSize::Large, Language::German).to_string(), "Groß");
    }

    #[test]
    fn every_option_is_labelled_in_every_language() {
        // Every assertion here names the language and the option it failed on.
        // Without that, the only thing a failure says is "somewhere in a nested
        // loop of five languages and sixty options", which is the whole table to
        // search by hand.
        fn labelled(text: &str, language: Language, what: &str) {
            assert!(!text.trim().is_empty(), "{what} has no text for {language}");
        }

        for &language in LANGUAGES {
            for &color in BgColor::ALL {
                labelled(color.localize(language), language, "background colour");
            }
            for &color in TextColor::ALL {
                labelled(color.localize(language), language, "text colour");
            }
            for &layout in Layout::ALL {
                labelled(layout.localize(language), language, "layout");
            }
            for &density in Density::ALL {
                labelled(density.localize(language), language, "density");
            }
            for &size in FontSize::ALL {
                labelled(size.localize(language), language, "text size");
            }
            for &mode in Transparency::ALL {
                labelled(mode.localize(language), language, "transparency mode");
            }
            for &metric in Metric::ALL {
                labelled(metric.localize(language), language, "metric");
                labelled(metric_short_name(language, metric), language, "metric short name");
            }
        }
    }

    #[test]
    fn no_label_is_the_empty_string_in_any_language() {
        // A missing translation used to show up as a blank row rather than as a
        // failure, so the per-language tables are swept for empties. `LANGUAGES`
        // is the dashboard's own list, so a language added there is swept here.
        fn check(text: &'static str, what: &str) {
            assert!(!text.trim().is_empty(), "{what} has no text for a language");
        }

        for &language in LANGUAGES {
            check(menu_resume(language), "menu resume");
            check(menu_settings(language), "menu settings");
            check(menu_pin(language), "menu pin");
            check(menu_unpin(language), "menu unpin");
            check(menu_exit(language), "menu exit");
            check(section_appearance(language), "section appearance");
            check(section_window(language), "section window");
            check(section_content(language), "section content");
            check(label_opacity(language), "opacity");
            check(label_bg_color(language), "background colour");
            check(label_text_color(language), "text colour");
            check(label_transparency(language), "transparency");
            check(label_shadow(language), "shadow");
            check(label_layout(language), "layout");
            check(label_density(language), "density");
            check(label_text_size(language), "text size");
            check(label_decimals(language), "decimals");
            check(label_refresh(language), "refresh");
            check(label_show_labels(language), "show labels");
            check(label_show_units(language), "show units");
            check(label_short_labels(language), "short labels");
            check(label_always_on_top(language), "always on top");
            check(label_pin_click_through(language), "click-through");
            check(label_width(language), "width");
            check(label_top_count(language), "top-app count");
            check(button_done(language), "done");
            check(button_quit_overlay(language), "quit");
            check(hint_pin_unavailable(language), "pin hint");
            check(hint_shadow_unavailable(language), "shadow hint");
            check(hint_opacity_layered(language), "layered opacity hint");
            check(hint_opacity_surface(language), "surface opacity hint");
            check(hint_opacity_off(language), "opaque opacity hint");
            check(metric_top_apps_setting(language), "top apps setting");
            for &metric in Metric::ALL {
                check(metric_name(language, metric), "metric name");
                check(metric_short_name(language, metric), "metric short name");
            }
        }
    }
}
