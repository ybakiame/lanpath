//! 本地覆写 Editor：原生高亮、撤销、补全菜单，不启动语言服务器或网络服务。
use nsh_base::editor::{EditorCompletionItem, EditorCompletionKind, EditorCompletionRequest};
use nsh_base::{Editor, EditorApi, editor, theme};
use nsh_core::prelude::*;

pub const DEFAULT_SCRIPT: &str = "function main(config) {\n  return config;\n}\n";

pub fn code_editor(value: Signal<String>, dirty: Signal<bool>) -> AnyElement {
    let reference = component_ref::<Editor>();
    view()
        .flex_1()
        .min_h(px(0.0))
        .flex_col()
        .gap_2()
        .child(
            view()
                .flex_row()
                .items_center()
                .gap_3()
                .child(
                    text(|| t!("profile.editor_hint"))
                        .flex_1()
                        .text_size(px(12.0))
                        .text_color(move || theme().muted_foreground),
                )
                .child(
                    super::button(super::ButtonVariant::Outline)
                        .size(super::ButtonSize::Sm)
                        .test_id("profile-completions")
                        .label(|| t!("profile.completions"))
                        .on_press(move || {
                            if let Some(api) = reference.api::<EditorApi>() {
                                api.focus();
                                api.show_completions();
                            }
                        }),
                ),
        )
        .child(
            editor()
                .test_id("profile-js")
                .component_ref(&reference)
                .accessibility_label(|| Some(t!("profile.source").into()))
                .value(move || Some(value.get().into()))
                .language("typescript")
                .line_numbers(true)
                .highlighting(true)
                .auto_close(true)
                .auto_indent(true)
                .font_size(px(14.0))
                .line_height(px(22.0))
                .completion(true)
                .completion_trigger_characters(vec![".".into()])
                .on_change(move |text: &SharedString| {
                    value.set(text.to_string());
                    dirty.set(true);
                })
                .on_completion_request(move |event: &EditorCompletionRequest| {
                    let items = completions(&value.get(), event.offset, event.query_start);
                    if let Some(api) = reference.api::<EditorApi>() {
                        api.present_completions(event.id, items, None);
                    }
                })
                .flex_1()
                .min_h(px(0.0))
                .border(px(1.0))
                .border_color(move || theme().input)
                .rounded(move || theme().radius),
        )
        .into_element()
}

fn completions(source: &str, _offset: u32, query_start: u32) -> Vec<EditorCompletionItem> {
    let prefix = String::from_utf16_lossy(
        &source
            .encode_utf16()
            .take(query_start as usize)
            .collect::<Vec<_>>(),
    );
    let candidates: &[(&str, &str)] = if prefix.ends_with("config.route.") {
        &[
            ("rules", "路由规则"),
            ("rule_set", "规则集"),
            ("final", "默认出口"),
        ]
    } else if prefix.ends_with("config.dns.") {
        &[
            ("servers", "DNS 上游"),
            ("rules", "DNS 规则"),
            ("final", "默认 DNS 上游"),
        ]
    } else if prefix.ends_with("config.") {
        &[
            ("dns", "DNS 配置"),
            ("route", "sing-box 路由"),
            ("outbounds", "sing-box 出站"),
            ("inbounds", "入站"),
            ("log", "日志"),
            ("proxies", "Mihomo 节点"),
            ("rules", "Mihomo 规则"),
        ]
    } else {
        &[
            ("config", "当前配置对象"),
            ("return", "返回修改后的配置"),
            ("const", "声明常量"),
            ("let", "声明变量"),
            ("if", "条件判断"),
            ("function", "定义函数"),
        ]
    };
    candidates
        .iter()
        .map(|(label, detail)| {
            let mut item = EditorCompletionItem::new(*label);
            item.kind = Some(if prefix.ends_with('.') {
                EditorCompletionKind::Property
            } else {
                EditorCompletionKind::Keyword
            });
            item.detail = Some((*detail).into());
            item
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn completion_context_uses_utf16_and_does_not_offer_unrelated_global_fields() {
        let source = "// 中文 🧭\nconfig.route.";
        let offset = source.encode_utf16().count() as u32;
        let items = completions(source, offset, offset);
        assert!(items.iter().any(|i| i.label.as_str() == "rules"));
        assert!(!items.iter().any(|i| i.label.as_str() == "outbounds"));
    }
}
