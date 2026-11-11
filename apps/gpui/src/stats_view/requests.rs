#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Request {
    Query(u64),
    Clear,
}

#[derive(Default)]
pub(super) struct StatsRequests {
    generation: u64,
    busy: bool,
    clear_pending: bool,
    clearing: bool,
}

impl StatsRequests {
    pub(super) fn invalidate(&mut self) {
        self.generation += 1;
    }

    pub(super) fn request_clear(&mut self) {
        if !self.clearing() {
            self.invalidate();
            self.clear_pending = true;
        }
    }

    pub(super) fn busy(&self) -> bool {
        self.busy
    }

    pub(super) fn clearing(&self) -> bool {
        self.clearing || self.clear_pending
    }

    pub(super) fn begin(&mut self, active: bool) -> Option<Request> {
        if self.busy || (!active && !self.clear_pending) {
            return None;
        }
        self.busy = true;
        if self.clear_pending {
            self.clear_pending = false;
            self.clearing = true;
            Some(Request::Clear)
        } else {
            Some(Request::Query(self.generation))
        }
    }

    pub(super) fn complete(&mut self, request: Request) -> bool {
        self.busy = false;
        match request {
            Request::Query(generation) => generation == self.generation && !self.clear_pending,
            Request::Clear => {
                self.clearing = false;
                self.invalidate();
                true
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Request, StatsRequests};

    #[test]
    fn clear_waits_for_pending_query_and_rejects_its_rows_even_when_hidden() {
        let mut requests = StatsRequests::default();
        let old_query = requests.begin(true).unwrap();
        requests.request_clear();
        assert!(requests.clearing());
        assert_eq!(requests.begin(true), None);
        assert!(!requests.complete(old_query));
        assert_eq!(requests.begin(false), Some(Request::Clear));
        assert!(requests.complete(Request::Clear));
        assert!(!requests.clearing());
        assert_eq!(requests.begin(false), None);
        let reopened_query = requests.begin(true).unwrap();
        assert_ne!(reopened_query, old_query);
        assert!(requests.complete(reopened_query));
    }

    #[test]
    fn multiple_query_edits_coalesce_without_accepting_previous_snapshot() {
        let mut requests = StatsRequests::default();
        let old_query = requests.begin(true).unwrap();
        requests.invalidate();
        requests.invalidate();
        assert_eq!(requests.begin(true), None);
        assert!(!requests.complete(old_query));
        let latest_query = requests.begin(true).unwrap();
        assert_eq!(latest_query, Request::Query(2));
        assert!(requests.complete(latest_query));
    }
}
