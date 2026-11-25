//! English and French for everything the user reads.

/// Interface language.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Lang {
    #[default]
    En,
    Fr,
}

impl Lang {
    /// Reads `SPACETIFY_LANG`, then the system locale. Defaults to English.
    pub fn detect() -> Self {
        std::env::var("SPACETIFY_LANG")
            .ok()
            .and_then(|tag| Self::parse(&tag))
            .or_else(|| sys_locale::get_locale().and_then(|tag| Self::parse(&tag)))
            .unwrap_or_default()
    }

    /// Accepts tags such as `fr`, `fr-FR` or `en_US.UTF-8`.
    pub fn parse(tag: &str) -> Option<Self> {
        let primary = tag.split(['-', '_', '.']).next()?.trim();
        if primary.eq_ignore_ascii_case("en") {
            Some(Self::En)
        } else if primary.eq_ignore_ascii_case("fr") {
            Some(Self::Fr)
        } else {
            None
        }
    }
}

/// Formats the English or French string depending on the language.
///
/// ```
/// use spacetify_core::{tr, Lang};
/// let n = 2;
/// assert_eq!(tr!(Lang::Fr, "{n} backups", "{n} sauvegardes"), "2 sauvegardes");
/// ```
#[macro_export]
macro_rules! tr {
    ($lang:expr, $en:literal, $fr:literal $(, $arg:expr)* $(,)?) => {
        match $lang {
            $crate::Lang::En => format!($en $(, $arg)*),
            $crate::Lang::Fr => format!($fr $(, $arg)*),
        }
    };
}

#[cfg(test)]
mod tests {
    use super::Lang;

    #[test]
    fn parses_common_locale_tags() {
        assert_eq!(Lang::parse("fr"), Some(Lang::Fr));
        assert_eq!(Lang::parse("fr-CA"), Some(Lang::Fr));
        assert_eq!(Lang::parse("en_US.UTF-8"), Some(Lang::En));
        assert_eq!(Lang::parse("de-DE"), None);
        assert_eq!(Lang::parse(""), None);
    }
}
