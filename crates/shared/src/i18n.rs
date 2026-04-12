use std::collections::HashMap;

/// Supported languages.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Language {
    English,
    French,
}

impl Language {
    pub fn from_code(code: &str) -> Self {
        match code.to_lowercase().as_str() {
            "fr" | "fr_fr" | "french" => Language::French,
            _ => Language::English,
        }
    }

    pub fn code(&self) -> &'static str {
        match self {
            Language::English => "en",
            Language::French => "fr",
        }
    }

    /// Auto-detect language from environment ($LANG, $LC_ALL).
    pub fn detect() -> Self {
        for var in ["LC_ALL", "LANG", "LANGUAGE"] {
            if let Ok(val) = std::env::var(var) {
                let lower = val.to_lowercase();
                if lower.starts_with("fr") {
                    return Language::French;
                }
            }
        }
        Language::English
    }
}

/// A flat key-value translator loaded from TOML.
///
/// Keys use dot notation: "login.title", "game.your_turn", etc.
pub struct Translator {
    translations: HashMap<String, String>,
    language: Language,
}

impl Translator {
    /// Load translations from a TOML string.
    /// The TOML is expected to have sections like [login], [game], etc.
    /// Keys are flattened to "section.key" format.
    pub fn from_toml(toml_str: &str, language: Language) -> Self {
        let table: toml::Table = toml::from_str(toml_str).unwrap_or_default();
        let mut translations = HashMap::new();
        flatten_table(&table, "", &mut translations);
        Self {
            translations,
            language,
        }
    }

    /// Load translations for a language from embedded strings.
    pub fn new(language: Language) -> Self {
        let toml_str = match language {
            Language::English => include_str!("../../../i18n/en.toml"),
            Language::French => include_str!("../../../i18n/fr.toml"),
        };
        Self::from_toml(toml_str, language)
    }

    /// Get a translation by key. Returns the key itself if not found.
    pub fn get<'a>(&'a self, key: &'a str) -> &'a str {
        self.translations
            .get(key)
            .map(|s| s.as_str())
            .unwrap_or(key)
    }

    /// Get a translation with positional arguments replaced.
    /// Replaces {0}, {1}, etc. with the provided args.
    pub fn get_fmt(&self, key: &str, args: &[&str]) -> String {
        let mut result = self.get(key).to_string();
        for (i, arg) in args.iter().enumerate() {
            result = result.replace(&format!("{{{i}}}"), arg);
        }
        result
    }

    pub fn language(&self) -> Language {
        self.language
    }
}

/// Flatten a TOML table into dot-notation keys.
fn flatten_table(table: &toml::Table, prefix: &str, out: &mut HashMap<String, String>) {
    for (key, value) in table {
        let full_key = if prefix.is_empty() {
            key.clone()
        } else {
            format!("{prefix}.{key}")
        };
        match value {
            toml::Value::String(s) => {
                out.insert(full_key, s.clone());
            }
            toml::Value::Table(t) => {
                flatten_table(t, &full_key, out);
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn load_english() {
        let t = Translator::new(Language::English);
        assert_eq!(t.get("app.title"), "Game Center");
        assert_eq!(t.get("login.title"), "Login");
        assert_eq!(t.get("game.your_turn"), "Your turn");
    }

    #[test]
    fn load_french() {
        let t = Translator::new(Language::French);
        assert_eq!(t.get("app.title"), "Centre de Jeux");
        assert_eq!(t.get("login.title"), "Connexion");
    }

    #[test]
    fn format_args() {
        let t = Translator::new(Language::English);
        let result = t.get_fmt("lobby.players", &["3", "4"]);
        assert_eq!(result, "3/4 players");
    }

    #[test]
    fn missing_key_returns_key() {
        let t = Translator::new(Language::English);
        assert_eq!(t.get("nonexistent.key"), "nonexistent.key");
    }

    #[test]
    fn language_from_code() {
        assert_eq!(Language::from_code("fr"), Language::French);
        assert_eq!(Language::from_code("FR"), Language::French);
        assert_eq!(Language::from_code("en"), Language::English);
        assert_eq!(Language::from_code("de"), Language::English); // fallback
    }

    #[test]
    fn all_english_keys_exist_in_french() {
        let en = Translator::new(Language::English);
        let fr = Translator::new(Language::French);
        for key in en.translations.keys() {
            assert!(
                fr.translations.contains_key(key),
                "French translation missing key: {key}"
            );
        }
    }
}
