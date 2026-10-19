use super::*;
use gpui_kit::component::list::List;
use lanpath_client::proxies::ProxyList;

impl DesktopView {
    pub(crate) fn clear_proxies(&mut self, cx: &mut Context<Self>) {
        self.proxy_list.update(cx, |state, cx| {
            state.delegate_mut().set_group(None);
            cx.notify();
        });
        self.proxy_load_error = None;
        self.proxy_action_error = None;
    }

    pub(crate) fn apply_proxy_result(
        &mut self,
        result: Result<ProxyList, String>,
        is_load: bool,
        cx: &mut Context<Self>,
    ) {
        match result {
            Ok(list) => {
                let group = list.groups.into_iter().find(|group| group.name == "proxy");
                self.proxy_list.update(cx, |state, cx| {
                    state.delegate_mut().set_group(group);
                    cx.notify();
                });
                if is_load {
                    self.proxy_load_error = None;
                } else {
                    self.proxy_action_error = None;
                }
            }
            Err(error) => {
                if is_load {
                    self.proxy_load_error = Some(error);
                } else {
                    self.proxy_action_error = Some(error);
                    self.proxy_list.update(cx, |state, cx| {
                        state.delegate_mut().reset_selection();
                        cx.notify();
                    });
                }
            }
        }
    }

    fn refresh_proxies(&mut self, cx: &mut Context<Self>) {
        if self.closing
            || !self.core_running
            || self.core_busy.is_some()
            || self.proxy_busy.is_some()
        {
            return;
        }
        self.proxy_busy = Some(ProxyOperation::Refresh);
        self.proxy_load_error = None;
        self.proxy_task = None;
        let service = self.core.clone();
        let proxies = self.proxies.clone();
        let task = cx.spawn(async move |this, cx| {
            let result = service
                .block_on(async { proxies.list().await })
                .and_then(|result| result);
            let _ = this.update(cx, |this, cx| {
                this.apply_proxy_result(result, true, cx);
                this.proxy_busy = None;
                cx.notify();
            });
        });
        self.proxy_task = Some(task);
        cx.notify();
    }

    pub(crate) fn select_proxy(&mut self, name: String, cx: &mut Context<Self>) {
        if self.closing
            || !self.core_running
            || self.core_busy.is_some()
            || self.proxy_busy.is_some()
        {
            self.proxy_list.update(cx, |state, cx| {
                state.delegate_mut().reset_selection();
                cx.notify();
            });
            return;
        }
        let (group, is_selector) = {
            let state = self.proxy_list.read(cx);
            (
                state.delegate().group_name(),
                state.delegate().is_selector(),
            )
        };
        let Some(group) = group.filter(|_| is_selector) else {
            return;
        };

        self.proxy_busy = Some(ProxyOperation::Select(name.clone()));
        self.proxy_action_error = None;
        self.proxy_task = None;
        let service = self.core.clone();
        let proxies = self.proxies.clone();
        let task = cx.spawn(async move |this, cx| {
            let result = service
                .block_on(async move { proxies.select(&group, &name).await })
                .and_then(|result| result);
            let _ = this.update(cx, |this, cx| {
                this.apply_proxy_result(result, false, cx);
                this.proxy_busy = None;
                cx.notify();
            });
        });
        self.proxy_task = Some(task);
        cx.notify();
    }

    fn proxy_status(&self) -> String {
        if let Some(operation) = &self.proxy_busy {
            return match operation {
                ProxyOperation::Refresh => "正在读取代理策略…".to_string(),
                ProxyOperation::Select(name) => format!("正在切换至 {name}…"),
            };
        }
        if let Some(error) = &self.proxy_load_error {
            return format!("代理正在运行，但策略读取失败：{error}");
        }
        if let Some(error) = &self.proxy_action_error {
            return format!("操作失败：{error}");
        }
        if self.core_running {
            "点击节点切换出口；仅当前核心会话生效".to_string()
        } else {
            "启动代理后可用".to_string()
        }
    }

    pub(crate) fn render_proxy_panel(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let operation_busy = self.closing || self.core_busy.is_some() || self.proxy_busy.is_some();
        div()
            .flex()
            .flex_col()
            .w_full()
            .flex_1()
            .min_h_0()
            .gap_2()
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .justify_between()
                    .child("代理策略")
                    .child(
                        Button::new("proxy-refresh")
                            .label("重新读取")
                            .disabled(operation_busy || !self.core_running)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.refresh_proxies(cx);
                            })),
                    ),
            )
            .child(
                div()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child(self.proxy_status()),
            )
            .child(List::new(&self.proxy_list).size_full())
    }
}
