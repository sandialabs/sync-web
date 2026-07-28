use crate::persistor::{MemoryPersistor, PERSISTOR, Persistor};
use crate::Word;
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
                .map(|branch| (branch, ResolveSource::Global))
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
                .map(|content| (content, ResolveSource::Global))
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
                .map(|digest| (digest, ResolveSource::Global))
        })
}

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
