//! Regression checks for the shared controls and available-height ownership.
mod common;

use nsh_testing::{Headless, LayoutNode};

fn by_id<'a>(nodes: &'a [LayoutNode], id: &str) -> &'a LayoutNode {
    nodes
        .iter()
        .find(|node| node.test_id.as_deref() == Some(id))
        .unwrap()
}

fn assert_button_centered(app: &Headless, anchor: &str, label: &str) {
    let nodes = app.screen().layout_nodes();
    let anchor = by_id(&nodes, anchor);
    let button = nodes.iter().find(|node| node.id == anchor.parent).unwrap();
    let text = nodes
        .iter()
        .find(|node| node.text.as_ref().is_some_and(|t| t.text.text == label))
        .unwrap();
    let a = button.bounds.center();
    let b = text.bounds.center();
    assert!(
        (a.x.0 - b.x.0).abs() < 1.0,
        "horizontal: {button:?} {text:?}"
    );
    assert!((a.y.0 - b.y.0).abs() < 1.0, "vertical: {button:?} {text:?}");
}

#[test]
fn overview_action_is_centered_and_sidebar_fills_available_height() {
    let (_dir, entries) = common::data_root();
    let mut app = common::start_app(entries).0;
    common::wait_ready(&mut app);
    for (width, height) in [(1180.0, 800.0), (960.0, 640.0), (1180.0, 1100.0)] {
        app.set_viewport(nsh_core::Size::new(
            nsh_core::Points(width),
            nsh_core::Points(height),
        ))
        .unwrap();
        assert_button_centered(&app, "overview-step-action", "去订阅页");
        let nodes = app.screen().layout_nodes();
        let sidebar = by_id(&nodes, "app-sidebar");
        let parent = nodes.iter().find(|node| node.id == sidebar.parent).unwrap();
        assert!(
            ((sidebar.bounds.origin.y.0 + sidebar.bounds.size.height.0)
                - (parent.bounds.origin.y.0 + parent.bounds.size.height.0))
                .abs()
                < 1.0
        );
        assert!(sidebar.bounds.size.height.0 > height - 120.0);
    }
}

#[test]
fn rule_select_trigger_centers_text_vertically() {
    let (_dir, entries) = common::data_root();
    let mut app = common::start_app(entries).0;
    common::wait_ready(&mut app);
    common::navigate(&mut app, "规则");
    app.wait_for("id:rules-target").unwrap();
    let nodes = app.screen().layout_nodes();
    let label = nodes
        .iter()
        .find(|node| {
            node.text
                .as_ref()
                .is_some_and(|t| t.text.text == "域名后缀")
        })
        .unwrap();
    let trigger = nodes.iter().find(|node| node.id == label.parent).unwrap();
    assert_eq!(trigger.bounds.size.height.0, 36.0);
    assert!((trigger.bounds.center().y.0 - label.bounds.center().y.0).abs() < 1.0);

    app.press("域名后缀").unwrap();
    app.wait_for("精确域名").unwrap();
    let nodes = app.screen().layout_nodes();
    let label = nodes
        .iter()
        .find(|node| {
            node.text
                .as_ref()
                .is_some_and(|t| t.text.text == "精确域名")
        })
        .unwrap();
    let row = nodes.iter().find(|node| node.id == label.parent).unwrap();
    assert_eq!(row.bounds.size.height.0, 32.0);
    assert!((row.bounds.center().y.0 - label.bounds.center().y.0).abs() < 1.0);
    app.press("精确域名").unwrap();
    assert!(!app.screen().matches("role:option[精确域名]").unwrap());
}
