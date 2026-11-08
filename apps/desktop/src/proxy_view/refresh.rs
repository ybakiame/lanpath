use super::{Operation, ProxyView};
use gpui_kit::{AppContext as _, Context};

impl ProxyView {
    pub(super) fn refresh(&mut self, cx: &mut Context<Self>) {
        if !self.running || self.closing || self.operation.is_some() {
            return;
        }
        self.snapshot_task = None;
        self.operation = Some(Operation::Refresh);
        self.load_error = None;
        self.task = None;
        let generation = self.generation;
        let core = self.core.clone();
        let service = self.service.clone();
        self.task = Some(cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn(async move {
                    core.block_on(async { service.list().await })
                        .and_then(|result| result)
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.operation = None;
                if generation == this.generation && this.running {
                    match result {
                        Ok(list) => this.apply_list(list, cx),
                        Err(error) => this.load_error = Some(error),
                    }
                }
                cx.notify();
            });
        }));
        cx.notify();
    }

    /// The global status bar shares this feature's five-second snapshot stream.
    /// Background reads neither lock commands nor notify unchanged snapshots.
    pub(super) fn poll_snapshot(&mut self, cx: &mut Context<Self>) {
        if self.snapshot_task.is_some() {
            return;
        }
        let generation = self.generation;
        let core = self.core.clone();
        let service = self.service.clone();
        self.snapshot_task = Some(cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn(async move {
                    core.block_on(async { service.list().await })
                        .and_then(|result| result)
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.snapshot_task = None;
                if generation != this.generation || !this.running || this.closing || this.busy() {
                    return;
                }
                match result {
                    Ok(list) => {
                        if this.loaded
                            && this.load_error.is_none()
                            && this.groups == list.groups
                            && this.list.read(cx).delegate().matches_nodes(&list.nodes)
                        {
                            return;
                        }
                        this.apply_list(list, cx);
                    }
                    Err(error) => {
                        if this.load_error.as_ref() == Some(&error) {
                            return;
                        }
                        this.load_error = Some(error);
                    }
                }
                cx.notify();
            });
        }));
    }
}
