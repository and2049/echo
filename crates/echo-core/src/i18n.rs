use std::collections::HashMap;
use std::sync::OnceLock;

static TRANSLATIONS: OnceLock<HashMap<&'static str, serde_json::Value>> = OnceLock::new();

pub fn init() {
    let mut map = HashMap::new();

    let en: serde_json::Value = serde_json::from_str(include_str!("../../../locales/en.json")).unwrap();
    let zh: serde_json::Value =
        serde_json::from_str(include_str!("../../../locales/zh-CN.json")).unwrap();
    let zh_tw: serde_json::Value =
        serde_json::from_str(include_str!("../../../locales/zh-TW.json")).unwrap();

    map.insert("en", en);
    map.insert("zh", zh.clone());
    map.insert("zh-CN", zh);
    map.insert("zh-TW", zh_tw);

    let _ = TRANSLATIONS.set(map);
}

pub fn t(key: &str, lang: &str) -> String {
    let map = TRANSLATIONS.get().expect("Translations not initialized");

    let locale_data = map.get(lang).unwrap_or_else(|| map.get("en").unwrap());

    let parts: Vec<&str> = key.split('.').collect();
    let mut current = locale_data;

    for part in parts {
        if let Some(next) = current.get(part) {
            current = next;
        } else {
            // Fallback to English if key is missing in current language
            if lang != "en" {
                return t(key, "en");
            }
            return key.to_string();
        }
    }

    if let Some(s) = current.as_str() {
        s.to_string()
    } else {
        key.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn setup_uri_keys_present_in_all_locales() {
        init();
        let map = TRANSLATIONS.get().unwrap();
        for lang in ["en", "zh-CN", "zh-TW"] {
            let setup = &map[lang]["desktop"]["setup"];
            for key in ["step2", "copy", "copied"] {
                assert!(
                    setup[key].is_string(),
                    "{lang} missing desktop.setup.{key}"
                );
            }
        }
    }

    #[test]
    fn tray_keys_present_in_all_locales() {
        init();
        let map = TRANSLATIONS.get().unwrap();
        for lang in ["en", "zh-CN", "zh-TW"] {
            let desktop = &map[lang]["desktop"];
            for key in ["window", "tray", "tray_desc"] {
                assert!(
                    desktop["settings"][key].is_string(),
                    "{lang} missing desktop.settings.{key}"
                );
            }
            assert!(
                desktop["tray"]["show"].is_string(),
                "{lang} missing desktop.tray.show"
            );
        }
    }
}
