use super::*;
use std::io::Write as _;
use std::net::TcpListener;
use std::sync::Barrier;

#[test]
fn response_retry_boundary_is_strictly_above_capacity() {
    assert!(!response_needs_retry(&vec![0; 4095], 4096));
    assert!(!response_needs_retry(&vec![0; 4096], 4096));
    assert!(response_needs_retry(&vec![0; 4097], 4096));
    assert!(!response_needs_retry(&[], -1));
}

#[test]
fn pending_response_is_one_shot_and_invocation_scoped() {
    let response = || PendingResponse {
        operation: 4,
        request: b"request".to_vec(),
        response: b"response".to_vec(),
    };
    let mut pending = Some(response());
    assert_eq!(
        take_pending_response(&mut pending, 4, b"request"),
        Some(b"response".to_vec())
    );
    assert!(pending.is_none());
    assert_eq!(take_pending_response(&mut pending, 4, b"request"), None);

    pending = Some(response());
    assert_eq!(take_pending_response(&mut pending, 5, b"request"), None);
    assert!(pending.is_none());
    pending = Some(response());
    assert_eq!(take_pending_response(&mut pending, 4, b"different"), None);
    assert!(pending.is_none());
}

#[test]
fn artifact_size_is_rejected_before_read() {
    let path = std::env::temp_dir().join(format!(
        "sync-web-oversized-aot-{}-{}",
        std::process::id(),
        rand::random::<u64>()
    ));
    let file = std::fs::File::create(&path).unwrap();
    file.set_len(AOT_ARTIFACT_LIMIT + 1).unwrap();
    drop(file);
    assert_eq!(
        read_artifact(path.to_str().unwrap()).unwrap_err(),
        "Wasmer kernel artifact exceeds 64 MiB"
    );
    std::fs::remove_file(path).unwrap();
}

#[test]
fn shared_request_budget_enforces_calls_bytes_deadline_and_memory() {
    let operation_memory = Arc::new(Mutex::new(0));
    let budget = Arc::new(Mutex::new(RequestBudget {
        calls: HOST_CALL_LIMIT,
        bytes: 0,
        deadline: Instant::now() + REQUEST_TIMEOUT,
        operation_memory: operation_memory.clone(),
    }));
    assert_eq!(
        debit_budget(&budget, 0, true).unwrap_err(),
        "bounded host capability exhausted"
    );
    {
        let mut state = budget.lock().unwrap();
        state.calls = 0;
        state.bytes = HOST_BYTE_LIMIT;
    }
    assert_eq!(
        debit_budget(&budget, 1, false).unwrap_err(),
        "bounded host capability exhausted"
    );
    {
        let mut state = budget.lock().unwrap();
        state.bytes = 0;
        state.deadline = Instant::now() - Duration::from_millis(1);
    }
    assert_eq!(
        debit_budget(&budget, 0, false).unwrap_err(),
        "bounded host capability exhausted"
    );
    budget.lock().unwrap().deadline = Instant::now() + REQUEST_TIMEOUT;
    *operation_memory.lock().unwrap() = OPERATION_GUEST_MEMORY_LIMIT;
    assert_eq!(
        MemoryReservation::enter(budget, 1).err().unwrap(),
        "bounded nested evaluator exhausted"
    );
}

#[test]
#[ignore = "explicit operation-memory and detached-work containment probe"]
fn operation_memory_and_detached_work_are_bounded() {
    let request = || request_budget(Arc::new(Mutex::new(0)));
    let causal_memory = Arc::new(Mutex::new(0));
    let first_request = request_budget(causal_memory.clone());
    let maximum = MemoryReservation::enter(first_request, MEMORY_MAXIMUM_BYTES)
        .expect("admitted operation maximum");
    let detached_child = request_budget(causal_memory);
    assert!(MemoryReservation::enter(detached_child, 1).is_err());
    let independent = MemoryReservation::enter(request(), MEMORY_MAXIMUM_BYTES)
        .expect("independent operation has no cross-request admission");
    drop(independent);
    drop(maximum);

    let growth_request = request();
    let growth = MemoryReservation::enter(growth_request.clone(), WASM_PAGE_SIZE as u64)
        .expect("admitted growing guest");
    growth
        .reserve(OPERATION_GUEST_MEMORY_LIMIT - WASM_PAGE_SIZE as u64)
        .expect("growth reserves the remaining operation envelope");
    assert!(MemoryReservation::enter(growth_request.clone(), 1).is_err());
    growth.release(OPERATION_GUEST_MEMORY_LIMIT - WASM_PAGE_SIZE as u64);
    let recovered = MemoryReservation::enter(growth_request, 1).expect("growth release recovers");
    drop(recovered);
    drop(growth);

    let detached = (0..DETACHED_WORK_LIMIT)
        .map(|_| {
            let message = "x".repeat(1024 * 1024);
            let lease = DetachedLease::acquire(message.len()).expect("admitted detached work");
            (lease, message)
        })
        .collect::<Vec<_>>();
    assert_eq!(
        DetachedLease::acquire(1).err().unwrap(),
        "bounded detached work exhausted"
    );
    assert_eq!(
        DetachedLease::acquire(CAPABILITY_BODY_LIMIT + 1)
            .err()
            .unwrap(),
        "bounded detached message exceeded"
    );
    drop(detached);
}

#[test]
#[ignore = "explicit instantiated Wasm memory-growth containment probe"]
fn instantiated_memory_growth_is_reserved_rolled_back_and_released() {
    assert!(std::env::var_os("SYNC_WEB_WASMER_KERNEL").is_some());
    TEST_MEMORY_GROWS.store(0, Ordering::Release);
    TEST_MEMORY_GROW_ROLLBACK_BYTES.store(0, Ordering::Release);
    assert_eq!(
        crate::JOURNAL.evaluate("(length (make-byte-vector 33554432 1))"),
        "33554432"
    );
    assert!(TEST_MEMORY_GROWS.load(Ordering::Acquire) > 0);

    TEST_MEMORY_GROW_ROLLBACK_BYTES.store(0, Ordering::Release);
    assert!(
        crate::JOURNAL
            .evaluate("(make-byte-vector 1073741824 1)")
            .starts_with("(error")
    );
    assert_eq!(crate::JOURNAL.evaluate("(+ 20 22)"), "42");

    let compiled = compiled().unwrap();
    let operation_memory = Arc::new(Mutex::new(0));
    let budget = request_budget(operation_memory.clone());
    let reservation = MemoryReservation::enter(budget, WASM_PAGE_SIZE as u64).unwrap();
    let context = ReservationContext::enter(reservation.clone());
    let tunables = BoundedTunables(BaseTunables::for_target(compiled.engine.target()));
    let mut memory = tunables
        .create_host_memory(
            &MemoryType {
                minimum: Pages(1),
                maximum: Some(Pages(MEMORY_MAXIMUM.0 - 1)),
                shared: false,
            },
            &tunables.memory_style(&MemoryType {
                minimum: Pages(1),
                maximum: Some(Pages(MEMORY_MAXIMUM.0 - 1)),
                shared: false,
            }),
        )
        .unwrap();
    drop(context);
    TEST_MEMORY_GROW_ROLLBACK_BYTES.store(0, Ordering::Release);
    assert!(memory.0.grow(Pages(MEMORY_MAXIMUM.0 - 1)).is_err());
    assert!(TEST_MEMORY_GROW_ROLLBACK_BYTES.load(Ordering::Acquire) > 0);
    assert!(memory.0.try_clone().is_err());
    assert!(memory.0.copy().is_err());
    let reserved = reservation.bytes.load(Ordering::Acquire);
    let _ = memory.0.reset();
    assert_eq!(reservation.bytes.load(Ordering::Acquire), reserved);
    drop(memory);
    drop(reservation);
    assert_eq!(*operation_memory.lock().unwrap(), 0);
}

#[test]
#[ignore = "explicit multi-generation detached memory-containment probe"]
fn detached_descendants_share_causal_operation_memory() {
    assert!(std::env::var_os("SYNC_WEB_WASMER_KERNEL").is_some());
    let mut records = [[0_u8; SIZE]; 3];
    for record in &mut records {
        rand::thread_rng().fill_bytes(record);
        let leaf = PERSISTOR
            .leaf_set(crate::GENESIS_STR.as_bytes().to_vec())
            .unwrap();
        let root = PERSISTOR
            .branch_set(leaf, crate::NULL, crate::NULL)
            .unwrap();
        PERSISTOR.root_new(*record, root).unwrap();
    }
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let server = std::thread::spawn(move || {
        for _ in 0..2 {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = [0_u8; 1024];
            let _ = stream.read(&mut request);
            std::thread::sleep(Duration::from_secs(5));
            let _ = stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nok");
        }
    });
    let id = |record: Word| format!("(hex-string->byte-vector \"{}\")", hex::encode(record));
    let child_c = "(let ((hold (make-byte-vector 104857600 1))) (set! *sync-state* (sync-cons (sync-car (sync-state)) #u(99))) (length hold))";
    let child_b = format!(
        "(let ((hold (make-byte-vector 157286400 1))) (sync-call '{} #f {}) (sync-http 'get \"http://{address}/b\") (length hold))",
        child_c,
        id(records[2])
    );
    let child_a = format!(
        "(let ((hold (make-byte-vector 314572800 1))) (sync-call '{} #f {}) (sync-http 'get \"http://{address}/a\") (length hold))",
        child_b,
        id(records[1])
    );
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let invoke = format!("(sync-call '{} #f {})", child_a, id(records[0]));
    assert_eq!(
        runtime
            .block_on(async { tokio::task::block_in_place(|| crate::JOURNAL.evaluate(&invoke)) }),
        "#t"
    );
    std::thread::sleep(Duration::from_secs(2));
    let inspect = format!("(sync-call '(sync-cdr (sync-state)) #t {})", id(records[2]));
    assert_ne!(
        runtime
            .block_on(async { tokio::task::block_in_place(|| crate::JOURNAL.evaluate(&inspect)) }),
        "#u(99)"
    );
    assert_eq!(crate::JOURNAL.evaluate("(+ 20 22)"), "42");
    server.join().unwrap();
    for record in records {
        PERSISTOR.root_delete(record).unwrap();
    }
}

#[test]
fn first_commit_collision_serializes_one_reevaluation() {
    assert!(std::env::var_os("SYNC_WEB_WASMER_KERNEL").is_some());
    let mut record = [0_u8; SIZE];
    rand::thread_rng().fill_bytes(&mut record);
    let leaf = PERSISTOR
        .leaf_set(crate::GENESIS_STR.as_bytes().to_vec())
        .unwrap();
    let root = PERSISTOR
        .branch_set(leaf, crate::NULL, crate::NULL)
        .unwrap();
    PERSISTOR.root_new(record, root).unwrap();
    TEST_SNAPSHOT_BARRIERS
        .lock()
        .expect("test snapshot barriers")
        .insert(record, Arc::new(Barrier::new(2)));
    TEST_EVALUATIONS
        .lock()
        .expect("test evaluation counts")
        .insert(record, 0);
    let handles = (1..=2)
            .map(|value| {
                std::thread::spawn(move || {
                    crate::JOURNAL.evaluate_record_with_context(
                        record,
                        &format!(
                            "(begin (let loop ((i 0)) (if (= i 20000000) #t (loop (+ i 1)))) (set! *sync-state* (sync-cons (sync-car (sync-state)) #u({value}))) #t)"
                        ),
                        None,
                    )
                })
            })
            .collect::<Vec<_>>();
    for handle in handles {
        assert_eq!(handle.join().expect("collision worker"), "#t");
    }
    TEST_SNAPSHOT_BARRIERS
        .lock()
        .expect("test snapshot barriers")
        .remove(&record);
    assert_eq!(
        TEST_EVALUATIONS
            .lock()
            .expect("test evaluation counts")
            .remove(&record),
        Some(3),
        "two initial evaluations plus exactly one serialized retry"
    );
    PERSISTOR.root_delete(record).unwrap();
}

#[test]
#[ignore = "explicit many-root parent-allocation containment probe"]
fn sync_all_many_roots_fails_before_unbounded_materialization() {
    assert!(std::env::var_os("SYNC_WEB_WASMER_KERNEL").is_some());
    let _guard = crate::LOCK.lock().expect("many-root lock");
    let leaf = PERSISTOR
        .leaf_set(crate::GENESIS_STR.as_bytes().to_vec())
        .unwrap();
    let root = PERSISTOR
        .branch_set(leaf, crate::NULL, crate::NULL)
        .unwrap();
    let records = (1_u64..=(CAPABILITY_BODY_LIMIT / SIZE + 1) as u64)
        .map(|index| {
            let mut record = [0_u8; SIZE];
            record.copy_from_slice(&Sha256::digest(index.to_be_bytes()));
            PERSISTOR.root_new(record, root).unwrap();
            record
        })
        .collect::<Vec<_>>();
    let budget = request_budget(Arc::new(Mutex::new(0)));
    let mut state = State {
        memory: None,
        budget,
        record: crate::NULL,
        scenario: None,
        isolated_persistor: None,
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
    };
    assert_eq!(persist(1, &[]).unwrap_err(), "bounded root-list");
    assert_eq!(
        capability(3, &[], &mut state).unwrap_err(),
        "bounded capability response exceeded"
    );
    let large_leaf = PERSISTOR
        .leaf_set(vec![7; CAPABILITY_BODY_LIMIT + 1])
        .unwrap();
    assert_eq!(
        persist(8, &large_leaf).unwrap().len(),
        CAPABILITY_BODY_LIMIT + 1
    );
    assert_eq!(
        persist(11, &large_leaf).unwrap().len(),
        CAPABILITY_BODY_LIMIT + 2
    );
    for record in records {
        PERSISTOR.root_delete(record).unwrap();
    }
}
