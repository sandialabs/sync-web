use super::sandbox::*;
use crate::*;

pub(crate) unsafe fn s7_word_arg(
    sc: *mut s7::s7_scheme,
    value: s7::s7_pointer,
    name: &CStr,
    position: i64,
) -> Result<Word, s7::s7_pointer> {
    unsafe {
        if !s7::s7_is_byte_vector(value) || s7::s7_vector_length(value) as usize != SIZE {
            return Err(s7::s7_wrong_type_arg_error(
                sc,
                name.as_ptr(),
                position,
                value,
                c"a hash-sized byte-vector".as_ptr(),
            ));
        }
        let mut word = [0; SIZE];
        for (index, byte) in word.iter_mut().enumerate() {
            *byte = s7::s7_byte_vector_ref(value, index as i64);
        }
        Ok(word)
    }
}

pub(crate) unsafe fn word_to_s7_byte_vector(sc: *mut s7::s7_scheme, word: &Word) -> s7::s7_pointer {
    unsafe {
        let result = s7::s7_make_byte_vector(sc, SIZE as i64, 1, std::ptr::null_mut());
        for (index, byte) in word.iter().enumerate() {
            s7::s7_byte_vector_set(result, index as i64, *byte);
        }
        result
    }
}

pub(crate) unsafe fn string_to_s7(sc: *mut s7::s7_scheme, string: &str) -> s7::s7_pointer {
    unsafe {
        let c_string = CString::new(string).expect("Failed to create CString from string");
        let s7_string = s7::s7_make_string(sc, c_string.as_ptr());
        s7::s7_object_to_string(sc, s7_string, false)
    }
}

pub(crate) unsafe fn sync_heap_make(word: Word) -> *mut libc::c_void {
    unsafe {
        let ptr = libc::malloc(SIZE);
        let array: &mut [u8] = std::slice::from_raw_parts_mut(ptr as *mut u8, SIZE);
        for i in 0..SIZE {
            array[i] = word[i] as u8;
        }
        ptr
    }
}

pub(crate) unsafe fn sync_heap_read(ptr: *mut libc::c_void) -> Word {
    unsafe {
        std::slice::from_raw_parts_mut(ptr as *mut u8, SIZE)
            .try_into()
            .expect("Failed to convert slice to Word array")
    }
}

pub(crate) unsafe fn sync_heap_free(ptr: *mut libc::c_void) {
    unsafe {
        libc::free(ptr);
    }
}

pub(crate) unsafe fn sync_is_node(obj: s7::s7_pointer) -> bool {
    unsafe { s7::s7_is_c_object(obj) && s7::s7_c_object_type(obj) == SYNC_NODE_TAG }
}

pub(crate) unsafe fn sync_cxr(
    sc: *mut s7::s7_scheme,
    args: s7::s7_pointer,
    name: &CStr,
    left_side: bool,
    selector: fn((Word, Word)) -> Word,
) -> s7::s7_pointer {
    unsafe {
        let node = s7::s7_car(args);
        let word = sync_heap_read(s7::s7_c_object_value(node));
        let persistor = session_persistor_for(sc);

        let child_return = |word| {
            let node_return = |word| s7::s7_make_c_object(sc, SYNC_NODE_TAG, sync_heap_make(word));

            let vector_return = |vector: Vec<u8>| {
                let bv = s7::s7_make_byte_vector(sc, vector.len() as i64, 1, std::ptr::null_mut());
                for i in 0..vector.len() {
                    s7::s7_byte_vector_set(bv, i as i64, vector[i]);
                }
                bv
            };

            if word == NULL {
                return node_return(word);
            }

            match resolve_node_with(&persistor, word) {
                Some(ResolvedNode::Branch((left, right, digest), ResolveSource::Global)) => {
                    persistor
                        .branch_set(left, right, digest)
                        .expect("Failed to add branch to session persistor");
                    #[cfg(feature = "wasm-kernel")]
                    persistor.mark_imported(word);
                    node_return(word)
                }
                Some(ResolvedNode::Branch(_, _)) => node_return(word),
                Some(ResolvedNode::Leaf(content, ResolveSource::Global)) => {
                    persistor
                        .leaf_set(content.clone())
                        .expect("Failed to add leaf to session persistor");
                    #[cfg(feature = "wasm-kernel")]
                    persistor.mark_imported(word);
                    vector_return(content)
                }
                Some(ResolvedNode::Leaf(content, _)) => vector_return(content),
                Some(ResolvedNode::Stump(digest, ResolveSource::Global)) => {
                    persistor
                        .stump_set(digest)
                        .expect("Failed to add stump to session persistor");
                    #[cfg(feature = "wasm-kernel")]
                    persistor.mark_imported(word);
                    node_return(word)
                }
                Some(ResolvedNode::Stump(_, _)) => node_return(word),
                None => sync_error(
                    sc,
                    format!(
                        "Cannot retrieve items for node that is not a sync-pair ({})",
                        name.to_string_lossy()
                    )
                    .as_str(),
                ),
            }
        };

        match sync_is_node(node) {
            true => match resolve_branch_with(&persistor, word) {
                Some(((left, right, _), _)) => {
                    let child = selector((left, right));
                    serialization_trace_child(sc, word, child, left_side);
                    child_return(child)
                }
                None => sync_error(
                    sc,
                    format!(
                        "Journal cannot retrieve leaf byte-vector ({})",
                        name.to_string_lossy()
                    )
                    .as_str(),
                ),
            },
            false => {
                s7::s7_wrong_type_arg_error(sc, name.as_ptr(), 1, node, c"a sync-node".as_ptr())
            }
        }
    }
}

pub(crate) unsafe fn sync_digest(sc: *mut s7::s7_scheme, word: Word) -> Result<Word, String> {
    let persistor = session_persistor_for(sc);

    if word == NULL {
        Ok(NULL)
    } else {
        match resolve_node_with(&persistor, word) {
            Some(ResolvedNode::Branch((_, _, digest), _)) => Ok(digest),
            Some(ResolvedNode::Leaf(_, _)) => Ok(word),
            Some(ResolvedNode::Stump(digest, _)) => Ok(digest),
            None => Err("Digest not found in persistor".to_string()),
        }
    }
}

pub(crate) unsafe fn sync_branch_children(
    sc: *mut s7::s7_scheme,
    word: Word,
) -> Result<(Word, Word), String> {
    let persistor = session_persistor_for(sc);

    if let Some(((left, right, _), _)) = resolve_branch_with(&persistor, word) {
        Ok((left, right))
    } else {
        Err("Node is not a sync-pair".to_string())
    }
}

pub(crate) fn session_persistor_for(sc: *mut s7::s7_scheme) -> MemoryPersistor {
    let session = SESSIONS.read().expect("Failed to acquire SESSIONS lock");
    session
        .get(&(sc as usize))
        .expect("Session not found for given context")
        .persistor
        .clone()
}

pub(crate) unsafe fn session_serialization_query_get(
    sc: *mut s7::s7_scheme,
    source: &str,
) -> Option<s7::s7_pointer> {
    unsafe {
        SESSIONS
            .read()
            .expect("Failed to acquire SESSIONS lock")
            .get(&(sc as usize))
            .and_then(|session| session.serialization_query_locs.get(source))
            .map(|location| s7::s7_gc_protected_at(sc, *location))
    }
}

pub(crate) unsafe fn session_serialization_query_store(
    sc: *mut s7::s7_scheme,
    source: String,
    expression: s7::s7_pointer,
) -> s7::s7_pointer {
    unsafe {
        let location = s7::s7_gc_protect(sc, expression);
        SESSIONS
            .write()
            .expect("Failed to acquire SESSIONS lock")
            .get_mut(&(sc as usize))
            .expect("Session not found for given context")
            .serialization_query_locs
            .insert(source, location);
        expression
    }
}

pub(crate) unsafe fn serialization_query_environment(
    sc: *mut s7::s7_scheme,
) -> (s7::s7_pointer, s7::s7_int) {
    unsafe {
        let environment = sync_let_env(sc);
        for name in [c"sync-serialize", c"sync-deserialize"] {
            s7::s7_define(
                sc,
                environment.value,
                s7::s7_make_symbol(sc, name.as_ptr()),
                s7::s7_undefined(sc),
            );
        }
        (environment.value, environment.location)
    }
}

pub(crate) unsafe fn serialization_error_copy(
    sc: *mut s7::s7_scheme,
    error_args: s7::s7_pointer,
) -> Option<(s7::s7_pointer, s7::s7_pointer)> {
    unsafe {
        let copied = sync_let_copy(sc, error_args, &mut HashSet::new(), None).ok()?;
        if !s7::s7_is_proper_list(sc, copied.value) || s7::s7_list_length(sc, copied.value) != 2 {
            sync_let_unprotect(sc, copied);
            return None;
        }
        let error_type = s7::s7_gc_protect_via_stack(sc, s7::s7_car(copied.value));
        let error_info = s7::s7_gc_protect_via_stack(sc, s7::s7_cadr(copied.value));
        sync_let_unprotect(sc, copied);
        Some((error_type, error_info))
    }
}
