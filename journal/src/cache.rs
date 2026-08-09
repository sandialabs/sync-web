use crate::Word;
use crate::persistor::{MemoryPersistor, PERSISTOR, Persistor};
#[cfg(feature = "wasm-kernel")]
use crate::persistor::{SIZE, host_node_get};
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum ResolveSource {
    Session,
    Global,
}

pub(crate) enum ResolvedNode {
    Branch((Word, Word, Word), ResolveSource),
    Leaf(Vec<u8>, ResolveSource),
    Stump(Word, ResolveSource),
}

pub(crate) fn resolve_branch_with(
    persistor: &MemoryPersistor,
    word: Word,
) -> Option<((Word, Word, Word), ResolveSource)> {
    persistor
        .branch_get(word)
        .ok()
        .map(|branch| (branch, ResolveSource::Session))
        .or_else(|| {
            PERSISTOR
                .branch_get(word)
                .ok()
                .map(|branch| {
                    #[cfg(feature = "wasm-kernel")]
                    {
                        persistor.branch_set(branch.0, branch.1, branch.2).ok()?;
                        persistor.mark_imported(word);
                    }
                    Some((branch, ResolveSource::Global))
                })
                .flatten()
        })
}

pub(crate) fn resolve_leaf_with(
    persistor: &MemoryPersistor,
    word: Word,
) -> Option<(Vec<u8>, ResolveSource)> {
    persistor
        .leaf_get(word)
        .ok()
        .map(|content| (content, ResolveSource::Session))
        .or_else(|| {
            PERSISTOR
                .leaf_get(word)
                .ok()
                .map(|content| {
                    #[cfg(feature = "wasm-kernel")]
                    {
                        persistor.leaf_set(content.clone()).ok()?;
                        persistor.mark_imported(word);
                    }
                    Some((content, ResolveSource::Global))
                })
                .flatten()
        })
}

pub(crate) fn resolve_stump_with(
    persistor: &MemoryPersistor,
    word: Word,
) -> Option<(Word, ResolveSource)> {
    persistor
        .stump_get(word)
        .ok()
        .map(|digest| (digest, ResolveSource::Session))
        .or_else(|| {
            PERSISTOR
                .stump_get(word)
                .ok()
                .map(|digest| {
                    #[cfg(feature = "wasm-kernel")]
                    {
                        persistor.stump_set(digest).ok()?;
                        persistor.mark_imported(word);
                    }
                    Some((digest, ResolveSource::Global))
                })
                .flatten()
        })
}

#[cfg(not(feature = "wasm-kernel"))]
pub(crate) fn resolve_node_with(persistor: &MemoryPersistor, word: Word) -> Option<ResolvedNode> {
    resolve_branch_with(persistor, word)
        .map(|(branch, source)| ResolvedNode::Branch(branch, source))
        .or_else(|| {
            resolve_leaf_with(persistor, word)
                .map(|(content, source)| ResolvedNode::Leaf(content, source))
        })
        .or_else(|| {
            resolve_stump_with(persistor, word)
                .map(|(digest, source)| ResolvedNode::Stump(digest, source))
        })
}

#[cfg(feature = "wasm-kernel")]
pub(crate) fn resolve_node_with(persistor: &MemoryPersistor, word: Word) -> Option<ResolvedNode> {
    if let Ok(branch) = persistor.branch_get(word) {
        return Some(ResolvedNode::Branch(branch, ResolveSource::Session));
    }
    if let Ok(content) = persistor.leaf_get(word) {
        return Some(ResolvedNode::Leaf(content, ResolveSource::Session));
    }
    if let Ok(digest) = persistor.stump_get(word) {
        return Some(ResolvedNode::Stump(digest, ResolveSource::Session));
    }
    let response = host_node_get(word).ok()?;
    let resolved = match response.first().copied()? {
        1 if response.len() == 1 + SIZE * 3 => {
            let branch = (
                response[1..1 + SIZE].try_into().ok()?,
                response[1 + SIZE..1 + SIZE * 2].try_into().ok()?,
                response[1 + SIZE * 2..].try_into().ok()?,
            );
            persistor.branch_set(branch.0, branch.1, branch.2).ok()?;
            ResolvedNode::Branch(branch, ResolveSource::Global)
        }
        2 => {
            let content = response[1..].to_vec();
            persistor.leaf_set(content.clone()).ok()?;
            ResolvedNode::Leaf(content, ResolveSource::Global)
        }
        3 if response.len() == 1 + SIZE => {
            let digest = response[1..].try_into().ok()?;
            persistor.stump_set(digest).ok()?;
            ResolvedNode::Stump(digest, ResolveSource::Global)
        }
        _ => return None,
    };
    persistor.mark_imported(word);
    Some(resolved)
}
