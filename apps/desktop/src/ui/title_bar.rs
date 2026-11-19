//! App-owned Windows title bar with system caption controls and snapping.
use nsh_core::prelude::*;

pub fn title_bar() -> AnyElement {
    #[cfg(target_os = "windows")]
    {
        use nsh_base::theme;
        use nsh_core::desktop::{TitleBarOptions, TitleBarStyle, WindowAttributes};
        use nsh_core::element::desktop::main_window;

        let window = main_window();
        let mut title_bar = TitleBarOptions::default();
        title_bar.style = TitleBarStyle::Hidden;
        title_bar.height = Some(px(40.0));
        let mut attributes = WindowAttributes::default();
        attributes.title_bar = Some(title_bar);
        window.set(attributes);
        let icon = nsh_core::platform::clipboard::data_uri(
            "image/png",
            include_bytes!("../../../../assets/icons/32x32.png"),
        );
        view()
            .test_id("app-title-bar")
            .h(px(40.0))
            .flex_none()
            .flex_row()
            .items_center()
            .gap_2()
            .pl(px(16.0))
            .pr(move || main_window().title_bar().right)
            .bg(move || theme().sidebar)
            .text_color(move || theme().sidebar_foreground)
            .border_b(px(1.0))
            .border_color(move || theme().sidebar_border)
            .window_drag(true)
            .child(image(icon).size(px(20.0)))
            .child(
                text("Lanpath")
                    .text_size(px(13.0))
                    .font_weight(FontWeight::MEDIUM),
            )
            .into_element()
    }
    #[cfg(not(target_os = "windows"))]
    {
        view().into_element()
    }
}
