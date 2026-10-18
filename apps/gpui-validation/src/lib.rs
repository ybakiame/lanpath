use gpui_kit::component::{
    ActiveTheme as _, WindowExt as _,
    button::Button,
    input::{Editor, EditorState},
    switch::Switch,
};
use gpui_kit::{
    AppContext as _, Context, Entity, IntoElement, ParentElement as _, Render, Styled as _,
    TitlebarOptions, Window, WindowOptions, div,
};

const YAML_SAMPLE: &str = r#"dns:
  servers:
    - tag: cloudflare
      address: 8.8.8.8
  rules:
    - domain: example.com
      server: cloudflare
"#;

const JSON_SAMPLE: &str = r#"{
  "mode": "rule",
  "log": {
    "level": "info",
    "timestamp": true
  }
}
"#;

const JAVASCRIPT_SAMPLE: &str = r#"export function selectOutbound(outbounds, name) {
  const selected = outbounds.find((outbound) => outbound.tag === name);
  if (!selected) {
    throw new Error(`Unknown outbound: ${name}`);
  }
  return selected;
}
"#;

#[derive(Clone, Copy, PartialEq)]
enum EditorLanguage {
    Yaml,
    Json,
    JavaScript,
}

impl EditorLanguage {
    fn label(self) -> &'static str {
        match self {
            Self::Yaml => "YAML",
            Self::Json => "JSON",
            Self::JavaScript => "JavaScript",
        }
    }
}

struct ValidationView {
    active: EditorLanguage,
    read_only: bool,
    yaml: Entity<EditorState>,
    json: Entity<EditorState>,
    javascript: Entity<EditorState>,
}

impl ValidationView {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let yaml = cx.new(|cx| {
            EditorState::new(window, cx)
                .language("yaml")
                .line_number(true)
                .folding(true)
                .default_value(YAML_SAMPLE)
        });
        let json = cx.new(|cx| {
            EditorState::new(window, cx)
                .language("json")
                .line_number(true)
                .folding(true)
                .default_value(JSON_SAMPLE)
        });
        let javascript = cx.new(|cx| {
            EditorState::new(window, cx)
                .language("javascript")
                .line_number(true)
                .folding(true)
                .default_value(JAVASCRIPT_SAMPLE)
        });

        Self {
            active: EditorLanguage::Yaml,
            read_only: false,
            yaml,
            json,
            javascript,
        }
    }
}

impl Render for ValidationView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let editor = match self.active {
            EditorLanguage::Yaml => &self.yaml,
            EditorLanguage::Json => &self.json,
            EditorLanguage::JavaScript => &self.javascript,
        };
        let mode = if self.read_only {
            "只读"
        } else {
            "可编辑"
        };

        div()
            .flex()
            .flex_col()
            .size_full()
            .p_4()
            .gap_3()
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child("Lanpath GPUI 本地验证")
                    .child(
                        div()
                            .text_sm()
                            .text_color(cx.theme().muted_foreground)
                            .child("验证编辑器、键盘焦点与 overlay；不读取现有 Lanpath 数据"),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_2()
                    .child(
                        Button::new("language-yaml")
                            .label("YAML")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.active = EditorLanguage::Yaml;
                                cx.notify();
                            })),
                    )
                    .child(
                        Button::new("language-json")
                            .label("JSON")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.active = EditorLanguage::Json;
                                cx.notify();
                            })),
                    )
                    .child(
                        Button::new("language-javascript")
                            .label("JavaScript")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.active = EditorLanguage::JavaScript;
                                cx.notify();
                            })),
                    )
                    .child(
                        Switch::new("editor-read-only")
                            .checked(self.read_only)
                            .label("只读")
                            .on_click(cx.listener(|this, checked, _, cx| {
                                this.read_only = *checked;
                                cx.notify();
                            })),
                    )
                    .child(
                        Button::new("validation-about")
                            .label("验证说明…")
                            .on_click(|_, window, cx| {
                                window.open_dialog(cx, |dialog, _, _| {
                                    dialog
                                        .title("本地验证范围")
                                        .child("编辑器使用真实 GPUI Kit 组件；未连接配置、sing-box、系统代理或 TUN。")
                                });
                            }),
                    ),
            )
            .child(
                div()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child(format!(
                        "当前：{} · {mode}；编辑器内 Ctrl-F 搜索，Esc 关闭搜索或 dialog",
                        self.active.label()
                    )),
            )
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .w_full()
                    .child(
                        Editor::new(editor)
                            .size_full()
                            .readonly(self.read_only)
                            .aria_label("Override 编辑器"),
                    ),
            )
    }
}

pub fn run() {
    gpui_kit::application()
        .with_assets(gpui_kit::assets::Assets)
        .run(|cx| {
            gpui_kit::init(cx);
            gpui_kit::open_window(
                WindowOptions {
                    titlebar: Some(TitlebarOptions {
                        title: Some("Lanpath GPUI 本地验证".into()),
                        ..Default::default()
                    }),
                    ..Default::default()
                },
                cx,
                |window, cx| cx.new(|cx| ValidationView::new(window, cx)),
            )
            .expect("failed to open GPUI validation window");
        });
}
