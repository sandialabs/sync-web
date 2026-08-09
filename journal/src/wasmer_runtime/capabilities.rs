use super::*;

pub(super) struct DetachedLease(u64);

impl DetachedLease {
    pub(super) fn acquire(bytes: usize) -> Result<Self, String> {
        if bytes > CAPABILITY_BODY_LIMIT {
            return Err("bounded detached message exceeded".into());
        }
        let bytes = u64::try_from(bytes).map_err(|_| "detached message overflow")?;
        let mut global = GLOBAL_DETACHED_WORK
            .lock()
            .map_err(|_| "detached work admission poisoned")?;
        let queued_bytes = global
            .1
            .checked_add(bytes)
            .ok_or("detached byte overflow")?;
        if global.0 >= DETACHED_WORK_LIMIT || queued_bytes > DETACHED_BYTE_LIMIT {
            return Err("bounded detached work exhausted".into());
        }
        global.0 += 1;
        global.1 = queued_bytes;
        Ok(Self(bytes))
    }
}

impl Drop for DetachedLease {
    fn drop(&mut self) {
        if let Ok(mut global) = GLOBAL_DETACHED_WORK.lock() {
            global.0 = global.0.saturating_sub(1);
            global.1 = global.1.saturating_sub(self.0);
        }
    }
}

pub(super) struct RequestBudget {
    pub(super) calls: u64,
    pub(super) bytes: u64,
    pub(super) deadline: Instant,
    pub(super) operation_memory: Arc<Mutex<u64>>,
}

pub(super) type SharedBudget = Arc<Mutex<RequestBudget>>;

pub(super) fn request_budget(operation_memory: Arc<Mutex<u64>>) -> SharedBudget {
    Arc::new(Mutex::new(RequestBudget {
        calls: 0,
        bytes: 0,
        deadline: Instant::now() + REQUEST_TIMEOUT,
        operation_memory,
    }))
}

pub(super) struct PendingResponse {
    pub(super) operation: i32,
    pub(super) request: Vec<u8>,
    pub(super) response: Vec<u8>,
}

#[derive(Clone, Copy)]
enum ResponseChannel {
    Persist,
    Capability,
}

pub(super) struct State {
    pub(super) memory: Option<Memory>,
    pub(super) budget: SharedBudget,
    pub(super) record: Word,
    pub(super) scenario: Option<ScenarioContext>,
    pub(super) isolated_persistor: Option<MemoryPersistor>,
    pub(super) capability_cache: HashMap<Vec<u8>, Vec<u8>>,
    pub(super) capability_pending: HashMap<Vec<u8>, Vec<u8>>,
    pub(super) persist_response: Option<PendingResponse>,
    pub(super) capability_response: Option<PendingResponse>,
    pub(super) persist_ops: [u64; 16],
    pub(super) capability_ops: [u64; 16],
    pub(super) capability_request_bytes: [u64; 16],
    pub(super) capability_request_max: [u64; 16],
    pub(super) response_capacity: u64,
    pub(super) persist_ns: u64,
    pub(super) capability_ns: u64,
}

fn pending_response(state: &mut State, channel: ResponseChannel) -> &mut Option<PendingResponse> {
    match channel {
        ResponseChannel::Persist => &mut state.persist_response,
        ResponseChannel::Capability => &mut state.capability_response,
    }
}

pub(super) fn take_pending_response(
    pending: &mut Option<PendingResponse>,
    operation: i32,
    request: &[u8],
) -> Option<Vec<u8>> {
    pending
        .take()
        .filter(|pending| pending.operation == operation && pending.request == request)
        .map(|pending| pending.response)
}

pub(super) fn debit_budget(budget: &SharedBudget, bytes: usize, call: bool) -> Result<(), String> {
    let mut budget = budget.lock().map_err(|_| "request budget poisoned")?;
    if call {
        budget.calls = budget.calls.saturating_add(1);
    }
    budget.bytes = budget.bytes.saturating_add(bytes as u64);
    if budget.calls > HOST_CALL_LIMIT
        || budget.bytes > HOST_BYTE_LIMIT
        || Instant::now() > budget.deadline
    {
        return Err("bounded host capability exhausted".into());
    }
    Ok(())
}

fn charge(ctx: &mut FunctionEnvMut<'_, State>, bytes: usize) -> Result<(), RuntimeError> {
    debit_budget(&ctx.data().budget, bytes, true).map_err(RuntimeError::new)
}

fn read(
    ctx: &mut FunctionEnvMut<'_, State>,
    pointer: i32,
    length: i32,
) -> Result<Vec<u8>, RuntimeError> {
    if pointer < 0 || length < 0 {
        return Err(RuntimeError::new("negative kernel range"));
    }
    charge(ctx, length as usize)?;
    let memory = ctx
        .data()
        .memory
        .clone()
        .ok_or_else(|| RuntimeError::new("kernel memory missing"))?;
    let mut bytes = vec![0; length as usize];
    memory
        .view(ctx)
        .read(pointer as u64, &mut bytes)
        .map_err(|error| RuntimeError::new(error.to_string()))?;
    Ok(bytes)
}

fn write(
    ctx: &mut FunctionEnvMut<'_, State>,
    pointer: i32,
    capacity: i32,
    bytes: &[u8],
) -> Result<i32, RuntimeError> {
    if pointer < 0 || capacity < 0 || bytes.len() > i32::MAX as usize {
        return Ok(-1);
    }
    charge(ctx, bytes.len())?;
    if bytes.len() > capacity as usize {
        return Ok(bytes.len() as i32);
    }
    let memory = ctx
        .data()
        .memory
        .clone()
        .ok_or_else(|| RuntimeError::new("kernel memory missing"))?;
    memory
        .view(ctx)
        .write(pointer as u64, bytes)
        .map_err(|error| RuntimeError::new(error.to_string()))?;
    Ok(bytes.len() as i32)
}

fn persist_with(
    persistor: &dyn BoundedPersistor,
    operation: i32,
    request: &[u8],
) -> Result<Vec<u8>, String> {
    let word = |bytes: &[u8]| -> Result<Word, String> {
        bytes
            .try_into()
            .map_err(|_| "invalid host word".to_string())
    };
    match operation {
        1 => Ok(persistor
            .root_list_bounded(CAPABILITY_BODY_LIMIT / SIZE)
            .map_err(|_| "bounded root-list")?
            .into_iter()
            .flatten()
            .collect()),
        2 if request.len() == SIZE * 2 => persistor
            .root_new(word(&request[..SIZE])?, word(&request[SIZE..])?)
            .map(Vec::from)
            .map_err(|_| "root-new".into()),
        3 if request.len() == SIZE => persistor
            .root_get(word(request)?)
            .map(Vec::from)
            .map_err(|_| "root-get".into()),
        4 if request.len() == SIZE => persistor
            .root_delete(word(request)?)
            .map(|_| Vec::new())
            .map_err(|_| "root-delete".into()),
        5 if request.len() == SIZE * 3 => persistor
            .branch_set(
                word(&request[..SIZE])?,
                word(&request[SIZE..SIZE * 2])?,
                word(&request[SIZE * 2..])?,
            )
            .map(Vec::from)
            .map_err(|_| "branch-set".into()),
        6 if request.len() == SIZE => persistor
            .branch_get(word(request)?)
            .map(|(a, b, c)| [a, b, c].into_iter().flatten().collect())
            .map_err(|_| "branch-get".into()),
        7 => persistor
            .leaf_set(request.to_vec())
            .map(Vec::from)
            .map_err(|_| "leaf-set".into()),
        8 if request.len() == SIZE => persistor
            .leaf_get_bounded(word(request)?, DEFAULT_MAX_LEAF_BYTES)
            .map_err(|_| "leaf-get".into()),
        9 if request.len() == SIZE => persistor
            .stump_set(word(request)?)
            .map(Vec::from)
            .map_err(|_| "stump-set".into()),
        10 if request.len() == SIZE => persistor
            .stump_get(word(request)?)
            .map(Vec::from)
            .map_err(|_| "stump-get".into()),
        11 if request.len() == SIZE => {
            let word = word(request)?;
            if let Ok((left, right, digest)) = persistor.branch_get(word) {
                let mut response = vec![1];
                response.extend(left);
                response.extend(right);
                response.extend(digest);
                Ok(response)
            } else if let Ok(content) = persistor.leaf_get_bounded(word, DEFAULT_MAX_LEAF_BYTES) {
                let mut response = Vec::with_capacity(1 + content.len());
                response.push(2);
                response.extend(content);
                Ok(response)
            } else if let Ok(digest) = persistor.stump_get(word) {
                let mut response = vec![3];
                response.extend(digest);
                Ok(response)
            } else {
                Err("node-get".into())
            }
        }
        _ => Err("invalid host persistence operation".into()),
    }
}

pub(super) fn persist(operation: i32, request: &[u8]) -> Result<Vec<u8>, String> {
    persist_with(&**PERSISTOR, operation, request)
}

async fn bounded_response_bytes(mut response: reqwest::Response) -> Result<Vec<u8>, String> {
    if !response.status().is_success() {
        return Err(format!("HTTP status {}", response.status()));
    }
    if response
        .content_length()
        .is_some_and(|length| length > u64::try_from(CAPABILITY_BODY_LIMIT).expect("body limit"))
    {
        return Err("bounded HTTP response exceeded".into());
    }
    let mut body = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|error| error.to_string())? {
        let next = body
            .len()
            .checked_add(chunk.len())
            .ok_or("bounded HTTP response overflow")?;
        if next > CAPABILITY_BODY_LIMIT {
            return Err("bounded HTTP response exceeded".into());
        }
        body.extend_from_slice(&chunk);
    }
    Ok(body)
}

fn materialize_capability(operation: i32, result: &[u8]) -> Result<Vec<u8>, String> {
    if result.len() > CAPABILITY_BODY_LIMIT {
        return Err("bounded capability response exceeded".into());
    }
    let materialized = if operation == 6 {
        let body = std::str::from_utf8(result)
            .map_err(|_| "Journal is unable to query remote peer (sync-remote)")?;
        if body.trim().is_empty() || body.as_bytes().contains(&0) {
            return Err("Journal is unable to query remote peer (sync-remote)".into());
        }
        let escaped = body.replace('\\', "\\\\").replace('"', "\\\"");
        format!(
            "(catch #t (lambda () (let* ((port (open-input-string \"{}\")) (value (read port)) (tail (read port))) (if (and (not (eof-object? value)) (eof-object? tail)) value (error 'sync-web-error \"Journal is unable to query remote peer (sync-remote)\")))) (lambda args (error 'sync-web-error \"Journal is unable to query remote peer (sync-remote)\")))",
            escaped
        ).into_bytes()
    } else {
        format!(
            "#u({})",
            result
                .iter()
                .map(u8::to_string)
                .collect::<Vec<_>>()
                .join(" ")
        )
        .into_bytes()
    };
    if materialized.len() > 16 * 1024 * 1024 {
        return Err("bounded capability response exceeded".into());
    }
    Ok(materialized)
}

pub(super) fn capability(
    operation: i32,
    request: &[u8],
    state: &mut State,
) -> Result<Vec<u8>, String> {
    let source = state.record;
    let scenario = state.scenario.clone();
    let quoted = |bytes: Vec<u8>| -> Result<Vec<u8>, String> {
        if bytes.len() > CAPABILITY_BODY_LIMIT {
            return Err("bounded capability response exceeded".into());
        }
        let text = String::from_utf8(bytes).map_err(|_| "capability response encoding")?;
        let quoted = format!("(quote {text})").into_bytes();
        if quoted.len() > 16 * 1024 * 1024 {
            return Err("bounded capability response exceeded".into());
        }
        Ok(quoted)
    };
    let unquote = |text: &str| -> Result<String, String> {
        text.strip_prefix('"')
            .and_then(|text| text.strip_suffix('"'))
            .map(str::to_string)
            .ok_or_else(|| "expected quoted string".to_string())
    };
    match operation {
        1 if request.len() == SIZE => {
            let record: Word = request.try_into().unwrap();
            let leaf = PERSISTOR
                .leaf_set(crate::GENESIS_STR.as_bytes().to_vec())
                .map_err(|_| "create leaf")?;
            let root = PERSISTOR
                .branch_set(leaf, crate::NULL, crate::NULL)
                .map_err(|_| "create branch")?;
            PERSISTOR
                .root_new(record, root)
                .map(|_| b"#t".to_vec())
                .map_err(|_| "record ID is already in use".into())
        }
        2 if request.len() == SIZE => {
            let record: Word = request.try_into().unwrap();
            if record == crate::NULL {
                return Err("cannot delete the root record".into());
            }
            PERSISTOR
                .root_delete(record)
                .map(|_| b"#t".to_vec())
                .map_err(|_| "record ID does not exist".into())
        }
        3 if request.is_empty() => {
            let mut response = String::from("(quote (");
            for word in root_list_bounded(CAPABILITY_BODY_LIMIT / SIZE)
                .map_err(|_| "bounded capability response exceeded")?
            {
                if response.len() > 7 {
                    response.push(' ');
                }
                response.push_str("#u(");
                for (index, byte) in word.iter().enumerate() {
                    if index > 0 {
                        response.push(' ');
                    }
                    response.push_str(&byte.to_string());
                }
                response.push(')');
                if response.len() > CAPABILITY_BODY_LIMIT {
                    return Err("bounded capability response exceeded".into());
                }
            }
            response.push_str("))");
            Ok(response.into_bytes())
        }
        4 if request.len() >= 1 + SIZE => {
            let blocking = request[0] != 0;
            let record: Word = request[1..1 + SIZE].try_into().unwrap();
            PERSISTOR
                .root_get(record)
                .map_err(|_| "record ID does not exist")?;
            if blocking {
                let message =
                    String::from_utf8(request[1 + SIZE..].to_vec()).map_err(|_| "call encoding")?;
                quoted(
                    evaluate_record_with_budget(record, &message, scenario, state.budget.clone())
                        .into_bytes(),
                )
            } else {
                let message =
                    String::from_utf8(request[1 + SIZE..].to_vec()).map_err(|_| "call encoding")?;
                let operation_memory = state
                    .budget
                    .lock()
                    .map_err(|_| "request budget poisoned")?
                    .operation_memory
                    .clone();
                let detached_budget = request_budget(operation_memory);
                if scenario.is_some() {
                    evaluate_record_with_budget(record, &message, scenario, detached_budget);
                } else if let Ok(handle) = tokio::runtime::Handle::try_current() {
                    let lease = DetachedLease::acquire(message.len())?;
                    handle.spawn(async move {
                        let _lease = lease;
                        evaluate_record_with_budget(record, &message, None, detached_budget);
                    });
                } else {
                    let lease = DetachedLease::acquire(message.len())?;
                    std::thread::Builder::new()
                        .name("sync-web-detached-evaluation".into())
                        .spawn(move || {
                            let _lease = lease;
                            evaluate_record_with_budget(record, &message, None, detached_budget);
                        })
                        .map_err(|_| "bounded detached work spawn failed")?;
                }
                Ok(b"#t".to_vec())
            }
        }
        5 | 6 if request.len() >= 8 => {
            let mut key = vec![operation as u8];
            key.extend(request);
            if let Some(result) = state.capability_cache.get(&key) {
                return materialize_capability(operation, result);
            }
            if let Some(result) = state.capability_pending.get(&key) {
                return materialize_capability(operation, result);
            }
            let first_len = u32::from_be_bytes(request[..4].try_into().unwrap()) as usize;
            let second_len = u32::from_be_bytes(request[4..8].try_into().unwrap()) as usize;
            let first_end = 8usize
                .checked_add(first_len)
                .ok_or("capability request length")?;
            let second_end = first_end
                .checked_add(second_len)
                .ok_or("capability request length")?;
            if second_end > request.len() {
                return Err("capability request length".into());
            }
            let first =
                std::str::from_utf8(&request[8..first_end]).map_err(|_| "capability encoding")?;
            let second = std::str::from_utf8(&request[first_end..second_end])
                .map_err(|_| "capability encoding")?;
            let rest =
                std::str::from_utf8(&request[second_end..]).map_err(|_| "capability encoding")?;
            let (url, body, method) = if operation == 6 {
                (unquote(first)?, second.to_string(), "post".to_string())
            } else {
                if scenario.is_some() {
                    return Err("sync-http is not supported by the scenario harness".into());
                }
                (
                    unquote(second)?,
                    rest.strip_prefix('"')
                        .and_then(|text| text.strip_suffix('"'))
                        .unwrap_or(rest)
                        .to_string(),
                    first.to_lowercase(),
                )
            };
            let result = if let Some(scenario) = scenario {
                scenario
                    .transport
                    .remote(scenario.action, source, url, body)
            } else {
                let timeout = state
                    .budget
                    .lock()
                    .map_err(|_| "request budget poisoned")?
                    .deadline
                    .saturating_duration_since(Instant::now());
                if timeout.is_zero() {
                    return Err("bounded host capability exhausted".into());
                }
                let client = crate::JOURNAL.client.clone();
                tokio::task::block_in_place(|| {
                    tokio::runtime::Handle::current().block_on(async move {
                        tokio::time::timeout(timeout, async move {
                            match method.as_str() {
                                "get" => {
                                    bounded_response_bytes(
                                        client
                                            .get(&url)
                                            .send()
                                            .await
                                            .map_err(|error| error.to_string())?,
                                    )
                                    .await
                                }
                                "post" => {
                                    bounded_response_bytes(
                                        client
                                            .post(&url)
                                            .body(body)
                                            .send()
                                            .await
                                            .map_err(|error| error.to_string())?,
                                    )
                                    .await
                                }
                                _ => Err("unsupported HTTP method".into()),
                            }
                        })
                        .await
                        .map_err(|_| "bounded host capability exhausted".to_string())?
                    })
                })
            }
            .map_err(|_| {
                if operation == 6 {
                    "Journal is unable to query remote peer (sync-remote)"
                } else {
                    "Journal is unable to fulfill HTTP request (sync-http)"
                }
                .to_string()
            })?;
            let materialized = materialize_capability(operation, &result)?;
            if operation == 6 {
                state.capability_pending.insert(key, result);
            } else {
                state.capability_cache.insert(key, result);
            }
            Ok(materialized)
        }
        7 => {
            let mut key = vec![6];
            key.extend(request);
            if let Some(result) = state.capability_pending.remove(&key) {
                state.capability_cache.insert(key, result);
            }
            Ok(b"#t".to_vec())
        }
        _ => Err("invalid native capability request".into()),
    }
}

pub(super) fn response_needs_retry(response: &[u8], capacity: i32) -> bool {
    capacity >= 0 && response.len() > capacity as usize
}

fn write_host_response(
    ctx: &mut FunctionEnvMut<'_, State>,
    channel: ResponseChannel,
    operation: i32,
    request: Vec<u8>,
    response: Vec<u8>,
    output: i32,
    capacity: i32,
) -> Result<i32, RuntimeError> {
    let needs_retry = response_needs_retry(&response, capacity);
    let result = write(ctx, output, capacity, &response);
    if needs_retry && result.as_ref().is_ok_and(|length| *length > capacity) {
        *pending_response(ctx.data_mut(), channel) = Some(PendingResponse {
            operation,
            request,
            response,
        });
    }
    result
}

fn host_persist(
    mut ctx: FunctionEnvMut<'_, State>,
    operation: i32,
    pointer: i32,
    length: i32,
    output: i32,
    capacity: i32,
) -> Result<i32, RuntimeError> {
    if (0..16).contains(&operation) {
        ctx.data_mut().persist_ops[operation as usize] += 1;
    }
    ctx.data_mut().response_capacity = ctx
        .data()
        .response_capacity
        .saturating_add(capacity.max(0) as u64);
    let started = Instant::now();
    if length < 0 {
        return Ok(-1);
    }
    let request_limit = if operation == 7 {
        DEFAULT_MAX_LEAF_BYTES
    } else {
        CAPABILITY_BODY_LIMIT
    };
    if length as usize > request_limit {
        charge(&mut ctx, length as usize)?;
        return Ok(-1);
    }
    let request = read(&mut ctx, pointer, length)?;
    let pending = take_pending_response(
        pending_response(ctx.data_mut(), ResponseChannel::Persist),
        operation,
        &request,
    );
    let response = pending.map(Ok).unwrap_or_else(|| {
        if let Some(persistor) = &ctx.data().isolated_persistor {
            persist_with(persistor, operation, &request)
        } else {
            persist(operation, &request)
        }
    });
    let result = match response {
        Ok(response) => write_host_response(
            &mut ctx,
            ResponseChannel::Persist,
            operation,
            request,
            response,
            output,
            capacity,
        ),
        Err(_) => Ok(-1),
    };
    ctx.data_mut().persist_ns = ctx
        .data()
        .persist_ns
        .saturating_add(started.elapsed().as_nanos() as u64);
    result
}

fn host_capability(
    mut ctx: FunctionEnvMut<'_, State>,
    operation: i32,
    pointer: i32,
    length: i32,
    output: i32,
    capacity: i32,
) -> Result<i32, RuntimeError> {
    if (0..16).contains(&operation) {
        let index = operation as usize;
        let bytes = length.max(0) as u64;
        ctx.data_mut().capability_ops[index] += 1;
        ctx.data_mut().capability_request_bytes[index] =
            ctx.data().capability_request_bytes[index].saturating_add(bytes);
        ctx.data_mut().capability_request_max[index] =
            ctx.data().capability_request_max[index].max(bytes);
    }
    ctx.data_mut().response_capacity = ctx
        .data()
        .response_capacity
        .saturating_add(capacity.max(0) as u64);
    let started = Instant::now();
    if length < 0 {
        return Ok(-1);
    }
    if length as usize > CAPABILITY_BODY_LIMIT {
        charge(&mut ctx, length as usize)?;
        let response = b"(error 'sync-web-error \"bounded capability request exceeded\")";
        return write(&mut ctx, output, capacity, response);
    }
    let request = read(&mut ctx, pointer, length)?;
    let response = take_pending_response(
        pending_response(ctx.data_mut(), ResponseChannel::Capability),
        operation,
        &request,
    )
    .unwrap_or_else(|| match capability(operation, &request, ctx.data_mut()) {
        Ok(response) => response,
        Err(error) => {
            format!("(error 'sync-web-error \"{}\")", error.replace('"', "'")).into_bytes()
        }
    });
    let result = write_host_response(
        &mut ctx,
        ResponseChannel::Capability,
        operation,
        request,
        response,
        output,
        capacity,
    );
    ctx.data_mut().capability_ns = ctx
        .data()
        .capability_ns
        .saturating_add(started.elapsed().as_nanos() as u64);
    result
}

fn random_get(
    mut ctx: FunctionEnvMut<'_, State>,
    pointer: i32,
    length: i32,
) -> Result<i32, RuntimeError> {
    if length < 0 || length as usize > 16 * 1024 * 1024 {
        return Ok(28);
    }
    let random = if let Some(scenario) = ctx.data().scenario.as_ref() {
        scenario.random_bytes(length as usize)
    } else {
        let mut random = vec![0; length as usize];
        rand::rngs::OsRng.fill_bytes(&mut random);
        random
    };
    write(&mut ctx, pointer, length, &random).map(|result| if result < 0 { 21 } else { 0 })
}

fn clock_time_get(
    mut ctx: FunctionEnvMut<'_, State>,
    _clock: i32,
    _precision: i64,
    pointer: i32,
) -> Result<i32, RuntimeError> {
    let nanos = if let Some(scenario) = ctx.data().scenario.as_ref() {
        scenario.unix_time().saturating_mul(1_000_000_000) as u64
    } else {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| RuntimeError::new("clock"))?
            .as_nanos() as u64
    };
    write(&mut ctx, pointer, 8, &nanos.to_le_bytes()).map(|result| if result < 0 { 21 } else { 0 })
}

pub(super) fn imports(
    store: &mut Store,
    env: &FunctionEnv<State>,
    module: &Module,
) -> Result<Imports, String> {
    let mut imports = Imports::new();
    imports.define(
        "env",
        "sync_web_host_persist",
        Function::new_typed_with_env(store, env, host_persist),
    );
    imports.define(
        "env",
        "sync_web_host_capability",
        Function::new_typed_with_env(store, env, host_capability),
    );
    imports.define(
        "wasi_snapshot_preview1",
        "random_get",
        Function::new_typed_with_env(store, env, random_get),
    );
    imports.define(
        "wasi_snapshot_preview1",
        "clock_time_get",
        Function::new_typed_with_env(store, env, clock_time_get),
    );
    for import in module.imports() {
        if import.module() != "wasi_snapshot_preview1"
            || matches!(import.name(), "random_get" | "clock_time_get")
        {
            continue;
        }
        let ty = import
            .ty()
            .func()
            .ok_or("non-function WASI import")?
            .clone();
        let has_result = !ty.results().is_empty();
        let denied = Function::new_with_env(store, env, ty, move |_ctx, _params| {
            if has_result {
                Ok(vec![Value::I32(76)])
            } else {
                Err(RuntimeError::new("denied WASI capability"))
            }
        });
        imports.define(import.module(), import.name(), denied);
    }
    Ok(imports)
}
