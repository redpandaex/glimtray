use std::collections::{BTreeMap, HashMap};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PendingWrite {
    pub id: String,
    pub percent: u8,
    pub revision: u64,
}

#[derive(Default)]
pub struct PendingWrites(BTreeMap<String, PendingWrite>);

impl PendingWrites {
    pub fn push(&mut self, write: PendingWrite) {
        self.0.insert(write.id.clone(), write);
    }

    pub fn drain(&mut self) -> Vec<PendingWrite> {
        std::mem::take(&mut self.0).into_values().collect()
    }

    pub fn clear(&mut self) {
        self.0.clear();
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

#[derive(Default)]
pub struct Revisions(HashMap<String, u64>);

impl Revisions {
    pub fn resolve(&mut self, id: &str, revision: u64, success: bool) -> bool {
        if success {
            self.accept(id, revision)
        } else {
            self.0.remove(id);
            true
        }
    }

    pub fn record(&mut self, id: &str, revision: u64) {
        self.0.insert(id.into(), revision);
    }

    pub fn accept(&mut self, id: &str, revision: u64) -> bool {
        if self.0.get(id) == Some(&revision) {
            self.0.remove(id);
            true
        } else {
            false
        }
    }

    pub fn clear(&mut self) {
        self.0.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(id: &str, percent: u8, revision: u64) -> PendingWrite {
        PendingWrite {
            id: id.into(),
            percent,
            revision,
        }
    }

    #[test]
    fn coalesces_only_the_same_display_and_keeps_the_latest_target() {
        let mut pending = PendingWrites::default();
        pending.push(write("a", 20, 1));
        pending.push(write("b", 60, 2));
        pending.push(write("a", 80, 3));
        assert_eq!(pending.drain(), vec![write("a", 80, 3), write("b", 60, 2)]);
        assert!(pending.is_empty());
    }

    #[test]
    fn refresh_discards_writes_for_old_device_handles() {
        let mut pending = PendingWrites::default();
        pending.push(write("old:display", 20, 1));
        pending.clear();
        assert!(pending.drain().is_empty());
    }

    #[test]
    fn an_old_ack_cannot_overwrite_a_newer_slider_target() {
        let mut revisions = Revisions::default();
        revisions.record("a", 1);
        revisions.record("a", 2);
        assert!(!revisions.accept("a", 1));
        assert!(revisions.accept("a", 2));
        assert!(!revisions.accept("a", 2));
    }

    #[test]
    fn refresh_invalidates_outstanding_acknowledgements() {
        let mut revisions = Revisions::default();
        revisions.record("a", 1);
        revisions.clear();
        assert!(!revisions.accept("a", 1));
    }

    #[test]
    fn an_older_failure_invalidates_all_pending_revisions_for_the_device() {
        let mut revisions = Revisions::default();
        revisions.record("a", 1);
        revisions.record("a", 2);
        assert!(revisions.resolve("a", 1, false));
        assert!(!revisions.accept("a", 2));
    }
}
