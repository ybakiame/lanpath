use gpui_kit::component::{ActiveTheme as _, Disableable as _, Selectable as _, button::Button};
use gpui_kit::{Context, IntoElement, ParentElement as _, Styled as _, div};
use lanpath_client::local_override::CustomRuleSetSource;

use super::{
    RulesView,
    market::{self, DownloadSource, MarketGroup},
    render::{field, section_title},
};

impl RulesView {
    pub(super) fn render_market(&self, disabled: bool, cx: &Context<Self>) -> impl IntoElement {
        let query = self.inputs.market_search.read(cx).value().to_string();
        let results = market::search(&query, self.market_group);
        let total = results.len();
        div()
            .flex()
            .flex_col()
            .gap_3()
            .p_2()
            .child(section_title(
                "MetaCubeX 规则集市场",
                "本地检索随应用发布的完整 GeoIP / GeoSite 清单，不调用 GitHub API。添加后先保存；首次启动或手动更新会下载缓存。",
                cx,
            ))
            .child(
                div()
                    .flex()
                    .gap_2()
                    .child(field("检索", &self.inputs.market_search, disabled))
                    .child(field("目标分类", &self.inputs.market_category, disabled)),
            )
            .child(
                div()
                    .flex()
                    .gap_2()
                    .children([
                        (MarketGroup::All, "全部"),
                        (MarketGroup::GeoIp, "GeoIP"),
                        (MarketGroup::GeoSite, "GeoSite"),
                    ].into_iter().map(|(group, label)| {
                        Button::new(format!("market-group-{label}"))
                            .label(label)
                            .selected(self.market_group == group)
                            .disabled(disabled)
                            .on_click(cx.listener(move |this, _, _, cx| this.set_market_group(group, cx)))
                    }))
                    .children([
                        (DownloadSource::GitHub, "GitHub Raw"),
                        (DownloadSource::JsDelivr, "jsDelivr CDN"),
                    ].into_iter().map(|(source, label)| {
                        Button::new(format!("market-source-{label}"))
                            .label(label)
                            .selected(self.download_source == source)
                            .disabled(disabled)
                            .on_click(cx.listener(move |this, _, _, cx| this.set_download_source(source, cx)))
                    })),
            )
            .children(query.trim().is_empty().then(|| {
                div()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child("输入关键词检索，例如 cn、ads、google、netflix、youtube 或 telegram。")
            }))
            .children((!query.trim().is_empty()).then(|| {
                div()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child(if total > 200 {
                        format!("找到 {total} 项；当前显示前 200 项，请继续缩小关键词。")
                    } else {
                        format!("找到 {total} 项")
                    })
            }))
            .children(results.into_iter().take(200).map(|entry| {
                let added = self.market_entry_added(entry);
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .child(format!(
                                "{} · {}",
                                entry.name,
                                if entry.group == MarketGroup::GeoIp { "GeoIP" } else { "GeoSite" }
                            )),
                    )
                    .child(
                        Button::new(format!("market-add-{}", entry.id))
                            .label(if added { "已添加" } else { "添加" })
                            .selected(added)
                            .disabled(disabled || added)
                            .on_click(cx.listener(move |this, _, _, cx| this.add_market_entry(entry, cx))),
                    )
            }))
    }

    fn market_entry_added(&self, entry: &market::MarketEntry) -> bool {
        self.draft.as_ref().is_some_and(|draft| {
            draft.custom_rule_sets.iter().any(|set| match &set.source {
                CustomRuleSetSource::Remote { url, .. } => entry.equivalent_to(url),
                CustomRuleSetSource::Manual { .. } => false,
            })
        })
    }
}
