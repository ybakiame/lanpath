//! Overview preview panel. A source document is read directly; only bound scripts invoke composition.
use crate::config_composer::{ConfigComposer, SelectedPreview};
use crate::ui::icon::{Icon, Tone, icon_sized};
use crate::ui::layout::{AlertTone, alert_banner};
use nsh_base::theme;
use nsh_core::prelude::*;

#[derive(Clone)]
struct PreviewState {
    service: Option<ConfigComposer>,
    document: Signal<Option<SelectedPreview>>,
    busy: Signal<bool>,
    error: Signal<Option<String>>,
    task: Stored<Option<Spawned<()>>>,
}

pub(crate) fn config_page() -> AnyElement {
    let service = ConfigComposer::open();
    let state = PreviewState {
        error: signal(service.as_ref().err().map(ToString::to_string)),
        service: service.ok(),
        document: signal(None),
        busy: signal(false),
        task: stored(None),
    };
    let document = state.document;
    let busy = state.busy;
    let error = state.error;
    let refresh = state.clone();
    let text_value = signal(String::new());
    let language = signal("yaml".to_owned());
    effect(move || {
        if let Some(preview) = document.get() {
            text_value.set(preview.content);
            language.set(preview.language);
        } else {
            text_value.set(String::new());
        }
    });
    refresh_preview(state);
    view()
        .flex_1()
        .min_h(px(0.0))
        .flex_col()
        .gap_3()
        .child(
            view()
                .flex_row()
                .flex_wrap()
                .items_center()
                .gap_3()
                .child(
                    text(move || document.get().map(|p| p.name).unwrap_or_default())
                        .flex_1()
                        .min_w(px(0.0))
                        .truncate()
                        .text_size(px(15.0))
                        .font_weight(FontWeight::SEMIBOLD),
                )
                .child(when(
                    move || busy.get() || document.get().is_some(),
                    move || {
                        crate::ui::badge(crate::ui::BadgeVariant::Outline, move || {
                            if busy.get() {
                                t!("config.loading")
                            } else {
                                document
                                    .get()
                                    .map(|p| {
                                        if p.generated {
                                            t!("config.final")
                                        } else {
                                            t!("config.raw")
                                        }
                                    })
                                    .unwrap_or_default()
                            }
                        })
                    },
                ))
                .child(
                    crate::ui::button(crate::ui::ButtonVariant::Outline)
                        .size(crate::ui::ButtonSize::Sm)
                        .prefix(|| icon_sized(Icon::Refresh, Tone::Muted, 13.0))
                        .test_id("config-compose")
                        .label(|| t!("config.refresh"))
                        .loading(move || busy.get())
                        .on_press(move || refresh_preview(refresh.clone())),
                ),
        )
        .child(when(
            move || document.get().is_some_and(|p| p.generated),
            || {
                text(|| t!("config.subtitle"))
                    .text_size(px(12.0))
                    .text_color(move || theme().muted_foreground)
            },
        ))
        .child(when(
            move || error.get().is_some(),
            move || {
                alert_banner(
                    AlertTone::Danger,
                    text(move || error.get().unwrap_or_default())
                        .selectable(true)
                        .text_size(px(12.5))
                        .text_color(move || theme().destructive),
                )
            },
        ))
        .child(when(
            move || document.get().is_some_and(|p| !p.warnings.is_empty()),
            move || {
                alert_banner(
                    AlertTone::Warning,
                    text(move || {
                        document
                            .get()
                            .map(|p| p.warnings.join("\n"))
                            .unwrap_or_default()
                    })
                    .selectable(true)
                    .text_size(px(12.5))
                    .text_color(move || theme().destructive),
                )
            },
        ))
        .child(crate::ui::data::code_viewer(text_value, language))
        .into_element()
}

fn refresh_preview(state: PreviewState) {
    if state.busy.get() {
        return;
    }
    let Some(service) = state.service.clone() else {
        return;
    };
    state.busy.set(true);
    state.error.set(None);
    let app_state = crate::app_state::current();
    let task = state.task;
    task.with(|slot| {
        slot.replace(spawn(async move {
            let result = background(async move { service.selected_preview().await }).await;
            state.busy.set(false);
            match result {
                Ok(Ok(preview)) => {
                    if preview.generated {
                        crate::app_state::refresh_active_subscription(&app_state);
                    }
                    state.document.set(Some(preview));
                }
                Ok(Err(error)) => {
                    state.document.set(None);
                    state.error.set(Some(error.to_string()));
                }
                Err(_) => {
                    state.document.set(None);
                    state.error.set(Some("配置预览任务中断，请重试".into()));
                }
            }
        }))
    });
}
