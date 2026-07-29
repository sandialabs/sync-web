use super::*;
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Condvar, MutexGuard};
use std::thread;
use std::time::{Duration, Instant};

#[derive(Debug)]
struct CapturedRequest {
    method: String,
    path: String,
    body: Vec<u8>,
}

struct LoopbackServer {
    address: SocketAddr,
    requests: Arc<(Mutex<Vec<CapturedRequest>>, Condvar)>,
    gate: Arc<(Mutex<bool>, Condvar)>,
    stopped: Arc<AtomicBool>,
    thread: Option<thread::JoinHandle<()>>,
}

impl LoopbackServer {
    fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback probe server");
        listener
            .set_nonblocking(true)
            .expect("set loopback listener nonblocking");
        let address = listener.local_addr().expect("read loopback address");
        let requests = Arc::new((Mutex::new(Vec::new()), Condvar::new()));
        let gate = Arc::new((Mutex::new(false), Condvar::new()));
        let stopped = Arc::new(AtomicBool::new(false));
        let server_requests = requests.clone();
        let server_gate = gate.clone();
        let server_stopped = stopped.clone();
        let thread = thread::spawn(move || {
            while !server_stopped.load(Ordering::SeqCst) {
                match listener.accept() {
                    Ok((mut stream, _)) => {
                        let request = read_request(&mut stream);
                        let path = request.path.clone();
                        let (lock, changed) = &*server_requests;
                        lock.lock().expect("lock request log").push(request);
                        changed.notify_all();
                        if path == "/gate" {
                            let (lock, changed) = &*server_gate;
                            let mut released = lock.lock().expect("lock response gate");
                            while !*released && !server_stopped.load(Ordering::SeqCst) {
                                released = changed.wait(released).expect("wait response gate");
                            }
                        }
                        let body: &[u8] = match path.as_str() {
                            "/invalid-utf8" => &[0xff, 0xfe],
                            "/scheme" => b"(remote data)",
                            _ => b"loopback-ok",
                        };
                        let header = format!(
                            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                            body.len()
                        );
                        stream
                            .write_all(header.as_bytes())
                            .expect("write response header");
                        stream.write_all(body).expect("write response body");
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(2));
                    }
                    Err(error) => panic!("loopback accept failed: {error}"),
                }
            }
        });
        Self {
            address,
            requests,
            gate,
            stopped,
            thread: Some(thread),
        }
    }

    fn url(&self, path: &str) -> String {
        format!("http://{}{}", self.address, path)
    }

    fn wait_for_requests(&self, expected: usize) {
        let deadline = Instant::now() + Duration::from_secs(10);
        let (lock, changed) = &*self.requests;
        let mut requests = lock.lock().expect("lock request log");
        while requests.len() < expected {
            let remaining = deadline
                .checked_duration_since(Instant::now())
                .expect("timed out waiting for loopback request");
            let (next, timeout) = changed
                .wait_timeout(requests, remaining)
                .expect("wait for loopback request");
            requests = next;
            assert!(
                !timeout.timed_out(),
                "timed out waiting for loopback request"
            );
        }
    }

    fn requests(&self) -> MutexGuard<'_, Vec<CapturedRequest>> {
        self.requests.0.lock().expect("lock request log")
    }

    fn release_gate(&self) {
        let (lock, changed) = &*self.gate;
        *lock.lock().expect("lock response gate") = true;
        changed.notify_all();
    }
}

impl Drop for LoopbackServer {
    fn drop(&mut self) {
        self.stopped.store(true, Ordering::SeqCst);
        self.release_gate();
        if let Some(thread) = self.thread.take() {
            thread.join().expect("join loopback server");
        }
    }
}

fn read_request(stream: &mut TcpStream) -> CapturedRequest {
    stream
        .set_read_timeout(Some(Duration::from_secs(10)))
        .expect("set request timeout");
    let mut bytes = Vec::new();
    let mut buffer = [0u8; 4096];
    let header_end = loop {
        let read = stream.read(&mut buffer).expect("read loopback request");
        assert!(read > 0, "request closed before headers");
        bytes.extend_from_slice(&buffer[..read]);
        if let Some(index) = bytes.windows(4).position(|window| window == b"\r\n\r\n") {
            break index + 4;
        }
    };
    let headers =
        String::from_utf8(bytes[..header_end].to_vec()).expect("request headers are UTF-8");
    let content_length = headers
        .lines()
        .find_map(|line| {
            let (name, value) = line.split_once(':')?;
            name.eq_ignore_ascii_case("content-length")
                .then(|| value.trim().parse::<usize>().expect("valid content length"))
        })
        .unwrap_or(0);
    while bytes.len() < header_end + content_length {
        let read = stream.read(&mut buffer).expect("read loopback body");
        assert!(read > 0, "request closed before body");
        bytes.extend_from_slice(&buffer[..read]);
    }
    let request_line = headers.lines().next().expect("request line");
    let mut fields = request_line.split_whitespace();
    CapturedRequest {
        method: fields.next().expect("request method").to_string(),
        path: fields.next().expect("request path").to_string(),
        body: bytes[header_end..header_end + content_length].to_vec(),
    }
}

fn with_runtime<T>(call: impl FnOnce() -> T) -> T {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .expect("create probe runtime");
    runtime.block_on(async move { call() })
}

fn probe_host_with_cache(
    cache: Arc<Mutex<HashMap<(String, String, Vec<u8>), Vec<u8>>>>,
) -> (RustHost, SharedSession) {
    let session = Rc::new(RefCell::new(RustSession {
        record: NULL,
        state: NULL,
        persistor: MemoryPersistor::new(),
        cache,
        external_called: Rc::new(Cell::new(false)),
    }));
    let mut host = RustHost::new();
    register_core(&mut host, session.clone());
    host.initialize_with(PRINT_INITIALIZATION);
    (host, session)
}

fn probe_host() -> RustHost {
    probe_host_with_cache(Arc::new(Mutex::new(HashMap::new()))).0
}

fn create_probe_record() -> Word {
    let mut record = [0u8; SIZE];
    OsRng.fill_bytes(&mut record);
    let genesis = PERSISTOR
        .leaf_set(GENESIS_STR.as_bytes().to_vec())
        .expect("create genesis leaf");
    let root = PERSISTOR
        .branch_set(genesis, NULL, NULL)
        .expect("create genesis branch");
    PERSISTOR
        .root_new(record, root)
        .expect("create probe record");
    record
}

#[test]
fn print_preserves_identity_and_indirect_inventory() {
    let host = probe_host();
    assert!(host.missing_sync_web_primitives().is_empty());
    assert!(!host.registered_primitive_names().contains(&"print"));
    assert!(host.provided_primitive_names().contains(&"print"));
    let source = "(let ((p (list 1)) (v (vector 2)) (n (sync-null))) (list (eq? p (print p)) (eq? v (print v)) (eq? n (print n))))";
    assert_eq!(host.evaluate(source).unwrap().to_string(), "(#t #t #t)");
    assert_eq!(
        host.evaluate_unified_output(source),
        Ok("(#t #t #t)".into())
    );
    let mutation = "(let* ((p (list 1)) (returned (print p))) (set-car! returned 9) (car p))";
    assert_eq!(host.evaluate(mutation).unwrap().to_string(), "9");
    assert_eq!(host.evaluate_unified_output(mutation), Ok("9".into()));
}

#[test]
fn network_edges_fail_closed_before_authority() {
    let server = LoopbackServer::start();
    let (host, session) = probe_host_with_cache(Arc::new(Mutex::new(HashMap::new())));
    host.cancel();
    let interrupted = with_runtime(|| {
        host.evaluate_unified_output(&format!("(sync-http 'get \"{}\")", server.url("/ok")))
    })
    .expect_err("pre-call cancellation must interrupt");
    assert!(interrupted.contains("interrupted"));
    assert!(server.requests().is_empty());
    assert!(!session.borrow().external_called.get());

    host.clear_cancellation();
    let unsupported = with_runtime(|| {
        host.evaluate_unified_output(&format!("(sync-http 'delete \"{}\")", server.url("/ok")))
    })
    .expect_err("unsupported method must fail");
    assert!(unsupported.contains("sync-web-error"));
    assert!(server.requests().is_empty());
    assert!(session.borrow().external_called.get());

    let invalid = with_runtime(|| {
        host.evaluate_unified_output(&format!(
            "(sync-remote \"{}\" '(request data))",
            server.url("/invalid-utf8")
        ))
    })
    .expect_err("invalid UTF-8 must fail");
    assert!(invalid.contains("encoding-error"));
    server.wait_for_requests(1);
    let requests = server.requests();
    assert_eq!(requests[0].method, "POST");
    assert_eq!(requests[0].path, "/invalid-utf8");
    assert_eq!(requests[0].body, b"(request data)");
}

#[test]
fn retry_cache_marks_external_and_does_not_cross_requests() {
    let server = LoopbackServer::start();
    let source = format!("(sync-http 'get \"{}\")", server.url("/cache"));
    let cache = Arc::new(Mutex::new(HashMap::new()));
    let (first, first_session) = probe_host_with_cache(cache.clone());
    assert!(with_runtime(|| first.evaluate_unified_output(&source)).is_ok());
    server.wait_for_requests(1);
    assert!(first_session.borrow().external_called.get());

    let (retry, retry_session) = probe_host_with_cache(cache);
    assert!(!retry_session.borrow().external_called.get());
    assert!(with_runtime(|| retry.evaluate_unified_output(&source)).is_ok());
    assert_eq!(server.requests().len(), 1, "retry must use request cache");
    assert!(
        retry_session.borrow().external_called.get(),
        "cache hit must still mark retry external"
    );

    let fresh = probe_host();
    assert!(with_runtime(|| fresh.evaluate_unified_output(&source)).is_ok());
    server.wait_for_requests(2);
    assert_eq!(
        server.requests().len(),
        2,
        "cache must not cross public requests"
    );
}

#[test]
fn external_call_and_state_change_never_commit() {
    let server = LoopbackServer::start();
    let record = create_probe_record();
    let root_before = PERSISTOR.root_get(record).expect("read initial root");
    let query = format!(
        "(begin (sync-http 'get \"{0}\") (sync-http 'get \"{0}\") (set! *sync-state* (sync-cons (expression->byte-vector 'changed) *sync-state*)) #t)",
        server.url("/external-state")
    );
    let result = with_runtime(|| evaluate_record_unified(record, &query));
    assert!(result.contains("external-state-error"));
    assert_eq!(PERSISTOR.root_get(record).unwrap(), root_before);
    server.wait_for_requests(1);
    assert_eq!(
        server.requests().len(),
        1,
        "second call must be a cache hit"
    );
    PERSISTOR.root_delete(record).expect("delete probe record");
}

#[test]
fn loopback_collision_cannot_overwrite_concurrent_state() {
    let server = LoopbackServer::start();
    let record = create_probe_record();
    let url = server.url("/gate");
    let external = thread::spawn(move || {
        with_runtime(|| evaluate_record_unified(record, &format!("(sync-http 'get \"{url}\")")))
    });
    server.wait_for_requests(1);

    let mutation = "(begin (set! *sync-state* (sync-cons (expression->byte-vector 'concurrent) *sync-state*)) #t)";
    assert_eq!(evaluate_record_unified(record, mutation), "#t");
    let concurrent_root = PERSISTOR.root_get(record).expect("read concurrent root");
    server.release_gate();
    assert!(
        external
            .join()
            .expect("join external evaluation")
            .starts_with("#u(")
    );
    assert_eq!(PERSISTOR.root_get(record).unwrap(), concurrent_root);
    assert_eq!(server.requests().len(), 1);
    PERSISTOR.root_delete(record).expect("delete probe record");
}
