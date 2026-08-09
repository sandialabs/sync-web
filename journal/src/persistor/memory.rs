use super::*;

#[derive(Clone)]
pub struct MemoryPersistor {
    pub(super) roots: Arc<RwLock<HashMap<Word, (Word, bool)>>>,
    pub(super) branches: Arc<RwLock<HashMap<Word, (Word, Word, Word)>>>,
    pub(super) leaves: Arc<RwLock<HashMap<Word, Vec<u8>>>>,
    pub(super) stumps: Arc<RwLock<HashMap<Word, Word>>>,
    pub(super) references: Arc<RwLock<HashMap<Word, usize>>>,
    maximum_leaf_bytes: Option<usize>,
    #[cfg(feature = "wasm-kernel")]
    imported: Arc<RwLock<HashSet<Word>>>,
}

impl MemoryPersistor {
    pub fn new() -> Self {
        Self {
            roots: Arc::new(RwLock::new(HashMap::new())),
            branches: Arc::new(RwLock::new(HashMap::new())),
            leaves: Arc::new(RwLock::new(HashMap::new())),
            stumps: Arc::new(RwLock::new(HashMap::new())),
            references: Arc::new(RwLock::new(HashMap::new())),
            maximum_leaf_bytes: None,
            #[cfg(feature = "wasm-kernel")]
            imported: Arc::new(RwLock::new(HashSet::new())),
        }
    }

    pub(crate) fn isolated(maximum_leaf_bytes: usize) -> Self {
        Self {
            maximum_leaf_bytes: Some(maximum_leaf_bytes),
            ..Self::new()
        }
    }

    fn check_leaf_length(&self, length: usize) -> Result<(), PersistorAccessError> {
        match self.maximum_leaf_bytes {
            Some(maximum) if length > maximum => {
                Err(PersistorAccessError("Leaf size exceeds maximum".into()))
            }
            Some(_) => Ok(()),
            None => check_leaf_length(length),
        }
    }

    #[cfg(feature = "wasm-kernel")]
    pub(crate) fn mark_imported(&self, word: Word) {
        self.imported
            .write()
            .expect("Failed to lock imports")
            .insert(word);
    }

    #[cfg(feature = "wasm-kernel")]
    pub(crate) fn encode_graph(&self, root: Word) -> Vec<u8> {
        let mut encoded = Vec::new();
        let imported = self.imported.read().expect("Failed to lock imports");
        let branches = self.branches.read().expect("Failed to lock branches");
        let leaves = self.leaves.read().expect("Failed to lock leaves");
        let stumps = self.stumps.read().expect("Failed to lock stumps");
        let mut pending = vec![root];
        let mut seen = HashSet::new();
        while let Some(word) = pending.pop() {
            if word == [0; SIZE] || !seen.insert(word) || imported.contains(&word) {
                continue;
            }
            if let Some((left, right, digest)) = branches.get(&word) {
                encoded.push(1);
                encoded.extend(word);
                encoded.extend(left);
                encoded.extend(right);
                encoded.extend(digest);
                pending.push(*right);
                pending.push(*left);
            } else if let Some(content) = leaves.get(&word) {
                encoded.push(2);
                encoded.extend(word);
                encoded.extend(
                    u32::try_from(content.len())
                        .expect("leaf exceeds kernel response")
                        .to_be_bytes(),
                );
                encoded.extend(content);
            } else if let Some(digest) = stumps.get(&word) {
                encoded.push(3);
                encoded.extend(word);
                encoded.extend(digest);
            }
        }
        encoded
    }

    fn reference_increment(&self, node: Word) {
        let mut references = self.references.write().expect("Failed to lock references");
        match references.get(&node) {
            Some(count) => {
                let count_ = *count;
                references.insert(node, count_ + 1);
            }
            None => {
                references.insert(node, 1);
            }
        };
    }

    fn reference_decrement(&self, node: Word) {
        let mut references = self.references.write().expect("Failed to lock references");
        match references.get(&node) {
            Some(count_old) => {
                let count_new = *count_old - 1;
                if count_new > 0 {
                    references.insert(node, count_new);
                } else {
                    references.remove(&node);
                    let mut branches = self.branches.write().expect("Failed to lock branches");
                    if let Some((left, right, _)) = branches.get(&node) {
                        let left_ = *left;
                        let right_ = *right;
                        branches.remove(&node);
                        drop(references);
                        drop(branches);
                        self.reference_decrement(left_);
                        self.reference_decrement(right_);
                    } else {
                        let mut leaves = self.leaves.write().expect("Failed to lock leaves");
                        let mut stumps = self.stumps.write().expect("Failed to lock stumps");
                        if let Some(_) = leaves.get(&node) {
                            leaves.remove(&node);
                        } else if let Some(_) = stumps.get(&node) {
                            stumps.remove(&node);
                        }
                    }
                }
            }
            None => {}
        };
    }

    fn merged_branch(
        &self,
        node: Word,
        plan: &MergePlan,
    ) -> Result<Option<(Word, Word, Word)>, PersistorAccessError> {
        if let Some(branch) = plan.branches.get(&node) {
            return Ok(Some(*branch));
        }

        match self
            .branches
            .read()
            .expect("Failed to lock branches")
            .get(&node)
        {
            Some((left, right, digest)) => Ok(Some((*left, *right, *digest))),
            None => Ok(None),
        }
    }

    fn base_refcount(&self, node: Word) -> isize {
        self.references
            .read()
            .expect("Failed to lock references")
            .get(&node)
            .copied()
            .unwrap_or(0) as isize
    }

    fn merge_collect(
        &self,
        source: &dyn Persistor,
        node: Word,
        plan: &mut MergePlan,
        seen: &mut HashSet<Word>,
    ) -> Result<(), PersistorAccessError> {
        if node == [0 as u8; SIZE] || !seen.insert(node) {
            return Ok(());
        }

        if plan.branches.contains_key(&node)
            || plan.leaves.contains_key(&node)
            || plan.stumps.contains_key(&node)
        {
            return Ok(());
        }

        if self
            .leaves
            .read()
            .expect("Failed to lock leaves")
            .contains_key(&node)
            || self
                .stumps
                .read()
                .expect("Failed to lock stumps")
                .contains_key(&node)
            || self
                .branches
                .read()
                .expect("Failed to lock branches")
                .contains_key(&node)
        {
            return Ok(());
        }

        if let Ok(content) = source.leaf_get(node) {
            plan.leaves.insert(node, content);
            return Ok(());
        }

        if let Ok(digest) = source.stump_get(node) {
            plan.stumps.insert(node, digest);
            return Ok(());
        }

        if let Ok((left, right, digest)) = source.branch_get(node) {
            self.merge_collect(source, left, plan, seen)?;
            self.merge_collect(source, right, plan, seen)?;
            plan.branches.insert(node, (left, right, digest));
            plan.delta_add(left, 1);
            plan.delta_add(right, 1);
            return Ok(());
        }

        Err(PersistorAccessError(format!(
            "Cannot materialize referenced node {:?}",
            node
        )))
    }

    fn release_plan(&self, node: Word, plan: &mut MergePlan) -> Result<(), PersistorAccessError> {
        if node == [0 as u8; SIZE] {
            return Ok(());
        }

        let effective = self.base_refcount(node) + plan.deltas.get(&node).copied().unwrap_or(0);
        if effective <= 0 {
            return Ok(());
        }

        plan.delta_add(node, -1);

        if effective == 1 {
            if !plan.deletes.insert(node) {
                return Ok(());
            }

            if let Some((left, right, _)) = self.merged_branch(node, plan)? {
                self.release_plan(left, plan)?;
                self.release_plan(right, plan)?;
            }
        }

        Ok(())
    }
}

impl Persistor for MemoryPersistor {
    fn root_list(&self) -> Vec<Word> {
        let mut keys: Vec<Word> = self
            .roots
            .read()
            .expect("Failed to get locked roots")
            .iter()
            .filter(|&(_, &(_, is_persistent))| is_persistent)
            .map(|(key, _)| key)
            .cloned()
            .collect();
        keys.sort();
        keys
    }

    fn root_new(&self, handle: Word, root: Word) -> Result<Word, PersistorAccessError> {
        let mut roots = self.roots.write().expect("Failed to lock roots map");
        match roots.get(&handle) {
            Some(_) => Err(PersistorAccessError(format!(
                "Handle {:?} already exists",
                handle
            ))),
            None => {
                self.reference_increment(root);
                roots.insert(handle, (root, true));
                Ok(handle)
            }
        }
    }

    fn root_temp(&self, root: Word) -> Result<Word, PersistorAccessError> {
        let mut roots = self.roots.write().expect("Failed to lock roots map");
        let mut handle_: Word = [0 as u8; 32];
        rand::thread_rng().fill_bytes(&mut handle_);
        match roots.get(&handle_) {
            Some(_) => Err(PersistorAccessError(format!(
                "Handle {:?} already exists",
                handle_
            ))),
            None => {
                self.reference_increment(root);
                roots.insert(handle_, (root, false));
                Ok(handle_)
            }
        }
    }

    fn root_get(&self, handle: Word) -> Result<Word, PersistorAccessError> {
        match self
            .roots
            .read()
            .expect("Failed to lock roots map")
            .get(&handle)
        {
            Some((root, _)) => Ok(*root),
            None => Err(PersistorAccessError(format!(
                "Handle {:?} not found",
                handle
            ))),
        }
    }

    fn root_set(
        &self,
        handle: Word,
        old: Word,
        new: Word,
        source: &dyn Persistor,
    ) -> Result<Word, PersistorAccessError> {
        let status = {
            let roots = self.roots.write().expect("Failed to lock roots map");
            match roots.get(&handle) {
                Some((root, true)) if *root == old => 0,
                Some((_, false)) => 1,
                Some((_, true)) => 2,
                None => 3,
            }
        };

        match status {
            0 => {
                let mut plan = MergePlan::new();
                let mut seen = HashSet::new();
                self.merge_collect(source, new, &mut plan, &mut seen)?;
                plan.delta_add(new, 1);
                self.release_plan(old, &mut plan)?;

                {
                    let mut leaves = self.leaves.write().expect("Failed to lock leaves");
                    for (node, content) in plan.leaves.iter() {
                        leaves.insert(*node, content.clone());
                    }
                    for node in plan.deletes.iter() {
                        leaves.remove(node);
                    }
                }

                {
                    let mut stumps = self.stumps.write().expect("Failed to lock stumps");
                    for (node, digest) in plan.stumps.iter() {
                        stumps.insert(*node, *digest);
                    }
                    for node in plan.deletes.iter() {
                        stumps.remove(node);
                    }
                }

                {
                    let mut branches = self.branches.write().expect("Failed to lock branches");
                    for (node, branch) in plan.branches.iter() {
                        branches.insert(*node, *branch);
                    }
                    for node in plan.deletes.iter() {
                        branches.remove(node);
                    }
                }

                {
                    let mut references =
                        self.references.write().expect("Failed to lock references");
                    for (node, delta) in plan.deltas.iter() {
                        let base = references.get(node).copied().unwrap_or(0) as isize;
                        let next = base + delta;
                        if next > 0 {
                            references.insert(*node, next as usize);
                        } else {
                            references.remove(node);
                        }
                    }
                }

                let mut roots = self.roots.write().expect("Failed to lock roots map");
                roots.insert(handle, (new, true));
                Ok(handle)
            }
            1 => Err(PersistorAccessError(format!(
                "Handle {:?} is temporary",
                handle
            ))),
            2 => Err(PersistorAccessError(format!(
                "Handle {:?} changed since compare",
                handle
            ))),
            _ => Err(PersistorAccessError(format!(
                "Handle {:?} not found",
                handle
            ))),
        }
    }

    fn root_delete(&self, handle: Word) -> Result<(), PersistorAccessError> {
        let mut roots = self.roots.write().expect("Failed to lock roots map");
        match roots.get(&handle) {
            Some((old, _)) => {
                let old_ = *old;
                roots.remove(&handle);
                self.reference_decrement(old_);
                Ok(())
            }
            None => Err(PersistorAccessError(format!(
                "Handle {:?} not found",
                handle
            ))),
        }
    }

    fn branch_set(
        &self,
        left: Word,
        right: Word,
        digest: Word,
    ) -> Result<Word, PersistorAccessError> {
        let branch = branch_word(digest, left, right);
        let mut branches = self.branches.write().expect("Failed to lock branches map");
        branches.insert(branch, (left, right, digest));
        drop(branches);
        self.reference_increment(left);
        self.reference_increment(right);
        Ok(branch)
    }

    fn branch_get(&self, branch: Word) -> Result<(Word, Word, Word), PersistorAccessError> {
        let branches = self.branches.read().expect("Failed to lock branches map");
        match branches.get(&branch) {
            Some((left, right, digest)) => {
                assert_eq!(branch, branch_word(*digest, *left, *right));
                Ok((*left, *right, *digest))
            }
            None => Err(PersistorAccessError(format!(
                "Branch {:?} not found",
                branch
            ))),
        }
    }

    fn leaf_set(&self, content: Vec<u8>) -> Result<Word, PersistorAccessError> {
        self.check_leaf_length(content.len())?;
        let leaf = leaf_word(&content);
        self.leaves
            .write()
            .expect("Failed to lock leaves map")
            .insert(leaf, content);
        Ok(leaf)
    }

    fn leaf_get(&self, leaf: Word) -> Result<Vec<u8>, PersistorAccessError> {
        let leaves = self.leaves.read().expect("Failed to lock leaves map");
        match leaves.get(&leaf) {
            Some(content) => {
                self.check_leaf_length(content.len())?;
                if leaf != leaf_word(content) {
                    return Err(PersistorAccessError("Leaf digest mismatch".into()));
                }
                Ok(content.to_vec())
            }
            None => Err(PersistorAccessError(format!("Leaf {:?} not found", leaf))),
        }
    }

    fn stump_set(&self, digest: Word) -> Result<Word, PersistorAccessError> {
        let stump = stump_word(digest);
        self.stumps
            .write()
            .expect("Failed to lock stump map")
            .insert(stump, digest);
        Ok(stump)
    }

    fn stump_get(&self, stump: Word) -> Result<Word, PersistorAccessError> {
        let stumps = self.stumps.read().expect("Failed to lock stumps map");
        match stumps.get(&stump) {
            Some(digest) => {
                assert_eq!(stump, stump_word(*digest));
                Ok(*digest)
            }
            None => Err(PersistorAccessError(format!("Stump {:?} not found", stump))),
        }
    }
}

#[cfg(not(feature = "wasm-kernel"))]
impl BoundedPersistor for MemoryPersistor {
    fn root_list_bounded(&self, limit: usize) -> Result<Vec<Word>, PersistorAccessError> {
        let roots = self.roots.read().expect("Failed to get locked roots");
        if roots.len() > limit {
            return Err(PersistorAccessError("Bounded root scan exceeded".into()));
        }
        let mut keys = Vec::with_capacity(roots.len());
        for (key, (_, persistent)) in roots.iter() {
            if *persistent {
                if keys.len() == limit {
                    return Err(PersistorAccessError("Bounded root list exceeded".into()));
                }
                keys.push(*key);
            }
        }
        keys.sort();
        Ok(keys)
    }

    fn leaf_get_bounded(&self, leaf: Word, limit: usize) -> Result<Vec<u8>, PersistorAccessError> {
        match self
            .leaves
            .read()
            .expect("Failed to lock leaf map")
            .get(&leaf)
        {
            Some(content) if content.len() <= limit => {
                self.check_leaf_length(content.len())?;
                Ok(content.clone())
            }
            Some(_) => Err(PersistorAccessError("Bounded leaf read exceeded".into())),
            None => Err(PersistorAccessError(format!("Leaf {:?} not found", leaf))),
        }
    }
}
