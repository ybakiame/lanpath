use gpui_kit::component::input::{EditorState, InputEvent, InputState};
use gpui_kit::{AppContext as _, Context, Entity, Subscription, Window};

use super::RulesView;

#[derive(Clone, Copy)]
pub(super) enum Field {
    RuleName,
    RuleTarget,
    RuleOutbound,
    RuleNote,
    SetName,
    SetTag,
    SetCategory,
    SetUrl,
    SetManual,
    BatchCategory,
    MarketSearch,
    MarketCategory,
}

pub(super) struct FormInputs {
    pub(super) rule_name: Entity<InputState>,
    pub(super) rule_target: Entity<InputState>,
    pub(super) rule_outbound: Entity<InputState>,
    pub(super) rule_note: Entity<InputState>,
    pub(super) set_name: Entity<InputState>,
    pub(super) set_tag: Entity<InputState>,
    pub(super) set_category: Entity<InputState>,
    pub(super) set_url: Entity<InputState>,
    pub(super) set_manual: Entity<EditorState>,
    pub(super) batch_category: Entity<InputState>,
    pub(super) market_search: Entity<InputState>,
    pub(super) market_category: Entity<InputState>,
}

impl FormInputs {
    pub(super) fn new(
        window: &mut Window,
        cx: &mut Context<RulesView>,
    ) -> (Self, Vec<Subscription>) {
        macro_rules! input {
            ($placeholder:literal) => {
                cx.new(|cx| InputState::new(window, cx).placeholder($placeholder))
            };
        }
        let inputs = Self {
            rule_name: input!("规则名称（可选）"),
            rule_target: input!("匹配目标；规则集 tag 用逗号分隔"),
            rule_outbound: input!("指定出站 tag"),
            rule_note: input!("备注（可选）"),
            set_name: input!("规则集名称"),
            set_tag: input!("唯一引用 tag"),
            set_category: input!("分类（可选）"),
            set_url: input!("https://example.com/rules.srs"),
            set_manual: cx.new(|cx| {
                EditorState::new(window, cx)
                    .language("json")
                    .line_number(true)
            }),
            batch_category: input!("批量分类；留空表示未分类"),
            market_search: input!("检索 cn、ads、netflix…"),
            market_category: input!("添加到分类（可选）"),
        };
        let mut subscriptions = Vec::new();
        macro_rules! watch {
            ($input:ident, $field:ident) => {{
                let entity = inputs.$input.clone();
                subscriptions.push(cx.subscribe(
                    &inputs.$input,
                    move |this, _, event: &InputEvent, cx| {
                        if matches!(event, InputEvent::Change) && !this.syncing {
                            this.update_field(
                                Field::$field,
                                entity.read(cx).value().to_string(),
                                cx,
                            );
                        }
                    },
                ));
            }};
        }
        watch!(rule_name, RuleName);
        watch!(rule_target, RuleTarget);
        watch!(rule_outbound, RuleOutbound);
        watch!(rule_note, RuleNote);
        watch!(set_name, SetName);
        watch!(set_tag, SetTag);
        watch!(set_category, SetCategory);
        watch!(set_url, SetUrl);
        watch!(batch_category, BatchCategory);
        watch!(market_search, MarketSearch);
        watch!(market_category, MarketCategory);
        let manual = inputs.set_manual.clone();
        subscriptions.push(cx.subscribe(
            &inputs.set_manual,
            move |this, _, event: &InputEvent, cx| {
                if matches!(event, InputEvent::Change) && !this.syncing {
                    this.update_field(Field::SetManual, manual.read(cx).value().to_string(), cx);
                }
            },
        ));
        (inputs, subscriptions)
    }

    pub(super) fn set_locked(&self, locked: bool, cx: &mut Context<RulesView>) {
        for input in [
            &self.rule_name,
            &self.rule_target,
            &self.rule_outbound,
            &self.rule_note,
            &self.set_name,
            &self.set_tag,
            &self.set_category,
            &self.set_url,
            &self.batch_category,
            &self.market_search,
            &self.market_category,
        ] {
            input.update(cx, |state, cx| state.set_readonly(locked, cx));
        }
        self.set_manual
            .update(cx, |state, cx| state.set_readonly(locked, cx));
    }
}
