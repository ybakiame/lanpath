//! sing-box 切片 GUI 表单：分派器 + `log` / `experimental` 表单与共享读写辅助。
//! 列表型表单（`dns` / `route` / `outbounds`）拆分在 `dns_form.rs` 与 `forms_lists.rs`。

use lanpath_client::singbox_overwrite::SingboxSliceKind;
use nsh_base::{SelectItem, theme};
use nsh_core::prelude::*;
use serde_json::{Map, Value};

use super::dns_form::dns_form;
use super::forms_lists::route_form;
use super::outbounds_form::outbounds_form;
use super::state::{EditMode, OverwriteState};
use crate::ui::button::{ButtonSize, ButtonVariant, button};
use crate::ui::input::{field, input};
use crate::ui::layout::{AlertTone, alert_banner};
use crate::ui::select::{select, switch_labeled};

pub(super) fn slice_form(state: OverwriteState) -> AnyElement {
    let parse_check = state.clone();
    let switch_to_json = state.clone();
    let kind_signal = state.active_slice;

    when(
        move || parse_active_json(&parse_check).is_none(),
        move || {
            let to_json = switch_to_json.clone();
            alert_banner(
                AlertTone::Danger,
                view()
                    .flex_row()
                    .flex_wrap()
                    .items_center()
                    .justify_between()
                    .gap_3()
                    .child(
                        text(|| t!("overwrite.form_invalid_json"))
                            .text_size(px(12.5))
                            .text_color(move || theme().destructive),
                    )
                    .child(
                        button(ButtonVariant::Outline)
                            .size(ButtonSize::Sm)
                            .label(|| t!("overwrite.mode.json"))
                            .on_press(move || to_json.edit_mode.set(EditMode::Json)),
                    ),
            )
        },
    )
    .otherwise(move || {
        let s_log = state.clone();
        let s_dns = state.clone();
        let s_route = state.clone();
        let s_out = state.clone();
        let s_exp = state.clone();
        view()
            .flex_col()
            .gap_4()
            .child(when(
                move || kind_signal.get() == SingboxSliceKind::Log,
                move || log_form(s_log.clone()),
            ))
            .child(when(
                move || kind_signal.get() == SingboxSliceKind::Dns,
                move || dns_form(s_dns.clone()),
            ))
            .child(when(
                move || kind_signal.get() == SingboxSliceKind::Route,
                move || route_form(s_route.clone()),
            ))
            .child(when(
                move || kind_signal.get() == SingboxSliceKind::Outbounds,
                move || outbounds_form(s_out.clone()),
            ))
            .child(when(
                move || kind_signal.get() == SingboxSliceKind::Experimental,
                move || experimental_form(s_exp.clone()),
            ))
    })
    .into_element()
}

pub(super) fn parse_active_json(state: &OverwriteState) -> Option<Value> {
    let kind = state.active_slice.get();
    let value: Value = serde_json::from_str(&state.active_draft()).ok()?;
    value.get(kind.section_key())?;
    Some(value)
}

pub(super) fn mutate_section(
    state: &OverwriteState,
    kind: SingboxSliceKind,
    f: impl FnOnce(&mut Value),
) {
    let Ok(mut root) = serde_json::from_str::<Value>(&state.active_draft()) else {
        return;
    };
    if let Some(section) = root.get_mut(kind.section_key()) {
        f(section);
        if let Ok(pretty) = serde_json::to_string_pretty(&root) {
            state.set_active_draft(format!("{pretty}\n"));
        }
    }
}

pub(super) fn read_str(state: &OverwriteState, pointer: &str, default: &str) -> String {
    parse_active_json(state)
        .and_then(|v| v.pointer(pointer).and_then(Value::as_str).map(String::from))
        .unwrap_or_else(|| default.to_owned())
}

pub(super) fn read_bool(state: &OverwriteState, pointer: &str, default: bool) -> bool {
    parse_active_json(state)
        .and_then(|v| v.pointer(pointer).and_then(Value::as_bool))
        .unwrap_or(default)
}

pub(super) fn toggle_row(
    checked: impl Fn() -> bool + 'static,
    label: impl Fn() -> String + Copy + 'static,
    test_id: &'static str,
    on_change: impl Fn(bool) + 'static,
) -> impl IntoElement {
    view()
        .test_id(test_id)
        .flex_row()
        .items_center()
        .gap_2()
        .child(switch_labeled(checked, move || label().into(), on_change))
        .child(text(label).text_size(px(13.0)))
}

fn log_form(state: OverwriteState) -> impl IntoElement {
    let output_sig = signal(read_str(&state, "/log/output", ""));
    let sync_state = state.clone();
    effect(move || {
        if sync_state.active_slice.get() == SingboxSliceKind::Log {
            let current = read_str(&sync_state, "/log/output", "");
            if output_sig.get_untracked() != current {
                output_sig.set(current);
            }
        }
    });
    let levels = ["trace", "debug", "info", "warn", "error", "fatal", "panic"];
    let level_read = state.clone();
    let level_memo = computed(move || read_str(&level_read, "/log/level", "info"));
    let level_write = state.clone();
    let disabled_read = state.clone();
    let disabled_write = state.clone();
    let ts_read = state.clone();
    let ts_write = state.clone();
    let out_write = state;

    view()
        .flex_col()
        .gap_4()
        .child(
            view()
                .flex_row()
                .flex_wrap()
                .items_start()
                .gap_4()
                .child(field().label(|| t!("overwrite.log.level")).control(select(
                    levels.iter().map(|l| SelectItem::new(*l, *l)).collect(),
                    move || Some(level_memo.get().into()),
                    move |v| {
                        if let Some(level) = v {
                            mutate_section(&level_write, SingboxSliceKind::Log, |s| {
                                if let Some(obj) = s.as_object_mut() {
                                    obj.insert("level".into(), Value::String(level.to_string()));
                                }
                            });
                        }
                    },
                )))
                .child(
                    view().flex_1().min_w(px(220.0)).child(
                        field()
                            .label(|| t!("overwrite.log.output"))
                            .hint(|| t!("overwrite.log.output_hint"))
                            .control(
                                input(output_sig)
                                    .test_id("overwrite-log-output")
                                    .accessible_name(|| t!("overwrite.log.output"))
                                    .placeholder(|| "logs/sing-box.log".into())
                                    .on_change(move |val| {
                                        output_sig.set(val.clone());
                                        mutate_section(&out_write, SingboxSliceKind::Log, |s| {
                                            if let Some(obj) = s.as_object_mut() {
                                                if val.trim().is_empty() {
                                                    obj.remove("output");
                                                } else {
                                                    obj.insert("output".into(), Value::String(val));
                                                }
                                            }
                                        });
                                    }),
                            ),
                    ),
                ),
        )
        .child(
            view()
                .flex_row()
                .flex_wrap()
                .items_center()
                .gap_6()
                .child(toggle_row(
                    move || read_bool(&ts_read, "/log/timestamp", true),
                    || t!("overwrite.log.timestamp"),
                    "overwrite-log-timestamp",
                    move |v| {
                        mutate_section(&ts_write, SingboxSliceKind::Log, |s| {
                            if let Some(obj) = s.as_object_mut() {
                                obj.insert("timestamp".into(), Value::Bool(v));
                            }
                        });
                    },
                ))
                .child(toggle_row(
                    move || read_bool(&disabled_read, "/log/disabled", false),
                    || t!("overwrite.log.disabled"),
                    "overwrite-log-disabled",
                    move |v| {
                        mutate_section(&disabled_write, SingboxSliceKind::Log, |s| {
                            if let Some(obj) = s.as_object_mut() {
                                if v {
                                    obj.insert("disabled".into(), Value::Bool(true));
                                } else {
                                    obj.remove("disabled");
                                }
                            }
                        });
                    },
                )),
        )
}

fn experimental_form(state: OverwriteState) -> impl IntoElement {
    let path_sig = signal(read_str(&state, "/experimental/cache_file/path", ""));
    let id_sig = signal(read_str(&state, "/experimental/cache_file/cache_id", ""));
    let sync_state = state.clone();
    effect(move || {
        if sync_state.active_slice.get() == SingboxSliceKind::Experimental {
            let p = read_str(&sync_state, "/experimental/cache_file/path", "");
            if path_sig.get_untracked() != p {
                path_sig.set(p);
            }
            let id = read_str(&sync_state, "/experimental/cache_file/cache_id", "");
            if id_sig.get_untracked() != id {
                id_sig.set(id);
            }
        }
    });
    let en_read = state.clone();
    let en_write = state.clone();
    let path_write = state.clone();
    let id_write = state.clone();
    let fake_read = state.clone();
    let fake_write = state.clone();
    let rdrc_read = state.clone();
    let rdrc_write = state;

    view()
        .flex_col()
        .gap_4()
        .child(alert_banner(
            AlertTone::Info,
            text(|| t!("overwrite.experimental.clash_api_hint"))
                .text_size(px(12.5))
                .text_color(move || theme().foreground),
        ))
        .child(toggle_row(
            move || read_bool(&en_read, "/experimental/cache_file/enabled", false),
            || t!("overwrite.experimental.cache_enabled"),
            "overwrite-exp-cache-enabled",
            move |v| {
                mutate_cache_file(&en_write, |cf| {
                    cf.insert("enabled".into(), Value::Bool(v));
                });
            },
        ))
        .child(
            view()
                .flex_row()
                .flex_wrap()
                .items_start()
                .gap_4()
                .child(
                    view().flex_1().min_w(px(220.0)).child(
                        field()
                            .label(|| t!("overwrite.experimental.cache_path"))
                            .hint(|| t!("overwrite.experimental.cache_path_hint"))
                            .control(
                                input(path_sig)
                                    .test_id("overwrite-exp-cache-path")
                                    .accessible_name(|| t!("overwrite.experimental.cache_path"))
                                    .placeholder(|| "runtime/db/cache.db".into())
                                    .on_change(move |val| {
                                        path_sig.set(val.clone());
                                        mutate_cache_file(&path_write, |cf| {
                                            cf.insert("path".into(), Value::String(val));
                                        });
                                    }),
                            ),
                    ),
                )
                .child(
                    view().w(px(200.0)).child(
                        field()
                            .label(|| t!("overwrite.experimental.cache_id"))
                            .control(
                                input(id_sig)
                                    .test_id("overwrite-exp-cache-id")
                                    .accessible_name(|| t!("overwrite.experimental.cache_id"))
                                    .on_change(move |val| {
                                        id_sig.set(val.clone());
                                        mutate_cache_file(&id_write, |cf| {
                                            cf.insert("cache_id".into(), Value::String(val));
                                        });
                                    }),
                            ),
                    ),
                ),
        )
        .child(
            view()
                .flex_row()
                .flex_wrap()
                .items_center()
                .gap_6()
                .child(toggle_row(
                    move || read_bool(&fake_read, "/experimental/cache_file/store_fakeip", false),
                    || t!("overwrite.experimental.store_fakeip"),
                    "overwrite-exp-store-fakeip",
                    move |v| {
                        mutate_cache_file(&fake_write, |cf| {
                            cf.insert("store_fakeip".into(), Value::Bool(v));
                        });
                    },
                ))
                .child(toggle_row(
                    move || read_bool(&rdrc_read, "/experimental/cache_file/store_rdrc", false),
                    || t!("overwrite.experimental.store_rdrc"),
                    "overwrite-exp-store-rdrc",
                    move |v| {
                        mutate_cache_file(&rdrc_write, |cf| {
                            cf.insert("store_rdrc".into(), Value::Bool(v));
                        });
                    },
                )),
        )
}

fn mutate_cache_file(state: &OverwriteState, f: impl FnOnce(&mut Map<String, Value>)) {
    mutate_section(state, SingboxSliceKind::Experimental, |s| {
        if let Some(exp) = s.as_object_mut() {
            let cf = exp
                .entry("cache_file")
                .or_insert_with(|| Value::Object(Map::new()));
            if let Some(map) = cf.as_object_mut() {
                f(map);
            }
        }
    });
}
