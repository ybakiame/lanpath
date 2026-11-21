//! Selection surfaces and responsive virtualized card rows shared by inventories.
use nsh_base::theme;
use nsh_core::prelude::*;
use std::rc::Rc;

pub fn selection_card(
    enabled: impl Fn() -> bool + 'static,
    selected: impl Fn() -> bool + 'static,
) -> View {
    let enabled = Rc::new(enabled);
    let border_enabled = enabled.clone();
    let selected = Rc::new(selected);
    let a11y = selected.clone();
    super::surface(super::SurfaceLevel::Card)
        .role(AccessibilityRole::Group)
        .accessibility_state(move || AccessibilityState {
            selected: a11y(),
            ..AccessibilityState::default()
        })
        .w_full()
        .flex_1()
        .min_w(px(0.0))
        .flex_col()
        .gap_2()
        .p_3()
        .rounded(px(12.0))
        .bg(move || {
            if enabled() {
                theme().primary.opacity(0.09)
            } else {
                theme().card
            }
        })
        .text_color(move || theme().card_foreground)
        .border_color(move || {
            if selected() {
                theme().primary.opacity(0.8)
            } else if border_enabled() {
                theme().primary.opacity(0.35)
            } else {
                theme().border
            }
        })
        .transition_colors()
        .duration(move || {
            super::motion::motion_duration(super::motion::FAST_MS, super::prefs().reduced.get())
        })
}

/// Collections recycle whole grid rows while cards retain their domain identity.
pub fn card_grid<T: Clone + PartialEq + 'static>(
    items: Memo<Vec<T>>,
    row_height: f32,
    max_rows: Option<usize>,
    key: impl Fn(&T) -> String + 'static,
    render: impl Fn(ReadSignal<T>) -> AnyElement + 'static,
) -> AnyElement {
    let width = signal(0.0_f32);
    let columns = computed(move || {
        if width.get() >= 372.0 && font_scale() <= 1.3 {
            2
        } else {
            1
        }
    });
    let key = Rc::new(key);
    let render = Rc::new(render);
    let keys = key.clone();
    view()
        .when(max_rows.is_none(), |root| root.flex_1().min_h(px(80.0)))
        .min_w(px(0.0))
        .on_layout(move |bounds: &Bounds<Points>| {
            let next = bounds.size.width.0;
            if (width.get_untracked() - next).abs() > 0.5 {
                width.set(next);
            }
        })
        .when(max_rows.is_some(), |root| {
            root.flex_none().min_h(px(0.0)).h(move || {
                px(items.with(|items| {
                    items
                        .len()
                        .div_ceil(columns.get())
                        .min(max_rows.unwrap_or(2))
                }) as f32
                    * row_height)
            })
        })
        .child(
            virtual_list(
                move || items.with(|items| items.len().div_ceil(columns.get())),
                move |at| {
                    let chunk = computed(move || {
                        items.with(|items| {
                            items
                                .iter()
                                .skip(at.index() * columns.get())
                                .take(columns.get())
                                .cloned()
                                .collect::<Vec<_>>()
                        })
                    });
                    let render = render.clone();
                    let key = key.clone();
                    view()
                        .flex_row()
                        .gap_3()
                        .pb_3()
                        .h(px(row_height))
                        .child(each(
                            chunk,
                            move |item: &T| key(item),
                            move |item, _| view().flex_1().min_w(px(0.0)).child(render(item)),
                        ))
                        .child(when(
                            move || chunk.with(Vec::len) < columns.get(),
                            || view().flex_1(),
                        ))
                },
            )
            .keys(move || {
                items.with(|items| {
                    items
                        .chunks(columns.get())
                        .map(|row| {
                            row.iter()
                                .map(|item| keys(item))
                                .collect::<Vec<_>>()
                                .join("|")
                        })
                        .collect::<Vec<_>>()
                })
            })
            .estimated_item_size(px(row_height))
            .flex_1()
            .min_h(px(0.0)),
        )
        .into_element()
}
