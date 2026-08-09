use crate::persistor::{
    BoundedPersistor, DEFAULT_MAX_LEAF_BYTES, MemoryPersistor, PERSISTOR, Persistor,
    leaf_get_bounded, root_list_bounded,
};
use crate::{GENESIS_STR, Journal, NULL, SIZE, ScenarioContext, Word};
use once_cell::sync::Lazy;
use rand::RngCore;
use sha2::{Digest, Sha256};
use std::cell::RefCell;
use std::collections::HashMap;
use std::io::Read;
use std::ptr::NonNull;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use wasmer::sys::{BaseTunables, EngineBuilder, NativeEngineExt, Tunables};
use wasmer::{
    Engine, ExternType, Function, FunctionEnv, FunctionEnvMut, Imports, Instance, Interrupter,
    Memory, MemoryError, MemoryStyle, MemoryType, Module, Pages, RuntimeError, Store, TableStyle,
    TableType, TypedFunction, Value, WASM_PAGE_SIZE,
};
use wasmer_vm::{LinearMemory, VMConfig, VMMemory, VMMemoryDefinition, VMTable, VMTableDefinition};

const EVALUATOR_NAME: &str = "wasmer-nofuel";
const MEMORY_MAXIMUM: Pages = Pages(8192);
const MEMORY_MAXIMUM_BYTES: u64 = 512 * 1024 * 1024;
const AOT_ARTIFACT_LIMIT: u64 = 64 * 1024 * 1024;
const REQUEST_TIMEOUT: Duration = Duration::from_secs(25);
const HOST_CALL_LIMIT: u64 = 10_000_000;
const HOST_BYTE_LIMIT: u64 = 1024 * 1024 * 1024;
const OPERATION_GUEST_MEMORY_LIMIT: u64 = MEMORY_MAXIMUM_BYTES;
const DETACHED_WORK_LIMIT: u64 = 64;
const DETACHED_BYTE_LIMIT: u64 = 64 * 1024 * 1024;
const CAPABILITY_BODY_LIMIT: usize = 4 * 1024 * 1024 - 4096;
const KERNEL_REQUEST_LIMIT: usize = 16 * 1024 * 1024;
const KERNEL_RESPONSE_LIMIT: usize = 256 * 1024 * 1024;

mod artifact;
mod capabilities;
mod memory;
#[cfg(test)]
mod tests;

use artifact::*;
use capabilities::*;
use memory::*;

static EVALUATIONS: AtomicU64 = AtomicU64::new(0);
#[cfg(test)]
static TEST_EVALUATIONS: Lazy<Mutex<HashMap<Word, u64>>> = Lazy::new(|| Mutex::new(HashMap::new()));
#[cfg(test)]
static TEST_MEMORY_GROWS: AtomicU64 = AtomicU64::new(0);
#[cfg(test)]
static TEST_MEMORY_GROW_ROLLBACK_BYTES: AtomicU64 = AtomicU64::new(0);
#[cfg(test)]
static TEST_SNAPSHOT_BARRIERS: Lazy<Mutex<HashMap<Word, Arc<std::sync::Barrier>>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));
static INTERRUPTS: Lazy<Mutex<Vec<(Instant, Interrupter, Arc<AtomicBool>)>>> =
    Lazy::new(|| Mutex::new(Vec::new()));
static GLOBAL_DETACHED_WORK: Lazy<Mutex<(u64, u64)>> = Lazy::new(|| Mutex::new((0, 0)));
static INTERRUPT_WATCHER: Lazy<()> = Lazy::new(|| {
    std::thread::spawn(|| {
        loop {
            std::thread::sleep(Duration::from_millis(10));
            let now = Instant::now();
            let mut pending = INTERRUPTS.lock().expect("interrupt queue");
            pending.retain(|(deadline, interrupter, complete)| {
                if complete.load(Ordering::Acquire) {
                    return false;
                }
                if *deadline <= now {
                    interrupter.interrupt();
                    return false;
                }
                true
            });
        }
    });
});
fn decode_graph(bytes: &[u8]) -> Result<MemoryPersistor, String> {
    let graph = MemoryPersistor::new();
    let mut offset = 0;
    while offset < bytes.len() {
        let kind = bytes[offset];
        offset += 1;
        let take_word = |offset: &mut usize| -> Result<Word, String> {
            let end = offset.checked_add(SIZE).ok_or("graph overflow")?;
            let word = bytes
                .get(*offset..end)
                .ok_or("truncated graph")?
                .try_into()
                .map_err(|_| "word")?;
            *offset = end;
            Ok(word)
        };
        let expected = take_word(&mut offset)?;
        let actual = match kind {
            1 => graph.branch_set(
                take_word(&mut offset)?,
                take_word(&mut offset)?,
                take_word(&mut offset)?,
            ),
            2 => {
                let end = offset.checked_add(4).ok_or("graph overflow")?;
                let length = u32::from_be_bytes(
                    bytes
                        .get(offset..end)
                        .ok_or("leaf length")?
                        .try_into()
                        .unwrap(),
                ) as usize;
                offset = end;
                let end = offset.checked_add(length).ok_or("leaf overflow")?;
                let content = bytes.get(offset..end).ok_or("truncated leaf")?.to_vec();
                offset = end;
                graph.leaf_set(content)
            }
            3 => graph.stump_set(take_word(&mut offset)?),
            _ => return Err("invalid graph node kind".into()),
        }
        .map_err(|_| "invalid graph node")?;
        if actual != expected {
            return Err("guest graph word mismatch".into());
        }
    }
    Ok(graph)
}

fn run_once(
    compiled: &Compiled,
    record: Word,
    state: Word,
    genesis: &[u8],
    query: &str,
    scenario: Option<ScenarioContext>,
    budget: SharedBudget,
) -> Result<(String, Word, bool, MemoryPersistor), String> {
    run_once_with_persistor(compiled, record, state, genesis, query, scenario, budget, None)
}

fn run_once_with_persistor(
    compiled: &Compiled,
    record: Word,
    state: Word,
    genesis: &[u8],
    query: &str,
    scenario: Option<ScenarioContext>,
    budget: SharedBudget,
    isolated_persistor: Option<MemoryPersistor>,
) -> Result<(String, Word, bool, MemoryPersistor), String> {
    let reservation = MemoryReservation::enter(budget.clone(), compiled.memory_minimum_bytes)?;
    let mut store = Store::new(compiled.engine.clone());
    let env = FunctionEnv::new(
        &mut store,
        State {
            memory: None,
            budget: budget.clone(),
            record,
            scenario,
            isolated_persistor,
            capability_cache: HashMap::new(),
            capability_pending: HashMap::new(),
            persist_response: None,
            capability_response: None,
            persist_ops: [0; 16],
            capability_ops: [0; 16],
            capability_request_bytes: [0; 16],
            capability_request_max: [0; 16],
            response_capacity: 0,
            persist_ns: 0,
            capability_ns: 0,
        },
    );
    let run_started = Instant::now();
    let instantiate_started = Instant::now();
    let import_object = imports(&mut store, &env, &compiled.module)?;
    let reservation_context = ReservationContext::enter(reservation.clone());
    let instance = Instance::new(&mut store, &compiled.module, &import_object)
        .map_err(|error| error.to_string())?;
    drop(reservation_context);
    let instantiate_ns = instantiate_started.elapsed().as_nanos();
    let memory = instance
        .exports
        .get_memory("memory")
        .map_err(|error| error.to_string())?
        .clone();
    if u64::try_from(memory.view(&store).data_size()).map_err(|_| "guest memory size overflow")?
        != compiled.memory_minimum_bytes
    {
        return Err("guest memory minimum mismatch".into());
    }
    env.as_mut(&mut store).memory = Some(memory.clone());
    if let Ok(initialize) = instance
        .exports
        .get_typed_function::<(), ()>(&store, "_initialize")
    {
        initialize
            .call(&mut store)
            .map_err(|error| error.to_string())?;
    }
    let alloc: TypedFunction<i32, i32> = instance
        .exports
        .get_typed_function(&store, "kernel_alloc")
        .map_err(|error| error.to_string())?;
    let evaluate: TypedFunction<(i32, i32), i32> = instance
        .exports
        .get_typed_function(&store, "kernel_evaluate")
        .map_err(|error| error.to_string())?;
    let result_pointer: TypedFunction<(), i32> = instance
        .exports
        .get_typed_function(&store, "kernel_result_pointer")
        .map_err(|error| error.to_string())?;
    let result_length: TypedFunction<(), i32> = instance
        .exports
        .get_typed_function(&store, "kernel_result_length")
        .map_err(|error| error.to_string())?;
    let genesis_len = u32::try_from(genesis.len()).map_err(|_| "genesis too large")?;
    let query_len = u32::try_from(query.len()).map_err(|_| "query too large")?;
    let request_capacity = (SIZE * 2 + 4 + 4)
        .checked_add(genesis.len())
        .and_then(|length| length.checked_add(query.len()))
        .ok_or("kernel request too large")?;
    if request_capacity > KERNEL_REQUEST_LIMIT {
        return Err("kernel request exceeded".into());
    }
    debit_budget(&budget, request_capacity, false)?;
    let mut request = Vec::with_capacity(request_capacity);
    request.extend(record);
    request.extend(state);
    request.extend(genesis_len.to_be_bytes());
    request.extend(query_len.to_be_bytes());
    request.extend(genesis);
    request.extend(query.as_bytes());
    debug_assert_eq!(request.len(), request_capacity);
    let request_len = i32::try_from(request.len()).map_err(|_| "kernel request too large")?;
    let pointer = alloc
        .call(&mut store, request_len)
        .map_err(|error| error.to_string())?;
    if pointer < 0 {
        return Err("kernel allocation failed".into());
    }
    memory
        .view(&store)
        .write(pointer as u64, &request)
        .map_err(|error| error.to_string())?;
    let evaluate_started = Instant::now();
    let complete = Arc::new(AtomicBool::new(false));
    let deadline = budget
        .lock()
        .map_err(|_| "request budget poisoned")?
        .deadline;
    INTERRUPTS.lock().expect("interrupt queue").push((
        deadline,
        store.interrupter(),
        complete.clone(),
    ));
    let evaluated = evaluate.call(&mut store, pointer, request_len);
    complete.store(true, Ordering::Release);
    let status = evaluated.map_err(|error| error.to_string())?;
    let evaluate_ns = evaluate_started.elapsed().as_nanos();
    if status != 0 {
        return Err(format!("kernel status {status}"));
    }
    let pointer = result_pointer
        .call(&mut store)
        .map_err(|error| error.to_string())?;
    let length = result_length
        .call(&mut store)
        .map_err(|error| error.to_string())?;
    if pointer < 0 || length < 0 || length as usize > KERNEL_RESPONSE_LIMIT {
        return Err("kernel response range".into());
    }
    debit_budget(&budget, length as usize, false)?;
    let mut response = vec![0; length as usize];
    memory
        .view(&store)
        .read(pointer as u64, &mut response)
        .map_err(|error| error.to_string())?;
    if response.len() < 4 {
        return Err(String::from_utf8_lossy(&response).into_owned());
    }
    let output_len = u32::from_be_bytes(response[..4].try_into().unwrap()) as usize;
    let state_offset = 4usize
        .checked_add(output_len)
        .ok_or("kernel response overflow")?;
    let state_end = state_offset
        .checked_add(SIZE)
        .ok_or("kernel response overflow")?;
    let graph_len_offset = state_end.checked_add(1).ok_or("kernel response overflow")?;
    let graph_start = graph_len_offset
        .checked_add(4)
        .ok_or("kernel response overflow")?;
    if response.len() < graph_start {
        return Err("truncated kernel response".into());
    }
    let output =
        String::from_utf8(response[4..state_offset].to_vec()).map_err(|_| "kernel output utf8")?;
    let state_new = response[state_offset..state_end].try_into().unwrap();
    let external = response[state_end] != 0;
    let graph_len =
        u32::from_be_bytes(response[graph_len_offset..graph_start].try_into().unwrap()) as usize;
    if graph_start.checked_add(graph_len) != Some(response.len()) {
        return Err("graph response length".into());
    }
    if std::env::var_os("SYNC_WEB_WASM_PROFILE").is_some() {
        let (calls, bytes) = {
            let budget = budget.lock().map_err(|_| "request budget poisoned")?;
            (budget.calls, budget.bytes)
        };
        eprintln!(
            "WASM-PROFILE calls={} bytes={} capacity={} persist={:?} capability={:?} capability-request-bytes={:?} capability-request-max={:?} response={} graph={} memory={} persist-ns={} capability-ns={} instantiate-ns={} evaluate-ns={} run-ns={}",
            calls,
            bytes,
            env.as_ref(&store).response_capacity,
            env.as_ref(&store).persist_ops,
            env.as_ref(&store).capability_ops,
            env.as_ref(&store).capability_request_bytes,
            env.as_ref(&store).capability_request_max,
            response.len(),
            graph_len,
            memory.view(&store).data_size(),
            env.as_ref(&store).persist_ns,
            env.as_ref(&store).capability_ns,
            instantiate_ns,
            evaluate_ns,
            run_started.elapsed().as_nanos(),
        );
    }
    Ok((
        output,
        state_new,
        external,
        decode_graph(&response[graph_start..])?,
    ))
}

struct TempRoot(Word);
impl Drop for TempRoot {
    fn drop(&mut self) {
        let _ = PERSISTOR.root_delete(self.0);
    }
}

#[cfg(target_env = "gnu")]
struct TrimAllocator;
#[cfg(target_env = "gnu")]
impl Drop for TrimAllocator {
    fn drop(&mut self) {
        unsafe extern "C" {
            fn malloc_trim(pad: usize) -> i32;
        }
        unsafe {
            malloc_trim(0);
        }
    }
}

fn attest(compiled: &Compiled) {
    if EVALUATIONS.fetch_add(1, Ordering::Relaxed) == 0 {
        eprintln!(
            "wasmer-runtime-attestation evaluator={} artifact-sha256={} target={}-{} evaluations=1",
            EVALUATOR_NAME,
            compiled.artifact_sha256,
            std::env::consts::ARCH,
            std::env::consts::OS,
        );
    }
}

pub(crate) fn health_check() -> Result<(), String> {
    let compiled = compiled()?;
    let genesis = GENESIS_STR.as_bytes();
    let health = MemoryPersistor::isolated(DEFAULT_MAX_LEAF_BYTES);
    let state = health
        .branch_set(
            health
                .leaf_set(genesis.to_vec())
                .map_err(|_| "health genesis leaf")?,
            NULL,
            NULL,
        )
        .map_err(|_| "health genesis branch")?;
    let (output, state_new, external, _) = run_once_with_persistor(
        &compiled,
        NULL,
        state,
        genesis,
        "#t",
        None,
        request_budget(Arc::new(Mutex::new(0))),
        Some(health),
    )?;
    if output != "#t" || state_new != state || external {
        return Err("request evaluator health result mismatch".into());
    }
    attest(&compiled);
    Ok(())
}

pub(crate) fn evaluate_record(
    _journal: &Journal,
    record: Word,
    query: &str,
    scenario: Option<ScenarioContext>,
) -> String {
    let budget = request_budget(Arc::new(Mutex::new(0)));
    evaluate_record_with_budget(record, query, scenario, budget)
}

fn evaluate_record_with_budget(
    record: Word,
    query: &str,
    scenario: Option<ScenarioContext>,
    budget: SharedBudget,
) -> String {
    #[cfg(target_env = "gnu")]
    let _trim_allocator = TrimAllocator;
    let error = |message: &str| format!("(error 'wasm-error \"{}\")", message.replace('"', "'"));
    let compiled = match compiled() {
        Ok(value) => value,
        Err(message) => return error(&message),
    };
    let mut runs = 0;
    loop {
        let serialized = if runs >= crate::RUNS {
            Some(crate::LOCK.lock().expect("serialized evaluation lock"))
        } else {
            None
        };
        let (state, temp) = {
            let snapshot = if serialized.is_none() {
                Some(crate::LOCK.lock().expect("snapshot lock"))
            } else {
                None
            };
            let state = match PERSISTOR.root_get(record) {
                Ok(value) => value,
                Err(_) => return error("record unavailable"),
            };
            let temp = match PERSISTOR.root_temp(state) {
                Ok(value) => value,
                Err(_) => return error("temporary root unavailable"),
            };
            drop(snapshot);
            (state, TempRoot(temp))
        };
        #[cfg(test)]
        if runs == 0 {
            let barrier = TEST_SNAPSHOT_BARRIERS
                .lock()
                .expect("test snapshot barriers")
                .get(&record)
                .cloned();
            if let Some(barrier) = barrier {
                barrier.wait();
            }
        }
        let genesis = match PERSISTOR
            .branch_get(state)
            .and_then(|branch| leaf_get_bounded(branch.0, CAPABILITY_BODY_LIMIT))
        {
            Ok(value) => value,
            Err(_) => return error("genesis unavailable"),
        };
        let (output, state_new, external, graph) = match run_once(
            &compiled,
            record,
            state,
            &genesis,
            query,
            scenario.clone(),
            budget.clone(),
        ) {
            Ok(value) => value,
            Err(message) => return error(&message),
        };
        #[cfg(test)]
        {
            *TEST_EVALUATIONS
                .lock()
                .expect("test evaluation counts")
                .entry(record)
                .or_default() += 1;
        }
        attest(&compiled);
        if external && state != state_new {
            return "(error 'external-state-error \"Request called an external function and changed state\")".into();
        }
        if state == state_new {
            return output;
        }
        let commit = if serialized.is_none() {
            Some(crate::LOCK.lock().expect("commit lock"))
        } else {
            None
        };
        if PERSISTOR.root_get(record).ok() == Some(state)
            && PERSISTOR.root_set(record, state, state_new, &graph).is_ok()
        {
            drop(commit);
            drop(temp);
            debug_assert_eq!(PERSISTOR.root_get(record).ok(), Some(state_new));
            return output;
        }
        drop(commit);
        drop(temp);
        drop(serialized);
        runs += 1;
    }
}
