use super::*;

impl DesktopView {
    pub(super) fn sync_request_lock(&mut self, cx: &mut Context<Self>) {
        let backup_work = self.backups.read(cx).busy() || self.backups.read(cx).is_pending();
        let distribution_work =
            self.distribution.read(cx).busy() || self.distribution.read(cx).is_pending();
        let recovery = self.core_recovery_needed && !self.core_running;
        let lifecycle = self.closing
            || self.core_busy.is_some()
            || backup_work
            || distribution_work
            || recovery;
        let core_work = self.cores.read(cx).busy();
        let diagnosing = self.diagnostics.read(cx).busy();
        let other_config_work = self.configuration.read(cx).busy()
            || self.subscriptions.read(cx).busy()
            || self.profiles.read(cx).busy()
            || self.structured.read(cx).busy()
            || self.rules.read(cx).busy()
            || self.mitm.read(cx).busy();
        let config_work = other_config_work || self.scripts.read(cx).busy();
        let system_work = self.system.read(cx).busy();
        let request_work = self.requests.read(cx).busy();
        let peer_work = config_work || request_work || system_work || self.proxy.read(cx).busy();
        self.backups.update(cx, |view, cx| {
            view.set_locked(
                self.closing
                    || self.core_busy.is_some()
                    || distribution_work
                    || recovery
                    || peer_work
                    || core_work
                    || diagnosing,
                cx,
            )
        });
        self.appearance.update(cx, |view, cx| {
            view.set_closing(self.closing || backup_work || distribution_work, cx)
        });
        let drafts = self.subscriptions.read(cx).dirty()
            || self.profiles.read(cx).dirty()
            || self.structured.read(cx).dirty()
            || self.rules.read(cx).dirty()
            || self.scripts.read(cx).dirty()
            || self.mitm.read(cx).dirty()
            || self.requests.read(cx).dirty()
            || self.system.read(cx).dirty();
        let observation_work = self.stats.read(cx).busy()
            || self.logs.read(cx).busy(cx)
            || self.connections.read(cx).busy();
        let automatic_refresh_eligible = !lifecycle
            && self.restore_task.is_none()
            && !core_work
            && !diagnosing
            && !other_config_work
            && !request_work
            && !system_work
            && !self.proxy.read(cx).busy()
            && !drafts
            && !self.distribution.read(cx).dirty();
        self.scripts.update(cx, |view, _| {
            view.set_automatic_refresh_eligible(automatic_refresh_eligible);
        });
        self.distribution.update(cx, |view, cx| {
            view.set_locked(
                self.closing
                    || self.core_busy.is_some()
                    || self.core_running
                    || recovery
                    || backup_work
                    || peer_work
                    || core_work
                    || diagnosing
                    || drafts
                    || observation_work,
                cx,
            )
        });
        let quiescing = self.closing || backup_work || distribution_work;
        self.stats
            .update(cx, |view, cx| view.set_closing(quiescing, cx));
        self.logs
            .update(cx, |view, cx| view.set_closing(quiescing, cx));
        self.requests.update(cx, |view, cx| {
            view.set_locked(
                lifecycle
                    || self.core_running
                    || config_work
                    || core_work
                    || diagnosing
                    || system_work,
                cx,
            )
        });
        self.system.update(cx, |view, cx| {
            view.set_locked(
                lifecycle
                    || self.core_running
                    || config_work
                    || core_work
                    || diagnosing
                    || request_work,
                cx,
            )
        });
        self.cores.update(cx, |view, cx| {
            view.set_locked(
                lifecycle || self.core_running || peer_work || diagnosing,
                cx,
            )
        });
        self.diagnostics.update(cx, |view, cx| {
            view.set_locked(lifecycle || peer_work || core_work, cx)
        });
        self.proxy.update(cx, |view, cx| {
            view.set_locked(lifecycle || core_work || diagnosing || system_work, cx)
        });
        // Older peers notify even on equal lock values. Publish a transition
        // once, not another notification loop through their observers.
        let locked = lifecycle || self.core_running || core_work || diagnosing || system_work;
        if self.business_locked != locked {
            self.business_locked = locked;
            self.configuration
                .update(cx, |view, cx| view.set_locked(locked, cx));
            self.subscriptions
                .update(cx, |view, cx| view.set_locked(locked, cx));
            self.profiles
                .update(cx, |view, cx| view.set_locked(locked, cx));
            self.structured
                .update(cx, |view, cx| view.set_locked(locked, cx));
            self.rules
                .update(cx, |view, cx| view.set_locked(locked, cx));
            self.scripts
                .update(cx, |view, cx| view.set_locked(locked, cx));
            self.mitm.update(cx, |view, cx| view.set_locked(locked, cx));
        }
        cx.notify();
    }
}
