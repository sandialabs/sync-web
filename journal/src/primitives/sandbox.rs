use super::support::*;
use crate::*;

const SYNC_LET_ALLOWED: &[&str] = &[
    "*",
    "+",
    "-",
    "/",
    "<",
    "<=",
    "=",
    ">",
    ">=",
    "and",
    "append",
    "apply",
    "apply-values",
    "ash",
    "assq",
    "assoc",
    "begin",
    "boolean?",
    "byte-vector?",
    "byte-vector->expression",
    "byte-vector->hex-string",
    "byte-vector-length",
    "byte-vector-ref",
    "byte-vector-set!",
    "caadar",
    "caadr",
    "caar",
    "cadar",
    "cadr",
    "caddr",
    "car",
    "case",
    "catch",
    "cdadar",
    "cddar",
    "cddr",
    "cdr",
    "char?",
    "complex?",
    "cond",
    "cons",
    "define",
    "define*",
    "do",
    "else",
    "eq?",
    "equal?",
    "eqv?",
    "error",
    "even?",
    "expt",
    "expression->byte-vector",
    "for-each",
    "if",
    "integer?",
    "keyword?",
    "keyword->symbol",
    "lambda",
    "lambda*",
    "length",
    "let",
    "let*",
    "letrec",
    "letrec*",
    "list",
    "list-tail",
    "list-values",
    "list?",
    "logand",
    "macro?",
    "make-list",
    "map",
    "max",
    "member",
    "memq",
    "min",
    "modulo",
    "negative?",
    "not",
    "null?",
    "number->string",
    "number?",
    "odd?",
    "or",
    "pair?",
    "positive?",
    "procedure?",
    "proper-list?",
    "quasiquote",
    "quote",
    "rational?",
    "real?",
    "remainder",
    "reverse",
    "set!",
    "string->symbol",
    "string?",
    "string=?",
    "string-length",
    "string-ref",
    "substring",
    "subvector",
    "symbol->string",
    "symbol?",
    "sync-car",
    "sync-cdr",
    "sync-cons",
    "sync-cut",
    "sync-deserialize",
    "sync-digest",
    "sync-eval",
    "sync-hash",
    "sync-node?",
    "sync-null",
    "sync-null?",
    "sync-pair?",
    "sync-serialize",
    "sync-stub",
    "sync-stub?",
    "throw",
    "unquote",
    "unquote-splicing",
    "vector",
    "vector-length",
    "vector-ref",
    "vector-set!",
    "vector?",
    "zero?",
];

enum SyncLetCopyTask {
    Visit(s7::s7_pointer),
    FinishVector {
        address: usize,
        length: s7::s7_int,
    },
    FinishList {
        addresses: Vec<usize>,
        length: usize,
    },
}

pub(crate) struct SyncLetProtected {
    pub(crate) value: s7::s7_pointer,
    pub(crate) location: s7::s7_int,
}

unsafe fn sync_let_protect(sc: *mut s7::s7_scheme, value: s7::s7_pointer) -> SyncLetProtected {
    unsafe {
        SyncLetProtected {
            value,
            location: s7::s7_gc_protect(sc, value),
        }
    }
}

pub(crate) unsafe fn sync_let_unprotect(sc: *mut s7::s7_scheme, protected: SyncLetProtected) {
    unsafe {
        s7::s7_gc_unprotect_at(sc, protected.location);
    }
}

unsafe fn sync_let_copy_cleanup(sc: *mut s7::s7_scheme, results: &mut Vec<SyncLetProtected>) {
    unsafe {
        for result in results.drain(..) {
            sync_let_unprotect(sc, result);
        }
    }
}

pub(crate) unsafe fn sync_let_copy(
    sc: *mut s7::s7_scheme,
    value: s7::s7_pointer,
    visiting: &mut HashSet<usize>,
    allowed_syntax: Option<&HashSet<usize>>,
) -> Result<SyncLetProtected, String> {
    unsafe {
        let mut tasks = vec![SyncLetCopyTask::Visit(value)];
        let mut results = Vec::new();
        while let Some(task) = tasks.pop() {
            match task {
                SyncLetCopyTask::Visit(value) => {
                    if value == s7::s7_undefined(sc)
                        || value == s7::s7_unspecified(sc)
                        || value == s7::s7_eof_object(sc)
                    {
                        sync_let_copy_cleanup(sc, &mut results);
                        return Err(
                            "sync-let does not accept undefined, unspecified, or eof values"
                                .to_string(),
                        );
                    }
                    if s7::s7_is_syntax(value) {
                        if allowed_syntax.is_some_and(|allowed| allowed.contains(&(value as usize)))
                        {
                            results.push(sync_let_protect(sc, value));
                            continue;
                        }
                        sync_let_copy_cleanup(sc, &mut results);
                        return Err("sync-let body contains unavailable syntax".to_string());
                    }
                    if sync_is_node(value)
                        || s7::s7_is_null(sc, value)
                        || s7::s7_is_boolean(value)
                        || s7::s7_is_number(value)
                        || s7::s7_is_character(value)
                        || s7::s7_is_symbol(value)
                        || s7::s7_is_keyword(value)
                    {
                        results.push(sync_let_protect(sc, value));
                        continue;
                    }
                    if s7::s7_is_string(value) {
                        results.push(sync_let_protect(
                            sc,
                            s7::s7_make_string_with_length(
                                sc,
                                s7::s7_string(value),
                                s7::s7_string_length(value),
                            ),
                        ));
                        continue;
                    }
                    if s7::s7_is_byte_vector(value) {
                        let length = s7::s7_vector_length(value);
                        let copy = sync_let_protect(
                            sc,
                            s7::s7_make_byte_vector(sc, length, 1, std::ptr::null_mut()),
                        );
                        for index in 0..length {
                            s7::s7_byte_vector_set(
                                copy.value,
                                index,
                                s7::s7_byte_vector_ref(value, index),
                            );
                        }
                        results.push(copy);
                        continue;
                    }
                    if s7::s7_is_vector(value) {
                        let address = value as usize;
                        if !visiting.insert(address) {
                            sync_let_copy_cleanup(sc, &mut results);
                            return Err("sync-let does not accept cyclic vectors".to_string());
                        }
                        let length = s7::s7_vector_length(value);
                        tasks.push(SyncLetCopyTask::FinishVector { address, length });
                        for index in (0..length).rev() {
                            tasks.push(SyncLetCopyTask::Visit(s7::s7_vector_ref(sc, value, index)));
                        }
                        continue;
                    }
                    if s7::s7_is_pair(value) {
                        let mut source = value;
                        let mut addresses = Vec::new();
                        let mut items = Vec::new();
                        while s7::s7_is_pair(source) {
                            let address = source as usize;
                            if !visiting.insert(address) {
                                sync_let_copy_cleanup(sc, &mut results);
                                return Err("sync-let does not accept cyclic lists".to_string());
                            }
                            addresses.push(address);
                            items.push(s7::s7_car(source));
                            source = s7::s7_cdr(source);
                        }
                        if !s7::s7_is_null(sc, source) {
                            sync_let_copy_cleanup(sc, &mut results);
                            return Err("sync-let accepts only proper lists".to_string());
                        }
                        let length = items.len();
                        tasks.push(SyncLetCopyTask::FinishList { addresses, length });
                        for item in items.into_iter().rev() {
                            tasks.push(SyncLetCopyTask::Visit(item));
                        }
                        continue;
                    }
                    sync_let_copy_cleanup(sc, &mut results);
                    return Err("sync-let values must be inert data or sync nodes".to_string());
                }
                SyncLetCopyTask::FinishVector { address, length } => {
                    let copy = sync_let_protect(sc, s7::s7_make_vector(sc, length));
                    for index in (0..length).rev() {
                        let item = results.pop().expect("sync-let vector copy result missing");
                        s7::s7_vector_set(sc, copy.value, index, item.value);
                        sync_let_unprotect(sc, item);
                    }
                    visiting.remove(&address);
                    results.push(copy);
                }
                SyncLetCopyTask::FinishList { addresses, length } => {
                    let mut copy: Option<SyncLetProtected> = None;
                    for _ in 0..length {
                        let item = results.pop().expect("sync-let list copy result missing");
                        let cell = sync_let_protect(
                            sc,
                            s7::s7_cons(
                                sc,
                                item.value,
                                copy.as_ref().map_or(s7::s7_nil(sc), |value| value.value),
                            ),
                        );
                        sync_let_unprotect(sc, item);
                        if let Some(previous) = copy {
                            sync_let_unprotect(sc, previous);
                        }
                        copy = Some(cell);
                    }
                    for address in addresses {
                        visiting.remove(&address);
                    }
                    results.push(copy.unwrap_or_else(|| sync_let_protect(sc, s7::s7_nil(sc))));
                }
            }
        }
        if results.len() != 1 {
            sync_let_copy_cleanup(sc, &mut results);
            return Err("sync-let boundary copy produced an invalid result".to_string());
        }
        Ok(results.pop().expect("sync-let copy result missing"))
    }
}

unsafe fn sync_let_failure(sc: *mut s7::s7_scheme, message: &str) -> s7::s7_pointer {
    unsafe {
        let message = CString::new(message).unwrap_or_else(|_| {
            CString::new("sync-let boundary error").expect("static string contains no null")
        });
        let message = sync_let_protect(sc, s7::s7_make_string(sc, message.as_ptr()));
        let info = sync_let_protect(sc, s7::s7_list(sc, 1, message.value));
        let error_args = sync_let_protect(
            sc,
            s7::s7_list(
                sc,
                2,
                s7::s7_make_symbol(sc, c"sync-web-error".as_ptr()),
                info.value,
            ),
        );
        let boundary = sync_let_protect(
            sc,
            s7::s7_list(
                sc,
                2,
                s7::s7_make_symbol(sc, c"%sync-let-error".as_ptr()),
                error_args.value,
            ),
        );
        let result = boundary.value;
        sync_let_unprotect(sc, boundary);
        sync_let_unprotect(sc, error_args);
        sync_let_unprotect(sc, info);
        sync_let_unprotect(sc, message);
        result
    }
}

pub(crate) fn primitive_s7_sync_let() -> Primitive {
    unsafe extern "C" fn code(sc: *mut s7::s7_scheme, args: s7::s7_pointer) -> s7::s7_pointer {
        unsafe {
            let names = s7::s7_car(args);
            let values = s7::s7_cadr(args);
            let body = s7::s7_caddr(args);
            if !s7::s7_is_proper_list(sc, names)
                || !s7::s7_is_proper_list(sc, values)
                || s7::s7_list_length(sc, names) != s7::s7_list_length(sc, values)
            {
                return sync_let_failure(
                    sc,
                    "sync-let binding names and values must be equal lists",
                );
            }

            let environment = sync_let_env(sc);
            let mut names_cursor = names;
            let mut values_cursor = values;
            let mut seen = HashSet::new();
            while !s7::s7_is_null(sc, names_cursor) {
                let name = s7::s7_car(names_cursor);
                if !s7::s7_is_symbol(name) || !seen.insert(name as usize) {
                    sync_let_unprotect(sc, environment);
                    return sync_let_failure(sc, "sync-let binding names must be distinct symbols");
                }
                if s7::s7_is_syntax(s7::s7_let_ref(sc, s7::s7_rootlet(sc), name)) {
                    sync_let_unprotect(sc, environment);
                    return sync_let_failure(sc, "sync-let binding names cannot shadow syntax");
                }
                let value =
                    match sync_let_copy(sc, s7::s7_car(values_cursor), &mut HashSet::new(), None) {
                        Ok(value) => value,
                        Err(error) => {
                            sync_let_unprotect(sc, environment);
                            return sync_let_failure(sc, error.as_str());
                        }
                    };
                s7::s7_varlet(sc, environment.value, name, value.value);
                sync_let_unprotect(sc, value);
                names_cursor = s7::s7_cdr(names_cursor);
                values_cursor = s7::s7_cdr(values_cursor);
            }

            if !s7::s7_is_byte_vector(body) {
                sync_let_unprotect(sc, environment);
                return sync_let_failure(sc, "sync-let body must be encoded code");
            }
            let bytes = (0..s7::s7_vector_length(body))
                .map(|index| s7::s7_byte_vector_ref(body, index))
                .collect::<Vec<_>>();
            let body = sync_let_protect(
                sc,
                s7::s7_make_string_with_length(
                    sc,
                    bytes.as_ptr() as *const c_char,
                    bytes.len() as s7::s7_int,
                ),
            );
            let wrapper_environment =
                sync_let_protect(sc, s7::s7_sublet(sc, s7::s7_rootlet(sc), s7::s7_nil(sc)));
            let ok = sync_let_protect(sc, s7::s7_make_symbol(sc, c"%sync-let-ok".as_ptr()));
            let error_tag =
                sync_let_protect(sc, s7::s7_make_symbol(sc, c"%sync-let-error".as_ptr()));
            s7::s7_varlet(
                sc,
                wrapper_environment.value,
                s7::s7_make_symbol(sc, c"%sync-let-body".as_ptr()),
                body.value,
            );
            s7::s7_varlet(
                sc,
                wrapper_environment.value,
                s7::s7_make_symbol(sc, c"%sync-let-environment".as_ptr()),
                environment.value,
            );
            s7::s7_varlet(
                sc,
                wrapper_environment.value,
                s7::s7_make_symbol(sc, c"%sync-let-eval".as_ptr()),
                s7::s7_let_ref(
                    sc,
                    s7::s7_rootlet(sc),
                    s7::s7_make_symbol(sc, c"eval-string".as_ptr()),
                ),
            );
            s7::s7_varlet(
                sc,
                wrapper_environment.value,
                s7::s7_make_symbol(sc, c"%sync-let-ok".as_ptr()),
                ok.value,
            );
            s7::s7_varlet(
                sc,
                wrapper_environment.value,
                s7::s7_make_symbol(sc, c"%sync-let-error".as_ptr()),
                error_tag.value,
            );
            s7::s7_varlet(
                sc,
                wrapper_environment.value,
                s7::s7_make_symbol(sc, c"%sync-let-list".as_ptr()),
                s7::s7_let_ref(
                    sc,
                    s7::s7_rootlet(sc),
                    s7::s7_make_symbol(sc, c"list".as_ptr()),
                ),
            );
            let wrapper = CString::new(
                "(catch #t\
                   (lambda ()\
                     (%sync-let-list %sync-let-ok\
                       (%sync-let-eval %sync-let-body %sync-let-environment)))\
                   (lambda args (%sync-let-list %sync-let-error args)))",
            )
            .expect("Failed to construct sync-let wrapper");
            SESSIONS
                .write()
                .expect("Failed to acquire sessions lock")
                .get_mut(&(sc as usize))
                .expect("Session not found for sync-let")
                .sync_let_boundaries
                .push(environment.location);
            let tagged = sync_let_protect(
                sc,
                s7::s7_eval_c_string_with_environment(
                    sc,
                    wrapper.as_ptr(),
                    wrapper_environment.value,
                ),
            );
            SESSIONS
                .write()
                .expect("Failed to acquire sessions lock")
                .get_mut(&(sc as usize))
                .expect("Session not found for sync-let")
                .sync_let_boundaries
                .pop();
            sync_let_unprotect(sc, body);
            sync_let_unprotect(sc, wrapper_environment);
            if s7::s7_is_pair(tagged.value)
                && s7::s7_list_length(sc, tagged.value) == 2
                && s7::s7_car(tagged.value) == error_tag.value
            {
                let error_args = s7::s7_cadr(tagged.value);
                let copied_error = match sync_let_copy(sc, error_args, &mut HashSet::new(), None) {
                    Ok(copied) => copied,
                    Err(_) => {
                        sync_let_unprotect(sc, tagged);
                        sync_let_unprotect(sc, error_tag);
                        sync_let_unprotect(sc, ok);
                        sync_let_unprotect(sc, environment);
                        return sync_let_failure(sc, "sync-let error contained a non-inert value");
                    }
                };
                if !s7::s7_is_proper_list(sc, copied_error.value)
                    || s7::s7_list_length(sc, copied_error.value) != 2
                {
                    sync_let_unprotect(sc, copied_error);
                    sync_let_unprotect(sc, tagged);
                    sync_let_unprotect(sc, error_tag);
                    sync_let_unprotect(sc, ok);
                    sync_let_unprotect(sc, environment);
                    return sync_let_failure(sc, "sync-let error has an invalid boundary shape");
                }
                s7::s7_set_car(s7::s7_cdr(tagged.value), copied_error.value);
                let result = tagged.value;
                sync_let_unprotect(sc, copied_error);
                sync_let_unprotect(sc, tagged);
                sync_let_unprotect(sc, error_tag);
                sync_let_unprotect(sc, ok);
                sync_let_unprotect(sc, environment);
                return result;
            }

            if !s7::s7_is_pair(tagged.value)
                || s7::s7_list_length(sc, tagged.value) != 2
                || s7::s7_car(tagged.value) != ok.value
            {
                sync_let_unprotect(sc, tagged);
                sync_let_unprotect(sc, error_tag);
                sync_let_unprotect(sc, ok);
                sync_let_unprotect(sc, environment);
                return sync_let_failure(
                    sc,
                    "sync-let evaluation returned an invalid boundary result",
                );
            }
            let copied =
                match sync_let_copy(sc, s7::s7_cadr(tagged.value), &mut HashSet::new(), None) {
                    Ok(copied) => copied,
                    Err(error) => {
                        sync_let_unprotect(sc, tagged);
                        sync_let_unprotect(sc, error_tag);
                        sync_let_unprotect(sc, ok);
                        sync_let_unprotect(sc, environment);
                        return sync_let_failure(sc, error.as_str());
                    }
                };
            s7::s7_set_car(s7::s7_cdr(tagged.value), copied.value);
            let result = tagged.value;
            sync_let_unprotect(sc, copied);
            sync_let_unprotect(sc, tagged);
            sync_let_unprotect(sc, error_tag);
            sync_let_unprotect(sc, ok);
            sync_let_unprotect(sc, environment);
            result
        }
    }

    Primitive::new(
        code,
        c"%sync-let",
        c"internal sync-let copied-data sandbox evaluator",
        3,
        0,
        false,
    )
}

pub(crate) fn primitive_s7_sync_let_return() -> Primitive {
    unsafe extern "C" fn code(sc: *mut s7::s7_scheme, args: s7::s7_pointer) -> s7::s7_pointer {
        unsafe {
            let boundary = s7::s7_car(args);
            if !s7::s7_is_proper_list(sc, boundary) || s7::s7_list_length(sc, boundary) != 2 {
                return sync_error(sc, "sync-let returned an invalid internal boundary shape");
            }
            let tag = s7::s7_car(boundary);
            let ok = s7::s7_make_symbol(sc, c"%sync-let-ok".as_ptr());
            let error = s7::s7_make_symbol(sc, c"%sync-let-error".as_ptr());
            if tag == ok {
                return s7::s7_cadr(boundary);
            }
            if tag != error {
                return sync_error(sc, "sync-let returned an invalid internal boundary tag");
            }
            let error_args = s7::s7_cadr(boundary);
            if !s7::s7_is_proper_list(sc, error_args) || s7::s7_list_length(sc, error_args) != 2 {
                return sync_error(sc, "sync-let error has an invalid boundary shape");
            }
            s7::s7_error(sc, s7::s7_car(error_args), s7::s7_cadr(error_args))
        }
    }

    Primitive::new(
        code,
        c"%sync-let-return",
        c"internal sync-let boundary result transfer",
        1,
        0,
        false,
    )
}

unsafe fn active_shared_boundary(sc: *mut s7::s7_scheme) -> Option<s7::s7_pointer> {
    unsafe {
        let location = SESSIONS
            .read()
            .expect("Failed to acquire sessions lock")
            .get(&(sc as usize))
            .and_then(|session| {
                if serialization_trace_active(sc) {
                    session.sync_let_base_env_loc
                } else {
                    session.sync_let_boundaries.last().copied()
                }
            })?;
        Some(s7::s7_gc_protected_at(sc, location))
    }
}

unsafe fn shared_boundary_contains(
    sc: *mut s7::s7_scheme,
    boundary: s7::s7_pointer,
    procedure: s7::s7_pointer,
) -> bool {
    unsafe {
        let mut environment = s7::s7_funclet(sc, procedure);
        while !environment.is_null() && s7::s7_is_let(environment) && environment != boundary {
            environment = s7::s7_outlet(sc, environment);
        }
        environment == boundary
    }
}

pub(crate) fn primitive_s7_sync_safe_setter() -> Primitive {
    unsafe extern "C" fn code(sc: *mut s7::s7_scheme, args: s7::s7_pointer) -> s7::s7_pointer {
        unsafe {
            let target = s7::s7_car(args);
            if !s7::s7_is_procedure(target) {
                return s7::s7_f(sc);
            }
            let Some(boundary) = active_shared_boundary(sc) else {
                return s7::s7_f(sc);
            };
            if !shared_boundary_contains(sc, boundary, target) {
                return s7::s7_f(sc);
            }
            s7::s7_setter(sc, target)
        }
    }

    Primitive::new(
        code,
        c"%sync-safe-setter",
        c"internal getter for shared-computation procedure setters",
        1,
        0,
        false,
    )
}

pub(crate) fn primitive_s7_sync_safe_setter_set() -> Primitive {
    unsafe extern "C" fn code(sc: *mut s7::s7_scheme, args: s7::s7_pointer) -> s7::s7_pointer {
        unsafe {
            let target = s7::s7_car(args);
            let setter = s7::s7_cadr(args);
            if !s7::s7_is_procedure(target) || !s7::s7_is_procedure(setter) {
                return sync_error(sc, "safe setter requires two procedures");
            }
            let Some(boundary) = active_shared_boundary(sc) else {
                return sync_error(sc, "safe setter shared boundary is unavailable");
            };
            if !shared_boundary_contains(sc, boundary, target)
                || !shared_boundary_contains(sc, boundary, setter)
            {
                return sync_error(
                    sc,
                    "safe setter rejects procedures outside the active shared boundary",
                );
            }
            s7::s7_set_setter(sc, target, setter)
        }
    }

    Primitive::new(
        code,
        c"%sync-safe-setter-set!",
        c"internal setter for shared-computation procedure setters",
        2,
        0,
        false,
    )
}

pub(crate) fn primitive_s7_sync_eval() -> Primitive {
    unsafe extern "C" fn code(sc: *mut s7::s7_scheme, args: s7::s7_pointer) -> s7::s7_pointer {
        unsafe {
            let eval_env = s7::s7_gc_protect_via_stack(sc, s7::s7_curlet(sc));
            let expression = s7::s7_gc_protect_via_stack(sc, s7::s7_car(args));
            if !sync_is_node(expression) {
                let value = expression;
                s7::s7_gc_unprotect_via_stack(sc, expression);
                s7::s7_gc_unprotect_via_stack(sc, eval_env);
                return s7::s7_wrong_type_arg_error(
                    sc,
                    c"sync-eval".as_ptr(),
                    1,
                    value,
                    c"a sync-node".as_ptr(),
                );
            }
            const SYNC_EVAL_HEADER_CACHE_LIMIT: usize = 64;
            let root = sync_heap_read(s7::s7_c_object_value(expression));
            let persistor = session_persistor_for(sc);
            let Some(((header_word, _, _), _)) = resolve_branch_with(&persistor, root) else {
                s7::s7_gc_unprotect_via_stack(sc, expression);
                s7::s7_gc_unprotect_via_stack(sc, eval_env);
                return sync_error(sc, "Journal cannot retrieve leaf byte-vector (sync-eval)");
            };
            serialization_trace_child(sc, root, header_word, true);
            let cached = SESSIONS
                .read()
                .expect("Failed to acquire sessions lock")
                .get(&(sc as usize))
                .and_then(|session| session.sync_eval_header_cache.get(&header_word).cloned());
            let mut bytes = if let Some(bytes) = cached {
                bytes
            } else {
                let bytes = match resolve_node_with(&persistor, header_word) {
                    Some(ResolvedNode::Leaf(content, ResolveSource::Global)) => {
                        persistor
                            .leaf_set(content.clone())
                            .expect("Failed to add sync-eval header leaf to session persistor");
                        #[cfg(feature = "wasm-kernel")]
                        persistor.mark_imported(header_word);
                        content
                    }
                    Some(ResolvedNode::Leaf(content, _)) => content,
                    _ => {
                        s7::s7_gc_unprotect_via_stack(sc, expression);
                        s7::s7_gc_unprotect_via_stack(sc, eval_env);
                        return sync_error(
                            sc,
                            "sync-eval first argument should be a sync-node with a byte-vector header",
                        );
                    }
                };
                let mut sessions = SESSIONS.write().expect("Failed to acquire sessions lock");
                let session = sessions
                    .get_mut(&(sc as usize))
                    .expect("Session not found for sync-eval header cache");
                if session.sync_eval_header_cache.len() < SYNC_EVAL_HEADER_CACHE_LIMIT {
                    session
                        .sync_eval_header_cache
                        .insert(header_word, bytes.clone());
                }
                bytes
            };
            drop(persistor);
            bytes.insert(0, 39);
            bytes.push(0);
            let code = match CString::from_vec_with_nul(bytes) {
                Ok(code) => code,
                Err(_) => {
                    s7::s7_gc_unprotect_via_stack(sc, expression);
                    s7::s7_gc_unprotect_via_stack(sc, eval_env);
                    return s7::s7_error(
                        sc,
                        s7::s7_make_symbol(sc, c"encoding-error".as_ptr()),
                        s7::s7_list(
                            sc,
                            1,
                            s7::s7_make_string(sc, c"Byte vector string is malformed".as_ptr()),
                        ),
                    );
                }
            };
            let loader_expression = s7::s7_gc_protect_via_stack(
                sc,
                s7::s7_eval_c_string_with_environment(sc, code.as_ptr(), eval_env),
            );
            let loader =
                s7::s7_gc_protect_via_stack(sc, s7::s7_eval(sc, loader_expression, eval_env));
            let result = s7::s7_gc_protect_via_stack(
                sc,
                s7::s7_apply_function(sc, loader, s7::s7_list(sc, 1, expression)),
            );
            s7::s7_gc_unprotect_via_stack(sc, result);
            s7::s7_gc_unprotect_via_stack(sc, loader);
            s7::s7_gc_unprotect_via_stack(sc, loader_expression);
            s7::s7_gc_unprotect_via_stack(sc, expression);
            s7::s7_gc_unprotect_via_stack(sc, eval_env);
            result
        }
    }

    Primitive::new(
        code,
        c"sync-eval",
        c"(sync-eval node) load a sync-node in the current environment",
        1,
        0,
        false,
    )
}

struct SyncLetMask {
    sc: *mut s7::s7_scheme,
    environment: s7::s7_pointer,
}

#[cfg(target_env = "msvc")]
type S7ForEachSymbolResult = c_uchar;
#[cfg(not(target_env = "msvc"))]
type S7ForEachSymbolResult = bool;

unsafe extern "C" fn sync_let_mask_symbol(
    name: *const c_char,
    data: *mut c_void,
) -> S7ForEachSymbolResult {
    unsafe {
        let mask = &mut *(data as *mut SyncLetMask);
        let symbol = s7::s7_make_symbol(mask.sc, name);
        s7::s7_define(mask.sc, mask.environment, symbol, s7::s7_undefined(mask.sc));
        #[cfg(target_env = "msvc")]
        return 0;
        #[cfg(not(target_env = "msvc"))]
        false
    }
}

#[cfg(target_env = "msvc")]
const _: unsafe extern "C" fn(*const c_char, *mut c_void) -> c_uchar = sync_let_mask_symbol;
#[cfg(not(target_env = "msvc"))]
const _: unsafe extern "C" fn(*const c_char, *mut c_void) -> bool = sync_let_mask_symbol;

unsafe fn sync_let_capability_template(sc: *mut s7::s7_scheme) -> SyncLetProtected {
    unsafe {
        let mask_environment = sync_let_protect(sc, s7::s7_make_closed_let(sc));
        let capabilities = SYNC_LET_ALLOWED
            .iter()
            .map(|name| {
                let name = CString::new(*name).expect("sync-let capability contains a null byte");
                let symbol = sync_let_protect(sc, s7::s7_make_symbol(sc, name.as_ptr()));
                let mut value = s7::s7_let_ref(sc, s7::s7_rootlet(sc), symbol.value);
                if value == s7::s7_undefined(sc) {
                    value = s7::s7_symbol_value(sc, symbol.value);
                }
                (symbol, sync_let_protect(sc, value))
            })
            .collect::<Vec<_>>();
        let mut mask = SyncLetMask {
            sc,
            environment: mask_environment.value,
        };
        s7::s7_for_each_symbol(
            sc,
            Some(sync_let_mask_symbol),
            &mut mask as *mut SyncLetMask as *mut c_void,
        );
        s7::s7_seal_let(sc, mask_environment.value);
        let environment = sync_let_protect(
            sc,
            s7::s7_sublet(sc, mask_environment.value, s7::s7_nil(sc)),
        );
        for (symbol, value) in capabilities {
            s7::s7_define(sc, environment.value, symbol.value, value.value);
            sync_let_unprotect(sc, value);
            sync_let_unprotect(sc, symbol);
        }
        s7::s7_define(
            sc,
            environment.value,
            s7::s7_make_symbol(sc, c"setter".as_ptr()),
            s7::s7_let_ref(
                sc,
                s7::s7_rootlet(sc),
                s7::s7_make_symbol(sc, c"%sync-safe-setter".as_ptr()),
            ),
        );
        s7::s7_seal_let(sc, environment.value);
        sync_let_unprotect(sc, mask_environment);
        environment
    }
}

pub(crate) unsafe fn sync_let_env(sc: *mut s7::s7_scheme) -> SyncLetProtected {
    unsafe {
        let existing = SESSIONS
            .read()
            .expect("Failed to acquire sessions lock")
            .get(&(sc as usize))
            .and_then(|session| session.sync_let_base_env_loc);
        let base = match existing {
            Some(location) => s7::s7_gc_protected_at(sc, location),
            None => {
                let template = sync_let_capability_template(sc);
                SESSIONS
                    .write()
                    .expect("Failed to acquire sessions lock")
                    .get_mut(&(sc as usize))
                    .expect("Session not found for sync-let capability template")
                    .sync_let_base_env_loc = Some(template.location);
                template.value
            }
        };
        sync_let_protect(sc, s7::s7_sublet_with_cloned_bindings(sc, base))
    }
}
