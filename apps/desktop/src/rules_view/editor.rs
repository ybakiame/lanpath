use super::*;
use crate::ui::{
    input::{field, textarea},
    select::{checkbox, select, tab_bar},
};
use nsh_base::SelectItem;
use nsh_core::prelude::*;

pub(super) fn editor(state: State) -> impl IntoElement {
    let form = state.clone();
    let enter = state.clone();
    let batch = state.batch;
    let mode = state.mode;
    let kind = state.kind;
    let target = state.target;
    let action = state.action;
    let submit = state.clone();
    let cancel = state.clone();
    let preview = state.clone();
    view().flex_col().gap_2()
        .child(tab_bar(vec![("rules-form", "rules.quick"), ("rules-batch-tab", "rules.batch")], mode))
        .child(when(move || mode.get() == 0, move || {
            let enter = enter.clone();
            let kind_items = [("domain","rules.domain"),("suffix","rules.suffix"),("keyword","rules.keyword"),
                ("cidr","rules.cidr"),("process","rules.process"),("port","rules.port")];
            let action_items = [("proxy","rules.proxy"),("direct","rules.direct"),("reject","rules.reject")];
            view().flex_row().flex_wrap().items_end().gap_3()
                .child(field().label(|| t!("rules.kind")).control(select(kind_items.iter().map(|(id,key)| SelectItem::new(*id,t!(*key))).collect(),
                    move || Some(kind.get().into()), move |v| { if let Some(v)=v {kind.set(v.to_string());} })))
                .child(view().flex_1().min_w(px(180.0)).child(field().label(|| t!("rules.target")).control(input(target).test_id("rules-target").accessible_name(|| t!("rules.target"))
                    .placeholder(|| "example.com".into()).on_change(move |v| target.set(v)).on_submit(move || enter.save()))))
                .child(field().label(|| t!("rules.action")).control(select(action_items.iter().map(|(id,key)| SelectItem::new(*id,t!(*key))).collect(),
                    move || Some(action.get().into()), move |v| { if let Some(v)=v { action.set(v.to_string()); } })))
        }))
        .child(when(move || mode.get() == 1, move || field().label(|| t!("rules.lines"))
            .hint(|| "suffix example.com -> proxy\ncidr 192.168.0.0/16 -> direct\nprocess chrome.exe -> proxy\nport 8000-9000 -> reject\n# comment".into())
            .control(textarea(batch).test_id("rules-batch").accessible_name(|| t!("rules.lines")).on_change(move |v| batch.set(v)))))
        .child(text(move || {
            if (preview.mode.get() == 0 && preview.target.get().trim().is_empty()) || preview.source().trim().is_empty() { return String::new(); }
            match crate::rules::parse_lines(&preview.source()) {
                Ok(rules) => rules.iter().take(3).map(rule_line).collect::<Vec<_>>().join("\n"),
                Err(error) => error.to_string(),
            }
        }))
        .child(view().flex_row().flex_wrap().items_center().gap_3()
            .child(checkbox(form.first, || t!("rules.first"), move |v| form.first.set(v)))
            .child(button(ButtonVariant::Primary).label(move || if submit.editing.get().is_some() {t!("rules.save_edit")}else{t!("rules.add")})
                .test_id("rules-save").on_press({ let submit = state.clone(); move || {
                    submit.save();
                }}))
            .child(button(ButtonVariant::Ghost).label(|| t!("rules.reset")).on_press(move || {
                cancel.editing.set(None); cancel.target.set(String::new()); cancel.batch.set(String::new()); cancel.error.set(None);
            })))
}
