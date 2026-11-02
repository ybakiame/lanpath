use gpui_kit::component::{
    Disableable as _, Selectable as _, Sizable as _,
    button::{Button, ButtonGroup},
    switch::Switch,
};
use gpui_kit::{Context, IntoElement, ParentElement as _, Styled as _, div};
use lanpath_client::config_slices::DnsStrategy;

use super::{
    ConfigSlicesView,
    render::{field, section_title},
};

impl ConfigSlicesView {
    pub(super) fn render_route_cache(
        &self,
        disabled: bool,
        cx: &Context<Self>,
    ) -> impl IntoElement {
        let Some(slices) = self.draft.as_ref() else {
            return div().child("路由与缓存配置不可用");
        };
        let cache = &slices.experimental.cache_file;
        div()
            .flex()
            .flex_col()
            .gap_3()
            .pb_4()
            .child(section_title(
                "默认路由",
                "留空时保留模板值；默认域名解析器使用 DNS server tag。",
                cx,
            ))
            .child(
                div()
                    .flex()
                    .gap_2()
                    .child(field("默认出站 tag", &self.inputs.route_final, disabled))
                    .child(field(
                        "默认域名解析服务器",
                        &self.inputs.route_resolver,
                        disabled,
                    )),
            )
            .child(
                div().flex().gap_2().child(
                    ButtonGroup::new("route-strategy")
                        .small()
                        .compact()
                        .outline()
                        .disabled(disabled)
                        .children(route_strategies().into_iter().map(|(strategy, label)| {
                            Button::new(format!("route-strategy-{label}"))
                                .label(label)
                                .selected(slices.route.resolver.strategy == strategy)
                        }))
                        .on_click(cx.listener(|this, indexes: &Vec<usize>, _, cx| {
                            if let Some((strategy, _)) =
                                indexes.first().map(|index| route_strategies()[*index])
                            {
                                this.set_route_strategy(strategy, cx);
                            }
                        })),
                ),
            )
            .child(section_title(
                "缓存文件",
                "启用后写入 sing-box experimental.cache_file；空路径使用核心默认 cache.db。",
                cx,
            ))
            .child(
                div()
                    .flex()
                    .gap_2()
                    .child(
                        Switch::new("cache-enabled")
                            .label("启用缓存")
                            .checked(cache.enabled)
                            .disabled(disabled)
                            .on_click(cx.listener(|this, _, _, cx| this.toggle_cache_enabled(cx))),
                    )
                    .child(
                        Switch::new("cache-fakeip")
                            .label("持久化 FakeIP")
                            .checked(cache.store_fakeip)
                            .disabled(disabled || !cache.enabled)
                            .on_click(cx.listener(|this, _, _, cx| this.toggle_cache_fakeip(cx))),
                    ),
            )
            .child(
                div()
                    .flex()
                    .gap_2()
                    .child(field(
                        "缓存路径",
                        &self.inputs.cache_path,
                        disabled || !cache.enabled,
                    ))
                    .child(field(
                        "Cache ID",
                        &self.inputs.cache_id,
                        disabled || !cache.enabled,
                    )),
            )
    }
}

fn route_strategies() -> [(Option<DnsStrategy>, &'static str); 5] {
    [
        (None, "继承核心"),
        (Some(DnsStrategy::PreferIpv4), "优先 IPv4"),
        (Some(DnsStrategy::PreferIpv6), "优先 IPv6"),
        (Some(DnsStrategy::Ipv4Only), "仅 IPv4"),
        (Some(DnsStrategy::Ipv6Only), "仅 IPv6"),
    ]
}
