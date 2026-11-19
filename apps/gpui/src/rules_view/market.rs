use std::sync::LazyLock;

const GEOIP_NAMES: &str = include_str!("../../../../assets/rules/geoip.txt");
const GEOSITE_NAMES: &str = include_str!("../../../../assets/rules/geosite.txt");
const GITHUB_BASE: &str = "https://raw.githubusercontent.com/MetaCubeX/meta-rules-dat/sing/geo";
const CDN_BASE: &str = "https://cdn.jsdelivr.net/gh/MetaCubeX/meta-rules-dat@sing/geo";

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum MarketGroup {
    All,
    GeoIp,
    GeoSite,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum DownloadSource {
    GitHub,
    JsDelivr,
}

pub(super) struct MarketEntry {
    pub(super) id: String,
    pub(super) name: String,
    pub(super) group: MarketGroup,
    github_url: String,
}

impl MarketEntry {
    pub(super) fn url(&self, source: DownloadSource) -> String {
        match source {
            DownloadSource::GitHub => self.github_url.clone(),
            DownloadSource::JsDelivr => self.github_url.replacen(GITHUB_BASE, CDN_BASE, 1),
        }
    }

    pub(super) fn equivalent_to(&self, url: &str) -> bool {
        if url.contains('?') {
            return false;
        }
        url == self.github_url || url == self.github_url.replacen(GITHUB_BASE, CDN_BASE, 1)
    }
}

pub(super) fn search(query: &str, group: MarketGroup) -> Vec<&'static MarketEntry> {
    let query = query.trim().to_lowercase();
    if query.is_empty() {
        return Vec::new();
    }
    catalog()
        .iter()
        .filter(|entry| group == MarketGroup::All || entry.group == group)
        .filter(|entry| entry.name.to_lowercase().contains(&query))
        .collect()
}

fn catalog() -> &'static [MarketEntry] {
    static CATALOG: LazyLock<Box<[MarketEntry]>> = LazyLock::new(|| {
        let mut entries = Vec::new();
        entries.extend(
            GEOIP_NAMES
                .split_whitespace()
                .map(|name| entry(name, MarketGroup::GeoIp)),
        );
        entries.extend(
            GEOSITE_NAMES
                .split_whitespace()
                .map(|name| entry(name, MarketGroup::GeoSite)),
        );
        entries.into_boxed_slice()
    });
    &CATALOG
}

fn entry(name: &str, group: MarketGroup) -> MarketEntry {
    let directory = if group == MarketGroup::GeoIp {
        "geoip"
    } else {
        "geosite"
    };
    MarketEntry {
        id: format!("{directory}-{name}"),
        name: name.to_string(),
        group,
        github_url: format!("{GITHUB_BASE}/{directory}/{name}.srs"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_catalog_searches_both_groups() {
        let catalog = catalog();
        assert!(catalog.len() > 1_000, "catalog unexpectedly truncated");
        let cn = search("cn", MarketGroup::All);
        assert!(cn.iter().any(|entry| entry.id == "geoip-cn"));
        assert!(cn.iter().any(|entry| entry.id == "geosite-cn"));
        let netflix = search("netflix", MarketGroup::GeoSite);
        assert_eq!(netflix[0].id, "geosite-netflix");
    }
}
