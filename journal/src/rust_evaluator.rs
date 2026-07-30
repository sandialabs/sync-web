use crate::cache::{
    OverlayPersistor, ResolveSource, ResolvedNode, resolve_branch_with, resolve_node_with,
    resolve_stump_with,
};
use crate::persistor::{MemoryPersistor, PERSISTOR, Persistor, SIZE};
use crate::{
    CallOnDrop, GENESIS_STR, JOURNAL, LOCK, NULL, RUNS, Word, escape_scheme_string,
    warn_on_error_result,
};
use crystals_dilithium::dilithium2::*;
use crystals_dilithium::sign::lvl2::*;
use log::{debug, info};
use rand::RngCore;
use rand::rngs::OsRng;
use s7_rust::{BorrowedValue, HostError, HostObject, HostOutput, PrimitiveSpec, RustHost};
use sha2::{Digest, Sha256};
use std::any::Any;
use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

#[derive(Clone)]
struct RustSyncNode(Word);

impl HostObject for RustSyncNode {
    fn type_name(&self) -> &str {
        "sync-node"
    }
    fn identity(&self) -> u64 {
        u64::from_le_bytes(self.0[..8].try_into().unwrap())
    }
    fn display(&self) -> String {
        format!(
            "(sync-node #u({}))",
            self.0
                .iter()
                .map(u8::to_string)
                .collect::<Vec<_>>()
                .join(" ")
        )
    }
    fn equals(&self, other: &dyn HostObject) -> bool {
        other
            .as_any()
            .downcast_ref::<Self>()
            .map(|other| self.0 == other.0)
            .unwrap_or(false)
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

struct RustSession {
    record: Word,
    state: Word,
    persistor: MemoryPersistor,
    cache: Arc<Mutex<HashMap<(String, String, Vec<u8>), Vec<u8>>>>,
    external_called: Rc<Cell<bool>>,
}

type SharedSession = Rc<RefCell<RustSession>>;

const PRINT_INITIALIZATION: &str = "(varlet (rootlet) 'print (lambda args (let ((result (apply %print args))) (if (null? args) result (car (reverse args))))))";
const JOURNAL_CONDITION_HANDLER: &str = concat!(
    "(let* ((type (car x)) ",
    "(data (if (pair? (cdr x)) (cadr x) '())) ",
    "(message (catch #t ",
    "(lambda () (if (and (pair? data) (string? (car data))) ",
    "(apply format (cons #f data)) ",
    "(object->string data))) ",
    "(lambda args (object->string data))))) ",
    "`(error ',type ,message ((data ,data))))",
);

fn wrap_journal_conditions(expression: &str) -> String {
    format!(
        "(catch #t (lambda () {expression}) (lambda x {JOURNAL_CONDITION_HANDLER}))"
    )
}

fn node(value: &BorrowedValue<'_>, name: &str, index: usize) -> Result<Word, HostError> {
    value
        .as_host()
        .and_then(|object| object.as_any().downcast_ref::<RustSyncNode>())
        .map(|node| node.0)
        .ok_or_else(|| HostError::wrong_type(name, index, "a sync-node", value))
}

fn node_or_bytes(
    value: &BorrowedValue<'_>,
    name: &str,
    index: usize,
) -> Result<NodeInput, HostError> {
    if let Some(bytes) = value.as_bytes() {
        return Ok(NodeInput::Bytes(bytes));
    }
    node(value, name, index).map(NodeInput::Node)
}

enum NodeInput {
    Node(Word),
    Bytes(Vec<u8>),
}

fn digest(session: &RustSession, word: Word) -> Result<Word, HostError> {
    if word == NULL {
        return Ok(NULL);
    }
    match resolve_node_with(&session.persistor, None, word) {
        Some(ResolvedNode::Branch((_, _, digest), _)) => Ok(digest),
        Some(ResolvedNode::Leaf(_, _)) => Ok(word),
        Some(ResolvedNode::Stump(digest, _)) => Ok(digest),
        None => Err(HostError::new(
            "sync-web-error",
            "Digest not found in persistor",
        )),
    }
}

fn branch_children(session: &RustSession, word: Word) -> Result<(Word, Word), HostError> {
    resolve_branch_with(&session.persistor, None, word)
        .map(|((left, right, _), _)| (left, right))
        .ok_or_else(|| HostError::new("sync-web-error", "Node is not a sync-pair"))
}

fn child_output(session: &RustSession, word: Word, name: &str) -> Result<HostOutput, HostError> {
    if word == NULL {
        return Ok(HostOutput::Host(Rc::new(RustSyncNode(word))));
    }
    match resolve_node_with(&session.persistor, None, word) {
        Some(ResolvedNode::Branch((left, right, digest), ResolveSource::Global)) => {
            session
                .persistor
                .branch_set(left, right, digest)
                .map_err(|_| {
                    HostError::new("sync-web-error", "Failed to copy branch into session")
                })?;
            Ok(HostOutput::Host(Rc::new(RustSyncNode(word))))
        }
        Some(ResolvedNode::Branch(_, _)) => Ok(HostOutput::Host(Rc::new(RustSyncNode(word)))),
        Some(ResolvedNode::Leaf(content, ResolveSource::Global)) => {
            session.persistor.leaf_set(content.clone()).map_err(|_| {
                HostError::new("sync-web-error", "Failed to copy leaf into session")
            })?;
            Ok(HostOutput::ByteVector(content))
        }
        Some(ResolvedNode::Leaf(content, _)) => Ok(HostOutput::ByteVector(content)),
        Some(ResolvedNode::Stump(digest, ResolveSource::Global)) => {
            session.persistor.stump_set(digest).map_err(|_| {
                HostError::new("sync-web-error", "Failed to copy stub into session")
            })?;
            Ok(HostOutput::Host(Rc::new(RustSyncNode(word))))
        }
        Some(ResolvedNode::Stump(_, _)) => Ok(HostOutput::Host(Rc::new(RustSyncNode(word)))),
        None => Err(HostError::new(
            "sync-web-error",
            format!("Cannot retrieve items for node that is not a sync-pair ({name})"),
        )),
    }
}

fn input_handle(session: &RustSession, input: NodeInput, name: &str) -> Result<Word, HostError> {
    match input {
        NodeInput::Node(word) => Ok(word),
        NodeInput::Bytes(bytes) => session.persistor.leaf_set(bytes).map_err(|_| {
            HostError::new(
                "sync-web-error",
                format!("Journal is unable to add leaf node ({name})"),
            )
        }),
    }
}

fn fixed_word(value: &BorrowedValue<'_>, name: &str, index: usize) -> Result<Word, HostError> {
    let bytes = value
        .as_bytes()
        .ok_or_else(|| HostError::wrong_type(name, index, "a hash-sized byte-vector", value))?;
    bytes
        .try_into()
        .map_err(|_| HostError::wrong_type(name, index, "a hash-sized byte-vector", value))
}

fn bytes_arg(value: &BorrowedValue<'_>, name: &str, index: usize) -> Result<Vec<u8>, HostError> {
    value
        .as_bytes()
        .ok_or_else(|| HostError::wrong_type(name, index, "a byte-vector", value))
}

fn register_core(host: &mut RustHost, session: SharedSession) {
    host.register_codecs();
    host.register_indirect_primitive("sync-eval");
    host.register_indirect_primitive("print");
    host.register(PrimitiveSpec::fixed(
        "random-byte-vector",
        1,
        |args, context| {
            context.check_cancelled()?;
            let length = args[0].as_i64().ok_or_else(|| {
                HostError::wrong_type("random-byte-vector", 1, "a non-negative integer", &args[0])
            })?;
            let length = usize::try_from(length).map_err(|_| {
                HostError::wrong_type("random-byte-vector", 1, "a non-negative integer", &args[0])
            })?;
            let mut bytes = vec![0; length];
            OsRng.fill_bytes(&mut bytes);
            context.check_cancelled()?;
            Ok(HostOutput::ByteVector(bytes))
        },
    ));
    host.register(PrimitiveSpec::new("%print", 0, None, |args, _| {
        println!(
            "{}",
            args.iter()
                .map(BorrowedValue::object_string)
                .collect::<Vec<_>>()
                .join(" ")
        );
        Ok(HostOutput::Unspecified)
    }));
    host.register(PrimitiveSpec::new("stacktrace", 0, None, |_, _| {
        Ok(HostOutput::String("<unavailable>".to_string()))
    }));
    host.register(PrimitiveSpec::fixed("sync-node?", 1, |args, _| {
        Ok(HostOutput::Bool(
            args[0]
                .as_host()
                .and_then(|v| v.as_any().downcast_ref::<RustSyncNode>())
                .is_some(),
        ))
    }));
    host.register(PrimitiveSpec::fixed("sync-null", 0, |_, _| {
        Ok(HostOutput::Host(Rc::new(RustSyncNode(NULL))))
    }));
    host.register(PrimitiveSpec::fixed("sync-null?", 1, |args, _| {
        if args[0].as_bytes().is_some() {
            return Ok(HostOutput::Bool(false));
        }
        Ok(HostOutput::Bool(node(&args[0], "sync-null?", 1)? == NULL))
    }));
    let pair_session = session.clone();
    host.register(PrimitiveSpec::fixed("sync-pair?", 1, move |args, _| {
        if args[0].as_bytes().is_some() {
            return Ok(HostOutput::Bool(false));
        }
        let word = node(&args[0], "sync-pair?", 1)?;
        Ok(HostOutput::Bool(
            branch_children(&pair_session.borrow(), word).is_ok(),
        ))
    }));
    let stub_session = session.clone();
    host.register(PrimitiveSpec::fixed("sync-stub?", 1, move |args, _| {
        if args[0].as_bytes().is_some() {
            return Ok(HostOutput::Bool(false));
        }
        let word = node(&args[0], "sync-stub?", 1)?;
        Ok(HostOutput::Bool(
            resolve_stump_with(&stub_session.borrow().persistor, None, word).is_some(),
        ))
    }));
    let state_session = session.clone();
    host.register(PrimitiveSpec::fixed("sync-state", 0, move |_, _| {
        Ok(HostOutput::Host(Rc::new(RustSyncNode(
            state_session.borrow().state,
        ))))
    }));
    host.register(PrimitiveSpec::fixed("sync-hash", 1, |args, _| {
        let bytes = args[0]
            .as_bytes()
            .ok_or_else(|| HostError::wrong_type("sync-hash", 1, "a byte-vector", &args[0]))?;
        Ok(HostOutput::ByteVector(Sha256::digest(bytes).to_vec()))
    }));
    let digest_session = session.clone();
    host.register(PrimitiveSpec::fixed("sync-digest", 1, move |args, _| {
        if let Some(bytes) = args[0].as_bytes() {
            return Ok(HostOutput::ByteVector(Sha256::digest(bytes).to_vec()));
        }
        Ok(HostOutput::ByteVector(
            digest(&digest_session.borrow(), node(&args[0], "sync-digest", 1)?)?.to_vec(),
        ))
    }));
    let stub_make_session = session.clone();
    host.register(PrimitiveSpec::fixed("sync-stub", 1, move |args, _| {
        let digest = fixed_word(&args[0], "sync-stub", 1)?;
        let stump = stub_make_session
            .borrow()
            .persistor
            .stump_set(digest)
            .map_err(|_| {
                HostError::new(
                    "sync-web-error",
                    "Journal is unable to create stub node (sync-stub)",
                )
            })?;
        Ok(HostOutput::Host(Rc::new(RustSyncNode(stump))))
    }));
    let cons_session = session.clone();
    host.register(PrimitiveSpec::fixed("sync-cons", 2, move |args, _| {
        let session = cons_session.borrow();
        let left = input_handle(
            &session,
            node_or_bytes(&args[0], "sync-cons", 1)?,
            "sync-cons",
        )?;
        let right = input_handle(
            &session,
            node_or_bytes(&args[1], "sync-cons", 2)?,
            "sync-cons",
        )?;
        let mut joined = [0u8; SIZE * 2];
        joined[..SIZE].copy_from_slice(&digest(&session, left)?);
        joined[SIZE..].copy_from_slice(&digest(&session, right)?);
        let pair = session
            .persistor
            .branch_set(left, right, Sha256::digest(joined).into())
            .map_err(|_| {
                HostError::new(
                    "sync-web-error",
                    "Journal is unable to add pair node (sync-cons)",
                )
            })?;
        Ok(HostOutput::Host(Rc::new(RustSyncNode(pair))))
    }));
    let car_session = session.clone();
    host.register(PrimitiveSpec::fixed("sync-car", 1, move |args, _| {
        let word = node(&args[0], "sync-car", 1)?;
        let session = car_session.borrow();
        let (first, _) = branch_children(&session, word).map_err(|_| {
            HostError::new(
                "sync-web-error",
                "Journal cannot retrieve leaf byte-vector (sync-car)",
            )
        })?;
        child_output(&session, first, "sync-car")
    }));
    let cdr_session = session.clone();
    host.register(PrimitiveSpec::fixed("sync-cdr", 1, move |args, _| {
        let word = node(&args[0], "sync-cdr", 1)?;
        let session = cdr_session.borrow();
        let (_, rest) = branch_children(&session, word).map_err(|_| {
            HostError::new(
                "sync-web-error",
                "Journal cannot retrieve leaf byte-vector (sync-cdr)",
            )
        })?;
        child_output(&session, rest, "sync-cdr")
    }));
    let cut_session = session.clone();
    host.register(PrimitiveSpec::fixed("sync-cut", 1, move |args, _| {
        let digest = if let Some(bytes) = args[0].as_bytes() {
            Sha256::digest(Sha256::digest(bytes)).into()
        } else {
            digest(&cut_session.borrow(), node(&args[0], "sync-cut", 1)?)?
        };
        let stump = cut_session
            .borrow()
            .persistor
            .stump_set(digest)
            .map_err(|_| {
                HostError::new(
                    "sync-web-error",
                    "Journal is unable to add stub node (sync-cut)",
                )
            })?;
        Ok(HostOutput::Host(Rc::new(RustSyncNode(stump))))
    }));
    let eval_session = session.clone();
    host.register(PrimitiveSpec::new("%sync-loader", 1, Some(2), move |args, _| {
        let strict = if let Some(strict) = args.get(1) { strict.as_bool().ok_or_else(|| HostError::wrong_type("sync-eval", 2, "a boolean", strict))? } else { true };
        let expression = node(&args[0], "sync-eval", 1)?;
        let session = eval_session.borrow();
        let (header, _) = branch_children(&session, expression).map_err(|error| HostError::new("sync-web-error", format!("sync-eval first argument should be a sync-node with a byte-vector header ({})", error.message)))?;
        let HostOutput::ByteVector(bytes) = child_output(&session, header, "sync-eval")? else {
            return Err(HostError::new("sync-web-error", "sync-eval first argument should be a sync-node with a byte-vector header"));
        };
        let source = String::from_utf8(bytes).map_err(|_| HostError::new("encoding-error", "Byte vector string is malformed"))?;
        let _ = strict;
        Ok(HostOutput::Expression(source))
    }));

    let http_session = session.clone();
    host.register(PrimitiveSpec::new(
        "sync-http",
        2,
        Some(4),
        move |args, context| {
            context.check_cancelled()?;
            let (external_called, cache) = {
                let session = http_session.borrow();
                (session.external_called.clone(), session.cache.clone())
            };
            external_called.set(true);
            let method_key = args[0].object_string();
            let method = method_key.to_lowercase();
            if !matches!(method.as_str(), "get" | "post") {
                return Err(HostError::new(
                    "sync-web-error",
                    "Unsupported HTTP method (sync-http)",
                ));
            }
            let url_key = args[1].object_string();
            let url = args[1]
                .as_string()
                .ok_or_else(|| HostError::wrong_type("sync-http", 2, "a URL string", &args[1]))?;
            let (body_key, body) = if let Some(body) = args.get(2) {
                (
                    body.object_string().into_bytes(),
                    body.as_string()
                        .ok_or_else(|| HostError::wrong_type("sync-http", 3, "a string", body))?,
                )
            } else {
                (Vec::new(), String::new())
            };
            let key = (method_key, url_key, body_key);
            let mut cache = cache.lock().map_err(|_| {
                HostError::new("sync-web-error", "Journal HTTP cache lock is unavailable")
            })?;
            if let Some(bytes) = cache.get(&key) {
                return Ok(HostOutput::ByteVector(bytes.clone()));
            }
            let request_url = url.clone();
            let request_body = body.clone();
            let result = tokio::task::block_in_place(move || {
                tokio::runtime::Handle::current().block_on(async move {
                    match method.as_str() {
                        "get" => JOURNAL.client.get(&request_url).send().await?.bytes().await,
                        "post" => {
                            JOURNAL
                                .client
                                .post(&request_url)
                                .body(request_body)
                                .send()
                                .await?
                                .bytes()
                                .await
                        }
                        _ => unreachable!("HTTP method validated before request"),
                    }
                })
            });
            let bytes = result
                .map_err(|_| {
                    HostError::new(
                        "sync-web-error",
                        "Journal is unable to fulfill HTTP request (sync-http)",
                    )
                })?
                .to_vec();
            context.check_cancelled()?;
            cache.insert(key, bytes.clone());
            Ok(HostOutput::ByteVector(bytes))
        },
    ));

    let remote_session = session.clone();
    host.register(PrimitiveSpec::fixed(
        "sync-remote",
        2,
        move |args, context| {
            context.check_cancelled()?;
            let (external_called, cache) = {
                let session = remote_session.borrow();
                (session.external_called.clone(), session.cache.clone())
            };
            external_called.set(true);
            let url_key = args[0].object_string();
            let url = args[0]
                .as_string()
                .ok_or_else(|| HostError::wrong_type("sync-remote", 1, "a URL string", &args[0]))?;
            let body = args[1].object_string();
            let key = ("post".to_string(), url_key, body.as_bytes().to_vec());
            let mut cache = cache.lock().map_err(|_| {
                HostError::new("sync-web-error", "Journal remote cache lock is unavailable")
            })?;
            let bytes = if let Some(bytes) = cache.get(&key) {
                bytes.clone()
            } else {
                let request_url = url.clone();
                let request_body = body.clone();
                let bytes = tokio::task::block_in_place(move || {
                    tokio::runtime::Handle::current().block_on(async move {
                        JOURNAL
                            .client
                            .post(&request_url)
                            .body(request_body)
                            .send()
                            .await?
                            .bytes()
                            .await
                    })
                })
                .map_err(|_| {
                    HostError::new(
                        "sync-web-error",
                        "Journal is unable to query remote peer (sync-remote)",
                    )
                })?
                .to_vec();
                context.check_cancelled()?;
                cache.insert(key, bytes.clone());
                bytes
            };
            let source = String::from_utf8(bytes).map_err(|_| {
                HostError::new("encoding-error", "Remote response is not valid Scheme text")
            })?;
            Ok(HostOutput::Expression(format!("'{source}")))
        },
    ));

    host.register(PrimitiveSpec::fixed("sync-create", 1, |args, _| {
        let record = fixed_word(&args[0], "sync-create", 1)?;
        let root = PERSISTOR
            .branch_set(
                PERSISTOR
                    .leaf_set(GENESIS_STR.as_bytes().to_vec())
                    .map_err(|_| {
                        HostError::new("sync-web-error", "Failed to create genesis leaf")
                    })?,
                NULL,
                NULL,
            )
            .map_err(|_| HostError::new("sync-web-error", "Failed to create genesis branch"))?;
        PERSISTOR
            .root_new(record, root)
            .map(|_| HostOutput::Bool(true))
            .map_err(|_| HostError::new("sync-web-error", "record ID is already in use"))
    }));
    host.register(PrimitiveSpec::fixed("sync-delete", 1, |args, _| {
        let record = fixed_word(&args[0], "sync-delete", 1)?;
        if record == NULL {
            return Err(HostError::new(
                "sync-web-error",
                "cannot delete the root record",
            ));
        }
        PERSISTOR
            .root_delete(record)
            .map(|_| HostOutput::Bool(true))
            .map_err(|_| HostError::new("sync-web-error", "record ID does not exist"))
    }));
    host.register(PrimitiveSpec::fixed("sync-all", 0, |_, _| {
        Ok(HostOutput::List(
            PERSISTOR
                .root_list()
                .into_iter()
                .map(|word| HostOutput::ByteVector(word.to_vec()))
                .collect(),
        ))
    }));
    host.register(PrimitiveSpec::fixed("crypto-generate", 1, |args, _| {
        let digest = Sha256::digest(bytes_arg(&args[0], "crypto-generate", 1)?).to_vec();
        let generated = std::panic::catch_unwind(|| {
            let mut public = [0u8; PUBLICKEYBYTES];
            let mut private = [0u8; SECRETKEYBYTES];
            keypair(&mut public, &mut private, Some(&digest));
            (public, private)
        })
        .map_err(|_| {
            HostError::new(
                "crypto-error",
                "cryptographic library encountered unexpected error",
            )
        })?;
        Ok(HostOutput::Pair(
            Box::new(HostOutput::ByteVector(generated.0.to_vec())),
            Box::new(HostOutput::ByteVector(generated.1.to_vec())),
        ))
    }));
    host.register(PrimitiveSpec::fixed("crypto-sign", 2, |args, _| {
        let private = bytes_arg(&args[0], "crypto-sign", 1)?;
        let digest = Sha256::digest(bytes_arg(&args[1], "crypto-sign", 2)?).to_vec();
        let signed = std::panic::catch_unwind(|| {
            let mut signed = [0u8; SIGNBYTES];
            signature(&mut signed, &digest, &private, false);
            signed
        })
        .map_err(|_| {
            HostError::new(
                "crypto-error",
                "cryptographic library encountered unexpected error",
            )
        })?;
        Ok(HostOutput::ByteVector(signed.to_vec()))
    }));
    host.register(PrimitiveSpec::fixed("crypto-verify", 3, |args, _| {
        let public = bytes_arg(&args[0], "crypto-verify", 1)?;
        let signed = bytes_arg(&args[1], "crypto-verify", 2)?;
        let digest = Sha256::digest(bytes_arg(&args[2], "crypto-verify", 3)?).to_vec();
        let verified =
            std::panic::catch_unwind(|| verify(&signed, &digest, &public)).map_err(|_| {
                HostError::new(
                    "crypto-error",
                    "cryptographic library encountered unexpected error",
                )
            })?;
        Ok(HostOutput::Bool(verified))
    }));

    host.register(PrimitiveSpec::new(
        "system-time-utc",
        0,
        Some(1),
        |args, _| {
            let time = if let Some(value) = args.first() {
                let unix = value.as_i64().ok_or_else(|| {
                    HostError::wrong_type("system-time-utc", 1, "a unix timestamp integer", value)
                })?;
                OffsetDateTime::from_unix_timestamp(unix)
                    .map_err(|_| HostError::new("time-error", "Invalid unix timestamp range"))?
            } else {
                OffsetDateTime::now_utc()
            };
            Ok(HostOutput::String(time.format(&Rfc3339).map_err(|_| {
                HostError::new("time-error", "Failed to format UTC time")
            })?))
        },
    ));
    host.register(PrimitiveSpec::new(
        "system-time-unix",
        0,
        Some(1),
        |args, _| {
            if let Some(value) = args.first() {
                let timestamp = value.as_string().ok_or_else(|| {
                    HostError::wrong_type("system-time-unix", 1, "an RFC3339 UTC string", value)
                })?;
                let parsed = OffsetDateTime::parse(&timestamp, &Rfc3339).map_err(|_| {
                    HostError::new("time-error", "Failed to parse RFC3339 timestamp")
                })?;
                Ok(HostOutput::Int(parsed.unix_timestamp()))
            } else {
                Ok(HostOutput::Int(OffsetDateTime::now_utc().unix_timestamp()))
            }
        },
    ));

    let call_session = session;
    host.register(PrimitiveSpec::new(
        "sync-call",
        2,
        Some(3),
        move |args, _| {
            let message = args[0].object_string();
            let blocking = args[1]
                .as_bool()
                .ok_or_else(|| HostError::wrong_type("sync-call", 2, "a boolean", &args[1]))?;
            let (record, external_called) = {
                let session = call_session.borrow();
                let record = if let Some(value) = args.get(2) {
                    fixed_word(value, "sync-call", 3)?
                } else {
                    session.record
                };
                (record, session.external_called.clone())
            };
            external_called.set(true);
            if PERSISTOR.root_get(record).is_err() {
                return Err(HostError::new("sync-web-error", "record ID does not exist"));
            }
            if blocking {
                Ok(HostOutput::Expression(
                    JOURNAL.evaluate_record(record, &message),
                ))
            } else {
                tokio::spawn(async move {
                    JOURNAL.evaluate_record(record, &message);
                });
                Ok(HostOutput::Bool(true))
            }
        },
    ));
}

fn parse_result(result: &str, state_old: Word) -> (String, Word) {
    if result.starts_with("(error ") {
        return (result.to_string(), state_old);
    }
    match result.rfind('.') {
        Some(index) if result.len() >= index + 19 => {
            let bytes = result[(index + 16)..(result.len() - 3)]
                .split(' ')
                .map(str::parse::<u8>)
                .collect::<Result<Vec<_>, _>>();
            if let Ok(bytes) = bytes {
                if let Ok(state_new) = bytes.try_into() {
                    return (result[1..(index - 1)].to_string(), state_new);
                }
            }
            (
                "(error 'sync-format \"Invalid return format\")".to_string(),
                state_old,
            )
        }
        _ => (
            "(error 'sync-format \"Invalid return format\")".to_string(),
            state_old,
        ),
    }
}

fn unified_removed_bindings_source() -> String {
    let mut seen = HashSet::new();
    let mut source = String::from("(begin");
    for name in crate::evaluator::REMOVE
        .iter()
        .filter_map(|name| name.to_str().ok())
    {
        if seen.insert(name) {
            source.push_str(" (varlet (rootlet) '");
            source.push_str(name);
            source.push_str(" '*removed*)");
        }
    }
    source.push(')');
    source
}

pub(crate) fn evaluate_record(record: Word, query: &str) -> String {
    evaluate_record_with(record, query, false)
}

pub(crate) fn evaluate_record_unified(record: Word, query: &str) -> String {
    evaluate_record_with(record, query, true)
}

fn evaluate_record_with(record: Word, query: &str, unified: bool) -> String {
    let mut runs = 0;
    let cache = Arc::new(Mutex::new(HashMap::new()));
    debug!(
        "Evaluating with Rust evaluator ({})",
        query.chars().take(128).collect::<String>()
    );
    loop {
        let lock1 = if runs >= RUNS {
            Some(LOCK.lock().expect("Failed to acquire concurrency lock"))
        } else {
            None
        };
        let (state_old, record_temp) = {
            let _lock2 = if lock1.is_some() {
                None
            } else {
                Some(LOCK.lock().expect("Failed to acquire secondary lock"))
            };
            let state_old = PERSISTOR
                .root_get(record)
                .expect("Failed to get current state");
            let record_temp = PERSISTOR
                .root_temp(state_old)
                .expect("Failed to create temporary record");
            (state_old, record_temp)
        };
        let _record_dropper = CallOnDrop(|| {
            PERSISTOR
                .root_delete(record_temp)
                .expect("Failed to delete temporary record");
        });
        let genesis_branch = PERSISTOR
            .branch_get(state_old)
            .expect("Failed to get genesis branch");
        let genesis_func = PERSISTOR
            .leaf_get(genesis_branch.0)
            .expect("Failed to get genesis function");
        let genesis_str = String::from_utf8_lossy(&genesis_func);
        let persistor = MemoryPersistor::new();
        let (left, right, digest) = PERSISTOR
            .branch_get(state_old)
            .expect("Failed to get state root branch");
        persistor
            .branch_set(left, right, digest)
            .expect("Could not set state root branch to session persistor");
        let external_called = Rc::new(Cell::new(false));
        let session = Rc::new(RefCell::new(RustSession {
            record,
            state: state_old,
            persistor: persistor.clone(),
            cache: cache.clone(),
            external_called: external_called.clone(),
        }));
        let mut host = RustHost::new();
        register_core(&mut host, session);
        if let Some(missing) = host.missing_sync_web_primitives().first() {
            return format!(
                "(error 'configuration-error \"Missing Rust host primitive: {missing}\")"
            );
        }
        if unified {
            host.initialize_with(unified_removed_bindings_source());
        }
        host.initialize_with(PRINT_INITIALIZATION);
        host.initialize_with("(varlet (rootlet) 'sync-eval (lambda* (node (strict #t) :rest rest) (with-let (curlet) ((eval (%sync-loader node strict) (rootlet)) node))))");
        let quoted_query = format!("(quote {}\n)", query);
        let expression = format!(
            "({} (sync-state) (eval-string \"{}\"))",
            genesis_str,
            escape_scheme_string(&quoted_query),
        );
        let expression = wrap_journal_conditions(&expression);
        let result = if unified {
            match host.evaluate_unified_output(&expression) {
                Ok(value) | Err(value) => value,
            }
        } else {
            match host.evaluate(&expression) {
                Ok(value) => value.to_string(),
                Err(error) => error.to_scheme(),
            }
        };
        runs += 1;
        let (output, state_new) = parse_result(&result, state_old);
        if external_called.get() && state_old != state_new {
            return "(error 'external-state-error \"Request called an external function and changed state\")".to_string();
        }
        if state_old == state_new {
            warn_on_error_result(query, &output);
            return output;
        }
        if state_old
            == PERSISTOR
                .root_get(record)
                .expect("Failed to get record state for comparison")
        {
            let _lock2 = if lock1.is_some() {
                None
            } else {
                Some(LOCK.lock().expect("Failed to acquire secondary lock"))
            };
            let source = OverlayPersistor {
                primary: persistor,
                overlay: None,
            };
            if PERSISTOR
                .root_set(record, state_old, state_new, &source)
                .is_ok()
            {
                warn_on_error_result(query, &output);
                return output;
            }
        }
        info!(
            "Rerunning (x{}) due to concurrency collision: {}",
            runs,
            query.chars().take(128).collect::<String>()
        );
    }
}

#[cfg(test)]
mod probes;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn journal_condition_boundary_runs_effects_once_without_replay() {
        let calls = Rc::new(Cell::new(0));
        let callback_calls = calls.clone();
        let mut host = RustHost::new();
        host.register(PrimitiveSpec::fixed("tick", 0, move |_, _| {
            callback_calls.set(callback_calls.get() + 1);
            Ok(HostOutput::Unspecified)
        }));
        let error = host
            .evaluate_unified_output(&wrap_journal_conditions("(begin (tick) (/ 1 0))"))
            .expect("Journal boundary must convert the condition to data");
        assert_eq!(
            error,
            "(error 'division-by-zero \"/: division by zero, (/ 1 0)\" ((data (\"~A: division by zero, (~A ~S ~S)\" / / 1 0))))"
        );
        assert_eq!(calls.get(), 1);
        assert_eq!(
            host.evaluate_unified_output(&wrap_journal_conditions(
                "(begin (tick) (catch #t (lambda () (/ 1 0)) (lambda args 'caught)))"
            )),
            Ok("caught".into())
        );
        assert_eq!(calls.get(), 2);
        assert_eq!(
            host.evaluate_unified_output(&wrap_journal_conditions("(begin (tick) 4)")),
            Ok("4".into())
        );
        assert_eq!(calls.get(), 3);
    }

    #[test]
    fn unified_inventory_includes_indirect_sync_eval() {
        let session = Rc::new(RefCell::new(RustSession {
            record: NULL,
            state: NULL,
            persistor: MemoryPersistor::new(),
            cache: Arc::new(Mutex::new(HashMap::new())),
            external_called: Rc::new(Cell::new(false)),
        }));
        let mut host = RustHost::new();
        register_core(&mut host, session);
        assert!(host.missing_sync_web_primitives().is_empty());
        assert!(!host.registered_primitive_names().contains(&"sync-eval"));
        assert!(!host.registered_primitive_names().contains(&"print"));
        assert!(host.provided_primitive_names().contains(&"sync-eval"));
        assert!(host.provided_primitive_names().contains(&"print"));
        host.initialize_with(PRINT_INITIALIZATION);
        assert_eq!(
            host.evaluate("(let ((x (list 1))) (eq? x (print x)))")
                .unwrap()
                .to_string(),
            "#t"
        );
        assert_eq!(
            host.evaluate_unified_output("(let ((x (list 1))) (eq? x (print x)))"),
            Ok("#t".into())
        );
    }

    #[test]
    fn unified_removed_bindings_are_exact_and_integration_only() {
        let unique = crate::evaluator::REMOVE
            .iter()
            .filter_map(|name| name.to_str().ok())
            .collect::<HashSet<_>>();
        assert_eq!(unique.len(), 82);
        let source = unified_removed_bindings_source();
        assert_eq!(source.matches("(varlet (rootlet)").count(), unique.len());
        for name in unique {
            assert!(source.contains(&format!("'{name} '*removed*")));
        }
    }
}
