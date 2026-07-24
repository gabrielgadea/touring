//! `Assists` — accumulator for collecting applicable assists.

use super::{Assist, AssistGroup, AssistId, AssistTarget};
use std::collections::BTreeMap;

/// Accumulates assists discovered by handlers.
/// Assists are grouped by file and sorted by priority.
#[derive(Default)]
pub struct Assists {
    assists: Vec<Assist>,
}

impl Assists {
    /// Create an empty accumulator.
    pub fn new() -> Self {
        Self::default()
    }

    /// Add an assist to the accumulator.
    pub fn add(&mut self, assist: Assist) {
        self.assists.push(assist);
    }

    /// Add an assist with a group.
    pub fn add_with_group(
        &mut self,
        id: AssistId,
        label: String,
        group: AssistGroup,
        target: AssistTarget,
        source_change: super::LazySourceChange,
    ) {
        self.add(Assist::new(id, label, group, target, source_change));
    }

    /// Return all assists sorted by priority (highest first).
    pub fn finish(self) -> Vec<Assist> {
        let mut assists = self.assists;
        assists.sort_by_key(|b| std::cmp::Reverse(b.priority));
        assists
    }

    /// Return assists grouped by file.
    pub fn by_file(self) -> BTreeMap<touring_generator::FileId, Vec<Assist>> {
        let mut by_file: BTreeMap<touring_generator::FileId, Vec<Assist>> = BTreeMap::new();
        for assist in self.assists {
            by_file
                .entry(assist.target.file_id)
                .or_default()
                .push(assist);
        }
        by_file
    }

    /// Return assists filtered by group.
    pub fn filter_group(self, group: &AssistGroup) -> Vec<Assist> {
        self.assists
            .into_iter()
            .filter(|a| &a.group == group)
            .collect()
    }

    /// Return the count of assists.
    pub fn len(&self) -> usize {
        self.assists.len()
    }

    /// Return true if no assists were accumulated.
    pub fn is_empty(&self) -> bool {
        self.assists.is_empty()
    }
}
