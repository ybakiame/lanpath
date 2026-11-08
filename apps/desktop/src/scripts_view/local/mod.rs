mod dialog;
mod form;
mod operations;
mod render;

use super::ScriptsView;
use gpui_kit::component::input::{EditorState, InputEvent, InputState};
use gpui_kit::{App, AppContext as _, Context, Entity, Image, ImageFormat, Subscription, Window};
use lanpath_client::remote::{LocalDocument, LocalIcon, LocalResourceStore, ScriptDialect};

pub(super) struct AssetInput {
    reference: String,
    editor: Entity<EditorState>,
    _subscription: Subscription,
}
pub(super) struct LocalState {
    rows: Vec<LocalDocument>,
    load_error: Option<String>,
    original: Option<LocalDocument>,
    name: Entity<InputState>,
    content: Entity<EditorState>,
    assets: Vec<AssetInput>,
    icon: Option<LocalIcon>,
    dialect: ScriptDialect,
    enabled: bool,
    pub(super) open: bool,
    confirming: bool,
    form_tab: usize,
    asset_index: usize,
    advisories: Vec<String>,
    pub(super) dirty: bool,
}
impl LocalState {
    pub(super) fn new(
        service: Option<&crate::business::BusinessService>,
        window: &mut Window,
        cx: &mut Context<ScriptsView>,
    ) -> (Self, Vec<Subscription>) {
        let loaded = service
            .ok_or_else(|| "实验数据目录不可用".to_owned())
            .and_then(|service| {
                LocalResourceStore::new(service.data_dir())
                    .load()
                    .map_err(|error| error.to_string())
            });
        let (rows, load_error) = match loaded {
            Ok(rows) => (rows, None),
            Err(error) => (Vec::new(), Some(error)),
        };
        let name = cx.new(|cx| InputState::new(window, cx).placeholder("资源名称"));
        let content = cx.new(|cx| EditorState::new(window, cx).line_number(true));
        let subscriptions = vec![
            cx.subscribe(&name, |this, _, event: &InputEvent, cx| {
                if matches!(event, InputEvent::Change) && !this.syncing {
                    this.local_changed(cx);
                }
            }),
            cx.subscribe(&content, |this, _, event: &InputEvent, cx| {
                if matches!(event, InputEvent::Change) && !this.syncing {
                    this.local.advisories.clear();
                    this.local_changed(cx);
                }
            }),
        ];
        (
            Self {
                rows,
                load_error,
                original: None,
                name,
                content,
                assets: Vec::new(),
                icon: None,
                dialect: ScriptDialect::Surge,
                enabled: true,
                open: false,
                confirming: false,
                form_tab: 0,
                asset_index: 0,
                advisories: Vec::new(),
                dirty: false,
            },
            subscriptions,
        )
    }
    fn asset_values(&self, cx: &App) -> Vec<(String, String)> {
        self.assets
            .iter()
            .map(|asset| {
                (
                    asset.reference.clone(),
                    asset.editor.read(cx).text().to_string(),
                )
            })
            .collect()
    }
}
impl ScriptsView {
    fn local_store(&self) -> Result<LocalResourceStore, String> {
        self.service
            .as_ref()
            .map(|service| LocalResourceStore::new(service.data_dir()))
            .ok_or_else(|| "实验数据目录不可用".into())
    }
    fn local_changed(&mut self, cx: &mut Context<Self>) {
        self.local.dirty = true;
        self.error = None;
        self.notice = None;
        cx.notify();
    }
    fn set_local_assets(
        &mut self,
        values: Vec<(String, String)>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.local.assets = values
            .into_iter()
            .map(|(reference, text)| {
                let editor = cx.new(|cx| {
                    EditorState::new(window, cx)
                        .language("javascript")
                        .line_number(true)
                });
                editor.update(cx, |state, cx| state.set_value(text, window, cx));
                let subscription = cx.subscribe(&editor, |this, _, event: &InputEvent, cx| {
                    if matches!(event, InputEvent::Change) && !this.syncing {
                        this.local_changed(cx);
                    }
                });
                AssetInput {
                    reference,
                    editor,
                    _subscription: subscription,
                }
            })
            .collect();
    }
}
fn icon_image(icon: &LocalIcon) -> Option<std::sync::Arc<Image>> {
    use base64::Engine as _;
    let format = match icon.mime.as_str() {
        "image/png" => ImageFormat::Png,
        "image/jpeg" => ImageFormat::Jpeg,
        "image/webp" => ImageFormat::Webp,
        _ => return None,
    };
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(&icon.base64)
        .ok()?;
    Some(std::sync::Arc::new(Image::from_bytes(format, bytes)))
}
