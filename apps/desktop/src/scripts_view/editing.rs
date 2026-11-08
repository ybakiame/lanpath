use std::collections::HashSet;

use gpui_kit::{Context, Window, component::WindowExt as _};
use lanpath_client::remote::{RemoteKind, RemoteResource, ScriptDialect};

use crate::script_dialects::{is_selectable, selection_allowed, validate_selection};

use super::ScriptsView;

impl ScriptsView {
    pub(super) fn select_resource(
        &mut self,
        name: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.locked
            || self.busy()
            || (self.resource_form_open && self.selected.as_deref() == Some(name.as_str()))
        {
            return;
        }
        if self.resource_dirty {
            let view = cx.weak_entity();
            window.open_alert_dialog(cx, move |dialog, _, _| {
                let view = view.clone();
                let name = name.clone();
                dialog
                    .title("放弃远程资源修改？")
                    .description("切换资源会丢弃当前表单修改；已保存资源不受影响。")
                    .confirm()
                    .ok_text("放弃修改")
                    .cancel_text("取消")
                    .on_ok(move |_, window, cx| {
                        let _ = view.update(cx, |view, cx| {
                            view.select_resource_now(name.clone(), window, cx)
                        });
                        true
                    })
            });
        } else {
            self.select_resource_now(name, window, cx);
        }
    }

    fn select_resource_now(&mut self, name: String, window: &mut Window, cx: &mut Context<Self>) {
        self.selected = Some(name);
        self.creating = false;
        self.sync_resource(window, cx);
        self.error = None;
        self.notice = None;
        self.open_resource_dialog(window, cx);
    }

    pub(super) fn start_new(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.locked || self.busy() {
            return;
        }
        if self.resource_dirty {
            let view = cx.weak_entity();
            window.open_alert_dialog(cx, move |dialog, _, _| {
                let view = view.clone();
                dialog
                    .title("放弃远程资源修改？")
                    .description("新建资源会丢弃当前表单修改；已保存资源不受影响。")
                    .confirm()
                    .ok_text("放弃并新建")
                    .cancel_text("取消")
                    .on_ok(move |_, window, cx| {
                        let _ = view.update(cx, |view, cx| view.start_new_now(window, cx));
                        true
                    })
            });
        } else {
            self.start_new_now(window, cx);
        }
    }

    fn start_new_now(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.selected = None;
        self.creating = true;
        self.sync_resource(window, cx);
        self.error = None;
        self.notice = Some("填写名称与 URL，可识别资源类型".into());
        self.open_resource_dialog(window, cx);
    }

    pub(super) fn set_kind(&mut self, kind: RemoteKind, cx: &mut Context<Self>) {
        if self.locked || self.busy() || self.kind == kind {
            return;
        }
        self.kind = kind;
        self.resource_changed(cx);
    }

    pub(super) fn set_dialect(&mut self, dialect: ScriptDialect, cx: &mut Context<Self>) {
        if self.locked || self.busy() || !is_selectable(dialect) || self.dialect == dialect {
            return;
        }
        self.dialect = dialect;
        self.resource_changed(cx);
    }

    pub(super) fn set_enabled(&mut self, enabled: bool, cx: &mut Context<Self>) {
        if self.locked || self.busy() || self.enabled == enabled {
            return;
        }
        self.enabled = enabled;
        self.resource_changed(cx);
    }

    fn previous_dialect(&self) -> Option<ScriptDialect> {
        self.selected_resource()
            .filter(|_| !self.creating)
            .map(|resource| resource.dialect)
    }

    pub(super) fn resource_dialect_allowed(&self) -> bool {
        selection_allowed(self.dialect, self.previous_dialect())
    }

    pub(super) fn resource_dialect_status(&self) -> Option<&'static str> {
        if is_selectable(self.dialect) {
            None
        } else if self.resource_dialect_allowed() {
            Some("当前为历史方言，暂不可选；可保留原方言保存修改，或明确选择 Surge / QX。")
        } else {
            Some("检测方言暂不可用，不能保存；请明确选择 Surge / QX，选择不会转换配置内容。")
        }
    }

    pub(super) fn form_resource(&self, cx: &Context<Self>) -> Result<RemoteResource, String> {
        validate_selection(self.dialect, self.previous_dialect())
            .map_err(|error| error.to_string())?;
        let name = self.inputs.name.read(cx).value().trim().to_string();
        if name.is_empty() {
            return Err("资源名称不能为空".into());
        }
        let url = self.inputs.url.read(cx).value().trim().to_string();
        if url.is_empty() {
            return Err("资源 URL 不能为空".into());
        }
        let interval = self
            .inputs
            .interval
            .read(cx)
            .value()
            .trim()
            .parse::<u64>()
            .map_err(|_| "更新间隔必须是正整数".to_string())?;
        if interval == 0 {
            return Err("更新间隔必须大于 0".into());
        }
        let interval = super::interval::to_seconds(
            interval,
            self.inputs
                .interval_unit
                .read(cx)
                .selected_value()
                .copied()
                .unwrap_or("天"),
        )?;
        let icon = self.inputs.icon.read(cx).value().trim().to_string();
        let icon = (!icon.is_empty()).then_some(icon);
        lanpath_client::validate_remote_url(&icon)?;
        Ok(RemoteResource {
            name,
            url,
            kind: self.kind,
            dialect: self.dialect,
            update_interval_secs: interval,
            enabled: self.enabled,
            description: text_option(self.inputs.description.read(cx).value().as_ref()),
            argument_values: parse_argument_values(
                &self.inputs.arguments.read(cx).text().to_string(),
            )?,
            icon,
            arguments: self.arguments.clone(),
        })
    }
}

fn parse_argument_values(content: &str) -> Result<Vec<(String, String)>, String> {
    let mut values = Vec::new();
    let mut keys = HashSet::new();
    for (line_ix, raw) in content.lines().enumerate() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            return Err(format!("模块参数第 {} 行缺少 '='", line_ix + 1));
        };
        let key = key.trim();
        if key.is_empty() {
            return Err(format!("模块参数第 {} 行的键为空", line_ix + 1));
        }
        if !keys.insert(key.to_string()) {
            return Err(format!("模块参数键 '{key}' 重复"));
        }
        let value = value.trim();
        if !value.is_empty() {
            values.push((key.to_string(), value.to_string()));
        }
    }
    Ok(values)
}

fn text_option(value: &str) -> Option<String> {
    let value = value.trim();
    (!value.is_empty()).then(|| value.to_string())
}

pub(super) fn kind_label(kind: RemoteKind) -> &'static str {
    match kind {
        RemoteKind::Script => "纯 JS 脚本",
        RemoteKind::Snippet => "配置片段",
    }
}

pub(super) fn dialect_label(dialect: ScriptDialect) -> &'static str {
    match dialect {
        ScriptDialect::Surge => "Surge",
        ScriptDialect::Loon => "Loon",
        ScriptDialect::QuantumultX => "Quantumult X",
        ScriptDialect::Stash => "Stash",
    }
}
