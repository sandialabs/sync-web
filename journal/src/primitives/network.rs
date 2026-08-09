#[cfg(feature = "wasm-kernel")]
use super::host::*;
use crate::*;

#[cfg(not(feature = "wasm-kernel"))]
pub(crate) fn primitive_s7_sync_http() -> Primitive {
    unsafe extern "C" fn code(sc: *mut s7::s7_scheme, args: s7::s7_pointer) -> s7::s7_pointer {
        unsafe {
            mark_external_called(sc);

            let vec2s7 = |vector: Vec<u8>| {
                let bv = s7::s7_make_byte_vector(sc, vector.len() as i64, 1, std::ptr::null_mut());
                for i in 0..vector.len() {
                    s7::s7_byte_vector_set(bv, i as i64, vector[i]);
                }
                bv
            };

            let method = obj2str(sc, s7::s7_car(args));
            let url = obj2str(sc, s7::s7_cadr(args));

            let body = if s7::s7_list_length(sc, args) >= 3 {
                obj2str(sc, s7::s7_caddr(args))
            } else {
                String::from("")
            };

            if SESSIONS
                .read()
                .expect("Failed to acquire session lock")
                .get(&(sc as usize))
                .and_then(|session| session.scenario.as_ref())
                .is_some()
            {
                return sync_error(sc, "sync-http is not supported by the scenario harness");
            }

            let cache_mutex = {
                let session = SESSIONS.read().expect("Failed to acquire sessions lock");
                session
                    .get(&(sc as usize))
                    .expect("Session ID not found in active sessions")
                    .cache
                    .clone()
            };

            let mut cache = cache_mutex
                .lock()
                .expect("Failed to acquire cache mutex lock");

            let key = (method.clone(), url.clone(), body.as_bytes().to_vec());

            match cache.get(&key) {
                Some(bytes) => {
                    debug!("Cache hit on key {:?}", key);
                    vec2s7(bytes.to_vec())
                }
                None => {
                    let result = tokio::task::block_in_place(move || {
                        tokio::runtime::Handle::current().block_on(async move {
                            match method.to_lowercase() {
                                method if method == "get" => {
                                    response_bytes(
                                        JOURNAL.client.get(&url[1..url.len() - 1]).send().await,
                                    )
                                    .await
                                }
                                method if method == "post" => {
                                    response_bytes(
                                        JOURNAL
                                            .client
                                            .post(&url[1..url.len() - 1])
                                            .body(String::from(&body[1..body.len() - 1]))
                                            .send()
                                            .await,
                                    )
                                    .await
                                }
                                _ => {
                                    panic!("Unsupported HTTP method")
                                }
                            }
                        })
                    });

                    match result {
                        Ok(vector) => {
                            cache.insert(key, vector.to_vec());
                            vec2s7(vector.to_vec())
                        }
                        Err(_) => {
                            sync_error(sc, "Journal is unable to fulfill HTTP request (sync-http)")
                        }
                    }
                }
            }
        }
    }

    Primitive::new(
        code,
        c"sync-http",
        c"(sync-http method url . data) make an http request where method is 'get or 'post",
        2,
        2,
        false,
    )
}

#[cfg(not(feature = "wasm-kernel"))]
pub(crate) fn primitive_s7_sync_remote() -> Primitive {
    unsafe extern "C" fn code(sc: *mut s7::s7_scheme, args: s7::s7_pointer) -> s7::s7_pointer {
        unsafe {
            mark_external_called(sc);

            let vec2s7 = |mut vector: Vec<u8>| {
                vector.insert(0, 39); // add quote character so that it evaluates correctly
                vector.push(0);
                match CString::from_vec_with_nul(vector) {
                    Ok(c_string) => s7::s7_eval_c_string(sc, c_string.as_ptr()),
                    Err(_) => {
                        sync_error(sc, "Journal is unable to query remote peer (sync-remote)")
                    }
                }
            };

            let url = obj2str(sc, s7::s7_car(args));

            let body = obj2str(sc, s7::s7_cadr(args));

            let cache_mutex = {
                let session = SESSIONS.read().expect("Failed to acquire session lock");
                session
                    .get(&(sc as usize))
                    .expect("Failed to get session from map")
                    .cache
                    .clone()
            };

            let mut cache = cache_mutex.lock().expect("Failed to acquire cache lock");

            let key = (String::from("post"), url.clone(), body.as_bytes().to_vec());

            match cache.get(&key) {
                Some(bytes) => {
                    debug!("Cache hit on key {:?}", key);
                    vec2s7(bytes.to_vec())
                }
                None => {
                    let scenario = {
                        let sessions = SESSIONS.read().expect("Failed to acquire session lock");
                        let session = sessions
                            .get(&(sc as usize))
                            .expect("Failed to get session from map");
                        session
                            .scenario
                            .as_ref()
                            .map(|scenario| (scenario.clone(), session.record))
                    };
                    let result: Result<Vec<u8>, String> = if let Some((scenario, source)) = scenario
                    {
                        scenario.transport.remote(
                            scenario.action,
                            source,
                            url[1..url.len() - 1].to_string(),
                            body,
                        )
                    } else {
                        tokio::task::block_in_place(move || {
                            tokio::runtime::Handle::current().block_on(async move {
                                remote_post(&JOURNAL.client, &url[1..url.len() - 1], body).await
                            })
                        })
                    };

                    match result {
                        Ok(bytes) if valid_remote_response(sc, &bytes) => {
                            cache.insert(key, bytes.to_vec());
                            vec2s7(bytes)
                        }
                        Ok(_) | Err(_) => {
                            sync_error(sc, "Journal is unable to query remote peer (sync-remote)")
                        }
                    }
                }
            }
        }
    }

    Primitive::new(
        code,
        c"sync-remote",
        c"(sync-remote url data) make a post http request with the data payload)",
        2,
        0,
        false,
    )
}

#[cfg(feature = "wasm-kernel")]
unsafe fn kernel_http_request(
    sc: *mut s7::s7_scheme,
    args: s7::s7_pointer,
    remote: bool,
) -> Vec<u8> {
    unsafe {
        let first = obj2str(sc, s7::s7_car(args));
        let second = obj2str(sc, s7::s7_cadr(args));
        let rest = if remote {
            String::new()
        } else if s7::s7_is_null(sc, s7::s7_cddr(args)) {
            String::new()
        } else {
            obj2str(sc, s7::s7_caddr(args))
        };
        let mut request = Vec::new();
        request.extend((first.len() as u32).to_be_bytes());
        request.extend((second.len() as u32).to_be_bytes());
        request.extend(first.as_bytes());
        request.extend(second.as_bytes());
        request.extend(rest.as_bytes());
        request
    }
}
#[cfg(feature = "wasm-kernel")]
pub(crate) fn primitive_s7_sync_http() -> Primitive {
    unsafe extern "C" fn code(sc: *mut s7::s7_scheme, args: s7::s7_pointer) -> s7::s7_pointer {
        unsafe { kernel_capability(sc, 5, &kernel_http_request(sc, args, false), true) }
    }
    Primitive::new(
        code,
        c"sync-http",
        c"(sync-http method url . data) make an http request where method is 'get or 'post",
        2,
        2,
        false,
    )
}
#[cfg(feature = "wasm-kernel")]
pub(crate) fn primitive_s7_sync_remote() -> Primitive {
    unsafe extern "C" fn code(sc: *mut s7::s7_scheme, args: s7::s7_pointer) -> s7::s7_pointer {
        unsafe { kernel_capability(sc, 6, &kernel_http_request(sc, args, true), true) }
    }
    Primitive::new(
        code,
        c"sync-remote",
        c"(sync-remote url data) make a post http request with the data payload)",
        2,
        0,
        false,
    )
}
