use super::support::*;
use crate::*;

pub(crate) fn type_s7_sync_node() -> Type {
    unsafe extern "C" fn free(_sc: *mut s7::s7_scheme, obj: s7::s7_pointer) -> s7::s7_pointer {
        unsafe {
            sync_heap_free(s7::s7_c_object_value(obj));
            std::ptr::null_mut()
        }
    }

    unsafe extern "C" fn mark(_sc: *mut s7::s7_scheme, _obj: s7::s7_pointer) -> s7::s7_pointer {
        std::ptr::null_mut()
    }

    unsafe extern "C" fn is_equal(sc: *mut s7::s7_scheme, args: s7::s7_pointer) -> s7::s7_pointer {
        unsafe {
            match sync_is_node(s7::s7_cadr(args)) {
                true => {
                    let word1 = sync_heap_read(s7::s7_c_object_value(s7::s7_car(args)));
                    let word2 = sync_heap_read(s7::s7_c_object_value(s7::s7_cadr(args)));
                    s7::s7_make_boolean(sc, word1 == word2)
                }
                false => s7::s7_wrong_type_arg_error(
                    sc,
                    c"equal?".as_ptr(),
                    2,
                    s7::s7_cadr(args),
                    c"a sync-node".as_ptr(),
                ),
            }
        }
    }

    unsafe extern "C" fn to_string(sc: *mut s7::s7_scheme, args: s7::s7_pointer) -> s7::s7_pointer {
        unsafe {
            string_to_s7(
                sc,
                format!(
                    "(sync-node #u({}))",
                    sync_heap_read(s7::s7_c_object_value(s7::s7_car(args)))
                        .iter()
                        .map(|&byte| byte.to_string())
                        .collect::<Vec<String>>()
                        .join(" "),
                )
                .as_str(),
            )
        }
    }

    Type::new(c"sync-node", free, mark, is_equal, to_string)
}

pub(crate) fn primitive_s7_sync_stub() -> Primitive {
    unsafe extern "C" fn code(sc: *mut s7::s7_scheme, args: s7::s7_pointer) -> s7::s7_pointer {
        unsafe {
            let digest = match s7_word_arg(sc, s7::s7_car(args), c"sync-cut", 1) {
                Ok(digest) => digest,
                Err(error) => return error,
            };

            let persistor = {
                let session = SESSIONS.read().expect("Failed to acquire SESSIONS lock");
                &session
                    .get(&(sc as usize))
                    .expect("Session not found for given context")
                    .persistor
                    .clone()
            };

            match persistor.stump_set(digest) {
                Ok(stump) => s7::s7_make_c_object(sc, SYNC_NODE_TAG, sync_heap_make(stump)),
                Err(_) => sync_error(sc, "Journal is unable to create stub node (sync-stub)"),
            }
        }
    }

    Primitive::new(
        code,
        c"sync-stub",
        c"(sync-stub digest) create a sync stub from the provided byte-vector",
        1,
        0,
        false,
    )
}

pub(crate) fn primitive_s7_sync_hash() -> Primitive {
    unsafe extern "C" fn code(sc: *mut s7::s7_scheme, args: s7::s7_pointer) -> s7::s7_pointer {
        unsafe {
            let data_bv = s7::s7_car(args);

            // check the input arguments
            if !s7::s7_is_byte_vector(data_bv) {
                return s7::s7_wrong_type_arg_error(
                    sc,
                    c"sync-hash".as_ptr(),
                    1,
                    data_bv,
                    c"a byte-vector".as_ptr(),
                );
            }

            // convert to rust data types
            let mut data = vec![];
            for i in 0..s7::s7_vector_length(data_bv) {
                data.push(s7::s7_byte_vector_ref(data_bv, i as i64))
            }

            let digest = Sha256::digest(data).to_vec();
            let digest_bv = s7::s7_make_byte_vector(sc, SIZE as i64, 1, std::ptr::null_mut());
            for i in 0..SIZE {
                s7::s7_byte_vector_set(digest_bv, i as i64, digest[i]);
            }
            digest_bv
        }
    }

    Primitive::new(
        code,
        c"sync-hash",
        c"(sync-hash bv) compute the SHA-256 digest of a byte vector",
        1,
        0,
        false,
    )
}

pub(crate) fn primitive_s7_sync_state() -> Primitive {
    unsafe extern "C" fn code(sc: *mut s7::s7_scheme, args: s7::s7_pointer) -> s7::s7_pointer {
        unsafe {
            if !s7::s7_is_null(sc, args) {
                return s7::s7_wrong_number_of_args_error(sc, c"sync-state".as_ptr(), args);
            }

            let state = {
                let session = SESSIONS.read().expect("Failed to acquire sessions lock");
                session
                    .get(&(sc as usize))
                    .expect("Session not found for sync-state")
                    .state
            };

            s7::s7_make_c_object(sc, SYNC_NODE_TAG, sync_heap_make(state))
        }
    }

    Primitive::new(
        code,
        c"sync-state",
        c"(sync-state) returns the current session state as a sync-node",
        0,
        0,
        false,
    )
}

pub(crate) fn primitive_s7_sync_is_node() -> Primitive {
    unsafe extern "C" fn code(sc: *mut s7::s7_scheme, args: s7::s7_pointer) -> s7::s7_pointer {
        unsafe { s7::s7_make_boolean(sc, sync_is_node(s7::s7_car(args))) }
    }

    Primitive::new(
        code,
        c"sync-node?",
        c"(sync-node?) returns whether the object is a sync node",
        1,
        0,
        false,
    )
}

pub(crate) fn primitive_s7_sync_null() -> Primitive {
    unsafe extern "C" fn code(sc: *mut s7::s7_scheme, _args: s7::s7_pointer) -> s7::s7_pointer {
        unsafe { s7::s7_make_c_object(sc, SYNC_NODE_TAG, sync_heap_make(NULL)) }
    }

    Primitive::new(
        code,
        c"sync-null",
        c"(sync-null) returns the null synchronic node",
        0,
        0,
        false,
    )
}

pub(crate) fn primitive_s7_sync_is_null() -> Primitive {
    unsafe extern "C" fn code(sc: *mut s7::s7_scheme, args: s7::s7_pointer) -> s7::s7_pointer {
        unsafe {
            let arg = s7::s7_car(args);
            if sync_is_node(arg) {
                let word = sync_heap_read(s7::s7_c_object_value(arg));
                for i in 0..SIZE {
                    if word[i] != 0 {
                        return s7::s7_make_boolean(sc, false);
                    }
                }
                s7::s7_make_boolean(sc, true)
            } else if s7::s7_is_byte_vector(arg) {
                s7::s7_make_boolean(sc, false)
            } else {
                s7::s7_wrong_type_arg_error(
                    sc,
                    c"sync-null?".as_ptr(),
                    1,
                    arg,
                    c"a sync-node or byte-vector".as_ptr(),
                )
            }
        }
    }

    Primitive::new(
        code,
        c"sync-null?",
        c"(sync-null? sp) returns whether a sync-node or byte-vector is equal to sync-null",
        1,
        0,
        false,
    )
}

pub(crate) fn primitive_s7_sync_is_pair() -> Primitive {
    unsafe extern "C" fn code(sc: *mut s7::s7_scheme, args: s7::s7_pointer) -> s7::s7_pointer {
        unsafe {
            let arg = s7::s7_car(args);
            if sync_is_node(arg) {
                let word = sync_heap_read(s7::s7_c_object_value(arg));
                s7::s7_make_boolean(sc, sync_branch_children(sc, word).is_ok())
            } else if s7::s7_is_byte_vector(arg) {
                s7::s7_make_boolean(sc, false)
            } else {
                s7::s7_wrong_type_arg_error(
                    sc,
                    c"sync-pair?".as_ptr(),
                    1,
                    arg,
                    c"a sync-node or byte-vector".as_ptr(),
                )
            }
        }
    }

    Primitive::new(
        code,
        c"sync-pair?",
        c"(sync-pair? sp) returns whether a sync-node or byte-vector is a pair",
        1,
        0,
        false,
    )
}

pub(crate) fn primitive_s7_sync_is_stub() -> Primitive {
    unsafe extern "C" fn code(sc: *mut s7::s7_scheme, args: s7::s7_pointer) -> s7::s7_pointer {
        unsafe {
            let arg = s7::s7_car(args);
            if sync_is_node(arg) {
                let word = sync_heap_read(s7::s7_c_object_value(arg));
                let persistor = session_persistor_for(sc);
                s7::s7_make_boolean(sc, resolve_stump_with(&persistor, word).is_some())
            } else if s7::s7_is_byte_vector(arg) {
                s7::s7_make_boolean(sc, false)
            } else {
                s7::s7_wrong_type_arg_error(
                    sc,
                    c"sync-stub?".as_ptr(),
                    1,
                    arg,
                    c"a sync-node or byte-vector".as_ptr(),
                )
            }
        }
    }

    Primitive::new(
        code,
        c"sync-stub?",
        c"(sync-stub? sp) returns whether a sync-node or byte-vector is a stub",
        1,
        0,
        false,
    )
}

pub(crate) fn primitive_s7_sync_digest() -> Primitive {
    unsafe extern "C" fn code(sc: *mut s7::s7_scheme, args: s7::s7_pointer) -> s7::s7_pointer {
        unsafe {
            let arg = s7::s7_car(args);
            if sync_is_node(arg) {
                let word = sync_heap_read(s7::s7_c_object_value(arg));
                let digest = sync_digest(sc, word).expect("Failed to obtain digest");
                word_to_s7_byte_vector(sc, &digest)
            } else if s7::s7_is_byte_vector(arg) {
                let mut data = vec![];
                for i in 0..s7::s7_vector_length(arg) {
                    data.push(s7::s7_byte_vector_ref(arg, i as i64))
                }
                let digest = Sha256::digest(data);
                let bv = s7::s7_make_byte_vector(sc, SIZE as i64, 1, std::ptr::null_mut());
                for i in 0..SIZE {
                    s7::s7_byte_vector_set(bv, i as i64, digest[i]);
                }
                bv
            } else {
                s7::s7_wrong_type_arg_error(
                    sc,
                    c"sync-digest".as_ptr(),
                    1,
                    arg,
                    c"a sync-node or byte-vector".as_ptr(),
                )
            }
        }
    }

    Primitive::new(
        code,
        c"sync-digest",
        c"(sync-digest value) returns the digest of a sync-node or byte-vector as a byte-vector",
        1,
        0,
        false,
    )
}

pub(crate) fn primitive_s7_sync_cons() -> Primitive {
    unsafe extern "C" fn code(sc: *mut s7::s7_scheme, args: s7::s7_pointer) -> s7::s7_pointer {
        unsafe {
            let persistor = {
                let session = SESSIONS.read().expect("Failed to acquire sessions lock");
                &session
                    .get(&(sc as usize))
                    .expect("Session not found for sync-cons")
                    .persistor
                    .clone()
            };

            let handle_arg = |obj, number| {
                if sync_is_node(obj) {
                    Ok(sync_heap_read(s7::s7_c_object_value(obj)))
                } else if s7::s7_is_byte_vector(obj) {
                    let mut content = vec![];
                    for i in 0..s7::s7_vector_length(obj) {
                        content.push(s7::s7_byte_vector_ref(obj, i as i64))
                    }
                    match persistor.leaf_set(content) {
                        Ok(atom) => Ok(atom),
                        Err(_) => Err(sync_error(
                            sc,
                            "Journal is unable to add leaf node (sync-cons)",
                        )),
                    }
                } else {
                    Err(s7::s7_wrong_type_arg_error(
                        sc,
                        c"sync-cons".as_ptr(),
                        number,
                        obj,
                        c"a byte vector or a sync node".as_ptr(),
                    ))
                }
            };

            match (
                handle_arg(s7::s7_car(args), 1),
                handle_arg(s7::s7_cadr(args), 2),
            ) {
                (Ok(left), Ok(right)) => match (sync_digest(sc, left), sync_digest(sc, right)) {
                    (Ok(digest_left), Ok(digest_right)) => {
                        let mut joined = [0 as u8; SIZE * 2];
                        joined[..SIZE].copy_from_slice(&digest_left);
                        joined[SIZE..].copy_from_slice(&digest_right);
                        let digest = Word::from(Sha256::digest(joined));

                        match persistor.branch_set(left, right, digest) {
                            Ok(pair) => {
                                serialization_trace_pair(sc, pair, left, right);
                                s7::s7_make_c_object(sc, SYNC_NODE_TAG, sync_heap_make(pair))
                            }
                            Err(_) => {
                                sync_error(sc, "Journal is unable to add pair node (sync-cons)")
                            }
                        }
                    }
                    _ => sync_error(sc, "Journal is unable to obtain node digests (sync-cons)"),
                },
                (Err(left), _) => left,
                (_, Err(right)) => right,
            }
        }
    }

    Primitive::new(
        code,
        c"sync-cons",
        c"(sync-cons first rest) construct a new sync pair node",
        2,
        0,
        false,
    )
}

pub(crate) fn primitive_s7_sync_car() -> Primitive {
    unsafe extern "C" fn code(sc: *mut s7::s7_scheme, args: s7::s7_pointer) -> s7::s7_pointer {
        unsafe {
            if !sync_is_node(s7::s7_car(args)) {
                return s7::s7_wrong_type_arg_error(
                    sc,
                    c"sync-car".as_ptr(),
                    1,
                    s7::s7_car(args),
                    c"a sync-pair".as_ptr(),
                );
            }
            sync_cxr(sc, args, c"sync-car", true, |children| children.0)
        }
    }

    Primitive::new(
        code,
        c"sync-car",
        c"(sync-car pair) retrieve the first element of a sync pair",
        1,
        0,
        false,
    )
}

pub(crate) fn primitive_s7_sync_cdr() -> Primitive {
    unsafe extern "C" fn code(sc: *mut s7::s7_scheme, args: s7::s7_pointer) -> s7::s7_pointer {
        unsafe {
            if !sync_is_node(s7::s7_car(args)) {
                return s7::s7_wrong_type_arg_error(
                    sc,
                    c"sync-cdr".as_ptr(),
                    1,
                    s7::s7_car(args),
                    c"a sync-pair".as_ptr(),
                );
            }
            sync_cxr(sc, args, c"sync-cdr", false, |children| children.1)
        }
    }

    Primitive::new(
        code,
        c"sync-cdr",
        c"(sync-cdr pair) retrieve the second element of a sync pair",
        1,
        0,
        false,
    )
}

pub(crate) fn primitive_s7_sync_cut() -> Primitive {
    unsafe extern "C" fn code(sc: *mut s7::s7_scheme, args: s7::s7_pointer) -> s7::s7_pointer {
        unsafe {
            let arg = s7::s7_car(args);

            let handle_digest = |digest| {
                let persistor = {
                    let session = SESSIONS.read().expect("Failed to acquire SESSIONS lock");
                    &session
                        .get(&(sc as usize))
                        .expect("Session not found for given context")
                        .persistor
                        .clone()
                };
                match persistor.stump_set(digest) {
                    Ok(stump) => s7::s7_make_c_object(sc, SYNC_NODE_TAG, sync_heap_make(stump)),
                    Err(_) => sync_error(sc, "Journal is unable to add stub node (sync-cut)"),
                }
            };

            if s7::s7_is_byte_vector(arg) {
                let mut content = vec![];
                for i in 0..s7::s7_vector_length(arg) {
                    content.push(s7::s7_byte_vector_ref(arg, i as i64))
                }
                handle_digest(Word::from(Sha256::digest(Sha256::digest(&content))))
            } else if sync_is_node(arg) {
                match sync_digest(sc, sync_heap_read(s7::s7_c_object_value(arg))) {
                    Ok(digest) => handle_digest(digest),
                    Err(_) => sync_error(sc, "Journal does not recognize input node (sync-cut)"),
                }
            } else {
                s7::s7_wrong_type_arg_error(
                    sc,
                    c"sync-cut".as_ptr(),
                    1,
                    s7::s7_car(args),
                    c"a sync-node or byte-vector".as_ptr(),
                )
            }
        }
    }

    Primitive::new(
        code,
        c"sync-cut",
        c"(sync-cut value) obtain the stub of a sync-node",
        1,
        0,
        false,
    )
}
