use super::ProxyView;
use lanpath_client::proxies::GroupView;

pub(crate) struct SelectionStatus {
    label: String,
    detail: String,
}

impl SelectionStatus {
    pub(crate) fn label(&self) -> &str {
        &self.label
    }
    pub(crate) fn detail(&self) -> &str {
        &self.detail
    }
}

impl ProxyView {
    /// Runtime group selection, never the list's keyboard highlight or a draft.
    pub(crate) fn selection_status(&self) -> SelectionStatus {
        let mode = match self.mode.as_str() {
            "direct" => "直连",
            "global" => "全局",
            _ => "规则",
        };
        if !self.running {
            return SelectionStatus {
                label: format!("{mode} · 未运行"),
                detail: format!("当前模式：{mode}\n启动核心后显示策略组与节点"),
            };
        }
        if let Some(error) = &self.load_error {
            return SelectionStatus {
                label: "节点状态不可用".into(),
                detail: error.clone(),
            };
        }
        if self.mode == "direct" {
            return SelectionStatus {
                label: "直连".into(),
                detail: "当前模式：直连".into(),
            };
        }
        let group = self
            .active_group
            .as_deref()
            .and_then(|name| self.groups.iter().find(|group| group.name == name));
        let Some(group) = group else {
            return SelectionStatus {
                label: if self.loaded {
                    "无策略组"
                } else {
                    "正在读取节点"
                }
                .into(),
                detail: "打开代理策略查看当前模式和可用节点".into(),
            };
        };
        let (leaf, chain) = resolve_selection(&self.groups, &group.name);
        SelectionStatus {
            label: format!("{mode} · {leaf}"),
            detail: format!("当前策略组：{chain}\n模式：{mode}；不同规则可使用不同出口"),
        }
    }
}

fn resolve_selection(groups: &[GroupView], root: &str) -> (String, String) {
    let mut chain = vec![root];
    let mut next = root;
    while let Some(group) = groups.iter().find(|group| group.name == next) {
        if group.now.is_empty() {
            return ("待选择".into(), chain.join(" → "));
        }
        next = &group.now;
        if chain.contains(&next) {
            return ("策略组循环".into(), chain.join(" → "));
        }
        chain.push(next);
    }
    let label = match next.to_ascii_lowercase().as_str() {
        "direct" => "直连",
        "reject" | "block" => "阻断",
        _ => next,
    };
    (label.into(), chain.join(" → "))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn group(name: &str, now: &str) -> GroupView {
        GroupView {
            name: name.into(),
            now: now.into(),
            group_type: "Selector".into(),
            members: vec![now.into()],
        }
    }
    #[test]
    fn follows_confirmed_nested_selection_without_using_highlighted_or_first_member() {
        let groups = [group("proxy", "auto"), group("auto", "Hong Kong 02")];
        assert_eq!(
            resolve_selection(&groups, "proxy"),
            ("Hong Kong 02".into(), "proxy → auto → Hong Kong 02".into())
        );
    }
    #[test]
    fn cycles_and_unselected_groups_do_not_report_a_node() {
        assert_eq!(
            resolve_selection(&[group("a", "b"), group("b", "a")], "a").0,
            "策略组循环"
        );
        assert_eq!(resolve_selection(&[group("a", "")], "a").0, "待选择");
    }
    #[test]
    fn direct_and_reject_are_states_rather_than_proxy_nodes() {
        assert_eq!(
            resolve_selection(&[group("proxy", "DIRECT")], "proxy").0,
            "直连"
        );
        assert_eq!(
            resolve_selection(&[group("proxy", "block")], "proxy").0,
            "阻断"
        );
    }
}
