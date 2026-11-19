//! 核心页跨页面库存活动：初始化快照与 revision monitor。页面 owner 只拥有
//! UI 观察；真正的在途状态由 `CoreInventory` 按数据根共享。

use std::time::Duration;

use nsh_core::prelude::*;

use super::{CorePageState, stage_label};
use crate::app_state;
use crate::core_inventory::{InventoryActivity, InventoryActivityKind};

/// 读取活动初始化快照：revision 必须先于 activity。worker 若在两次读取
/// 之间完成，基线仍是完成前版本，monitor 下一次 tick 能看到版本变化；
/// 反过来会得到“旧 activity + 新 revision”，永久 busy。
pub(super) fn read_activity_initial<A>(
    read_revision: impl FnOnce() -> u64,
    read_activity: impl FnOnce() -> Option<A>,
) -> (u64, Option<A>) {
    (read_revision(), read_activity())
}

/// 跨页面共享活动对应的真实阶段文案。
pub(super) fn activity_stage(activity: &InventoryActivity) -> String {
    match activity.kind {
        InventoryActivityKind::Download => activity
            .stage
            .map(|stage| stage_label(stage).to_owned())
            .unwrap_or_else(|| "正在下载并校验核心…".to_owned()),
        InventoryActivityKind::Select => "正在探测并保存选中核心…".to_owned(),
        InventoryActivityKind::Delete => "正在删除核心版本目录…".to_owned(),
    }
}

/// 观察同根库存活动版本：旧页面离页后业务 worker 仍可能完成，新页面
/// 据此清除 busy、重载本地列表并同步 App 级核心摘要。
pub(super) fn spawn_inventory_activity_monitor(state: CorePageState) {
    let Some(inventory) = state.inventory.clone() else {
        return;
    };
    // 使用页面初始化时记录的基线，而不是启动 monitor 时重新读取；后者会
    // 吞掉“初始化读 activity → monitor 记 revision”之间的完成。
    let revision = signal(state.shared_activity_revision);
    set_interval(Duration::from_millis(200), move || {
        let current = inventory.activity_revision();
        if current == revision.get() {
            return;
        }
        revision.set(current);
        let activity = inventory.activity();
        match activity {
            Some(activity) => {
                state.busy.set(true);
                state.stage.set(Some(activity_stage(&activity)));
            }
            None => {
                let was_busy = state.busy.get();
                state.busy.set(false);
                state.stage.set(None);
                if was_busy {
                    state.reload_local();
                    app_state::refresh_active_core(&app_state::current());
                }
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;

    use super::read_activity_initial;

    /// 可控完成窗口：在读取 revision 之后、读取 activity 之前完成 worker。
    /// 快照必须保留完成前的 revision，让 monitor 下一次 tick 看到版本变化；
    /// 若实现反过来先读 activity，将得到“有活动 + 新 revision”并永久 busy。
    #[test]
    fn activity_snapshot_keeps_pre_completion_revision() {
        let revision = Cell::new(7_u64);
        let activity = Cell::new(Some("download"));
        let (base, initial) = read_activity_initial(
            || revision.get(),
            || {
                // worker 恰在两次读取之间完成。
                revision.set(8);
                activity.set(None);
                activity.get()
            },
        );
        assert_eq!(base, 7, "基线必须是完成前的 revision");
        assert_eq!(initial, None, "完成后 activity 已清除");
        assert_ne!(
            revision.get(),
            base,
            "monitor 下一次读取能看到完成后的版本变化"
        );
    }
}
