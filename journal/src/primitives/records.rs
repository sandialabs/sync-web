#[cfg(feature = "wasm-kernel")]
use super::host::*;
#[cfg(feature = "wasm-kernel")]
use super::support::*;
use crate::*;

#[cfg(not(feature = "wasm-kernel"))]
pub(crate) fn primitive_s7_sync_create() -> Primitive {
    unsafe extern "C" fn code(sc: *mut s7::s7_scheme, args: s7::s7_pointer) -> s7::s7_pointer {
        unsafe {
            let record = match s7_word_arg(sc, s7::s7_car(args), c"sync-create", 1) {
                Ok(record) => record,
                Err(error) => return error,
            };

            debug!("Adding record: {}", hex::encode(record));

            match PERSISTOR.root_new(
                record,
                PERSISTOR
                    .branch_set(
                        PERSISTOR
                            .leaf_set(GENESIS_STR.as_bytes().to_vec())
                            .expect("Failed to create genesis leaf for new record"),
                        NULL,
                        NULL,
                    )
                    .expect("Failed to create genesis branch for new record"),
            ) {
                Ok(_) => s7::s7_make_boolean(sc, true),
                Err(_) => s7::s7_error(
                    sc,
                    s7::s7_make_symbol(sc, c"sync-web-error".as_ptr()),
                    s7::s7_list(
                        sc,
                        1,
                        s7::s7_make_string(sc, c"record ID is already in use".as_ptr()),
                    ),
                ),
            }
        }
    }

    Primitive::new(
        code,
        c"sync-create",
        c"(sync-create id) create a new synchronic record with the given 32-byte ID",
        1,
        0,
        false,
    )
}

#[cfg(not(feature = "wasm-kernel"))]
pub(crate) fn primitive_s7_sync_delete() -> Primitive {
    unsafe extern "C" fn code(sc: *mut s7::s7_scheme, args: s7::s7_pointer) -> s7::s7_pointer {
        unsafe {
            let record = match s7_word_arg(sc, s7::s7_car(args), c"sync-delete", 1) {
                Ok(record) => record,
                Err(error) => return error,
            };

            if record == NULL {
                return s7::s7_error(
                    sc,
                    s7::s7_make_symbol(sc, c"sync-web-error".as_ptr()),
                    s7::s7_list(
                        sc,
                        1,
                        s7::s7_make_string(sc, c"cannot delete the root record".as_ptr()),
                    ),
                );
            }

            debug!("Deleting record: {}", hex::encode(record));

            match PERSISTOR.root_delete(record) {
                Ok(_) => s7::s7_make_boolean(sc, true),
                Err(_) => s7::s7_error(
                    sc,
                    s7::s7_make_symbol(sc, c"sync-web-error".as_ptr()),
                    s7::s7_list(
                        sc,
                        1,
                        s7::s7_make_string(sc, c"record ID does not exist".as_ptr()),
                    ),
                ),
            }
        }
    }

    Primitive::new(
        code,
        c"sync-delete",
        c"(sync-delete id) delete the synchronic record with the given 32-byte ID",
        1,
        0,
        false,
    )
}

#[cfg(not(feature = "wasm-kernel"))]
pub(crate) fn primitive_s7_sync_all() -> Primitive {
    unsafe extern "C" fn code(sc: *mut s7::s7_scheme, _args: s7::s7_pointer) -> s7::s7_pointer {
        unsafe {
            let mut list = s7::s7_list(sc, 0);

            for record in PERSISTOR.root_list().into_iter().rev() {
                list = s7::s7_cons(sc, word_to_s7_byte_vector(sc, &record), list)
            }

            list
        }
    }

    Primitive::new(
        code,
        c"sync-all",
        c"(sync-all) list all synchronic record IDs in ascending order",
        0,
        0,
        false,
    )
}

#[cfg(not(feature = "wasm-kernel"))]
pub(crate) fn primitive_s7_sync_call() -> Primitive {
    unsafe extern "C" fn code(sc: *mut s7::s7_scheme, args: s7::s7_pointer) -> s7::s7_pointer {
        unsafe {
            mark_external_called(sc);

            let message_expr = s7::s7_car(args);
            let blocking = s7::s7_cadr(args);

            if !s7::s7_is_boolean(blocking) {
                return s7::s7_wrong_type_arg_error(
                    sc,
                    c"sync-call".as_ptr(),
                    2,
                    blocking,
                    c"a boolean".as_ptr(),
                );
            }

            let record = match s7::s7_is_null(sc, s7::s7_cddr(args)) {
                true => {
                    let session = SESSIONS.read().expect("Failed to acquire sessions lock");
                    session
                        .get(&(sc as usize))
                        .expect("Session number not found in sessions map")
                        .record
                }
                false => match s7_word_arg(sc, s7::s7_caddr(args), c"sync-call", 3) {
                    Ok(record) => record,
                    Err(error) => return error,
                },
            };

            match PERSISTOR.root_get(record) {
                Ok(_) => {
                    let message = obj2str(sc, message_expr);
                    let scenario = SESSIONS
                        .read()
                        .expect("Failed to acquire sessions lock")
                        .get(&(sc as usize))
                        .and_then(|session| session.scenario.clone());
                    if s7::s7_boolean(sc, blocking) {
                        let result = JOURNAL.evaluate_record_with_context(
                            record,
                            message.as_str(),
                            scenario,
                        );
                        let c_result = CString::new(format!("(quote {})", result))
                            .expect("Failed to create C string from journal evaluation result");
                        s7::s7_eval_c_string(sc, c_result.as_ptr())
                    } else {
                        if scenario.is_some() {
                            JOURNAL.evaluate_record_with_context(
                                record,
                                message.as_str(),
                                scenario,
                            );
                        } else {
                            tokio::spawn(async move {
                                JOURNAL.evaluate_record(record, message.as_str());
                            });
                        }
                        s7::s7_make_boolean(sc, true)
                    }
                }
                Err(_) => s7::s7_error(
                    sc,
                    s7::s7_make_symbol(sc, c"sync-web-error".as_ptr()),
                    s7::s7_list(
                        sc,
                        1,
                        s7::s7_make_string(sc, c"record ID does not exist".as_ptr()),
                    ),
                ),
            }
        }
    }

    Primitive::new(
        code,
        c"sync-call",
        c"(sync-call query blocking? id) query the provided record ID or self if ID not provided",
        2,
        1,
        false,
    )
}

#[cfg(feature = "wasm-kernel")]
pub(crate) fn primitive_s7_sync_create() -> Primitive {
    unsafe extern "C" fn code(sc: *mut s7::s7_scheme, args: s7::s7_pointer) -> s7::s7_pointer {
        unsafe {
            match s7_word_arg(sc, s7::s7_car(args), c"sync-create", 1) {
                Ok(word) => kernel_capability(sc, 1, &word, false),
                Err(error) => error,
            }
        }
    }
    Primitive::new(
        code,
        c"sync-create",
        c"(sync-create id) create a new synchronic record with the given 32-byte ID",
        1,
        0,
        false,
    )
}
#[cfg(feature = "wasm-kernel")]
pub(crate) fn primitive_s7_sync_delete() -> Primitive {
    unsafe extern "C" fn code(sc: *mut s7::s7_scheme, args: s7::s7_pointer) -> s7::s7_pointer {
        unsafe {
            match s7_word_arg(sc, s7::s7_car(args), c"sync-delete", 1) {
                Ok(word) => kernel_capability(sc, 2, &word, false),
                Err(error) => error,
            }
        }
    }
    Primitive::new(
        code,
        c"sync-delete",
        c"(sync-delete id) delete the synchronic record with the given 32-byte ID",
        1,
        0,
        false,
    )
}
#[cfg(feature = "wasm-kernel")]
pub(crate) fn primitive_s7_sync_all() -> Primitive {
    unsafe extern "C" fn code(sc: *mut s7::s7_scheme, _args: s7::s7_pointer) -> s7::s7_pointer {
        unsafe { kernel_capability(sc, 3, &[], false) }
    }
    Primitive::new(
        code,
        c"sync-all",
        c"(sync-all) list all synchronic record IDs in ascending order",
        0,
        0,
        false,
    )
}
#[cfg(feature = "wasm-kernel")]
pub(crate) fn primitive_s7_sync_call() -> Primitive {
    unsafe extern "C" fn code(sc: *mut s7::s7_scheme, args: s7::s7_pointer) -> s7::s7_pointer {
        unsafe {
            let blocking = s7::s7_cadr(args);
            if !s7::s7_is_boolean(blocking) {
                return s7::s7_wrong_type_arg_error(
                    sc,
                    c"sync-call".as_ptr(),
                    2,
                    blocking,
                    c"a boolean".as_ptr(),
                );
            }
            let record = if s7::s7_is_null(sc, s7::s7_cddr(args)) {
                SESSIONS
                    .read()
                    .expect("sessions lock")
                    .get(&(sc as usize))
                    .expect("session")
                    .record
            } else {
                match s7_word_arg(sc, s7::s7_caddr(args), c"sync-call", 3) {
                    Ok(word) => word,
                    Err(error) => return error,
                }
            };
            let message = obj2str(sc, s7::s7_car(args));
            let is_blocking = s7::s7_boolean(sc, blocking);
            let mut request = vec![u8::from(is_blocking)];
            request.extend(record);
            request.extend(message.as_bytes());
            if !is_blocking {
                return kernel_capability(sc, 4, &request, true);
            }
            let response = match kernel_capability_bytes(sc, 4, &request, true) {
                Ok(response) => response,
                Err(error) => return error,
            };
            let Ok(output) = CString::new(response) else {
                return sync_error(sc, "native call returned invalid data");
            };
            s7::s7_eval_c_string(sc, output.as_ptr())
        }
    }
    Primitive::new(
        code,
        c"sync-call",
        c"(sync-call query blocking? id) query the provided record ID or self if ID not provided",
        2,
        1,
        false,
    )
}
