use super::*;

#[cfg(not(feature = "wasm-kernel"))]
pub struct DatabasePersistor {
    pub(super) db: RwLock<DB>,
}

#[cfg(not(feature = "wasm-kernel"))]
impl DatabasePersistor {
    pub fn new(path: &str) -> Self {
        initialize_storage_format(Path::new(path));
        let mut opts = Options::default();
        opts.create_if_missing(true);
        opts.create_missing_column_families(true);
        opts.set_max_total_wal_size(MAX_TOTAL_WAL_SIZE);

        let cfs = PERSISTENT_COLUMN_FAMILIES
            .iter()
            .map(|name| ColumnFamilyDescriptor::new(*name, Options::default()));
        let db = DB::open_cf_descriptors(&opts, path, cfs).expect("Failed to open database");
        let persistor = Self {
            db: RwLock::new(db),
        };

        // TODO: clear this due to memory leakage
        {
            let mut handles: Vec<Word> = Vec::new();
            let db = persistor
                .db
                .read()
                .expect("Failed to acquire database lock");
            let mut iter = db.raw_iterator_cf(
                db.cf_handle("roots")
                    .expect("Failed to get roots column family"),
            );
            iter.seek_to_first();
            while iter.valid() {
                if (*iter.value().expect("Failed to get iterator value"))[SIZE] == false as u8 {
                    handles.push(
                        (*iter.key().expect("Failed to get iterator key"))
                            .try_into()
                            .expect("Failed to convert key to Word"),
                    );
                }
                iter.next();
            }
            for handle in handles {
                db.delete_cf(
                    db.cf_handle("roots")
                        .expect("Failed to get roots column family"),
                    handle,
                )
                .expect("Failed to delete value from roots");
            }
        }

        persistor
    }

    fn reference_increment(&self, db: &DB, node: Word) {
        let references = db
            .cf_handle("references")
            .expect("Failed to get references handle");
        match db.get_cf(references, node) {
            Ok(Some(count)) => {
                let count_old =
                    usize::from_ne_bytes(count.try_into().expect("Invalid count bytes"));
                let count_new = count_old + 1;
                db.put_cf(references, node, count_new.to_ne_bytes())
                    .expect("Failed to increment reference count");
            }
            Ok(None) => {
                db.put_cf(references, node, (1 as usize).to_ne_bytes())
                    .expect("Failed to set initial reference count");
            }
            Err(e) => {
                panic! {"{}", e}
            }
        };
    }

    fn reference_decrement(&self, db: &DB, node: Word) {
        let branches = db
            .cf_handle("branches")
            .expect("Failed to get branches handle");
        let leaves = db.cf_handle("leaves").expect("Failed to get leaves handle");
        let stumps = db.cf_handle("stumps").expect("Failed to get stumps handle");
        let references = db
            .cf_handle("references")
            .expect("Failed to get references handle");
        match db
            .get_cf(references, node)
            .expect("Failed to get reference count")
        {
            Some(count_old) => {
                let count_old =
                    usize::from_ne_bytes(count_old.try_into().expect("Invalid count bytes"));
                let count_new = count_old - 1;
                if count_new > 0 {
                    db.put_cf(references, node, count_new.to_ne_bytes())
                        .expect("Failed to update reference count");
                } else {
                    db.delete_cf(references, node)
                        .expect("Failed to delete reference");
                    if let Some(value) = db.get_cf(branches, node).expect("Failed to get branch") {
                        let left: Word = value[SIZE..SIZE * 2]
                            .try_into()
                            .expect("Invalid left node bytes");
                        let right: Word = value[SIZE * 2..]
                            .try_into()
                            .expect("Invalid right node bytes");
                        db.delete_cf(branches, node)
                            .expect("Failed to delete branch");
                        self.reference_decrement(db, left);
                        self.reference_decrement(db, right);
                    } else {
                        if let Some(_) = db.get_cf(leaves, node).expect("Failed to get leaf") {
                            db.delete_cf(leaves, node).expect("Failed to delete leaf");
                        } else if let Some(_) =
                            db.get_cf(stumps, node).expect("Failed to get stump")
                        {
                            db.delete_cf(stumps, node).expect("Failed to delete stump");
                        }
                    }
                }
            }
            None => {}
        };
    }

    fn merge_collect(
        &self,
        db: &DB,
        source: &dyn Persistor,
        node: Word,
        plan: &mut MergePlan,
        seen: &mut HashSet<Word>,
    ) -> Result<(), PersistorAccessError> {
        if node == [0 as u8; SIZE] || !seen.insert(node) {
            return Ok(());
        }

        let branches = db
            .cf_handle("branches")
            .expect("Failed to get branches handle");
        let leaves = db.cf_handle("leaves").expect("Failed to get leaves handle");
        let stumps = db.cf_handle("stumps").expect("Failed to get stumps handle");

        if plan.branches.contains_key(&node)
            || plan.leaves.contains_key(&node)
            || plan.stumps.contains_key(&node)
        {
            return Ok(());
        }

        if db
            .get_cf(leaves, node)
            .expect("Failed to get leaf")
            .is_some()
            || db
                .get_cf(stumps, node)
                .expect("Failed to get stump")
                .is_some()
            || db
                .get_cf(branches, node)
                .expect("Failed to get branch")
                .is_some()
        {
            // Durable database branches are inserted only through a complete
            // merge plan. Avoid walking the entire retained DAG on every
            // commit; missing dependencies in newly copied source branches
            // are rejected below before the batch is applied.
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
            self.merge_collect(db, source, left, plan, seen)?;
            self.merge_collect(db, source, right, plan, seen)?;
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

    fn merged_branch(
        &self,
        db: &DB,
        node: Word,
        plan: &MergePlan,
    ) -> Result<Option<(Word, Word, Word)>, PersistorAccessError> {
        if let Some(branch) = plan.branches.get(&node) {
            return Ok(Some(*branch));
        }

        let branches = db
            .cf_handle("branches")
            .expect("Failed to get branches handle");
        match db.get_cf(branches, node) {
            Ok(Some(value)) => {
                let digest = value[..SIZE]
                    .try_into()
                    .expect("Invalid digest branch size");
                let left = value[SIZE..SIZE * 2]
                    .try_into()
                    .expect("Invalid left branch size");
                let right = value[SIZE * 2..]
                    .try_into()
                    .expect("Invalid right branch size");
                Ok(Some((left, right, digest)))
            }
            Ok(None) => Ok(None),
            Err(e) => Err(PersistorAccessError(format!("{}", e))),
        }
    }

    fn base_refcount(&self, db: &DB, node: Word) -> Result<isize, PersistorAccessError> {
        let references = db
            .cf_handle("references")
            .expect("Failed to get references handle");
        match db.get_cf(references, node) {
            Ok(Some(count)) => {
                Ok(usize::from_ne_bytes(count.try_into().expect("Invalid count bytes")) as isize)
            }
            Ok(None) => Ok(0),
            Err(e) => Err(PersistorAccessError(format!("{}", e))),
        }
    }

    fn release_plan(
        &self,
        db: &DB,
        node: Word,
        plan: &mut MergePlan,
    ) -> Result<(), PersistorAccessError> {
        if node == [0 as u8; SIZE] {
            return Ok(());
        }

        let effective =
            self.base_refcount(db, node)? + plan.deltas.get(&node).copied().unwrap_or(0);
        if effective <= 0 {
            return Ok(());
        }

        plan.delta_add(node, -1);

        if effective == 1 {
            if !plan.deletes.insert(node) {
                return Ok(());
            }

            if let Some((left, right, _)) = self.merged_branch(db, node, plan)? {
                self.release_plan(db, left, plan)?;
                self.release_plan(db, right, plan)?;
            }
        }

        Ok(())
    }
}

#[cfg(not(feature = "wasm-kernel"))]
impl Persistor for DatabasePersistor {
    fn root_list(&self) -> Vec<Word> {
        let mut handles: Vec<Word> = Vec::new();
        let db = self.db.read().expect("Failed to acquire db lock");
        let roots = db
            .cf_handle("roots")
            .expect("Failed to get roots column family");
        let mut iter = db.raw_iterator_cf(roots);
        iter.seek_to_first();
        while iter.valid() {
            if (*iter.value().expect("Failed to get iterator value"))[SIZE] != false as u8 {
                handles.push(
                    (*iter.key().expect("Failed to get iterator key"))
                        .try_into()
                        .expect("Failed to convert key to Word"),
                );
            }
            iter.next();
        }

        handles.shuffle(&mut rand::thread_rng());
        handles
    }

    fn root_new(&self, handle: Word, root: Word) -> Result<Word, PersistorAccessError> {
        let mut root_marked = [0 as u8; SIZE + 1];
        root_marked[..SIZE].copy_from_slice(&root);
        root_marked[SIZE] = true as u8;

        let db = self.db.write().expect("Failed to acquire db lock");
        let roots = db
            .cf_handle("roots")
            .expect("Failed to get roots column family");
        match db.get_cf(roots, handle) {
            Ok(Some(_)) => Err(PersistorAccessError(format!(
                "Handle {:?} already exists",
                handle
            ))),
            Ok(None) => {
                self.reference_increment(&db, root);
                db.put_cf(roots, handle, root_marked)
                    .expect("Failed to put root in db");
                Ok(handle)
            }
            Err(e) => Err(PersistorAccessError(format!("{}", e))),
        }
    }

    fn root_temp(&self, root: Word) -> Result<Word, PersistorAccessError> {
        let db = self.db.write().expect("Failed to acquire db lock");
        let roots = db
            .cf_handle("roots")
            .expect("Failed to get roots column family");
        let mut root_marked = [0 as u8; SIZE + 1];
        root_marked[..SIZE].copy_from_slice(&root);
        root_marked[SIZE] = false as u8;

        let mut handle_: Word = [0 as u8; 32];
        rand::thread_rng().fill_bytes(&mut handle_);

        match db.get_cf(roots, handle_) {
            Ok(Some(_)) => Err(PersistorAccessError(format!(
                "Handle {:?} already exists",
                handle_
            ))),
            Ok(None) => {
                self.reference_increment(&db, root);
                db.put_cf(roots, handle_, root_marked)
                    .expect("Failed to put root in db");
                Ok(handle_)
            }
            Err(e) => Err(PersistorAccessError(format!("{}", e))),
        }
    }

    fn root_get(&self, handle: Word) -> Result<Word, PersistorAccessError> {
        let db = self.db.read().expect("Failed to acquire db lock");
        let roots = db.cf_handle("roots").expect("Failed to get roots handle");
        match db.get_cf(roots, handle) {
            Ok(Some(root_marked)) => Ok(((*root_marked)[..SIZE])
                .try_into()
                .expect("Invalid root size")),
            Ok(None) => Err(PersistorAccessError(format!(
                "Handle {:?} not found",
                handle
            ))),
            Err(e) => Err(PersistorAccessError(format!("{}", e))),
        }
    }

    fn root_set(
        &self,
        handle: Word,
        old: Word,
        new: Word,
        source: &dyn Persistor,
    ) -> Result<Word, PersistorAccessError> {
        let db = self.db.write().expect("Failed to acquire db lock");
        let roots = db.cf_handle("roots").expect("Failed to get roots handle");
        match db.get_cf(roots, handle) {
            Ok(Some(root_marked)) => match root_marked[SIZE] != false as u8 {
                true => match root_marked[..SIZE] == old.to_vec() {
                    true => {
                        let mut plan = MergePlan::new();
                        let mut seen = HashSet::new();
                        self.merge_collect(&db, source, new, &mut plan, &mut seen)?;
                        plan.delta_add(new, 1);
                        self.release_plan(&db, old, &mut plan)?;

                        let mut new_marked = [0 as u8; SIZE + 1];
                        new_marked[..SIZE].copy_from_slice(&new);
                        new_marked[SIZE] = true as u8;

                        let branches = db
                            .cf_handle("branches")
                            .expect("Failed to get branches handle");
                        let leaves = db.cf_handle("leaves").expect("Failed to get leaves handle");
                        let stumps = db.cf_handle("stumps").expect("Failed to get stumps handle");
                        let references = db
                            .cf_handle("references")
                            .expect("Failed to get references handle");

                        let mut batch = WriteBatch::default();

                        for (node, content) in plan.leaves.iter() {
                            batch.put_cf(leaves, node, content);
                        }

                        for (node, digest) in plan.stumps.iter() {
                            batch.put_cf(stumps, node, digest);
                        }

                        for (node, (left, right, digest)) in plan.branches.iter() {
                            batch.put_cf(branches, node, branch_encoding(*digest, *left, *right));
                        }

                        for (node, delta) in plan.deltas.iter() {
                            let base = self.base_refcount(&db, *node)?;
                            let next = base + delta;
                            if next > 0 {
                                batch.put_cf(references, node, (next as usize).to_ne_bytes());
                            } else {
                                batch.delete_cf(references, node);
                            }
                        }

                        for node in plan.deletes.iter() {
                            if plan.branches.contains_key(node)
                                || db
                                    .get_cf(branches, node)
                                    .expect("Failed to get branch")
                                    .is_some()
                            {
                                batch.delete_cf(branches, node);
                            } else if plan.leaves.contains_key(node)
                                || db
                                    .get_cf(leaves, node)
                                    .expect("Failed to get leaf")
                                    .is_some()
                            {
                                batch.delete_cf(leaves, node);
                            } else if plan.stumps.contains_key(node)
                                || db
                                    .get_cf(stumps, node)
                                    .expect("Failed to get stump")
                                    .is_some()
                            {
                                batch.delete_cf(stumps, node);
                            }
                        }

                        batch.put_cf(roots, handle, new_marked);
                        db.write(batch).expect("Failed to apply root merge batch");
                        Ok(handle)
                    }
                    false => Err(PersistorAccessError(format!(
                        "Handle {:?} changed since compare",
                        handle
                    ))),
                },
                false => Err(PersistorAccessError(format!(
                    "Handle {:?} is temporary",
                    handle
                ))),
            },
            Ok(None) => Err(PersistorAccessError(format!(
                "Handle {:?} not found",
                handle
            ))),
            Err(e) => Err(PersistorAccessError(format!("{}", e))),
        }
    }

    fn root_delete(&self, handle: Word) -> Result<(), PersistorAccessError> {
        let db = self.db.write().expect("Failed to acquire db lock");
        let roots = db.cf_handle("roots").expect("Failed to get roots handle");
        match db.get_cf(roots, handle) {
            Ok(Some(root_marked)) => {
                let root: Word = root_marked[..SIZE].try_into().expect("Invalid root size");
                db.delete_cf(roots, handle).expect("Failed to delete root");
                self.reference_decrement(&db, root);
                Ok(())
            }
            Ok(None) => Err(PersistorAccessError(format!(
                "Handle {:?} not found",
                handle
            ))),
            Err(e) => Err(PersistorAccessError(format!("{}", e))),
        }
    }

    fn branch_set(
        &self,
        left: Word,
        right: Word,
        digest: Word,
    ) -> Result<Word, PersistorAccessError> {
        let encoded = branch_encoding(digest, left, right);
        let branch = branch_word(digest, left, right);

        let db = self.db.write().expect("Failed to acquire db lock");
        let branches = db
            .cf_handle("branches")
            .expect("Failed to get branches handle");
        db.put_cf(branches, branch, encoded)
            .expect("Failed to put branch");
        self.reference_increment(&db, left);
        self.reference_increment(&db, right);

        Ok(branch)
    }

    fn branch_get(&self, branch: Word) -> Result<(Word, Word, Word), PersistorAccessError> {
        let db = self.db.read().expect("Failed to acquire db lock");
        let branches = db
            .cf_handle("branches")
            .expect("Failed to get branches handle");
        match db.get_cf(branches, branch) {
            Ok(Some(value)) => {
                let digest = &value[..SIZE]
                    .try_into()
                    .expect("Invalid digest branch size");
                let left = &value[SIZE..SIZE * 2]
                    .try_into()
                    .expect("Invalid left branch size");
                let right = &value[SIZE * 2..]
                    .try_into()
                    .expect("Invalid right branch size");
                assert_eq!(branch, branch_word(*digest, *left, *right));
                Ok((*left, *right, *digest))
            }
            Ok(None) => Err(PersistorAccessError(format!(
                "Branch {:?} not found",
                branch
            ))),
            Err(e) => Err(PersistorAccessError(format!("{}", e))),
        }
    }

    fn leaf_set(&self, content: Vec<u8>) -> Result<Word, PersistorAccessError> {
        check_leaf_length(content.len())?;
        let leaf = leaf_word(&content);
        let db = self.db.write().expect("Failed to acquire db lock");
        let leaves = db.cf_handle("leaves").expect("Failed to get leaves handle");
        db.put_cf(leaves, leaf, content)
            .expect("Failed to put leaf");
        Ok(leaf)
    }

    fn leaf_get(&self, leaf: Word) -> Result<Vec<u8>, PersistorAccessError> {
        let db = self.db.read().expect("Failed to acquire db lock");
        let leaves = db.cf_handle("leaves").expect("Failed to get leaves handle");
        match db.get_cf(leaves, leaf) {
            Ok(Some(content)) => {
                check_leaf_length(content.len())?;
                if leaf != leaf_word(&content) {
                    return Err(PersistorAccessError("Leaf digest mismatch".into()));
                }
                Ok(content.to_vec())
            }
            Ok(None) => Err(PersistorAccessError(format!("Leaf {:?} not found", leaf))),
            Err(e) => Err(PersistorAccessError(format!("{}", e))),
        }
    }

    fn stump_set(&self, digest: Word) -> Result<Word, PersistorAccessError> {
        let stump = stump_word(digest);
        let db = self.db.write().expect("Failed to acquire db lock");
        let stumps = db.cf_handle("stumps").expect("Failed to get stumps handle");
        db.put_cf(stumps, stump, digest)
            .expect("Failed to put stump");
        Ok(stump)
    }

    fn stump_get(&self, stump: Word) -> Result<Word, PersistorAccessError> {
        let db = self.db.read().expect("Failed to acquire db lock");
        let stumps = db.cf_handle("stumps").expect("Failed to get stumps handle");
        match db.get_cf(stumps, stump) {
            Ok(Some(digest)) => {
                let digest: Word = (&digest[..SIZE])
                    .try_into()
                    .expect("Invalid stump digest size");
                assert_eq!(stump, stump_word(digest));
                Ok(digest)
            }
            Ok(None) => Err(PersistorAccessError(format!(
                "Stumps {:?} not found",
                stump
            ))),
            Err(e) => Err(PersistorAccessError(format!("{}", e))),
        }
    }
}

#[cfg(not(feature = "wasm-kernel"))]
impl BoundedPersistor for DatabasePersistor {
    fn root_list_bounded(&self, limit: usize) -> Result<Vec<Word>, PersistorAccessError> {
        let db = self.db.read().expect("Failed to acquire db lock");
        let roots = db
            .cf_handle("roots")
            .expect("Failed to get roots column family");
        let mut handles = Vec::with_capacity(limit.min(1024));
        let mut iter = db.raw_iterator_cf(roots);
        iter.seek_to_first();
        let mut scanned = 0;
        while iter.valid() {
            if scanned == limit {
                return Err(PersistorAccessError("Bounded root scan exceeded".into()));
            }
            scanned += 1;
            if iter.value().expect("Failed to get iterator value")[SIZE] != 0 {
                if handles.len() == limit {
                    return Err(PersistorAccessError("Bounded root list exceeded".into()));
                }
                handles.push(
                    iter.key()
                        .expect("Failed to get iterator key")
                        .try_into()
                        .expect("Failed to convert key to Word"),
                );
            }
            iter.next();
        }
        handles.shuffle(&mut rand::thread_rng());
        Ok(handles)
    }

    fn leaf_get_bounded(&self, leaf: Word, limit: usize) -> Result<Vec<u8>, PersistorAccessError> {
        let db = self.db.read().expect("Failed to acquire db lock");
        let leaves = db.cf_handle("leaves").expect("Failed to get leaves handle");
        match db.get_pinned_cf(leaves, leaf) {
            Ok(Some(content)) if content.len() <= limit => {
                check_leaf_length(content.len())?;
                if leaf != leaf_word(&content) {
                    return Err(PersistorAccessError("Leaf digest mismatch".into()));
                }
                Ok(content.to_vec())
            }
            Ok(Some(_)) => Err(PersistorAccessError("Bounded leaf read exceeded".into())),
            Ok(None) => Err(PersistorAccessError(format!("Leaf {:?} not found", leaf))),
            Err(error) => Err(PersistorAccessError(error.to_string())),
        }
    }
}
