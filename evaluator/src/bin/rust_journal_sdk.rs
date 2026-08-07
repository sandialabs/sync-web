use s7_rust::{
    BorrowedValue, HostError, HostObject, HostOutput, PrimitiveSpec, RustHost,
    SYNC_WEB_HOST_PRIMITIVES,
};
use sha2::{Digest, Sha256};
use std::any::Any;
use std::cell::RefCell;
use std::collections::HashMap;
use std::io::{self, Read};
use std::rc::Rc;

#[derive(Clone)]
enum NodeData {
    Null,
    Leaf(Vec<u8>),
    Pair(SyncNode, SyncNode),
    Stub([u8; 32]),
}
#[derive(Clone)]
struct SyncNode(Rc<NodeData>);
thread_local! {static NODE_STORE:RefCell<HashMap<[u8;32],SyncNode>>=RefCell::new(HashMap::new());}
impl SyncNode {
    fn null() -> Self {
        Self(Rc::new(NodeData::Null))
    }
    fn remember(self) -> Self {
        let digest = self.digest();
        NODE_STORE.with(|store| {
            store.borrow_mut().insert(digest, self.clone());
        });
        self
    }
    fn resolved(&self) -> Self {
        if let NodeData::Stub(digest) = &*self.0 {
            NODE_STORE
                .with(|store| store.borrow().get(digest).cloned())
                .unwrap_or_else(|| self.clone())
        } else {
            self.clone()
        }
    }
    fn leaf(bytes: Vec<u8>) -> Self {
        Self(Rc::new(NodeData::Leaf(bytes))).remember()
    }
    fn pair(first: Self, rest: Self) -> Self {
        Self(Rc::new(NodeData::Pair(first, rest))).remember()
    }
    fn stub(digest: [u8; 32]) -> Self {
        Self(Rc::new(NodeData::Stub(digest)))
    }
    fn digest(&self) -> [u8; 32] {
        match &*self.0 {
            NodeData::Null => [0; 32],
            NodeData::Leaf(bytes) => Sha256::digest(Sha256::digest(bytes)).into(),
            NodeData::Pair(first, rest) => {
                let mut joined = [0u8; 64];
                joined[..32].copy_from_slice(&first.digest());
                joined[32..].copy_from_slice(&rest.digest());
                Sha256::digest(joined).into()
            }
            NodeData::Stub(digest) => *digest,
        }
    }
    fn output(&self) -> HostOutput {
        match &*self.0 {
            NodeData::Leaf(bytes) => HostOutput::ByteVector(bytes.clone()),
            _ => HostOutput::Host(Rc::new(self.clone())),
        }
    }
}
impl HostObject for SyncNode {
    fn type_name(&self) -> &str {
        "sync-node"
    }
    fn identity(&self) -> u64 {
        u64::from_le_bytes(self.digest()[..8].try_into().unwrap())
    }
    fn display(&self) -> String {
        format!(
            "(sync-node #u({}))",
            self.digest()
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
            .map(|other| self.digest() == other.digest())
            .unwrap_or(false)
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}
fn node(value: &BorrowedValue<'_>, name: &str, index: usize) -> Result<SyncNode, HostError> {
    value
        .as_host()
        .and_then(|value| value.as_any().downcast_ref::<SyncNode>())
        .cloned()
        .ok_or_else(|| HostError::wrong_type(name, index, "a sync-node", value))
}
fn node_or_leaf(
    value: &BorrowedValue<'_>,
    name: &str,
    index: usize,
) -> Result<SyncNode, HostError> {
    if let Some(bytes) = value.as_bytes() {
        Ok(SyncNode::leaf(bytes))
    } else {
        node(value, name, index)
    }
}
fn digest_bytes(value: &BorrowedValue<'_>, name: &str) -> Result<[u8; 32], HostError> {
    if let Some(bytes) = value.as_bytes() {
        Ok(Sha256::digest(bytes).into())
    } else {
        Ok(node(value, name, 1)?.digest())
    }
}
fn register_nodes(host: &mut RustHost, state: Rc<RefCell<SyncNode>>) {
    host.register(PrimitiveSpec::fixed("sync-node?", 1, |args, _| {
        Ok(HostOutput::Bool(
            args[0]
                .as_host()
                .and_then(|value| value.as_any().downcast_ref::<SyncNode>())
                .is_some(),
        ))
    }));
    host.register(PrimitiveSpec::fixed("sync-null", 0, |_, _| {
        Ok(HostOutput::Host(Rc::new(SyncNode::null())))
    }));
    host.register(PrimitiveSpec::fixed("sync-null?", 1, |args, _| {
        Ok(HostOutput::Bool(
            node(&args[0], "sync-null?", 1)
                .map(|value| matches!(&*value.0, NodeData::Null))
                .unwrap_or(false),
        ))
    }));
    host.register(PrimitiveSpec::fixed("sync-pair?", 1, |args, _| {
        Ok(HostOutput::Bool(
            node(&args[0], "sync-pair?", 1)
                .map(|value| matches!(&*value.0, NodeData::Pair(..)))
                .unwrap_or(false),
        ))
    }));
    host.register(PrimitiveSpec::fixed("sync-stub?", 1, |args, _| {
        Ok(HostOutput::Bool(
            node(&args[0], "sync-stub?", 1)
                .map(|value| matches!(&*value.0, NodeData::Stub(_)))
                .unwrap_or(false),
        ))
    }));
    host.register(PrimitiveSpec::fixed("sync-cons", 2, |args, _| {
        Ok(HostOutput::Host(Rc::new(SyncNode::pair(
            node_or_leaf(&args[0], "sync-cons", 1)?,
            node_or_leaf(&args[1], "sync-cons", 2)?,
        ))))
    }));
    host.register(PrimitiveSpec::fixed("sync-car", 1, |args, _| {
        let value = node(&args[0], "sync-car", 1)?.resolved();
        if let NodeData::Pair(first, _) = &*value.0 {
            Ok(first.output())
        } else {
            Err(HostError::new(
                "wrong-type-arg",
                "sync-car argument is not a sync pair",
            ))
        }
    }));
    host.register(PrimitiveSpec::fixed("sync-cdr", 1, |args, _| {
        let value = node(&args[0], "sync-cdr", 1)?.resolved();
        if let NodeData::Pair(_, rest) = &*value.0 {
            Ok(rest.output())
        } else {
            Err(HostError::new(
                "wrong-type-arg",
                "sync-cdr argument is not a sync pair",
            ))
        }
    }));
    host.register(PrimitiveSpec::fixed("sync-hash", 1, |args, _| {
        Ok(HostOutput::ByteVector(
            Sha256::digest(
                args[0].as_bytes().ok_or_else(|| {
                    HostError::wrong_type("sync-hash", 1, "a byte-vector", &args[0])
                })?,
            )
            .to_vec(),
        ))
    }));
    host.register(PrimitiveSpec::fixed("sync-digest", 1, |args, _| {
        Ok(HostOutput::ByteVector(
            digest_bytes(&args[0], "sync-digest")?.to_vec(),
        ))
    }));
    host.register(PrimitiveSpec::fixed("sync-cut", 1, |args, _| {
        let digest = if let Some(bytes) = args[0].as_bytes() {
            Sha256::digest(Sha256::digest(bytes)).into()
        } else {
            node(&args[0], "sync-cut", 1)?.digest()
        };
        Ok(HostOutput::Host(Rc::new(SyncNode::stub(digest))))
    }));
    host.register(PrimitiveSpec::fixed("sync-stub", 1, |args, _| {
        let bytes = args[0]
            .as_bytes()
            .ok_or_else(|| HostError::wrong_type("sync-stub", 1, "a byte-vector", &args[0]))?;
        let digest: [u8; 32] = bytes
            .try_into()
            .map_err(|_| HostError::new("wrong-type-arg", "sync-stub requires a 32-byte digest"))?;
        Ok(HostOutput::Host(Rc::new(SyncNode::stub(digest))))
    }));
    let read_state = state;
    host.register(PrimitiveSpec::fixed("sync-state", 0, move |_, _| {
        Ok(HostOutput::Host(Rc::new(read_state.borrow().clone())))
    }));
    host.register(PrimitiveSpec::new("sync-eval", 1, Some(4), |args, _| {
        let value = node(&args[0], "sync-eval", 1)?.resolved();
        let NodeData::Pair(header, _) = &*value.0 else {
            return Err(HostError::new(
                "sync-web-error",
                "sync-eval expects a node with a byte-vector header",
            ));
        };
        let header = header.resolved();
        let NodeData::Leaf(bytes) = &*header.0 else {
            return Err(HostError::new(
                "sync-web-error",
                format!(
                    "sync-eval expects a byte-vector header, got {}",
                    header.display()
                ),
            ));
        };
        let source = String::from_utf8(bytes.clone())
            .map_err(|_| HostError::new("encoding-error", "malformed sync-eval header"))?;
        Ok(HostOutput::Apply {
            source,
            args: vec![HostOutput::Host(Rc::new(value))],
        })
    }));
}
fn main() {
    let mut host = RustHost::new();
    host.register_codecs();
    host.register(PrimitiveSpec::new("stacktrace", 0, None, |_, _| {
        Ok(HostOutput::String("<unavailable>".into()))
    }));
    register_nodes(&mut host, Rc::new(RefCell::new(SyncNode::null())));
    if std::env::args().any(|arg| arg == "--contract-primitive-inventory") {
        for name in SYNC_WEB_HOST_PRIMITIVES {
            println!("{name}");
        }
        return;
    }
    if std::env::args().any(|arg| arg == "--missing-primitive-inventory") {
        for name in host.missing_sync_web_primitives() {
            println!("{name}");
        }
        return;
    }
    if std::env::args().any(|arg| arg == "--primitive-inventory") {
        for name in host.registered_primitive_names() {
            println!("{name}");
        }
        return;
    }
    let mut input = String::new();
    io::stdin().read_to_string(&mut input).unwrap();
    match host.evaluate(&input) {
        Ok(value) => println!("{}", value),
        Err(error) => println!("{}", error.to_scheme()),
    }
}
