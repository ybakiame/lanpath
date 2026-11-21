//! 高级 JSON 切片编辑器：复用 NSH 原生 Editor（JSON 语法高亮、行号、自动闭合、
//! 本地字段补全与实时结构校验状态）。

use lanpath_client::singbox_overwrite::SingboxSliceKind;
use nsh_base::editor::{EditorCompletionItem, EditorCompletionKind, EditorCompletionRequest};
use nsh_base::{Editor, EditorApi, editor, theme};
use nsh_core::prelude::*;

use super::state::OverwriteState;
use crate::singbox_overwrite::SingboxOverwriteService;

pub(super) fn json_editor(state: OverwriteState) -> AnyElement {
    let reference = component_ref::<Editor>();
    let value_state = state.clone();
    let change_state = state.clone();
    let check_state = state.clone();
    let check_color = state.clone();
    let completion_state = state;

    view()
        .flex_col()
        .gap_2()
        .child(
            view()
                .flex_row()
                .flex_wrap()
                .items_center()
                .justify_between()
                .gap_2()
                .child(
                    text(|| t!("overwrite.editor_hint"))
                        .text_size(px(12.0))
                        .text_color(move || theme().muted_foreground),
                )
                .child(
                    text(move || {
                        let kind = check_state.active_slice.get();
                        let raw = check_state.active_draft();
                        match SingboxOverwriteService::validate_slice(kind, &raw) {
                            Ok(_) => t!("overwrite.validation_ok"),
                            Err(err) => err.to_string(),
                        }
                    })
                    .test_id("overwrite-live-validation")
                    .text_size(px(12.0))
                    .text_color(move || {
                        let kind = check_color.active_slice.get();
                        let raw = check_color.active_draft();
                        if SingboxOverwriteService::validate_slice(kind, &raw).is_ok() {
                            theme().color("success").unwrap_or(theme().primary)
                        } else {
                            theme().destructive
                        }
                    }),
                ),
        )
        .child(
            editor()
                .test_id("overwrite-json-editor")
                .component_ref(&reference)
                .accessibility_label(|| Some(t!("overwrite.mode.json").into()))
                .value(move || Some(value_state.active_draft().into()))
                .language("json")
                .line_numbers(true)
                .highlighting(true)
                .auto_close(true)
                .auto_indent(true)
                .font_size(px(13.5))
                .line_height(px(21.0))
                .completion(true)
                .completion_trigger_characters(vec!["\"".into()])
                .on_change(move |text: &SharedString| {
                    change_state.set_active_draft(text.to_string());
                })
                .on_completion_request(move |event: &EditorCompletionRequest| {
                    let items = slice_completions(completion_state.active_slice.get());
                    if let Some(api) = reference.api::<EditorApi>() {
                        api.present_completions(event.id, items, None);
                    }
                })
                .h(px(360.0))
                .w_full()
                .border(px(1.0))
                .border_color(move || theme().input)
                .rounded(move || theme().radius),
        )
        .into_element()
}

fn slice_completions(kind: SingboxSliceKind) -> Vec<EditorCompletionItem> {
    let entries: &[(&str, &str)] = match kind {
        SingboxSliceKind::Log => &[
            ("disabled", "是否禁用日志输出"),
            ("level", "日志级别：trace / debug / info / warn / error"),
            ("output", "日志文件路径（限数据根 logs/ 内）"),
            ("timestamp", "是否在日志行首输出时间戳"),
        ],
        SingboxSliceKind::Dns => &[
            ("servers", "DNS 上游服务器数组"),
            ("rules", "DNS 分流规则数组"),
            ("final", "默认 DNS 上游 tag"),
            (
                "strategy",
                "解析策略：prefer_ipv4 / prefer_ipv6 / ipv4_only / ipv6_only",
            ),
            ("reverse_mapping", "启用 IP 到域名的反向映射"),
            ("independent_cache", "按上游独立缓存 DNS 响应"),
        ],
        SingboxSliceKind::Route => &[
            ("rules", "路由规则数组"),
            ("rule_set", "规则集声明数组"),
            ("final", "默认兜底出站 tag"),
            ("auto_detect_interface", "自动绑定默认物理网卡"),
            ("default_domain_resolver", "默认域名解析器对象"),
        ],
        SingboxSliceKind::Outbounds => &[
            ("$nodes", "动态展开当前订阅的全部节点 tag"),
            ("selector", "手动选择出站分组"),
            ("urltest", "自动测速出站分组"),
            ("direct", "直连出站"),
            ("block", "拦截出站"),
            ("outbounds", "分组包含的成员 tag 列表"),
            ("default", "selector 分组默认选中的成员"),
            ("interval", "urltest 测速间隔（如 5m）"),
            ("tolerance", "urltest 切换容差毫秒数（如 150）"),
        ],
        SingboxSliceKind::Experimental => &[
            ("cache_file", "本地持久化缓存文件配置"),
            ("enabled", "启用 cache_file"),
            ("path", "缓存文件路径（限数据根 cache/ 内）"),
            ("cache_id", "缓存命名空间 ID"),
            ("store_fakeip", "持久化 FakeIP 映射"),
            ("store_rdrc", "持久化拒绝解析缓存"),
        ],
    };
    entries
        .iter()
        .map(|(label, detail)| {
            let mut item = EditorCompletionItem::new(*label);
            item.kind = Some(EditorCompletionKind::Property);
            item.detail = Some((*detail).into());
            item
        })
        .collect()
}
