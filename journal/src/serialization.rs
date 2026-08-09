use crate::cache::{ResolvedNode, resolve_node_with};
use crate::evaluator::{self as s7, Primitive, obj2str};
use crate::persistor::{MemoryPersistor, Persistor};
use crate::{
    NULL, SIZE, SYNC_NODE_TAG, Word, serialization_error_copy, serialization_query_environment,
    session_persistor_for, session_serialization_query_get, session_serialization_query_store,
    sync_heap_make, sync_heap_read, sync_is_node,
};
use sha2::{Digest, Sha256};
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::ffi::CString;

#[derive(Default)]
struct SerializationEdges {
    left: Option<Word>,
    right: Option<Word>,
}

#[derive(Default)]
struct SerializationTrace {
    edges: HashMap<Word, SerializationEdges>,
}

thread_local! {
    static SERIALIZATION_TRACES: RefCell<HashMap<usize, Vec<SerializationTrace>>> =
        RefCell::new(HashMap::new());
}

pub(crate) fn serialization_trace_pair(
    sc: *mut s7::s7_scheme,
    node: Word,
    left: Word,
    right: Word,
) {
    SERIALIZATION_TRACES.with(|traces| {
        if let Some(trace) = traces
            .borrow_mut()
            .get_mut(&(sc as usize))
            .and_then(|stack| stack.last_mut())
        {
            let edges = trace.edges.entry(node).or_default();
            edges.left = Some(left);
            edges.right = Some(right);
        }
    });
}

pub(crate) fn serialization_trace_child(
    sc: *mut s7::s7_scheme,
    node: Word,
    child: Word,
    left: bool,
) {
    SERIALIZATION_TRACES.with(|traces| {
        if let Some(trace) = traces
            .borrow_mut()
            .get_mut(&(sc as usize))
            .and_then(|stack| stack.last_mut())
        {
            let edges = trace.edges.entry(node).or_default();
            if left {
                edges.left = Some(child);
            } else {
                edges.right = Some(child);
            }
        }
    });
}

pub(crate) fn serialization_trace_active(sc: *mut s7::s7_scheme) -> bool {
    SERIALIZATION_TRACES.with(|traces| {
        traces
            .borrow()
            .get(&(sc as usize))
            .is_some_and(|stack| !stack.is_empty())
    })
}

fn serialization_trace_start(sc: *mut s7::s7_scheme) {
    SERIALIZATION_TRACES.with(|traces| {
        traces
            .borrow_mut()
            .entry(sc as usize)
            .or_default()
            .push(SerializationTrace::default());
    });
}

fn serialization_trace_finish(sc: *mut s7::s7_scheme) -> SerializationTrace {
    SERIALIZATION_TRACES.with(|traces| {
        let mut traces = traces.borrow_mut();
        let stack = traces
            .get_mut(&(sc as usize))
            .expect("Serialization trace stack is empty");
        let trace = stack.pop().expect("Serialization trace stack is empty");
        if stack.is_empty() {
            traces.remove(&(sc as usize));
        }
        trace
    })
}

#[derive(Clone, Copy, Hash, PartialEq, Eq)]
enum SerializationId {
    Node(Word),
    Stub(Word),
}

enum SerializationDefinition {
    Constant(SerializationId, Vec<u8>),
    Stub(SerializationId, Word),
    Pair(SerializationId, SerializationId, SerializationId),
}

enum CompactSerializationDefinition {
    Constant(u64, Vec<u8>),
    Stub(u64, Word),
    Pair(u64, u64, u64),
}

fn serialization_digest(persistor: &MemoryPersistor, word: Word) -> Result<Word, String> {
    if word == NULL {
        return Ok(NULL);
    }
    match resolve_node_with(persistor, word) {
        Some(ResolvedNode::Branch((_, _, digest), _)) => Ok(digest),
        Some(ResolvedNode::Leaf(_, _)) => Ok(word),
        Some(ResolvedNode::Stump(digest, _)) => Ok(digest),
        None => Err("Serialization contains an unavailable node".to_string()),
    }
}

fn serialization_branch(
    persistor: &MemoryPersistor,
    left: Word,
    right: Word,
) -> Result<Word, String> {
    let left_digest = serialization_digest(persistor, left)?;
    let right_digest = serialization_digest(persistor, right)?;
    let mut joined = [0_u8; SIZE * 2];
    joined[..SIZE].copy_from_slice(&left_digest);
    joined[SIZE..].copy_from_slice(&right_digest);
    persistor
        .branch_set(left, right, Word::from(Sha256::digest(joined)))
        .map_err(|_| "Serialization could not construct a pair".to_string())
}

fn serialization_cut(persistor: &MemoryPersistor, word: Word) -> Result<Word, String> {
    persistor
        .stump_set(serialization_digest(persistor, word)?)
        .map_err(|_| "Serialization could not construct a stub".to_string())
}

fn serialization_partial_tree(
    persistor: &MemoryPersistor,
    trace: &SerializationTrace,
    root: Word,
) -> Result<Word, String> {
    if !matches!(
        resolve_node_with(persistor, root),
        Some(ResolvedNode::Branch(_, _))
    ) {
        return Ok(root);
    }

    let mut partial = HashMap::new();
    let mut scheduled = HashSet::new();
    let mut stack = vec![(root, false)];
    while let Some((word, expanded)) = stack.pop() {
        if partial.contains_key(&word) {
            continue;
        }
        let (left, right) = match resolve_node_with(persistor, word) {
            Some(ResolvedNode::Branch((left, right, _), _)) => (left, right),
            Some(_) if word != root => {
                partial.insert(word, word);
                continue;
            }
            _ => return Err("Serialization query referenced an unavailable node".to_string()),
        };
        let edges = trace.edges.get(&word);
        if !expanded {
            if !scheduled.insert(word) {
                continue;
            }
            stack.push((word, true));
            for child in [
                edges.and_then(|edges| edges.right),
                edges.and_then(|edges| edges.left),
            ]
            .into_iter()
            .flatten()
            {
                if child != NULL
                    && matches!(
                        resolve_node_with(persistor, child),
                        Some(ResolvedNode::Branch(_, _))
                    )
                    && !partial.contains_key(&child)
                {
                    stack.push((child, false));
                }
            }
            continue;
        }

        let partial_child = |original: Word, traced: Option<Word>| -> Result<Word, String> {
            match traced {
                None => serialization_cut(persistor, original),
                Some(child) if child == NULL => Ok(NULL),
                Some(child) => match resolve_node_with(persistor, child) {
                    Some(ResolvedNode::Branch(_, _)) => {
                        partial.get(&child).copied().ok_or_else(|| {
                            "Serialization query contains a cyclic node graph".to_string()
                        })
                    }
                    Some(ResolvedNode::Leaf(_, _) | ResolvedNode::Stump(_, _)) => Ok(child),
                    None => Err("Serialization query referenced an unavailable node".to_string()),
                },
            }
        };
        let left = partial_child(left, edges.and_then(|edges| edges.left))?;
        let right = partial_child(right, edges.and_then(|edges| edges.right))?;
        partial.insert(word, serialization_branch(persistor, left, right)?);
    }
    partial
        .remove(&root)
        .ok_or_else(|| "Serialization could not construct the partial tree".to_string())
}

enum SerializationNode {
    Null,
    Constant(SerializationId, Vec<u8>),
    Stub(SerializationId, Word),
    Pair(SerializationId, Word, Word),
}

impl SerializationNode {
    fn id(&self) -> SerializationId {
        match self {
            Self::Null => SerializationId::Node(NULL),
            Self::Constant(id, _) | Self::Stub(id, _) | Self::Pair(id, _, _) => *id,
        }
    }
}

fn serialization_node(
    persistor: &MemoryPersistor,
    word: Word,
) -> Result<SerializationNode, String> {
    if word == NULL {
        return Ok(SerializationNode::Null);
    }
    match resolve_node_with(persistor, word) {
        Some(ResolvedNode::Leaf(content, _)) => Ok(SerializationNode::Constant(
            SerializationId::Node(word),
            content,
        )),
        Some(ResolvedNode::Stump(digest, _)) => Ok(SerializationNode::Stub(
            SerializationId::Stub(digest),
            digest,
        )),
        Some(ResolvedNode::Branch((left, right, digest), _)) => Ok(SerializationNode::Pair(
            SerializationId::Node(digest),
            left,
            right,
        )),
        None => Err("Serialization contains an unavailable node".to_string()),
    }
}

enum SerializationFrame {
    Visit(SerializationNode),
    Pair(SerializationId, SerializationId, SerializationId),
}

fn serialization_collect(
    persistor: &MemoryPersistor,
    root: Word,
) -> Result<Vec<SerializationDefinition>, String> {
    let mut definitions = Vec::new();
    let mut seen = HashSet::new();
    let mut stack = vec![SerializationFrame::Visit(serialization_node(
        persistor, root,
    )?)];
    while let Some(frame) = stack.pop() {
        match frame {
            SerializationFrame::Visit(SerializationNode::Null) => {}
            SerializationFrame::Visit(SerializationNode::Constant(id, content)) => {
                if seen.insert(id) {
                    definitions.push(SerializationDefinition::Constant(id, content));
                }
            }
            SerializationFrame::Visit(SerializationNode::Stub(id, digest)) => {
                if seen.insert(id) {
                    definitions.push(SerializationDefinition::Stub(id, digest));
                }
            }
            SerializationFrame::Visit(SerializationNode::Pair(id, left, right)) => {
                if !seen.insert(id) {
                    continue;
                }
                let left = serialization_node(persistor, left)?;
                let right = serialization_node(persistor, right)?;
                stack.push(SerializationFrame::Pair(id, left.id(), right.id()));
                stack.push(SerializationFrame::Visit(right));
                stack.push(SerializationFrame::Visit(left));
            }
            SerializationFrame::Pair(id, left, right) => {
                definitions.push(SerializationDefinition::Pair(id, left, right));
            }
        }
    }
    Ok(definitions)
}

fn serialization_shorten(
    id: SerializationId,
    ids: &mut HashMap<SerializationId, u64>,
    counter: &mut u64,
) -> u64 {
    if id == SerializationId::Node(NULL) {
        return 0;
    }
    *ids.entry(id).or_insert_with(|| {
        *counter += 1;
        *counter
    })
}

fn serialization_compact(
    mut definitions: Vec<SerializationDefinition>,
) -> Vec<CompactSerializationDefinition> {
    definitions.reverse();
    let mut ids = HashMap::new();
    let mut counter = 0;
    definitions
        .into_iter()
        .map(|definition| match definition {
            SerializationDefinition::Constant(id, content) => {
                CompactSerializationDefinition::Constant(
                    serialization_shorten(id, &mut ids, &mut counter),
                    content,
                )
            }
            SerializationDefinition::Stub(id, digest) => CompactSerializationDefinition::Stub(
                serialization_shorten(id, &mut ids, &mut counter),
                digest,
            ),
            SerializationDefinition::Pair(id, left, right) => CompactSerializationDefinition::Pair(
                serialization_shorten(id, &mut ids, &mut counter),
                serialization_shorten(left, &mut ids, &mut counter),
                serialization_shorten(right, &mut ids, &mut counter),
            ),
        })
        .collect()
}

unsafe fn serialization_symbol(sc: *mut s7::s7_scheme, id: u64) -> s7::s7_pointer {
    let name = CString::new(format!("n-{id}")).expect("Serialization ID contains a null byte");
    unsafe { s7::s7_make_symbol(sc, name.as_ptr()) }
}

unsafe fn serialization_byte_vector(sc: *mut s7::s7_scheme, bytes: &[u8]) -> s7::s7_pointer {
    unsafe {
        let value = s7::s7_make_byte_vector(sc, bytes.len() as s7::s7_int, 1, std::ptr::null_mut());
        for (index, byte) in bytes.iter().enumerate() {
            s7::s7_byte_vector_set(value, index as s7::s7_int, *byte);
        }
        value
    }
}

unsafe fn serialization_to_s7(
    sc: *mut s7::s7_scheme,
    definitions: Vec<CompactSerializationDefinition>,
) -> s7::s7_pointer {
    unsafe {
        let mut result = s7::s7_nil(sc);
        let mut result_loc = s7::s7_gc_protect(sc, result);
        for definition in definitions.into_iter().rev() {
            let (id, value) = match definition {
                CompactSerializationDefinition::Constant(id, content) => {
                    let content = serialization_byte_vector(sc, &content);
                    let content_loc = s7::s7_gc_protect(sc, content);
                    let value = s7::s7_list(sc, 2, s7::s7_make_symbol(sc, c"c".as_ptr()), content);
                    s7::s7_gc_unprotect_at(sc, content_loc);
                    (id, value)
                }
                CompactSerializationDefinition::Stub(id, digest) => {
                    let digest = serialization_byte_vector(sc, &digest);
                    let digest_loc = s7::s7_gc_protect(sc, digest);
                    let value = s7::s7_list(sc, 2, s7::s7_make_symbol(sc, c"s".as_ptr()), digest);
                    s7::s7_gc_unprotect_at(sc, digest_loc);
                    (id, value)
                }
                CompactSerializationDefinition::Pair(id, left, right) => (
                    id,
                    s7::s7_list(
                        sc,
                        3,
                        s7::s7_make_symbol(sc, c"p".as_ptr()),
                        serialization_symbol(sc, left),
                        serialization_symbol(sc, right),
                    ),
                ),
            };
            let value_loc = s7::s7_gc_protect(sc, value);
            let entry = s7::s7_list(sc, 2, serialization_symbol(sc, id), value);
            let entry_loc = s7::s7_gc_protect(sc, entry);
            let next = s7::s7_cons(sc, entry, result);
            let next_loc = s7::s7_gc_protect(sc, next);
            s7::s7_gc_unprotect_at(sc, entry_loc);
            s7::s7_gc_unprotect_at(sc, value_loc);
            s7::s7_gc_unprotect_at(sc, result_loc);
            result = next;
            result_loc = next_loc;
        }
        s7::s7_gc_unprotect_at(sc, result_loc);
        result
    }
}

struct SerializationProtected {
    value: s7::s7_pointer,
    location: s7::s7_int,
}

unsafe fn serialization_protect(
    sc: *mut s7::s7_scheme,
    value: s7::s7_pointer,
) -> SerializationProtected {
    unsafe {
        SerializationProtected {
            value,
            location: s7::s7_gc_protect(sc, value),
        }
    }
}

unsafe fn serialization_unprotect(sc: *mut s7::s7_scheme, protected: SerializationProtected) {
    unsafe {
        s7::s7_gc_unprotect_at(sc, protected.location);
    }
}

unsafe fn serialization_query(
    sc: *mut s7::s7_scheme,
    node: s7::s7_pointer,
    query: s7::s7_pointer,
) -> Result<SerializationTrace, (s7::s7_pointer, s7::s7_pointer)> {
    unsafe {
        let query_source = obj2str(sc, query);
        let expression =
            if let Some(expression) = session_serialization_query_get(sc, &query_source) {
                expression
            } else {
                let source = CString::new(query_source.as_bytes()).map_err(|_| {
                    let tag = s7::s7_gc_protect_via_stack(
                        sc,
                        s7::s7_make_symbol(sc, c"serialization-error".as_ptr()),
                    );
                    let info = s7::s7_gc_protect_via_stack(
                        sc,
                        s7::s7_list(
                            sc,
                            1,
                            s7::s7_make_string(
                                sc,
                                c"Serialization query contains a null byte".as_ptr(),
                            ),
                        ),
                    );
                    (tag, info)
                })?;
                let port = serialization_protect(sc, s7::s7_open_input_string(sc, source.as_ptr()));
                let expression = serialization_protect(sc, s7::s7_read(sc, port.value));
                s7::s7_close_input_port(sc, port.value);
                serialization_unprotect(sc, port);
                let stored = session_serialization_query_store(sc, query_source, expression.value);
                serialization_unprotect(sc, expression);
                stored
            };

        let (query_environment_value, query_environment_location) =
            serialization_query_environment(sc);
        let query_environment = SerializationProtected {
            value: query_environment_value,
            location: query_environment_location,
        };
        let wrapper_environment =
            serialization_protect(sc, s7::s7_sublet(sc, s7::s7_rootlet(sc), s7::s7_nil(sc)));
        let ok = serialization_protect(sc, s7::s7_make_symbol(sc, c"%sync-serialize-ok".as_ptr()));
        let error = serialization_protect(
            sc,
            s7::s7_make_symbol(sc, c"%sync-serialize-error".as_ptr()),
        );
        for (name, value) in [
            (c"%expression".as_ptr(), expression),
            (c"%query-environment".as_ptr(), query_environment.value),
            (c"%node".as_ptr(), node),
            (
                c"%eval".as_ptr(),
                s7::s7_let_ref(
                    sc,
                    s7::s7_rootlet(sc),
                    s7::s7_make_symbol(sc, c"eval".as_ptr()),
                ),
            ),
            (
                c"%list".as_ptr(),
                s7::s7_let_ref(
                    sc,
                    s7::s7_rootlet(sc),
                    s7::s7_make_symbol(sc, c"list".as_ptr()),
                ),
            ),
            (c"%ok".as_ptr(), ok.value),
            (c"%error".as_ptr(), error.value),
        ] {
            s7::s7_varlet(
                sc,
                wrapper_environment.value,
                s7::s7_make_symbol(sc, name),
                value,
            );
        }

        let wrapper = c"(catch #t (lambda () (%list %ok ((%eval %expression %query-environment) %node))) (lambda args (%list %error args)))";
        serialization_trace_start(sc);
        let tagged = serialization_protect(
            sc,
            s7::s7_eval_c_string_with_environment(sc, wrapper.as_ptr(), wrapper_environment.value),
        );
        let trace = serialization_trace_finish(sc);
        let is_error = s7::s7_is_pair(tagged.value)
            && s7::s7_list_length(sc, tagged.value) == 2
            && s7::s7_car(tagged.value) == error.value;
        let valid = s7::s7_is_pair(tagged.value)
            && s7::s7_list_length(sc, tagged.value) == 2
            && s7::s7_car(tagged.value) == ok.value;

        if is_error {
            let args = s7::s7_cadr(tagged.value);
            let copied = serialization_error_copy(sc, args).unwrap_or_else(|| {
                let tag = s7::s7_gc_protect_via_stack(
                    sc,
                    s7::s7_make_symbol(sc, c"serialization-error".as_ptr()),
                );
                let info = s7::s7_gc_protect_via_stack(
                    sc,
                    s7::s7_list(
                        sc,
                        1,
                        s7::s7_make_string(
                            sc,
                            c"Serialization query returned a non-inert error".as_ptr(),
                        ),
                    ),
                );
                (tag, info)
            });
            serialization_unprotect(sc, tagged);
            serialization_unprotect(sc, error);
            serialization_unprotect(sc, ok);
            serialization_unprotect(sc, wrapper_environment);
            serialization_unprotect(sc, query_environment);
            return Err(copied);
        }

        serialization_unprotect(sc, tagged);
        serialization_unprotect(sc, error);
        serialization_unprotect(sc, ok);
        serialization_unprotect(sc, wrapper_environment);
        serialization_unprotect(sc, query_environment);
        if valid {
            Ok(trace)
        } else {
            let tag = s7::s7_gc_protect_via_stack(
                sc,
                s7::s7_make_symbol(sc, c"serialization-error".as_ptr()),
            );
            let info = s7::s7_gc_protect_via_stack(
                sc,
                s7::s7_list(
                    sc,
                    1,
                    s7::s7_make_string(
                        sc,
                        c"Serialization query returned an invalid result".as_ptr(),
                    ),
                ),
            );
            Err((tag, info))
        }
    }
}

unsafe fn serialization_error(sc: *mut s7::s7_scheme, message: &str) -> s7::s7_pointer {
    unsafe {
        let message =
            CString::new(message).unwrap_or_else(|_| CString::new("Serialization error").unwrap());
        s7::s7_error(
            sc,
            s7::s7_make_symbol(sc, c"serialization-error".as_ptr()),
            s7::s7_list(sc, 1, s7::s7_make_string(sc, message.as_ptr())),
        )
    }
}

pub(crate) fn primitive_s7_sync_serialize() -> Primitive {
    unsafe extern "C" fn code(sc: *mut s7::s7_scheme, args: s7::s7_pointer) -> s7::s7_pointer {
        unsafe {
            let node = s7::s7_car(args);
            let query = s7::s7_cadr(args);
            if !sync_is_node(node) {
                return s7::s7_wrong_type_arg_error(
                    sc,
                    c"sync-serialize".as_ptr(),
                    1,
                    node,
                    c"a sync-node".as_ptr(),
                );
            }
            let persistor = session_persistor_for(sc);
            let mut word = sync_heap_read(s7::s7_c_object_value(node));
            if s7::s7_is_boolean(query) && !s7::s7_boolean(sc, query) {
                // Full subtree.
            } else {
                match serialization_query(sc, node, query) {
                    Ok(trace) => match serialization_partial_tree(&persistor, &trace, word) {
                        Ok(partial) => word = partial,
                        Err(message) => return serialization_error(sc, &message),
                    },
                    Err((error_type, error_info)) => {
                        return s7::s7_error(sc, error_type, error_info);
                    }
                }
            }
            let definitions = match serialization_collect(&persistor, word) {
                Ok(definitions) => definitions,
                Err(message) => return serialization_error(sc, &message),
            };
            serialization_to_s7(sc, serialization_compact(definitions))
        }
    }
    Primitive::new(
        code,
        c"sync-serialize",
        c"(sync-serialize node query) serialize a sync-node proof selected by query, or the full subtree when query is #f",
        2,
        0,
        false,
    )
}

unsafe fn serialization_byte_vector_word(value: s7::s7_pointer) -> Option<Word> {
    unsafe {
        if !s7::s7_is_byte_vector(value) || s7::s7_vector_length(value) as usize != SIZE {
            return None;
        }
        let mut word = [0_u8; SIZE];
        for (index, byte) in word.iter_mut().enumerate() {
            *byte = s7::s7_byte_vector_ref(value, index as s7::s7_int);
        }
        Some(word)
    }
}

unsafe fn serialization_resolve(
    sc: *mut s7::s7_scheme,
    persistor: &MemoryPersistor,
    root: s7::s7_pointer,
    null_id: s7::s7_pointer,
    definitions: &HashMap<usize, s7::s7_pointer>,
    nodes: &mut HashMap<usize, Word>,
    visiting: &mut HashSet<usize>,
) -> Result<Word, String> {
    unsafe {
        let constant_tag = s7::s7_make_symbol(sc, c"c".as_ptr());
        let stub_tag = s7::s7_make_symbol(sc, c"s".as_ptr());
        let pair_tag = s7::s7_make_symbol(sc, c"p".as_ptr());
        let mut stack = vec![(root, false)];
        while let Some((id, expanded)) = stack.pop() {
            if id == null_id {
                continue;
            }
            if !s7::s7_is_symbol(id) {
                return Err("Serialization contains an invalid node reference".to_string());
            }
            let key = id as usize;
            if nodes.contains_key(&key) {
                continue;
            }
            let value = *definitions
                .get(&key)
                .ok_or_else(|| "Serialization contains an unresolved node reference".to_string())?;
            if !s7::s7_is_proper_list(sc, value) || s7::s7_is_null(sc, value) {
                return Err("Unknown serialization entry type".to_string());
            }
            let tag = s7::s7_car(value);
            if expanded {
                if tag != pair_tag || s7::s7_list_length(sc, value) != 3 {
                    return Err("Serialized pair is malformed".to_string());
                }
                let left_id = s7::s7_cadr(value);
                let right_id = s7::s7_caddr(value);
                let child = |id: s7::s7_pointer| {
                    if id == null_id {
                        Some(NULL)
                    } else if s7::s7_is_symbol(id) {
                        nodes.get(&(id as usize)).copied()
                    } else {
                        None
                    }
                };
                let left = child(left_id).ok_or_else(|| {
                    "Serialization contains an invalid node reference".to_string()
                })?;
                let right = child(right_id).ok_or_else(|| {
                    "Serialization contains an invalid node reference".to_string()
                })?;
                nodes.insert(key, serialization_branch(persistor, left, right)?);
                visiting.remove(&key);
                continue;
            }
            if !visiting.insert(key) {
                return Err("Serialization contains a cyclic node reference".to_string());
            }
            if tag == constant_tag {
                if s7::s7_list_length(sc, value) != 2 || !s7::s7_is_byte_vector(s7::s7_cadr(value))
                {
                    return Err("Serialized constant is malformed".to_string());
                }
                let bytes = s7::s7_cadr(value);
                let mut content = Vec::with_capacity(s7::s7_vector_length(bytes) as usize);
                for index in 0..s7::s7_vector_length(bytes) {
                    content.push(s7::s7_byte_vector_ref(bytes, index));
                }
                let node = persistor
                    .leaf_set(content)
                    .map_err(|_| "Serialized constant could not be stored".to_string())?;
                nodes.insert(key, node);
                visiting.remove(&key);
            } else if tag == stub_tag {
                if s7::s7_list_length(sc, value) != 2 {
                    return Err("Serialized stub is malformed".to_string());
                }
                let digest = serialization_byte_vector_word(s7::s7_cadr(value))
                    .ok_or_else(|| "Serialized stub is malformed".to_string())?;
                let node = persistor
                    .stump_set(digest)
                    .map_err(|_| "Serialized stub could not be stored".to_string())?;
                nodes.insert(key, node);
                visiting.remove(&key);
            } else if tag == pair_tag {
                if s7::s7_list_length(sc, value) != 3 {
                    return Err("Serialized pair is malformed".to_string());
                }
                stack.push((id, true));
                stack.push((s7::s7_caddr(value), false));
                stack.push((s7::s7_cadr(value), false));
            } else {
                return Err("Unknown serialization entry type".to_string());
            }
        }
        if root == null_id {
            Ok(NULL)
        } else {
            nodes
                .get(&(root as usize))
                .copied()
                .ok_or_else(|| "Serialization could not resolve the root node".to_string())
        }
    }
}

unsafe fn serialization_word_to_s7(
    sc: *mut s7::s7_scheme,
    persistor: &MemoryPersistor,
    word: Word,
) -> s7::s7_pointer {
    unsafe {
        if word == NULL {
            return s7::s7_make_c_object(sc, SYNC_NODE_TAG, sync_heap_make(NULL));
        }
        match resolve_node_with(persistor, word) {
            Some(ResolvedNode::Leaf(content, _)) => serialization_byte_vector(sc, &content),
            _ => s7::s7_make_c_object(sc, SYNC_NODE_TAG, sync_heap_make(word)),
        }
    }
}

pub(crate) fn primitive_s7_sync_deserialize() -> Primitive {
    unsafe extern "C" fn code(sc: *mut s7::s7_scheme, args: s7::s7_pointer) -> s7::s7_pointer {
        unsafe {
            let serialization = s7::s7_car(args);
            if !s7::s7_is_proper_list(sc, serialization) {
                return serialization_error(sc, "Serialization must be a list");
            }
            if s7::s7_is_null(sc, serialization) {
                return s7::s7_make_c_object(sc, SYNC_NODE_TAG, sync_heap_make(NULL));
            }
            let null_id = s7::s7_make_symbol(sc, c"n-0".as_ptr());
            let stub_tag = s7::s7_make_symbol(sc, c"s".as_ptr());
            let mut definitions = HashMap::new();
            let mut cursor = serialization;
            while s7::s7_is_pair(cursor) {
                let entry = s7::s7_car(cursor);
                if !s7::s7_is_proper_list(sc, entry)
                    || s7::s7_list_length(sc, entry) != 2
                    || !s7::s7_is_symbol(s7::s7_car(entry))
                    || s7::s7_car(entry) == null_id
                    || !s7::s7_is_proper_list(sc, s7::s7_cadr(entry))
                    || definitions.contains_key(&(s7::s7_car(entry) as usize))
                {
                    return serialization_error(sc, "Malformed or duplicate serialization entry");
                }
                let value = s7::s7_cadr(entry);
                if s7::s7_is_pair(value)
                    && s7::s7_car(value) == stub_tag
                    && s7::s7_list_length(sc, value) == 2
                    && s7::s7_is_byte_vector(s7::s7_cadr(value))
                    && s7::s7_vector_length(s7::s7_cadr(value)) as usize != SIZE
                {
                    return s7::s7_wrong_type_arg_error(
                        sc,
                        c"sync-cut".as_ptr(),
                        1,
                        s7::s7_cadr(value),
                        c"a hash-sized byte-vector".as_ptr(),
                    );
                }
                definitions.insert(s7::s7_car(entry) as usize, value);
                cursor = s7::s7_cdr(cursor);
            }
            let persistor = session_persistor_for(sc);
            let root_id = s7::s7_make_symbol(sc, c"n-1".as_ptr());
            let mut nodes = HashMap::new();
            let mut visiting = HashSet::new();
            let root = match serialization_resolve(
                sc,
                &persistor,
                root_id,
                null_id,
                &definitions,
                &mut nodes,
                &mut visiting,
            ) {
                Ok(root) => root,
                Err(message) => return serialization_error(sc, &message),
            };
            let mut cursor = serialization;
            while s7::s7_is_pair(cursor) {
                if let Err(message) = serialization_resolve(
                    sc,
                    &persistor,
                    s7::s7_car(s7::s7_car(cursor)),
                    null_id,
                    &definitions,
                    &mut nodes,
                    &mut visiting,
                ) {
                    return serialization_error(sc, &message);
                }
                cursor = s7::s7_cdr(cursor);
            }
            serialization_word_to_s7(sc, &persistor, root)
        }
    }
    Primitive::new(
        code,
        c"sync-deserialize",
        c"(sync-deserialize serialization) reconstruct sync-node data from compact serialization",
        1,
        0,
        false,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nested_traces_are_isolated_and_cleaned_up() {
        let sc = std::ptr::dangling_mut::<s7::s7_scheme>();
        let outer = [1; SIZE];
        let inner = [2; SIZE];
        serialization_trace_start(sc);
        serialization_trace_pair(sc, outer, [3; SIZE], [4; SIZE]);
        serialization_trace_start(sc);
        serialization_trace_child(sc, inner, [5; SIZE], true);

        let nested = serialization_trace_finish(sc);
        assert!(nested.edges.contains_key(&inner));
        assert!(!nested.edges.contains_key(&outer));
        assert!(serialization_trace_active(sc));

        let enclosing = serialization_trace_finish(sc);
        assert!(enclosing.edges.contains_key(&outer));
        assert!(!enclosing.edges.contains_key(&inner));
        assert!(!serialization_trace_active(sc));
    }
}
