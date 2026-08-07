use std::any::Any;
use std::cell::Cell;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::rc::Rc;

use crate::core::{
    new_pair_arena, with_pair_arena, Procedure, Result as SchemeResult, SchemeError, Value,
};
use crate::{Evaluator, OwnedValue};

pub trait HostObject: Any {
    fn type_name(&self) -> &str;
    fn identity(&self) -> u64;
    fn display(&self) -> String;
    fn equals(&self, other: &dyn HostObject) -> bool {
        self.type_name() == other.type_name() && self.identity() == other.identity()
    }
    fn as_any(&self) -> &dyn Any;
}

pub struct HostValueData {
    pub(crate) object: Rc<dyn HostObject>,
}
pub(crate) fn host_values_equal(left: &HostValueData, right: &HostValueData) -> bool {
    catch_unwind(AssertUnwindSafe(|| {
        left.object.equals(right.object.as_ref())
    }))
    .unwrap_or(false)
}

#[derive(Clone, Copy)]
pub struct BorrowedValue<'a> {
    value: &'a Value,
}
impl<'a> BorrowedValue<'a> {
    pub fn kind(&self) -> &'static str {
        match self.value {
            Value::Bool(_) => "boolean",
            Value::Nil => "nil",
            Value::Int(_) => "integer",
            Value::String(_) => "string",
            Value::ByteVector(_) => "byte-vector",
            Value::Host(_) => "host",
            Value::Pair(_) => "pair",
            Value::HashTable(_) => "hash-table",
            Value::Vector(_) => "vector",
            _ => "object",
        }
    }
    pub fn as_bool(&self) -> Option<bool> {
        if let Value::Bool(value) = self.value {
            Some(*value)
        } else {
            None
        }
    }
    pub fn as_i64(&self) -> Option<i64> {
        if let Value::Int(value) = self.value {
            Some(*value)
        } else {
            None
        }
    }
    pub fn as_string(&self) -> Option<String> {
        if let Value::String(value) = self.value {
            Some(value.borrow().clone())
        } else {
            None
        }
    }
    pub fn as_bytes(&self) -> Option<Vec<u8>> {
        if let Value::ByteVector(value) = self.value {
            Some(value.borrow().clone())
        } else {
            None
        }
    }
    pub fn as_host(&self) -> Option<&dyn HostObject> {
        if let Value::Host(value) = self.value {
            Some(value.object.as_ref())
        } else {
            None
        }
    }
    pub fn clone_host(&self) -> Option<Rc<dyn HostObject>> {
        if let Value::Host(value) = self.value {
            Some(value.object.clone())
        } else {
            None
        }
    }
    pub fn object_string(&self) -> String {
        self.value.to_string()
    }
}

pub enum HostOutput {
    Bool(bool),
    Int(i64),
    String(String),
    ByteVector(Vec<u8>),
    Expression(String),
    List(Vec<HostOutput>),
    Pair(Box<HostOutput>, Box<HostOutput>),
    Apply {
        source: String,
        args: Vec<HostOutput>,
    },
    Host(Rc<dyn HostObject>),
    Nil,
    Unspecified,
}

#[derive(Clone, Debug)]
pub struct HostError {
    pub tag: String,
    pub message: String,
}
impl HostError {
    pub fn new(tag: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            tag: tag.into(),
            message: message.into(),
        }
    }
    pub fn wrong_type(
        name: &str,
        index: usize,
        expected: &str,
        actual: &BorrowedValue<'_>,
    ) -> Self {
        Self::new(
            "wrong-type-arg",
            format!(
                "{name} argument {index} is {} but should be {expected}",
                actual.kind()
            ),
        )
    }
}

pub struct HostCallContext {
    cancelled: bool,
    charge: u64,
}
impl HostCallContext {
    pub fn is_cancelled(&self) -> bool {
        self.cancelled
    }
    pub fn check_cancelled(&self) -> std::result::Result<(), HostError> {
        if self.cancelled {
            Err(HostError::new("interrupted", "host call cancelled"))
        } else {
            Ok(())
        }
    }
    pub fn charge(&mut self, amount: u64) {
        self.charge = self.charge.saturating_add(amount)
    }
}

type HostCallback = Rc<
    dyn for<'a> Fn(
        &[BorrowedValue<'a>],
        &mut HostCallContext,
    ) -> std::result::Result<HostOutput, HostError>,
>;
pub(crate) type HostAdapter = Rc<dyn Fn(&mut Evaluator, &[Value]) -> SchemeResult<Value>>;
macro_rules! host_trampolines {
    ($(($function:ident, $index:expr)),* $(,)?) => {
        $(
            fn $function(evaluator: &mut Evaluator, args: &[Value]) -> SchemeResult<Value> {
                evaluator.invoke_host_primitive($index, args)
            }
        )*

        pub(crate) const HOST_TRAMPOLINES: &[fn(
            &mut Evaluator,
            &[Value],
        ) -> SchemeResult<Value>] = &[$($function),*];
    };
}
host_trampolines!(
    (host_0, 0),
    (host_1, 1),
    (host_2, 2),
    (host_3, 3),
    (host_4, 4),
    (host_5, 5),
    (host_6, 6),
    (host_7, 7),
    (host_8, 8),
    (host_9, 9),
    (host_10, 10),
    (host_11, 11),
    (host_12, 12),
    (host_13, 13),
    (host_14, 14),
    (host_15, 15),
    (host_16, 16),
    (host_17, 17),
    (host_18, 18),
    (host_19, 19),
    (host_20, 20),
    (host_21, 21),
    (host_22, 22),
    (host_23, 23),
    (host_24, 24),
    (host_25, 25),
    (host_26, 26),
    (host_27, 27),
    (host_28, 28),
    (host_29, 29),
    (host_30, 30),
    (host_31, 31)
);
#[derive(Clone)]
pub struct PrimitiveSpec {
    name: &'static str,
    min: usize,
    max: Option<usize>,
    callback: HostCallback,
}
impl PrimitiveSpec {
    pub fn new(
        name: impl Into<String>,
        min: usize,
        max: Option<usize>,
        callback: impl for<'a> Fn(
                &[BorrowedValue<'a>],
                &mut HostCallContext,
            ) -> std::result::Result<HostOutput, HostError>
            + 'static,
    ) -> Self {
        Self {
            name: Box::leak(name.into().into_boxed_str()),
            min,
            max,
            callback: Rc::new(callback),
        }
    }
    pub fn fixed(
        name: impl Into<String>,
        arity: usize,
        callback: impl for<'a> Fn(
                &[BorrowedValue<'a>],
                &mut HostCallContext,
            ) -> std::result::Result<HostOutput, HostError>
            + 'static,
    ) -> Self {
        Self::new(name, arity, Some(arity), callback)
    }
}

pub const SYNC_WEB_HOST_PRIMITIVES: &[&str] = &[
    "sync-stub",
    "sync-hash",
    "sync-state",
    "sync-node?",
    "sync-null",
    "sync-null?",
    "sync-pair?",
    "sync-stub?",
    "sync-digest",
    "sync-cons",
    "sync-car",
    "sync-cdr",
    "sync-cut",
    "sync-create",
    "sync-delete",
    "sync-all",
    "sync-call",
    "sync-eval",
    "sync-http",
    "sync-remote",
    "print",
    "expression->byte-vector",
    "byte-vector->expression",
    "hex-string->byte-vector",
    "byte-vector->hex-string",
    "random-byte-vector",
    "crypto-generate",
    "crypto-sign",
    "crypto-verify",
    "system-time-utc",
    "system-time-unix",
];

pub struct RustHost {
    primitives: Vec<PrimitiveSpec>,
    initialization: Vec<String>,
    cancelled: Rc<Cell<bool>>,
    in_callback: Rc<Cell<bool>>,
}
impl Default for RustHost {
    fn default() -> Self {
        Self::new()
    }
}
impl RustHost {
    pub fn new() -> Self {
        Self {
            primitives: Vec::new(),
            initialization: Vec::new(),
            cancelled: Rc::new(Cell::new(false)),
            in_callback: Rc::new(Cell::new(false)),
        }
    }
    pub fn register(&mut self, spec: PrimitiveSpec) {
        self.primitives.push(spec)
    }
    pub fn register_codecs(&mut self) {
        self.register(PrimitiveSpec::fixed(
            "expression->byte-vector",
            1,
            |args, _| Ok(HostOutput::ByteVector(args[0].object_string().into_bytes())),
        ));
        self.register(PrimitiveSpec::fixed(
            "byte-vector->expression",
            1,
            |args, _| {
                let bytes = args[0].as_bytes().ok_or_else(|| {
                    HostError::wrong_type("byte-vector->expression", 1, "a byte-vector", &args[0])
                })?;
                let source = String::from_utf8(bytes).map_err(|_| {
                    HostError::new("encoding-error", "byte vector string is malformed")
                })?;
                Ok(HostOutput::Expression(source))
            },
        ));
        self.register(PrimitiveSpec::fixed(
            "hex-string->byte-vector",
            1,
            |args, _| {
                let input = args[0].as_string().ok_or_else(|| {
                    HostError::wrong_type("hex-string->byte-vector", 1, "a string", &args[0])
                })?;
                if input.len() % 2 != 0 {
                    return Err(HostError::new(
                        "encoding-error",
                        "hex string has odd length",
                    ));
                }
                let bytes = (0..input.len())
                    .step_by(2)
                    .map(|index| {
                        u8::from_str_radix(&input[index..index + 2], 16)
                            .map_err(|_| HostError::new("encoding-error", "malformed hex string"))
                    })
                    .collect::<std::result::Result<Vec<_>, _>>()?;
                Ok(HostOutput::ByteVector(bytes))
            },
        ));
        self.register(PrimitiveSpec::fixed(
            "byte-vector->hex-string",
            1,
            |args, _| {
                let bytes = args[0].as_bytes().ok_or_else(|| {
                    HostError::wrong_type("byte-vector->hex-string", 1, "a byte-vector", &args[0])
                })?;
                let mut output = String::with_capacity(bytes.len() * 2);
                use std::fmt::Write;
                for byte in bytes {
                    write!(output, "{byte:02x}").unwrap();
                }
                Ok(HostOutput::String(output))
            },
        ));
    }
    pub fn registered_primitive_names(&self) -> Vec<&'static str> {
        self.primitives.iter().map(|spec| spec.name).collect()
    }
    pub fn missing_sync_web_primitives(&self) -> Vec<&'static str> {
        SYNC_WEB_HOST_PRIMITIVES
            .iter()
            .copied()
            .filter(|name| !self.primitives.iter().any(|spec| spec.name == *name))
            .collect()
    }
    pub fn initialize_with(&mut self, source: impl Into<String>) {
        self.initialization.push(source.into())
    }
    pub fn cancel(&self) {
        self.cancelled.set(true)
    }
    pub fn clear_cancellation(&self) {
        self.cancelled.set(false)
    }
    pub fn evaluate(&self, source: &str) -> std::result::Result<OwnedValue, SchemeError> {
        let arena = new_pair_arena();
        let result: SchemeResult<Value> = with_pair_arena(&arena, || {
            let mut evaluator = Evaluator::new();
            for spec in &self.primitives {
                let name = spec.name;
                let callback = spec.callback.clone();
                let cancelled = self.cancelled.clone();
                let in_callback = self.in_callback.clone();
                let adapter = Rc::new(
                    move |evaluator: &mut Evaluator, values: &[Value]| -> SchemeResult<Value> {
                        if in_callback.replace(true) {
                            return Err(SchemeError::new(
                                "host-error",
                                vec![Value::string("reentrant host callbacks are not supported")],
                            ));
                        }
                        struct Reset(Rc<Cell<bool>>);
                        impl Drop for Reset {
                            fn drop(&mut self) {
                                self.0.set(false)
                            }
                        }
                        let _reset = Reset(in_callback.clone());
                        let args = values
                            .iter()
                            .map(|value| BorrowedValue { value })
                            .collect::<Vec<_>>();
                        let mut context = HostCallContext {
                            cancelled: cancelled.get(),
                            charge: 0,
                        };
                        let called =
                            catch_unwind(AssertUnwindSafe(|| callback(&args, &mut context)));
                        drop(_reset);
                        evaluator.charge_host(context.charge)?;
                        match called {
                            Ok(Ok(output)) => output.into_value(evaluator),
                            Ok(Err(error)) => Err(error.into_scheme()),
                            Err(_) => Err(SchemeError::new(
                                "host-error",
                                vec![Value::string(format!("host primitive {name} panicked"))],
                            )),
                        }
                    },
                );
                let index = evaluator.register_host_adapter(adapter);
                let Some(function) = HOST_TRAMPOLINES.get(index).copied() else {
                    return Err(SchemeError::new(
                        "host-error",
                        vec![Value::string("too many host primitives")],
                    ));
                };
                evaluator.global.define(
                    name,
                    Value::Procedure(Rc::new(Procedure::Builtin {
                        name,
                        func: function,
                        min: spec.min,
                        max: spec.max,
                        doc: "Rust host primitive",
                    })),
                );
            }
            for init in &self.initialization {
                for expression in crate::parse_all(init)? {
                    evaluator.eval(expression, evaluator.global.clone())?;
                }
            }
            let mut last = Value::Unspecified;
            for expression in crate::parse_all(source)? {
                last = evaluator.eval(expression, evaluator.global.clone())?;
            }
            Ok(last)
        });
        match result {
            Ok(value) => Ok(OwnedValue {
                value,
                _pair_arena: arena,
            }),
            Err(mut error) => {
                error.retain_pair_arena(arena);
                Err(error)
            }
        }
    }
}

impl HostOutput {
    fn into_value(self, evaluator: &mut Evaluator) -> SchemeResult<Value> {
        Ok(match self {
            Self::Bool(value) => Value::Bool(value),
            Self::Int(value) => Value::Int(value),
            Self::String(value) => Value::string(value),
            Self::ByteVector(value) => Value::ByteVector(Rc::new(std::cell::RefCell::new(value))),
            Self::Expression(source) => crate::parse_all(&source)?
                .into_iter()
                .next()
                .unwrap_or(Value::Nil),
            Self::List(values) => {
                let values = values
                    .into_iter()
                    .map(|value| value.into_value(evaluator))
                    .collect::<SchemeResult<Vec<_>>>()?;
                Value::list(values)
            }
            Self::Pair(first, rest) => {
                Value::cons(first.into_value(evaluator)?, rest.into_value(evaluator)?)
            }
            Self::Apply { source, args } => {
                let arguments = args
                    .into_iter()
                    .map(|value| value.into_value(evaluator))
                    .collect::<SchemeResult<Vec<_>>>()?;
                evaluator.apply_host_source(&source, arguments)?
            }
            Self::Host(object) => Value::Host(Rc::new(HostValueData { object })),
            Self::Nil => Value::Nil,
            Self::Unspecified => Value::Unspecified,
        })
    }
}
impl HostError {
    fn into_scheme(self) -> SchemeError {
        SchemeError::new(self.tag, vec![Value::string(self.message)])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[derive(Clone)]
    struct Node {
        id: u64,
        text: String,
    }
    impl HostObject for Node {
        fn type_name(&self) -> &str {
            "sync-node"
        }
        fn identity(&self) -> u64 {
            self.id
        }
        fn display(&self) -> String {
            self.text.clone()
        }
        fn as_any(&self) -> &dyn Any {
            self
        }
    }
    fn node<'a>(value: &'a BorrowedValue<'a>) -> std::result::Result<&'a Node, HostError> {
        value
            .as_host()
            .and_then(|value| value.as_any().downcast_ref::<Node>())
            .ok_or_else(|| HostError::wrong_type("sync-node", 1, "a sync-node", value))
    }
    fn host() -> RustHost {
        let mut host = RustHost::new();
        host.register(PrimitiveSpec::fixed("sync-null", 0, |_, _| {
            Ok(HostOutput::Host(Rc::new(Node {
                id: 0,
                text: "#<sync-null>".into(),
            })))
        }));
        host.register(PrimitiveSpec::fixed("sync-node?", 1, |args, _| {
            Ok(HostOutput::Bool(
                args[0]
                    .as_host()
                    .map(|value| value.type_name() == "sync-node")
                    .unwrap_or(false),
            ))
        }));
        host.register(PrimitiveSpec::fixed("sync-cons", 2, |args, context| {
            context.charge(2);
            let first = node(&args[0])?;
            let rest = node(&args[1])?;
            Ok(HostOutput::Host(Rc::new(Node {
                id: first
                    .id
                    .wrapping_mul(37)
                    .wrapping_add(rest.id)
                    .wrapping_add(1),
                text: format!("#<sync-pair {} {}>", first.id, rest.id),
            })))
        }));
        host
    }
    #[test]
    fn sync_web_inventory_includes_current_print_primitive() {
        assert!(SYNC_WEB_HOST_PRIMITIVES.contains(&"print"));
        let unique = SYNC_WEB_HOST_PRIMITIVES
            .iter()
            .copied()
            .collect::<std::collections::HashSet<_>>();
        assert_eq!(unique.len(), SYNC_WEB_HOST_PRIMITIVES.len());
    }

    #[test]
    fn native_host_values_have_identity_and_deterministic_printing() {
        let host = host();
        let value = host
            .evaluate("(let ((n (sync-null))) (list (sync-node? n) (equal? n (sync-null)) n))")
            .unwrap();
        assert_eq!(value.to_string(), "(#t #t #<sync-null>)");
    }
    #[cfg(panic = "unwind")]
    #[test]
    fn unwind_profile_converts_host_panics_to_errors() {
        let mut host = host();
        host.register(PrimitiveSpec::fixed(
            "explode",
            0,
            |_, _| -> std::result::Result<HostOutput, HostError> { panic!("host panic") },
        ));
        let wrong = host.evaluate("(sync-cons 1 (sync-null))").unwrap_err();
        assert_eq!(wrong.tag(), "wrong-type-arg");
        let panic = host.evaluate("(explode)").unwrap_err();
        assert_eq!(panic.tag(), "host-error");
        assert!(panic.to_scheme().contains("panicked"));
    }
    fn state_host(state: Rc<std::cell::RefCell<Node>>) -> RustHost {
        let mut host = RustHost::new();
        let read = state.clone();
        host.register(PrimitiveSpec::fixed("sync-state", 0, move |_, _| {
            Ok(HostOutput::Host(Rc::new(read.borrow().clone())))
        }));
        let write = state;
        host.register(PrimitiveSpec::fixed("sync-create", 1, move |args, _| {
            let id = args[0]
                .as_i64()
                .ok_or_else(|| HostError::wrong_type("sync-create", 1, "an integer", &args[0]))?
                as u64;
            *write.borrow_mut() = Node {
                id,
                text: format!("#<sync-node {id}>"),
            };
            Ok(HostOutput::Host(Rc::new(write.borrow().clone())))
        }));
        host
    }
    #[test]
    fn external_state_survives_engine_recreation() {
        let state = Rc::new(std::cell::RefCell::new(Node {
            id: 0,
            text: "#<sync-node 0>".into(),
        }));
        let host = state_host(state.clone());
        assert_eq!(
            host.evaluate("(sync-create 42)").unwrap().to_string(),
            "#<sync-node 42>"
        );
        let persisted = state.borrow().clone();
        drop(host);
        let restarted = state_host(Rc::new(std::cell::RefCell::new(persisted)));
        assert_eq!(
            restarted.evaluate("(sync-state)").unwrap().to_string(),
            "#<sync-node 42>"
        );
    }
    #[test]
    fn native_codecs_round_trip_scheme_and_hex() {
        let mut host = RustHost::new();
        host.register_codecs();
        assert_eq!(
            host.evaluate("(byte-vector->expression (expression->byte-vector '(a 1)))")
                .unwrap()
                .to_string(),
            "(a 1)"
        );
        assert_eq!(
            host.evaluate("(byte-vector->hex-string (hex-string->byte-vector \"00ff10\"))")
                .unwrap()
                .to_string(),
            "\"00ff10\""
        );
    }
    #[test]
    fn controlled_host_apply_releases_callback_guard() {
        let mut host = RustHost::new();
        host.register(PrimitiveSpec::fixed("inner", 0, |_, _| {
            Ok(HostOutput::Bool(true))
        }));
        host.register(PrimitiveSpec::fixed("bridge", 0, |_, _| {
            Ok(HostOutput::Apply {
                source: "(lambda () (inner))".into(),
                args: vec![],
            })
        }));
        host.register(PrimitiveSpec::fixed("shape", 0, |_, _| {
            Ok(HostOutput::List(vec![
                HostOutput::Int(1),
                HostOutput::Pair(Box::new(HostOutput::Int(2)), Box::new(HostOutput::Int(3))),
            ]))
        }));
        assert_eq!(host.evaluate("(bridge)").unwrap().to_string(), "#t");
        assert_eq!(host.evaluate("(shape)").unwrap().to_string(), "(1 (2 . 3))");
    }
    #[test]
    fn cancellation_is_explicit_at_host_boundaries() {
        let mut host = RustHost::new();
        host.register(PrimitiveSpec::fixed("checked", 0, |_, context| {
            context.check_cancelled()?;
            Ok(HostOutput::Bool(true))
        }));
        host.cancel();
        assert_eq!(host.evaluate("(checked)").unwrap_err().tag(), "interrupted");
        host.clear_cancellation();
        assert_eq!(host.evaluate("(checked)").unwrap().to_string(), "#t");
    }
}
