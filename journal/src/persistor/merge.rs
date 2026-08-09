use super::*;

pub(super) struct MergePlan {
    pub(super) branches: HashMap<Word, (Word, Word, Word)>,
    pub(super) leaves: HashMap<Word, Vec<u8>>,
    pub(super) stumps: HashMap<Word, Word>,
    pub(super) deltas: HashMap<Word, isize>,
    pub(super) deletes: HashSet<Word>,
}

impl MergePlan {
    pub(super) fn new() -> Self {
        Self {
            branches: HashMap::new(),
            leaves: HashMap::new(),
            stumps: HashMap::new(),
            deltas: HashMap::new(),
            deletes: HashSet::new(),
        }
    }

    pub(super) fn delta_add(&mut self, node: Word, amount: isize) {
        let next = self.deltas.get(&node).copied().unwrap_or(0) + amount;
        if next == 0 {
            self.deltas.remove(&node);
        } else {
            self.deltas.insert(node, next);
        }
    }
}
