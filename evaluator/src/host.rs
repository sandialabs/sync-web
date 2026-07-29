use std::any::Any;
use std::cell::Cell;
use std::collections::HashSet;
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

fn collapse_serialized_syntax_quotes(input:String)->String{let pattern="'(#_quote ";let mut output=String::with_capacity(input.len());let mut offset=0;while let Some(relative)=input[offset..].find(pattern){let start=offset+relative;output.push_str(&input[offset..start]);let mut depth=0i32;let mut quoted=false;let mut escaped=false;let mut end=None;for (index,ch) in input[start+1..].char_indices(){if quoted{if escaped{escaped=false}else if ch=='\\'{escaped=true}else if ch=='"'{quoted=false}continue}match ch{'"'=>quoted=true,'('=>depth+=1,')'=>{depth-=1;if depth==0{end=Some(start+1+index);break}},_=>{}}}let Some(end)=end else{output.push_str(&input[start..]);return output};let content_start=start+pattern.len();output.push_str("''");output.push_str(&input[content_start..end]);offset=end+1;}output.push_str(&input[offset..]);output}
pub(crate) struct HostArgument {
    pub(crate) kind: &'static str,
    pub(crate) boolean: Option<bool>,
    pub(crate) integer: Option<i64>,
    pub(crate) string: Option<String>,
    pub(crate) bytes: Option<Vec<u8>>,
    pub(crate) host: Option<Rc<dyn HostObject>>,
    pub(crate) object_string: String,
    pub(crate) code_string: String,
}

#[derive(Clone, Copy)]
enum BorrowedValueInner<'a> {
    Legacy(&'a Value),
    Unified(&'a HostArgument),
}

#[derive(Clone, Copy)]
pub struct BorrowedValue<'a> {
    inner: BorrowedValueInner<'a>,
}
impl<'a> BorrowedValue<'a> {
    pub(crate) fn legacy(value: &'a Value) -> Self {
        Self { inner: BorrowedValueInner::Legacy(value) }
    }
    pub(crate) fn unified(value: &'a HostArgument) -> Self {
        Self { inner: BorrowedValueInner::Unified(value) }
    }
    pub fn kind(&self) -> &'static str {
        match self.inner {
            BorrowedValueInner::Unified(value) => value.kind,
            BorrowedValueInner::Legacy(value) => match value {
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
            },
        }
    }
    pub fn as_bool(&self) -> Option<bool> {
        match self.inner {
            BorrowedValueInner::Unified(value) => value.boolean,
            BorrowedValueInner::Legacy(Value::Bool(value)) => Some(*value),
            _ => None,
        }
    }
    pub fn as_i64(&self) -> Option<i64> {
        match self.inner {
            BorrowedValueInner::Unified(value) => value.integer,
            BorrowedValueInner::Legacy(Value::Int(value)) => Some(*value),
            _ => None,
        }
    }
    pub fn as_string(&self) -> Option<String> {
        match self.inner {
            BorrowedValueInner::Unified(value) => value.string.clone(),
            BorrowedValueInner::Legacy(Value::String(value)) => Some(value.borrow().clone()),
            _ => None,
        }
    }
    pub fn as_bytes(&self) -> Option<Vec<u8>> {
        match self.inner {
            BorrowedValueInner::Unified(value) => value.bytes.clone(),
            BorrowedValueInner::Legacy(Value::ByteVector(value)) => Some(value.borrow().clone()),
            _ => None,
        }
    }
    pub fn as_host(&self) -> Option<&dyn HostObject> {
        match self.inner {
            BorrowedValueInner::Unified(value) => value.host.as_deref(),
            BorrowedValueInner::Legacy(Value::Host(value)) => Some(value.object.as_ref()),
            _ => None,
        }
    }
    pub fn clone_host(&self) -> Option<Rc<dyn HostObject>> {
        match self.inner {
            BorrowedValueInner::Unified(value) => value.host.clone(),
            BorrowedValueInner::Legacy(Value::Host(value)) => Some(value.object.clone()),
            _ => None,
        }
    }
    pub fn object_string(&self) -> String {
        match self.inner {
            BorrowedValueInner::Unified(value) => value.object_string.clone(),
            BorrowedValueInner::Legacy(value) => value.to_string(),
        }
    }
    pub fn code_string(&self) -> String {
        match self.inner {
            BorrowedValueInner::Unified(value) => value.code_string.clone(),
            BorrowedValueInner::Legacy(value) => {
                let has_syntax=crate::value_has_syntax_origin(value,&mut HashSet::new());
                let cyclic=crate::value_has_pair_cycle(value,&mut HashSet::new(),&mut HashSet::new());
                if has_syntax&&!cyclic{let mut output=crate::printer::code_repr(value);for _ in 0..4{let Ok(mut parsed)=crate::parse_all(&output)else{break};if parsed.len()!=1{break}let normalized=crate::printer::code_repr(&parsed.remove(0));if normalized==output{break}output=normalized;}collapse_serialized_syntax_quotes(output)}else{crate::s7_object_string(value)}
            }
        }
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
    pub(crate) cancelled: bool,
    pub(crate) charge: u64,
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
    pub(crate) name: &'static str,
    pub(crate) min: usize,
    pub(crate) max: Option<usize>,
    pub(crate) callback: HostCallback,
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
            |args, _| Ok(HostOutput::ByteVector(args[0].code_string().into_bytes())),
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
    pub fn evaluate_unified_output(&self, source: &str) -> std::result::Result<String, String> {
        crate::unified_runtime::run_host_source_output(
            source,
            &self.initialization,
            self.primitives.clone(),
            self.cancelled.clone(),
            self.in_callback.clone(),
        )
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
                            .map(BorrowedValue::legacy)
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
                let procedure = Value::Procedure(Rc::new(Procedure::Builtin {
                    name,
                    func: function,
                    min: spec.min,
                    max: spec.max,
                    doc: "Rust host primitive",
                }));
                evaluator.root.define(name, procedure.clone());
                evaluator.global.define(name, procedure);
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
        assert_eq!(
            host.evaluate_unified_output("(let ((n (sync-null))) (list (sync-node? n) (equal? n (sync-null)) n))"),
            Ok("(#t #t #<sync-null>)".into())
        );
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
    fn transient_results_reclaim_environment_cycles() {
        let mut host=RustHost::new();
        host.initialize_with("(define (loop n) (if (= n 0) 0 (loop (- n 1))))");
        for _ in 0..100{let result=host.evaluate("(loop 10)").unwrap();let arena=result.pair_arena_weak();assert_eq!(result.to_string(),"0");drop(result);assert!(arena.upgrade().is_none());}
        let procedure=host.evaluate("(letrec ((loop (lambda (n) (if (= n 0) 0 (loop (- n 1)))))) loop)").unwrap();let arena=procedure.pair_arena_weak();assert!(!procedure.to_string().is_empty());drop(procedure);assert!(arena.upgrade().is_none());
        let environment=host.evaluate("(inlet 'x 1)").unwrap();let arena=environment.pair_arena_weak();assert!(environment.to_string().contains("'x 1"));drop(environment);assert!(arena.upgrade().is_none());
        let error=host.evaluate("(error 'escaped (lambda () 1))").unwrap_err();let arena=error.pair_arena_weak();assert_eq!(error.tag(),"escaped");drop(error);assert!(arena.upgrade().is_none());
    }

    #[test]
    fn nested_host_evaluation_restores_generation_scope() {
        let mut outer=RustHost::new();
        outer.initialize_with("(define (loop n) (if (= n 0) 1 (loop (- n 1))))");
        outer.register(PrimitiveSpec::fixed("nested-evaluate",0,|_,_|{let mut inner=RustHost::new();inner.initialize_with("(define (loop n) (if (= n 0) 6 (loop (- n 1))))");let value=inner.evaluate("(loop 4)").map_err(|error|HostError::new(error.tag(),error.to_scheme()))?;Ok(HostOutput::Int(value.to_string().parse().unwrap()))}));
        for _ in 0..25{let value=outer.evaluate("(+ (loop 4) (nested-evaluate))").unwrap();let arena=value.pair_arena_weak();assert_eq!(value.to_string(),"7");drop(value);assert!(arena.upgrade().is_none());}
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
    fn unified_host_expression_can_be_evaluated_and_applied() {
        let mut host = RustHost::new();
        host.register_codecs();
        host.register(PrimitiveSpec::fixed("loader", 2, |_, _| {
            Ok(HostOutput::Expression(
                "(lambda (state) (define* (self (arg #f)) (if arg arg state)))".into(),
            ))
        }));
        host.register(PrimitiveSpec::fixed("make-node", 0, |_, _| {
            Ok(HostOutput::Host(Rc::new(Node { id: 7, text: "#<sync-node 7>".into() })))
        }));
        assert_eq!(
            host.evaluate_unified_output("(let ((code '(lambda (state) (define* (self (arg #f)) (if arg arg state))))) (byte-vector->expression (expression->byte-vector code)))"),
            Ok("(lambda (state) (define* (self (arg #f)) (if arg arg state)))".into())
        );
        assert_eq!(host.evaluate_unified_output("(loader #f #f)"), Ok("(lambda (state) (define* (self (arg #f)) (if arg arg state)))".into()));
        host.initialize_with("(varlet (rootlet) 'sync-eval (lambda* (node (strict #t) :rest rest) (with-let (curlet) ((eval (loader node strict) (rootlet)) node))))");
        assert_eq!(
            host.evaluate_unified_output("((sync-eval (make-node) #f) 'hello)"),
            Ok("hello".into())
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
        assert_eq!(host.evaluate("(byte-vector->hex-string (expression->byte-vector ''x))").unwrap().to_string(),"\"2778\"");
        assert_eq!(host.evaluate("(byte-vector->hex-string (expression->byte-vector '(quote x)))").unwrap().to_string(),"\"2871756f7465207829\"");
        assert_eq!(host.evaluate("(byte-vector->hex-string (expression->byte-vector '`(a ,x)))").unwrap().to_string(),"\"286c6973742d76616c756573202761207829\"");
        assert_eq!(host.evaluate("(byte-vector->hex-string (expression->byte-vector '`(a . ,x)))").unwrap().to_string(),"\"283c6c6973742a3e20286c6973742d76616c75657320276129207829\"");
        assert_eq!(host.evaluate("(byte-vector->hex-string (expression->byte-vector '(quasiquote (a (unquote x)))))").unwrap().to_string(),"\"28717561736971756f74652028612028756e71756f74652078292929\"");
        assert_eq!(host.evaluate("(byte-vector->hex-string (expression->byte-vector '#(a b)))").unwrap().to_string(),"\"232861206229\"");
        assert_eq!(host.evaluate("(byte-vector->hex-string (let ((x (cons 1 '()))) (set-cdr! x x) (expression->byte-vector x)))").unwrap().to_string(),"\"23313d2831202e2023312329\"");
        assert_eq!(host.evaluate("'`(outer ,x)").unwrap().to_string(),"(list-values 'outer x)");
        assert_eq!(host.evaluate("(byte-vector->hex-string (expression->byte-vector '`(outer `(foo ''(*crypto* public-key)))))").unwrap().to_string(),"\"27286f75746572202728666f6f202727282a63727970746f2a207075626c69632d6b6579292929\"");
        assert_eq!(host.evaluate("(let ((v (list (car ''x) '(*crypto* public-key)))) (byte-vector->hex-string (expression->byte-vector (list (car ''x) v))))").unwrap().to_string(),"\"2727282a63727970746f2a207075626c69632d6b657929\"");
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
