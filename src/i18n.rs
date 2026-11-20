use std::cell::Cell;

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq, Serialize)]
pub enum Language {
    #[default]
    SimplifiedChinese,
    TraditionalChinese,
    English,
}

impl Language {
    pub const ALL: [Self; 3] = [
        Self::SimplifiedChinese,
        Self::TraditionalChinese,
        Self::English,
    ];
    pub const LABELS: [&str; 3] = ["简体中文", "繁體中文", "English"];

    pub fn text(
        self,
        simplified: &'static str,
        traditional: &'static str,
        english: &'static str,
    ) -> &'static str {
        match self {
            Self::SimplifiedChinese => simplified,
            Self::TraditionalChinese => traditional,
            Self::English => english,
        }
    }
}

thread_local! {
    static LANGUAGE: Cell<Language> = const { Cell::new(Language::SimplifiedChinese) };
}

pub fn language() -> Language {
    LANGUAGE.with(Cell::get)
}

pub fn set(language: Language) {
    LANGUAGE.with(|current| current.set(language));
}

macro_rules! tr {
    ($simplified:literal, $traditional:literal, $english:literal) => {
        $crate::i18n::language().text($simplified, $traditional, $english)
    };
}

macro_rules! tr_format {
    ($simplified:literal, $traditional:literal, $english:literal $(, $args:expr)* $(,)?) => {
        match $crate::i18n::language() {
            $crate::i18n::Language::SimplifiedChinese => format!($simplified $(, $args)*),
            $crate::i18n::Language::TraditionalChinese => format!($traditional $(, $args)*),
            $crate::i18n::Language::English => format!($english $(, $args)*),
        }
    };
}
