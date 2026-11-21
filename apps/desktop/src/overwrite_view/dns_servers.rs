//! DNS 上游服务器行、内联编辑器与实时连通性探测辅助。

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use lanpath_client::dns_probe::{DnsProbeInput, probe_dns_server};
use lanpath_client::singbox_overwrite::SingboxSliceKind;
use nsh_base::{SelectItem, theme};
use nsh_core::prelude::*;
use serde_json::{Map, Value, json};

use super::forms::mutate_section;
use super::state::OverwriteState;
use crate::ui::button::{ButtonSize, ButtonVariant, button};
use crate::ui::icon::{Icon, Tone, icon_sized};
use crate::ui::input::input;
use crate::ui::layout::{SurfaceLevel, surface};
use crate::ui::select::select;
use crate::ui::{BadgeVariant, badge};

const PROBE_POLL: Duration = Duration::from_millis(80);

/// 单个 DNS 上游的实时连通性探测状态。
#[derive(Clone, PartialEq, Eq)]
pub(super) enum ProbeState {
    Probing,
    Ok(u64),
    Err(String),
}

#[derive(Clone, Copy)]
pub(super) struct DnsEditorDraft {
    pub(super) editing_index: Signal<Option<usize>>,
    pub(super) tag: Signal<String>,
    pub(super) server_type: Signal<String>,
    pub(super) server: Signal<String>,
    pub(super) port: Signal<String>,
    pub(super) detour: Signal<String>,
}

impl DnsEditorDraft {
    pub(super) fn new() -> Self {
        Self {
            editing_index: signal(None),
            tag: signal(String::new()),
            server_type: signal("https".to_owned()),
            server: signal(String::new()),
            port: signal("443".to_owned()),
            detour: signal(String::new()),
        }
    }

    pub(super) fn load_server(&self, index: usize, value: &Value) {
        let kind = value
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or("https")
            .to_owned();
        let default_port = default_port_for(&kind);
        let port = value
            .get("server_port")
            .and_then(Value::as_u64)
            .map(|p| p.to_string())
            .unwrap_or_else(|| default_port.map(|p| p.to_string()).unwrap_or_default());
        self.editing_index.set(Some(index));
        self.tag.set(
            value
                .get("tag")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned(),
        );
        self.server_type.set(kind);
        self.server.set(
            value
                .get("server")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned(),
        );
        self.port.set(port);
        self.detour.set(
            value
                .get("detour")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned(),
        );
    }

    pub(super) fn clear(&self) {
        self.editing_index.set(None);
        self.tag.set(String::new());
        self.server_type.set("https".to_owned());
        self.server.set(String::new());
        self.port.set("443".to_owned());
        self.detour.set(String::new());
    }

    fn build_server_value(&self) -> Option<Value> {
        let tag = self.tag.get().trim().to_owned();
        let kind = self.server_type.get().trim().to_owned();
        let server = self.server.get().trim().to_owned();
        if tag.is_empty() || (kind != "local" && server.is_empty()) {
            return None;
        }
        let mut obj = Map::new();
        obj.insert("tag".into(), Value::String(tag));
        obj.insert("type".into(), Value::String(kind.clone()));
        if !server.is_empty() {
            obj.insert("server".into(), Value::String(server));
        }
        let port = self
            .port
            .get()
            .trim()
            .parse::<u16>()
            .ok()
            .or_else(|| default_port_for(&kind));
        if let Some(port) = port {
            obj.insert("server_port".into(), json!(port));
        }
        let detour = self.detour.get().trim().to_owned();
        if !detour.is_empty() {
            obj.insert("detour".into(), Value::String(detour));
        }
        Some(Value::Object(obj))
    }
}

pub(super) fn default_port_for(kind: &str) -> Option<u16> {
    match kind {
        "https" | "h3" => Some(443),
        "udp" => Some(53),
        "tls" | "quic" => Some(853),
        _ => None,
    }
}

pub(super) fn probe_input_from_server(value: &Value) -> Option<(String, DnsProbeInput)> {
    let tag = value.get("tag").and_then(Value::as_str)?.to_owned();
    let server_type = value.get("type").and_then(Value::as_str)?.to_owned();
    let server = value
        .get("server")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned();
    let server_port = value
        .get("server_port")
        .and_then(Value::as_u64)
        .and_then(|p| u16::try_from(p).ok());
    Some((
        format!("srv:{tag}"),
        DnsProbeInput {
            server_type,
            server,
            server_port,
            domain: None,
        },
    ))
}

pub(super) fn spawn_probe(
    owner: Owner,
    tasks: Stored<Vec<Spawned<()>>>,
    probes: Signal<HashMap<String, ProbeState>>,
    key: String,
    input: DnsProbeInput,
) {
    probes.update(|map| {
        map.insert(key.clone(), ProbeState::Probing);
    });
    let shared: Arc<Mutex<Option<Result<u64, String>>>> = Arc::new(Mutex::new(None));
    let worker_slot = shared.clone();
    owner.run(move || {
        tasks.with(|list| {
            list.push(spawn(async move {
                let _worker = background(async move {
                    let res = probe_dns_server(&input).await;
                    if let Ok(mut guard) = worker_slot.lock() {
                        *guard = Some(res);
                    }
                });
                loop {
                    let done = shared.lock().ok().and_then(|mut g| g.take());
                    if let Some(outcome) = done {
                        probes.update(|map| {
                            map.insert(
                                key.clone(),
                                match outcome {
                                    Ok(ms) => ProbeState::Ok(ms),
                                    Err(err) => ProbeState::Err(err),
                                },
                            );
                        });
                        break;
                    }
                    tokio::time::sleep(PROBE_POLL).await;
                }
            }));
        });
    });
}

pub(super) fn dns_server_row(
    state: OverwriteState,
    draft: DnsEditorDraft,
    owner: Owner,
    tasks: Stored<Vec<Spawned<()>>>,
    probes: Signal<HashMap<String, ProbeState>>,
    item: ReadSignal<Value>,
    index: ReadSignal<usize>,
) -> impl IntoElement {
    let obj = item.get_untracked();
    let tag = obj
        .get("tag")
        .and_then(Value::as_str)
        .unwrap_or("—")
        .to_owned();
    let kind = obj
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or("unknown")
        .to_owned();
    let server = obj
        .get("server")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_owned();
    let port = obj.get("server_port").and_then(Value::as_u64);
    let detour = obj
        .get("detour")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_owned();

    let probe_key = format!("srv:{tag}");
    let probe_key_check = probe_key.clone();
    let probe_key_badge = probe_key;
    let probe_obj = obj.clone();
    let edit_obj = obj;
    let tag_id = tag.clone();
    let tag_edit_id = tag.clone();

    let address_line = match (server.is_empty(), port, detour.is_empty()) {
        (true, _, _) => String::new(),
        (false, Some(p), true) => format!("{server}:{p}"),
        (false, None, true) => server,
        (false, Some(p), false) => format!("{server}:{p} · detour: {detour}"),
        (false, None, false) => format!("{server} · detour: {detour}"),
    };

    view()
        .flex_row()
        .flex_wrap()
        .items_center()
        .justify_between()
        .gap_2()
        .px_3()
        .py_2()
        .rounded(move || theme().radius)
        .bg(move || theme().muted.opacity(0.4))
        .border(px(1.0))
        .border_color(move || theme().border)
        .child(
            view()
                .flex_row()
                .flex_wrap()
                .items_center()
                .gap_2()
                .child(badge(BadgeVariant::Default, move || kind.clone()))
                .child(
                    text(move || tag.clone())
                        .text_size(px(13.5))
                        .font_weight(FontWeight::MEDIUM),
                )
                .child(
                    text(move || address_line.clone())
                        .text_size(px(12.5))
                        .text_color(move || theme().muted_foreground),
                )
                .child(when(
                    move || probes.with(|m| m.contains_key(&probe_key_check)),
                    move || probe_badge(probes, probe_key_badge.clone()),
                )),
        )
        .child(
            view()
                .flex_row()
                .items_center()
                .gap_1()
                .child(
                    button(ButtonVariant::Ghost)
                        .test_id(format!("overwrite-dns-probe-{tag_id}"))
                        .size(ButtonSize::Sm)
                        .label(|| t!("overwrite.dns.probe"))
                        .on_press(move || {
                            if let Some((key, input)) = probe_input_from_server(&probe_obj) {
                                spawn_probe(owner, tasks, probes, key, input);
                            }
                        }),
                )
                .child(
                    button(ButtonVariant::Ghost)
                        .test_id(format!("overwrite-dns-edit-{tag_edit_id}"))
                        .size(ButtonSize::Sm)
                        .label(|| t!("rulesets.edit"))
                        .on_press(move || draft.load_server(index.get(), &edit_obj)),
                )
                .child(
                    button(ButtonVariant::Ghost)
                        .size(ButtonSize::Sm)
                        .label(|| t!("rules.delete"))
                        .on_press(move || {
                            mutate_section(&state, SingboxSliceKind::Dns, |s| {
                                let idx = index.get();
                                if let Some(arr) =
                                    s.get_mut("servers").and_then(Value::as_array_mut)
                                    && idx < arr.len()
                                {
                                    arr.remove(idx);
                                }
                            });
                        }),
                ),
        )
}

pub(super) fn probe_badge(probes: Signal<HashMap<String, ProbeState>>, key: String) -> AnyElement {
    let state = computed(move || probes.with(|m| m.get(&key).cloned()));
    view()
        .px_2()
        .py(px(2.0))
        .rounded(move || theme().number("radius-sm").map(px).unwrap_or(px(4.0)))
        .bg(move || match state.get() {
            Some(ProbeState::Ok(_)) => theme()
                .color("success-surface")
                .unwrap_or(theme().secondary),
            Some(ProbeState::Probing) | None => theme().secondary,
            Some(ProbeState::Err(_)) => theme().destructive.opacity(0.14),
        })
        .child(
            text(move || match state.get() {
                Some(ProbeState::Probing) => t!("overwrite.dns.probing"),
                Some(ProbeState::Ok(ms)) => format!("{ms} ms"),
                Some(ProbeState::Err(err)) => err,
                None => String::new(),
            })
            .text_size(px(11.5))
            .text_color(move || match state.get() {
                Some(ProbeState::Ok(_)) => theme().color("success").unwrap_or(theme().primary),
                Some(ProbeState::Probing) | None => theme().muted_foreground,
                Some(ProbeState::Err(_)) => theme().destructive,
            }),
        )
        .into_element()
}

pub(super) fn dns_server_editor(state: OverwriteState, draft: DnsEditorDraft) -> impl IntoElement {
    let editing_index = draft.editing_index;
    let tag_sig = draft.tag;
    let type_sig = draft.server_type;
    let server_sig = draft.server;
    let port_sig = draft.port;
    let detour_sig = draft.detour;
    let save_draft = draft;
    let cancel_draft = draft;

    surface(SurfaceLevel::Card)
        .p_3()
        .flex_col()
        .gap_2()
        .child(when(
            move || editing_index.get().is_some(),
            move || {
                text(move || t!("overwrite.dns.editing_hint", tag = tag_sig.get()))
                    .text_size(px(12.0))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(move || theme().primary)
            },
        ))
        .child(
            view()
                .flex_row()
                .flex_wrap()
                .items_center()
                .gap_2()
                .child(
                    view().w(px(115.0)).child(
                        input(tag_sig)
                            .test_id("overwrite-dns-new-tag")
                            .placeholder(|| "tag".into())
                            .accessible_name(|| "DNS tag".into())
                            .on_change(move |v| tag_sig.set(v)),
                    ),
                )
                .child(select(
                    ["https", "tls", "udp", "quic", "h3", "local"]
                        .iter()
                        .map(|k| SelectItem::new(*k, *k))
                        .collect(),
                    move || Some(type_sig.get().into()),
                    move |v| {
                        if let Some(v) = v {
                            let kind = v.to_string();
                            if let Some(p) = default_port_for(&kind) {
                                port_sig.set(p.to_string());
                            } else {
                                port_sig.set(String::new());
                            }
                            type_sig.set(kind);
                        }
                    },
                ))
                .child(
                    view().flex_1().min_w(px(140.0)).child(
                        input(server_sig)
                            .test_id("overwrite-dns-new-server")
                            .placeholder(|| "1.1.1.1".into())
                            .accessible_name(|| "DNS server".into())
                            .on_change(move |v| server_sig.set(v)),
                    ),
                )
                .child(
                    view().w(px(80.0)).child(
                        input(port_sig)
                            .test_id("overwrite-dns-new-port")
                            .placeholder(|| "443".into())
                            .accessible_name(|| "DNS port".into())
                            .on_change(move |v| port_sig.set(v)),
                    ),
                )
                .child(
                    view().w(px(105.0)).child(
                        input(detour_sig)
                            .test_id("overwrite-dns-new-detour")
                            .placeholder(|| "detour".into())
                            .accessible_name(|| "DNS detour".into())
                            .on_change(move |v| detour_sig.set(v)),
                    ),
                )
                .child(
                    button(ButtonVariant::Outline)
                        .size(ButtonSize::Md)
                        .test_id("overwrite-dns-add")
                        .prefix(|| icon_sized(Icon::Plus, Tone::Muted, 14.0))
                        .label(move || {
                            if editing_index.get().is_some() {
                                t!("overwrite.dns.save_server")
                            } else {
                                t!("overwrite.dns.add_server")
                            }
                        })
                        .on_press(move || {
                            let Some(entry) = save_draft.build_server_value() else {
                                return;
                            };
                            let target_idx = save_draft.editing_index.get();
                            mutate_section(&state, SingboxSliceKind::Dns, |s| {
                                if let Some(root) = s.as_object_mut() {
                                    let arr = root
                                        .entry("servers")
                                        .or_insert_with(|| Value::Array(Vec::new()));
                                    if let Some(arr) = arr.as_array_mut() {
                                        if let Some(idx) = target_idx
                                            && idx < arr.len()
                                        {
                                            arr[idx] = entry;
                                        } else {
                                            arr.push(entry);
                                        }
                                    }
                                }
                            });
                            save_draft.clear();
                        }),
                )
                .child(when(
                    move || editing_index.get().is_some(),
                    move || {
                        button(ButtonVariant::Ghost)
                            .size(ButtonSize::Md)
                            .label(|| t!("common.cancel"))
                            .on_press(move || cancel_draft.clear())
                    },
                )),
        )
}
