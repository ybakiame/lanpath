mod connection_list;
mod connections_view;
mod core_validation;
mod editor_view;
mod proxy_list;
mod proxy_validation;
mod proxy_view;

use std::sync::Arc;

use connections_view::ConnectionsView;
use core_validation::CoreValidationService;
use gpui_kit::component::{
    ActiveTheme as _, Disableable as _, Selectable as _, WindowExt as _,
    button::Button,
    input::{Editor, EditorState},
    list::{ListEvent, ListState},
    switch::Switch,
};
use gpui_kit::{
    AppContext as _, Context, Entity, IntoElement, ParentElement as _, Render, Styled as _,
    Subscription, Task, TitlebarOptions, Window, WindowOptions, div,
};
use proxy_list::ProxyListDelegate;
use proxy_validation::ProxyValidationService;

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

#[derive(Clone, Copy)]
enum CoreOperationKind {
    Start,
    Stop,
}

enum ProxyOperation {
    Refresh,
    Select(String),
}

#[derive(Clone, Copy, PartialEq)]
enum ValidationPage {
    Editor,
    Proxies,
    Connections,
}

struct ValidationView {
    page: ValidationPage,
    active: EditorLanguage,
    read_only: bool,
    yaml: Entity<EditorState>,
    json: Entity<EditorState>,
    javascript: Entity<EditorState>,
    core: Arc<CoreValidationService>,
    connections: Entity<ConnectionsView>,
    core_running: bool,
    core_busy: Option<CoreOperationKind>,
    core_task: Option<Task<()>>,
    core_error: Option<String>,
    proxies: ProxyValidationService,
    proxy_list: Entity<ListState<ProxyListDelegate>>,
    proxy_busy: Option<ProxyOperation>,
    proxy_task: Option<Task<()>>,
    proxy_load_error: Option<String>,
    proxy_action_error: Option<String>,
    _proxy_subscription: Subscription,
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

        let core = Arc::new(CoreValidationService::from_env());
        let proxies = ProxyValidationService::new(core.api_port());
        let connections = cx.new(|cx| ConnectionsView::new(core.clone(), window, cx));
        let proxy_list = cx.new(|cx| ListState::new(ProxyListDelegate::default(), window, cx));
        let _proxy_subscription = cx.subscribe_in(
            &proxy_list,
            window,
            |this, state, event: &ListEvent, _, cx| {
                if let ListEvent::Confirm(ix) = event {
                    let Some(name) = state.read(cx).delegate().member(ix.row) else {
                        return;
                    };
                    this.select_proxy(name, cx);
                }
            },
        );

        Self {
            page: ValidationPage::Editor,
            active: EditorLanguage::Yaml,
            read_only: false,
            yaml,
            json,
            javascript,
            core,
            connections,
            core_running: false,
            core_busy: None,
            core_task: None,
            core_error: None,
            proxies,
            proxy_list,
            proxy_busy: None,
            proxy_task: None,
            proxy_load_error: None,
            proxy_action_error: None,
            _proxy_subscription,
        }
    }

    fn run_core_operation(&mut self, kind: CoreOperationKind, cx: &mut Context<Self>) {
        let invalid_state = match kind {
            CoreOperationKind::Start => self.core_running,
            CoreOperationKind::Stop => !self.core_running,
        };
        if self.core_busy.is_some()
            || self.proxy_busy.is_some()
            || !self.core.is_configured()
            || invalid_state
        {
            return;
        }

        self.connections
            .update(cx, |view, cx| view.set_running(false, cx));
        self.core_busy = Some(kind);
        self.core_error = None;
        self.core_task = None;
        if matches!(kind, CoreOperationKind::Start) {
            self.clear_proxies(cx);
        }
        let service = self.core.clone();
        let proxies = self.proxies.clone();
        let task = cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn(async move {
                    match kind {
                        CoreOperationKind::Start => {
                            let result = service.start();
                            let proxy_result = if result.is_ok() {
                                Some(
                                    service
                                        .block_on(async {
                                            proxies.wait_ready().await?;
                                            proxies.list().await
                                        })
                                        .and_then(|result| result),
                                )
                            } else {
                                None
                            };
                            (result, proxy_result)
                        }
                        CoreOperationKind::Stop => (service.stop(), None),
                    }
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                let (result, proxy_result) = result;
                let succeeded = result.is_ok();
                match result {
                    Ok(()) => {
                        this.core_running = matches!(kind, CoreOperationKind::Start);
                    }
                    Err(error) => {
                        this.core_error = Some(error);
                    }
                }
                this.connections
                    .update(cx, |view, cx| view.set_running(this.core_running, cx));
                if succeeded && matches!(kind, CoreOperationKind::Stop) {
                    this.clear_proxies(cx);
                }
                if let Some(proxy_result) = proxy_result {
                    this.apply_proxy_result(proxy_result, true, cx);
                }
                this.core_busy = None;
                cx.notify();
            });
        });
        self.core_task = Some(task);
        cx.notify();
    }

    fn core_status(&self) -> String {
        if let Some(kind) = self.core_busy {
            return match kind {
                CoreOperationKind::Start => "正在启动隔离核心…".to_string(),
                CoreOperationKind::Stop => "正在停止核心…".to_string(),
            };
        }
        if let Some(error) = &self.core_error {
            return format!("核心错误：{error}");
        }
        if self.core_running {
            return format!("核心运行中 · 127.0.0.1:{}", self.core.port());
        }
        if self.core.is_configured() {
            return format!("核心已停止 · 隔离端口 {}", self.core.port());
        }
        format!(
            "核心未配置 · {}",
            self.core.configuration_error().unwrap_or("不可用")
        )
    }

    fn set_page(&mut self, page: ValidationPage, cx: &mut Context<Self>) {
        self.page = page;
        self.connections.update(cx, |view, cx| {
            view.set_active(page == ValidationPage::Connections, cx)
        });
        cx.notify();
    }
}

impl Render for ValidationView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let operation_busy = self.core_busy.is_some() || self.proxy_busy.is_some();
        let content = match self.page {
            ValidationPage::Editor => self.render_editor_panel(cx).into_any_element(),
            ValidationPage::Proxies => self.render_proxy_panel(cx).into_any_element(),
            ValidationPage::Connections => self.connections.clone().into_any_element(),
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
                        Button::new("page-editor")
                            .label("编辑器")
                            .selected(self.page == ValidationPage::Editor)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.set_page(ValidationPage::Editor, cx);
                            })),
                    )
                    .child(
                        Button::new("page-proxies")
                            .label("代理策略")
                            .selected(self.page == ValidationPage::Proxies)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.set_page(ValidationPage::Proxies, cx);
                            })),
                    )
                    .child(
                        Button::new("page-connections")
                            .label("活动连接")
                            .selected(self.page == ValidationPage::Connections)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.set_page(ValidationPage::Connections, cx);
                            })),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_2()
                    .child(
                        Button::new("core-start")
                            .label("启动隔离核心")
                            .disabled(
                                operation_busy || self.core_running || !self.core.is_configured(),
                            )
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.run_core_operation(CoreOperationKind::Start, cx);
                            })),
                    )
                    .child(
                        Button::new("core-stop")
                            .label("停止核心")
                            .disabled(operation_busy || !self.core_running)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.run_core_operation(CoreOperationKind::Stop, cx);
                            })),
                    ),
            )
            .child(
                div()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child(self.core_status()),
            )
            .child(content)
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
