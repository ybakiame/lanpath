use lanpath_client::{node_tags_from_nodes, parse_subscription_body};
use nsh_base::{button, theme};
use nsh_core::prelude::*;

const FIXTURE: &str = r"proxies:
  - name: 东京测试节点
    type: ss
    server: 192.0.2.1
    port: 8388
    cipher: aes-256-gcm
    password: test-only
  - name: 新加坡测试节点
    type: ss
    server: 192.0.2.2
    port: 8388
    cipher: aes-256-gcm
    password: test-only
";

#[native_module("lanpath-nsh/native")]
fn native(registry: &mut Registry) {
    registry.register::<App>();
}

/// Validates shared subscription parsing with the same Rust code on every target.
#[component]
pub fn App() -> impl IntoElement {
    let report = signal("尚未解析共享订阅。".to_owned());
    view()
        .flex_1()
        .bg(move || theme().background)
        .text_color(move || theme().foreground)
        .child(
            view().flex_1().p_safe_area().child(
                view()
                    .flex_1()
                    .p_6()
                    .gap_4()
                    .child(text("Lanpath NSH 验证").text_size(px(24.0)))
                    .child(
                        text("纯 Rust · 共享 lanpath-client · 不启动核心或网络请求")
                            .text_color(move || theme().muted_foreground),
                    )
                    .child(
                        button()
                            .px_4()
                            .py_3()
                            .rounded(move || theme().radius)
                            .bg(move || theme().primary)
                            .on_press(move || {
                                let message = match parse_subscription_body(FIXTURE, None) {
                                    Ok(result) => {
                                        let nodes = node_tags_from_nodes(&result.singbox_nodes);
                                        let names = nodes
                                            .iter()
                                            .map(|node| node.name.as_str())
                                            .collect::<Vec<_>>()
                                            .join("\n");
                                        format!(
                                            "解析成功：{} 个节点\n格式：{:?}\n警告：{} 条\n\n{}",
                                            nodes.len(),
                                            result.format,
                                            result.warnings.len(),
                                            names
                                        )
                                    }
                                    Err(error) => format!("解析失败：{error}"),
                                };
                                report.set(message);
                            })
                            .child(
                                text("解析共享订阅").text_color(move || theme().primary_foreground),
                            ),
                    )
                    .child(
                        scroll()
                            .flex_1()
                            .child(text(move || report.get()).selectable(true)),
                    ),
            ),
        )
}
