/// Process-local startup bookkeeping. The caller holds the startup-discovery
/// mutex while changing this state and the runtime's network availability.
#[derive(Default)]
pub(crate) struct StartupDiscovery {
    started: bool,
    running: bool,
    available: bool,
    network_refresh_pending: bool,
}

impl StartupDiscovery {
    #[cfg(test)]
    pub(crate) fn is_running(&self) -> bool {
        self.running
    }

    pub(crate) fn start(&mut self, available: bool) -> bool {
        if self.started {
            return false;
        }
        self.started = true;
        self.running = true;
        self.available = available;
        self.network_refresh_pending = !available;
        true
    }

    /// Returns true only when reconnection must launch a deferred refresh.
    pub(crate) fn network_changed(&mut self, available: bool) -> bool {
        self.available = available;
        if !self.started {
            return false;
        }
        if !available && self.running {
            self.network_refresh_pending = true;
        }
        if available && self.network_refresh_pending && !self.running {
            self.running = true;
            return true;
        }
        false
    }

    pub(crate) fn begin_refresh(&mut self) {
        self.network_refresh_pending = !self.available;
    }

    /// A path change during a pass can leave earlier managers unchecked. Replay
    /// once online, rather than losing the deferred work at the completion edge.
    pub(crate) fn finish_refresh(&mut self) -> bool {
        if self.available && self.network_refresh_pending {
            return true;
        }
        self.running = false;
        false
    }
}

#[cfg(test)]
mod tests {
    use super::StartupDiscovery;

    #[test]
    fn discovery_is_once_per_process_and_reconnect_resumes_only_refresh() {
        let mut state = StartupDiscovery::default();
        assert!(state.start(false));
        assert!(!state.start(true));
        state.begin_refresh();
        assert!(!state.finish_refresh());
        assert!(state.network_changed(true));
        assert!(!state.network_changed(true));
        state.begin_refresh();
        assert!(!state.finish_refresh());
        assert!(!state.network_changed(true));
        assert!(!state.start(true));
        assert!(StartupDiscovery::default().start(true));
    }

    #[test]
    fn reconnect_during_discovery_needs_no_second_pass() {
        let mut state = StartupDiscovery::default();
        assert!(state.start(false));
        assert!(!state.network_changed(true));
        state.begin_refresh();
        assert!(!state.finish_refresh());
    }

    #[test]
    fn reconnect_during_or_after_refresh_cannot_drop_network_work() {
        for reconnect_before_completion in [false, true] {
            let mut state = StartupDiscovery::default();
            assert!(state.start(true));
            state.begin_refresh();
            assert!(!state.network_changed(false));
            if reconnect_before_completion {
                assert!(!state.network_changed(true));
                assert!(state.finish_refresh());
            } else {
                assert!(!state.finish_refresh());
                assert!(state.network_changed(true));
            }
            state.begin_refresh();
            assert!(!state.finish_refresh());
        }
    }

    #[test]
    fn legacy_network_changes_do_not_schedule_startup_work() {
        let mut state = StartupDiscovery::default();
        assert!(!state.network_changed(false));
        assert!(!state.network_changed(true));
    }

    #[test]
    fn finished_online_startup_does_not_replace_normal_refresh_scheduling() {
        let mut state = StartupDiscovery::default();
        assert!(state.start(true));
        state.begin_refresh();
        assert!(!state.finish_refresh());
        assert!(!state.network_changed(false));
        assert!(!state.network_changed(true));
    }
}
