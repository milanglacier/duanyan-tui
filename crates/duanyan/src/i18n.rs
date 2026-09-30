//! Simplified or traditional Chinese for the text duanyan draws itself.
//!
//! Traditional text follows Taiwan usage (設定檔, 剪貼簿, 送出), which also
//! reads naturally in Hong Kong and Macau. It differs from the simplified
//! text in wording, not only in characters, so both are written out.

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Lang {
    #[default]
    Simplified,
    Traditional,
}

impl Lang {
    pub fn tr(self, simplified: &'static str, traditional: &'static str) -> &'static str {
        match self {
            Self::Simplified => simplified,
            Self::Traditional => traditional,
        }
    }

    /// Picks the language the way gettext picks a message catalog: the
    /// locale from `LC_ALL`, `LC_MESSAGES` or `LANG`, unless `LANGUAGE`
    /// lists a Chinese locale and the locale is not C. Anything that is not
    /// traditional Chinese falls back to simplified.
    pub fn detect(var: impl Fn(&str) -> Option<String>) -> Self {
        let var = |k: &str| var(k).filter(|v| !v.is_empty());
        let locale = ["LC_ALL", "LC_MESSAGES", "LANG"].into_iter().find_map(var);
        let Some(locale) = locale else {
            return Self::Simplified;
        };
        if !matches!(strip_codeset(&locale), "C" | "POSIX")
            && let Some(lang) = var("LANGUAGE").and_then(|l| l.split(':').find_map(parse))
        {
            return lang;
        }
        parse(&locale).unwrap_or_default()
    }
}

/// `zh_TW.UTF-8@mod` -> `zh_TW`.
fn strip_codeset(locale: &str) -> &str {
    locale.split(['.', '@']).next().unwrap_or_default()
}

/// The language of a Chinese locale such as `zh_TW` or `zh-Hant`; `None` for
/// other languages. A script subtag decides over the region.
fn parse(locale: &str) -> Option<Lang> {
    let mut parts = strip_codeset(locale).split(['_', '-']);
    if !parts.next()?.eq_ignore_ascii_case("zh") {
        return None;
    }
    let mut traditional_region = false;
    for part in parts {
        if part.eq_ignore_ascii_case("hant") {
            return Some(Lang::Traditional);
        }
        if part.eq_ignore_ascii_case("hans") {
            return Some(Lang::Simplified);
        }
        if ["tw", "hk", "mo"]
            .iter()
            .any(|r| part.eq_ignore_ascii_case(r))
        {
            traditional_region = true;
        }
    }
    Some(if traditional_region {
        Lang::Traditional
    } else {
        Lang::Simplified
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use Lang::{Simplified as S, Traditional as T};

    fn detect(vars: &[(&str, &str)]) -> Lang {
        Lang::detect(|k| {
            vars.iter()
                .find(|(n, _)| *n == k)
                .map(|(_, v)| v.to_string())
        })
    }

    #[test]
    fn locale_values() {
        for (value, want) in [
            ("zh_TW.UTF-8", T),
            ("zh_HK", T),
            ("zh_MO.Big5", T),
            ("zh-tw", T),
            ("zh_CN.UTF-8", S),
            ("zh_SG", S),
            ("zh", S),
            ("zh-Hant", T),
            ("zh_Hant_TW", T),
            ("zh_Hans_HK", S),
            ("zh_TW@radical", T),
            ("en_US.UTF-8", S),
            ("C", S),
            ("POSIX", S),
            ("", S),
        ] {
            assert_eq!(detect(&[("LANG", value)]), want, "{value:?}");
        }
        assert_eq!(detect(&[]), S);
    }

    #[test]
    fn precedence() {
        assert_eq!(detect(&[("LC_ALL", "zh_TW"), ("LANG", "zh_CN")]), T);
        assert_eq!(detect(&[("LC_ALL", "en_US"), ("LANG", "zh_TW")]), S);
        assert_eq!(detect(&[("LC_MESSAGES", "zh_TW"), ("LANG", "zh_CN")]), T);
        assert_eq!(detect(&[("LC_MESSAGES", "zh_CN"), ("LANG", "zh_TW")]), S);
        // Empty values count as unset.
        assert_eq!(detect(&[("LC_ALL", ""), ("LANG", "zh_TW")]), T);
    }

    #[test]
    fn language_list() {
        let with = |language, lang| detect(&[("LANGUAGE", language), ("LANG", lang)]);
        // The first Chinese entry wins.
        assert_eq!(with("en_US:zh_TW:zh_CN", "en_US.UTF-8"), T);
        assert_eq!(with("zh_CN:zh_TW", "zh_TW.UTF-8"), S);
        // Without a Chinese entry, the locale decides.
        assert_eq!(with("en_US:fr", "zh_TW.UTF-8"), T);
        // gettext ignores LANGUAGE under the C locale.
        assert_eq!(with("zh_TW", "C"), S);
        assert_eq!(with("zh_TW", "C.UTF-8"), S);
        assert_eq!(detect(&[("LANGUAGE", "zh_TW")]), S);
    }
}
