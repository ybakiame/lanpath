//! 翻译装载：编译期嵌入 `locales/`（rust-i18n v1 单文件格式），构造
//! `nsh_core::i18n::Translations` 供根组件注入。首发 zh-CN 与 en；
//! zh-TW 结构预留——运行时回退链 `zh-Hant-TW → zh → en` 由
//! `Translations` 原生支持，新增语言只需在此登记并加一个 yml 文件。
//! key 为意图稳定的点分层级（`nav.subscription`、`appearance.mode.light`），
//! 参数用 `%{name}`（`t!` 读取）。

use std::collections::{BTreeMap, HashMap};

use nsh_core::i18n::Translations;

/// 简体中文翻译。
const ZH_CN_YML: &str = include_str!("../locales/zh-CN.yml");
/// 英文翻译。
const EN_YML: &str = include_str!("../locales/en.yml");

/// 一个 locale yml 的扁平 key → 文本（`_version` 等元键忽略）。
fn locale_map(yaml: &str, locale: &str) -> Result<BTreeMap<String, String>, String> {
    let raw: BTreeMap<String, serde_yaml::Value> = serde_yaml::from_str(yaml)
        .map_err(|error| format!("locales/{locale}.yml 解析失败：{error}"))?;
    let mut map = BTreeMap::new();
    for (key, value) in raw {
        if key.starts_with('_') {
            continue;
        }
        let Some(text) = value.as_str() else {
            return Err(format!("locales/{locale}.yml 的 {key} 不是字符串"));
        };
        map.insert(key, text.to_owned());
    }
    Ok(map)
}

/// 应用全部翻译（zh-CN、en）。任一文件结构非法即返回真实错误——
/// 翻译是编译期嵌入的开发者资源，损坏属于构建错误，不静默降级。
pub fn translations() -> Result<Translations, String> {
    let mut map: HashMap<String, HashMap<String, String>> = HashMap::new();
    for (locale, yaml) in [("zh-CN", ZH_CN_YML), ("en", EN_YML)] {
        map.insert(
            locale.to_owned(),
            locale_map(yaml, locale)?.into_iter().collect(),
        );
    }
    Ok(Translations::from_map(map))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn both_locales_load_with_matching_keys() {
        let translations = translations().expect("locales parse");
        assert_eq!(translations.locales(), ["en", "zh-CN"]);
        let zh = locale_map(ZH_CN_YML, "zh-CN").unwrap();
        let en = locale_map(EN_YML, "en").unwrap();
        assert_eq!(zh.keys().collect::<Vec<_>>(), en.keys().collect::<Vec<_>>());
        // 参数占位一致：同一 key 的 %{..} 集合两种语言相同。
        for key in zh.keys() {
            let args = |text: &str| {
                let mut args: Vec<String> = text
                    .split("%{")
                    .skip(1)
                    .filter_map(|rest| rest.split('}').next())
                    .map(str::to_owned)
                    .collect();
                args.sort_unstable();
                args
            };
            assert_eq!(
                args(&zh[key]),
                args(&en[key]),
                "{key} 的参数占位在 zh-CN 与 en 不一致"
            );
        }
    }

    #[test]
    fn lookup_falls_back_and_interpolates() {
        let translations = translations().expect("locales parse");
        assert_eq!(
            translations.translate("zh-CN", "nav.subscriptions", &[]),
            "订阅"
        );
        assert_eq!(
            translations.translate("en", "nav.subscriptions", &[]),
            "Subscriptions"
        );
        // 参数插值与缺失 key 的回退显示。
        assert_eq!(
            translations.translate("zh-CN", "common.selected", &[("name", "深色".into())]),
            "已选择：深色"
        );
        assert_eq!(
            translations.translate("zh-CN", "no.such.key", &[]),
            "zh-CN.no.such.key"
        );
        let _ = translations;
    }
}
