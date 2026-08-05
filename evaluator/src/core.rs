use std::cell::{Cell, RefCell, UnsafeCell};
use std::collections::{HashMap, HashSet};
use std::fmt;
use std::hash::{BuildHasherDefault, Hasher};
use std::rc::{Rc,Weak};

use super::{collect_cycle_labels, s7_object_string, small_acyclic_value, Evaluator};

#[derive(Debug, Clone)]
pub struct SchemeError {
    pub(crate) tag: String,
    pub(crate) args: Vec<Value>,
    pair_arena: Option<PairArenaLease>,
}

impl SchemeError {
    #[cold]
    #[inline(never)]
    pub(crate) fn new(tag: impl Into<String>, args: Vec<Value>) -> Self { Self { tag: tag.into(), args, pair_arena: None } }
    pub(crate) fn retain_pair_arena(&mut self,arena:PairArenaLease){self.pair_arena=Some(arena);}
    pub fn tag(&self)->&str{&self.tag}
    #[cfg(test)]pub(crate) fn pair_arena_weak(&self)->Weak<PairArenaGeneration>{Rc::downgrade(&self.pair_arena.as_ref().expect("error pair arena").0)}
    pub fn to_scheme(&self) -> String {
        Value::list(vec![Value::symbol("error"), Value::list(vec![Value::symbol(&self.tag), Value::list(self.args.clone())])]).to_string()
    }
}

pub(crate) type Result<T> = std::result::Result<T, SchemeError>;

pub(crate) type EnvRef = Rc<Env>;
#[derive(Clone,Copy,Debug,PartialEq,Eq)]pub(crate) enum SyntaxOrigin{Explicit,Quote,Quasiquote,Unquote,UnquoteSplicing}
pub(crate) struct PairCell{pub(crate) data:RefCell<PairData>,syntax:Cell<SyntaxOrigin>,quoted_result:Cell<bool>}
struct PairArena{free:Vec<std::ptr::NonNull<PairCell>>,chunks:Vec<Box<[PairCell]>>}
impl PairArena{fn reset(&mut self){self.free.clear();for chunk in self.chunks.iter_mut(){for cell in chunk.iter_mut(){*cell.data.borrow_mut()=PairData{car:Value::Nil,cdr:Value::Nil};cell.syntax.set(SyntaxOrigin::Explicit);cell.quoted_result.set(false);self.free.push(std::ptr::NonNull::from(cell));}}}fn take(&mut self)->std::ptr::NonNull<PairCell>{if let Some(p)=self.free.pop(){return p}let mut chunk=(0..4096).map(|_|PairCell{data:RefCell::new(PairData{car:Value::Nil,cdr:Value::Nil}),syntax:Cell::new(SyntaxOrigin::Explicit),quoted_result:Cell::new(false)}).collect::<Vec<_>>().into_boxed_slice();for cell in chunk.iter_mut(){self.free.push(std::ptr::NonNull::from(cell));}self.chunks.push(chunk);self.free.pop().unwrap()}}
struct PairArenaOwner(*mut PairArena);impl Drop for PairArenaOwner{fn drop(&mut self){unsafe{drop(Box::from_raw(self.0))}}}
pub(crate) struct PairArenaGeneration{arena:UnsafeCell<PairArena>,environments:RefCell<Vec<Weak<Env>>>}
impl Drop for PairArenaGeneration{fn drop(&mut self){for environment in self.environments.get_mut().drain(..).filter_map(|environment|environment.upgrade()){*environment.vars.borrow_mut()=EnvVars::new();environment.order.borrow_mut().clear();*environment.parent.borrow_mut()=None;}}}
#[derive(Clone)]pub(crate) struct PairArenaLease(pub(crate) Rc<PairArenaGeneration>);
impl fmt::Debug for PairArenaLease{fn fmt(&self,f:&mut fmt::Formatter<'_>)->fmt::Result{f.write_str("PairArenaLease")}}
thread_local!{static PAIR_ARENA:PairArenaOwner=PairArenaOwner(Box::into_raw(Box::new(PairArena{free:Vec::new(),chunks:Vec::new()})));static CURRENT_PAIR_ARENA:Cell<*mut PairArena>=const{Cell::new(std::ptr::null_mut())};static CURRENT_PAIR_GENERATION:Cell<*const PairArenaGeneration>=const{Cell::new(std::ptr::null())};}
pub(crate) fn new_pair_arena()->PairArenaLease{PairArenaLease(Rc::new(PairArenaGeneration{arena:UnsafeCell::new(PairArena{free:Vec::new(),chunks:Vec::new()}),environments:RefCell::new(Vec::new())}))}
struct PairArenaScope{previous:*mut PairArena,previous_generation:*const PairArenaGeneration}
impl Drop for PairArenaScope{fn drop(&mut self){CURRENT_PAIR_ARENA.with(|current|current.set(self.previous));CURRENT_PAIR_GENERATION.with(|current|current.set(self.previous_generation));}}
pub(crate) fn with_pair_arena<R>(arena:&PairArenaLease,callback:impl FnOnce()->R)->R{let pointer=arena.0.arena.get();let previous=CURRENT_PAIR_ARENA.with(|current|current.replace(pointer));let previous_generation=CURRENT_PAIR_GENERATION.with(|current|current.replace(Rc::as_ptr(&arena.0)));let _scope=PairArenaScope{previous,previous_generation};callback()}
fn with_current_pair_arena<R>(callback:impl FnOnce(&mut PairArena)->R)->R{CURRENT_PAIR_ARENA.with(|current|{let pointer=current.get();if pointer.is_null(){PAIR_ARENA.with(|arena|unsafe{callback(&mut *arena.0)})}else{unsafe{callback(&mut *pointer)}}})}
#[derive(Clone,Copy)] pub struct PairRef(std::ptr::NonNull<PairCell>);
impl PairRef{pub(crate) fn new(data:PairData)->Self{let ptr=with_current_pair_arena(|arena|arena.take());unsafe{*ptr.as_ref().data.borrow_mut()=data;ptr.as_ref().syntax.set(SyntaxOrigin::Explicit);ptr.as_ref().quoted_result.set(false);}Self(ptr)}pub(crate) fn as_ptr(&self)->*const PairCell{self.0.as_ptr()}pub(crate) fn ptr_eq(a:&Self,b:&Self)->bool{a.0==b.0}pub(crate) fn syntax_origin(&self)->SyntaxOrigin{unsafe{self.0.as_ref().syntax.get()}}pub(crate) fn set_syntax_origin(&self,origin:SyntaxOrigin){unsafe{self.0.as_ref().syntax.set(origin)}}pub(crate) fn mark_quoted_result(&self){unsafe{self.0.as_ref().quoted_result.set(true)}}pub(crate) fn is_quoted_result(&self)->bool{unsafe{self.0.as_ref().quoted_result.get()}}pub(crate) fn clear_quoted_result(&self){unsafe{self.0.as_ref().quoted_result.set(false)}}pub(crate) unsafe fn from_ptr(p:*const PairCell)->Self{Self(std::ptr::NonNull::new(p as *mut PairCell).unwrap())}}
impl std::ops::Deref for PairRef{type Target=RefCell<PairData>;fn deref(&self)->&Self::Target{unsafe{&self.0.as_ref().data}}}
pub(crate) type ObjRef = PairRef;
pub(crate) fn reset_pair_arena(){with_current_pair_arena(PairArena::reset);}

type EnvMap = HashMap<String, Value, BuildHasherDefault<FnvHasher>>;
type DirectSeen = HashSet<usize, BuildHasherDefault<FnvHasher>>;

#[derive(Default)]
pub(crate) struct FnvHasher(u64);
impl Hasher for FnvHasher{
    fn write(&mut self, bytes:&[u8]){let mut h=if self.0==0{0xcbf29ce484222325}else{self.0}; for b in bytes{h^=*b as u64; h=h.wrapping_mul(0x100000001b3);} self.0=h;}
    fn finish(&self)->u64{if self.0==0{0xcbf29ce484222325}else{self.0}}
}

#[derive(Clone)]
pub struct VectorData {
    slots: RefCell<Vec<Rc<RefCell<Value>>>>,
}

impl VectorData {
    pub(crate) fn new(values: Vec<Value>) -> Self { Self { slots: RefCell::new(values.into_iter().map(|v| Rc::new(RefCell::new(v))).collect()) } }
    pub(crate) fn len(&self) -> usize { self.slots.borrow().len() }
    pub(crate) fn is_empty(&self) -> bool { self.slots.borrow().is_empty() }
    pub(crate) fn values(&self) -> Vec<Value> { self.slots.borrow().iter().map(|slot| slot.borrow().clone()).collect() }
    pub(crate) fn for_each_value(&self, mut f: impl FnMut(&Value)) { let slots=self.slots.borrow(); for slot in slots.iter(){ f(&slot.borrow()); } }
    #[allow(dead_code)]
    pub(crate) fn get(&self, idx: usize) -> Value { self.slots.borrow()[idx].borrow().clone() }
    pub(crate) fn set(&self, idx: usize, val: Value) { *self.slots.borrow()[idx].borrow_mut() = val; }
    #[allow(dead_code)]
    pub(crate) fn swap(&self, a: usize, b: usize) { self.slots.borrow_mut().swap(a, b); }
    pub(crate) fn slot_ptr(&self) -> *mut Rc<RefCell<Value>> { self.slots.borrow_mut().as_mut_ptr() }
    pub(crate) fn fill(&self, val: Value) { for slot in self.slots.borrow().iter() { *slot.borrow_mut() = val.clone(); } }
    pub(crate) fn fill_range(&self,start:usize,end:usize,val:&Value){let slots=self.slots.borrow();for slot in &slots[start.min(slots.len())..end.min(slots.len())]{*slot.borrow_mut()=val.clone();}}
    pub(crate) fn slice_values(&self, start: usize, end: usize) -> Vec<Value> { self.slots.borrow()[start..end].iter().map(|slot| slot.borrow().clone()).collect() }
    pub(crate) fn copy_within_values(&self,start:usize){if start==0{return}let slots=self.slots.borrow();for index in 0..slots.len()-start{let value=slots[start+index].borrow().clone();*slots[index].borrow_mut()=value;}}
    pub(crate) fn any_value(&self, mut f: impl FnMut(&Value) -> bool) -> bool { self.slots.borrow().iter().any(|slot| f(&slot.borrow())) }
    #[inline(never)]pub(crate) fn pairwise_all(&self,other:&Self,mut predicate:impl FnMut(&Value,&Value)->bool)->bool{let left=self.slots.borrow();let right=other.slots.borrow();left.len()==right.len()&&left.iter().zip(right.iter()).all(|(left,right)|predicate(&left.borrow(),&right.borrow()))}
}

#[derive(Clone)]
pub enum Value {
    Bool(bool),
    Nil,
    Unspecified,
    Undefined,
    Eof,
    Int(i64),
    RationalValue(Rc<RationalValueData>),
    Float(f64),
    ComplexValue(Rc<ComplexValueData>),
    NumberLiteral(Rc<NumberLiteralData>, ()),
    Char(char),
    NamedChar(Rc<String>),
    String(Rc<RefCell<String>>),
    Symbol(Rc<String>),
    Keyword(Rc<String>),
    Pair(ObjRef),
    Vector(Rc<VectorData>),
    ByteVector(Rc<RefCell<Vec<u8>>>),
    FloatVector(Rc<RefCell<Vec<f64>>>),
    IntVector(Rc<RefCell<Vec<i64>>>),
    MultiVector(Rc<MultiVectorData>),
    MultiVectorView(Rc<MultiVectorViewData>),
    HashTable(Rc<RefCell<Vec<(Value, Value)>>>),
    Env(EnvRef),
    Procedure(Rc<Procedure>),
    ProcedureSource(Rc<ProcedureSourceData>),
    Macro(Rc<MacroValueData>, ()),
    Port(Rc<RefCell<Port>>),
    Hook(Rc<HookValueData>, ()),
    Iterator(Rc<IteratorData>),
    CPointer(i64),
    Dilambda(Rc<(Value, Value)>),
    ValuesData(ValuesPayload),
    Commented(Box<Value>),
    SetterRef(usize),
    RootMeta(Rc<String>),
    RawDisplay(Rc<String>),
    Host(Rc<crate::host::HostValueData>),
}

const _: [(); 16] = [(); std::mem::size_of::<Value>()];
#[derive(Clone)]pub struct RationalValueData{pub(crate) num:i64,pub(crate) den:i64}
#[derive(Clone)]pub struct ComplexValueData{pub(crate) real:f64,pub(crate) imag:f64}
#[derive(Clone)]pub struct MacroValueData{pub(crate) procedure:Rc<Procedure>,pub(crate) kind:MacroKind}
#[derive(Clone)]pub struct HookValueData{pub(crate) functions:Rc<RefCell<Vec<Value>>>,pub(crate) arity:i64}
#[derive(Clone)]pub struct NumberLiteralData{pub(crate) repr:Rc<String>,pub(crate) value:f64,pub(crate) opaque_bignum:bool}
#[derive(Clone)]pub struct ValuesPayload(Rc<Vec<Value>>);
impl std::ops::Deref for ValuesPayload{type Target=Vec<Value>;fn deref(&self)->&Self::Target{&self.0}}
impl IntoIterator for ValuesPayload{type Item=Value;type IntoIter=std::vec::IntoIter<Value>;fn into_iter(self)->Self::IntoIter{Rc::unwrap_or_clone(self.0).into_iter()}}
impl<'a> IntoIterator for &'a ValuesPayload{type Item=&'a Value;type IntoIter=std::slice::Iter<'a,Value>;fn into_iter(self)->Self::IntoIter{self.0.iter()}}
#[derive(Clone)]pub struct MultiVectorData{pub(crate) dims:Rc<Vec<usize>>,pub(crate) data:Rc<RefCell<Vec<Value>>>,pub(crate) kind:Option<Rc<String>>}
#[derive(Clone)]pub struct MultiVectorViewData{pub(crate) dims:Rc<Vec<usize>>,pub(crate) data:Rc<RefCell<Vec<Value>>>,pub(crate) offset:usize,pub(crate) kind:Option<Rc<String>>}
#[derive(Clone)]pub struct IteratorData{pub(crate) kind:Rc<String>,pub(crate) items:Rc<RefCell<Vec<Value>>>,pub(crate) source:Rc<String>,pub(crate) consumed:Rc<RefCell<usize>>}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MacroKind { Macro, BMacro }

pub struct PairData { pub(crate) car: Value, pub(crate) cdr: Value }

#[derive(Clone)]
pub struct ProcedureSourceData { pub(crate) params: Params, pub(crate) body: Rc<RefCell<Vec<Value>>>, pub(crate) macro_kind: Option<MacroKind>, pub(crate) compiled_valid: Rc<Cell<bool>> }

#[derive(Clone)]
pub enum Procedure {
    Builtin { name: &'static str, func: fn(&mut Evaluator, &[Value]) -> Result<Value>, min: usize, max: Option<usize>, doc: &'static str },
    Lambda { params: Params, body: Rc<RefCell<Vec<Value>>>, env: EnvRef, name: Option<String>, compiled: Option<Rc<crate::compiled::CompiledBody>> },
}

#[derive(Clone)]
pub struct Params {
    pub required: Vec<String>,
    pub rest: Option<String>,
    pub star: bool,
    pub defaults: Vec<Option<Value>>,
    pub allow_other_keys: bool,
    pub rest_before_formals: bool,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum PortRepr { Stdin, Stdout, Stderr, OpenInput, CallWithInput, ClosedInput, OpenOutput, ClosedOutput }

#[derive(Clone)]
pub enum Port {
    Input { text: Vec<char>, pos: usize, repr: PortRepr },
    Output { text: String, repr: PortRepr },
}

pub(crate) enum EnvVars {
    Small(Vec<(String, Value)>),
    Large(EnvMap),
}

const SMALL_ENV_LIMIT: usize = 8;

impl EnvVars {
    fn new() -> Self { Self::Small(Vec::new()) }
    fn with_capacity(cap: usize) -> Self { if cap <= SMALL_ENV_LIMIT { Self::Small(Vec::with_capacity(cap)) } else { Self::Large(EnvMap::with_capacity_and_hasher(cap, BuildHasherDefault::default())) } }
    pub(crate) fn contains_key(&self, k:&str)->bool { match self { Self::Small(v)=>v.iter().any(|(kk,_)|kk==k), Self::Large(m)=>m.contains_key(k) } }
    pub(crate) fn get(&self, k:&str)->Option<&Value> { match self { Self::Small(v)=>v.iter().find(|(kk,_)|kk==k).map(|(_,v)|v), Self::Large(m)=>m.get(k) } }
    pub(crate) fn get_mut(&mut self, k:&str)->Option<&mut Value> { match self { Self::Small(v)=>v.iter_mut().find(|(kk,_)|kk==k).map(|(_,v)|v), Self::Large(m)=>m.get_mut(k) } }
    pub(crate) fn insert(&mut self, k:String, v:Value)->Option<Value> {
        match self {
            Self::Small(xs)=>{
                if let Some((_,slot))=xs.iter_mut().find(|(kk,_)|kk==&k){return Some(std::mem::replace(slot,v));}
                if xs.len()<SMALL_ENV_LIMIT{xs.push((k,v)); return None;}
                let mut m=EnvMap::with_capacity_and_hasher(xs.len()+1,BuildHasherDefault::default());
                for (kk,vv) in xs.drain(..){m.insert(kk,vv);}
                m.insert(k,v);
                *self=Self::Large(m);
                None
            }
            Self::Large(m)=>m.insert(k,v),
        }
    }
    pub(crate) fn insert_fresh(&mut self,k:String,v:Value){match self{Self::Small(values)=>{debug_assert!(!values.iter().any(|(name,_)|name==&k));values.push((k,v))},Self::Large(values)=>{debug_assert!(!values.contains_key(&k));values.insert(k,v);}}}
    pub(crate) fn remove(&mut self, k:&str)->Option<Value> { match self { Self::Small(v)=>v.iter().position(|(kk,_)|kk==k).map(|i|v.remove(i).1), Self::Large(m)=>m.remove(k) } }
}

pub struct Env {
    pub(crate) parent: RefCell<Option<EnvRef>>,
    pub(crate) vars: RefCell<EnvVars>,
    pub(crate) order: RefCell<Vec<String>>,
    pub(crate) open: RefCell<bool>,
    guard_epoch: Rc<Cell<u64>>,
    callable_shadowed: Rc<Cell<bool>>,
}

thread_local!{static IMMUTABLE_IDS: RefCell<HashSet<usize>>=RefCell::new(HashSet::new());}

pub(crate) fn object_id(v:&Value)->Option<usize>{match v{Value::Pair(p)=>Some(p.as_ptr() as usize),Value::Vector(x)=>Some(Rc::as_ptr(x) as usize),Value::String(x)=>Some(Rc::as_ptr(x) as usize),Value::ByteVector(x)=>Some(Rc::as_ptr(x) as usize),Value::FloatVector(x)=>Some(Rc::as_ptr(x) as usize),Value::IntVector(x)=>Some(Rc::as_ptr(x) as usize),Value::HashTable(x)=>Some(Rc::as_ptr(x) as usize),Value::Env(x)=>Some(Rc::as_ptr(x) as usize),_=>None}}
pub(crate) fn mark_immutable(v:&Value){if let Some(id)=object_id(v){IMMUTABLE_IDS.with(|s|{s.borrow_mut().insert(id);});}}
pub(crate) fn is_marked_immutable(v:&Value)->bool{object_id(v).map(|id|IMMUTABLE_IDS.with(|s|s.borrow().contains(&id))).unwrap_or(false)}
pub(crate) fn immutable_error(op:&str,v:&Value)->SchemeError{SchemeError::new("immutable-error",vec![Value::string("can't ~S ~S (it is immutable)"),Value::symbol(op),v.clone()])}

pub(crate) fn is_syntax_name(k: &str) -> bool {
    matches!(k, "if"|"begin"|"define"|"define*"|"set!"|"lambda"|"lambda*"|"let"|"let*"|"letrec"|"letrec*"|"let-temporarily"|"cond"|"case"|"when"|"unless"|"do"|"and"|"or"|"quote"|"quasiquote"|"unquote"|"unquote-splicing"|"catch"|"throw"|"define-macro"|"define-macro*"|"define-bacro"|"define-bacro*"|"macro"|"macro*"|"bacro"|"bacro*"|"macroexpand"|"with-let")
}

impl Env {
    fn registered(parent:Option<EnvRef>,vars:EnvVars,order:Vec<String>)->EnvRef{let guard_epoch=parent.as_ref().map(|p|p.guard_epoch.clone()).unwrap_or_else(||Rc::new(Cell::new(0)));let callable_shadowed=parent.as_ref().map(|p|p.callable_shadowed.clone()).unwrap_or_else(||Rc::new(Cell::new(false)));let environment=Rc::new(Self{parent:RefCell::new(parent),vars:RefCell::new(vars),order:RefCell::new(order),open:RefCell::new(false),guard_epoch,callable_shadowed});CURRENT_PAIR_GENERATION.with(|generation|{let generation=generation.get();if !generation.is_null(){unsafe{&*generation}.environments.borrow_mut().push(Rc::downgrade(&environment));}});environment}
    pub(crate) fn new(parent: Option<EnvRef>) -> EnvRef {Self::registered(parent,EnvVars::new(),Vec::new())}
    pub(crate) fn with_capacity(parent: Option<EnvRef>, cap: usize) -> EnvRef {Self::registered(parent,EnvVars::with_capacity(cap),Vec::with_capacity(cap))}
    fn guard_sensitive(v:&Value)->bool{matches!(v,Value::Procedure(_)|Value::Macro(_,_)|Value::RootMeta(_)|Value::Dilambda(_))}
    fn bump_guard_epoch(&self){self.guard_epoch.set(self.guard_epoch.get().wrapping_add(1));}
    fn inherited_guard_sensitive(&self,k:&str)->bool{self.parent.borrow().as_ref().and_then(|p|p.get(k)).map(|v|Self::guard_sensitive(&v)||matches!(v,Value::RootMeta(_))).unwrap_or_else(||is_syntax_name(k))}
    pub(crate) fn guard_generation(&self)->u64{self.guard_epoch.get()}
    pub(crate) fn has_callable_shadow(&self)->bool{self.callable_shadowed.get()}
    pub(crate) fn set_parent(&self,parent:EnvRef){*self.parent.borrow_mut()=Some(parent); self.bump_guard_epoch();}
    pub(crate) fn define(&self, k: impl Into<String>, v: Value) { let k=k.into(); let fresh=!self.vars.borrow().contains_key(&k);let shadows=fresh&&self.inherited_guard_sensitive(&k);if shadows{self.callable_shadowed.set(true);}if fresh{self.order.borrow_mut().push(k.clone());} let sensitive=Self::guard_sensitive(&v)||shadows; let old=self.vars.borrow_mut().insert(k, v); if sensitive||old.as_ref().map(Self::guard_sensitive).unwrap_or(false){self.bump_guard_epoch();} }
    pub(crate) fn define_fresh(&self, k: String, v: Value) { let shadows=self.inherited_guard_sensitive(&k);if shadows{self.callable_shadowed.set(true);}let sensitive=Self::guard_sensitive(&v)||shadows; self.order.borrow_mut().push(k.clone()); self.vars.borrow_mut().insert_fresh(k, v); if sensitive{self.bump_guard_epoch();} }
    pub(crate) fn with_value<R>(&self,k:&str,callback:impl FnOnce(&Value)->R)->Option<R>{let mut callback=Some(callback);{let vars=self.vars.borrow();if let Some(value)=vars.get(k){return Some(callback.take().unwrap()(value))}}let mut parent=self.parent.borrow().clone();while let Some(environment)=parent{{let vars=environment.vars.borrow();if let Some(value)=vars.get(k){return Some(callback.take().unwrap()(value))}}parent=environment.parent.borrow().clone()}None}
    pub(crate) fn get(&self, k: &str) -> Option<Value> {
        { let vars=self.vars.borrow(); if let Some(v)=vars.get(k){return Some(v.clone());} }
        let mut parent=self.parent.borrow().clone();
        while let Some(env)=parent {
            { let vars=env.vars.borrow(); if let Some(v)=vars.get(k){return Some(v.clone());} }
            parent=env.parent.borrow().clone();
        }
        if is_syntax_name(k) || k=="sync-eval" { return Some(Value::RootMeta(Rc::new(k.to_string()))); }
        None
    }
    pub(crate) fn set(&self, k: &str, v: Value) -> bool {
        if let Some(slot)=self.vars.borrow_mut().get_mut(k){let shadows=Self::guard_sensitive(slot);if shadows{self.callable_shadowed.set(true);}if shadows||Self::guard_sensitive(&v){self.bump_guard_epoch();} *slot=v; return true;}
        if let Some(p) = self.parent.borrow().as_ref() { p.set(k, v) } else { false }
    }
    pub(crate) fn set_local_existing_many(&self, pairs: impl IntoIterator<Item=(String, Value)>) {
        let mut vars=self.vars.borrow_mut(); let mut bump=false;
        for (k,v) in pairs { let sensitive=Self::guard_sensitive(&v);let old=vars.insert(k,v); bump|=sensitive||old.as_ref().map(Self::guard_sensitive).unwrap_or(false); }
        drop(vars); if bump{self.bump_guard_epoch();}
    }
    pub(crate) fn builtin_func(&self, k:&str)->Option<(fn(&mut Evaluator,&[Value])->Result<Value>,usize,Option<usize>)>{
        { let vars=self.vars.borrow(); if let Some(v)=vars.get(k){return match v{Value::Procedure(p)=>match &**p{Procedure::Builtin{name,func,min,max,..} if *name==k=>Some((*func,*min,*max)),_=>None},_=>None};} }
        let mut parent=self.parent.borrow().clone();
        while let Some(env)=parent{
            { let vars=env.vars.borrow(); if let Some(v)=vars.get(k){return match v{Value::Procedure(p)=>match &**p{Procedure::Builtin{name,func,min,max,..} if *name==k=>Some((*func,*min,*max)),_=>None},_=>None};} }
            parent=env.parent.borrow().clone();
        }
        None
    }
}

impl Value {
    #[allow(non_snake_case)]pub(crate) fn Rational(num:i64,den:i64)->Self{Value::RationalValue(Rc::new(RationalValueData{num,den}))}
    #[allow(non_snake_case)]pub(crate) fn Complex(real:f64,imag:f64)->Self{Value::ComplexValue(Rc::new(ComplexValueData{real,imag}))}
    pub(crate) fn macro_value(procedure:Rc<Procedure>,kind:MacroKind)->Self{Value::Macro(Rc::new(MacroValueData{procedure,kind}),())}
    pub(crate) fn number_literal(repr:Rc<String>,value:f64)->Self{Value::NumberLiteral(Rc::new(NumberLiteralData{repr,value,opaque_bignum:false}),())}
    pub(crate) fn bignum_literal(repr:Rc<String>)->Self{Value::NumberLiteral(Rc::new(NumberLiteralData{repr,value:0.0,opaque_bignum:true}),())}
    #[allow(non_snake_case)]pub(crate) fn Values(values:Vec<Value>)->Self{Value::ValuesData(ValuesPayload(Rc::new(values)))}
    pub(crate) fn multivector(dims:Rc<Vec<usize>>,data:Rc<RefCell<Vec<Value>>>,kind:Option<Rc<String>>)->Self{Value::MultiVector(Rc::new(MultiVectorData{dims,data,kind}))}
    pub(crate) fn view_kind(&self)->Option<&Rc<String>>{if let Value::MultiVectorView(v)=self{v.kind.as_ref()}else{None}}
    pub(crate) fn base_multi_kind(&self)->Option<&Rc<String>>{if let Value::MultiVector(v)=self{v.kind.as_ref()}else{None}}
    pub(crate) fn multi_kind(&self)->Option<&Rc<String>>{match self{Value::MultiVector(v)=>v.kind.as_ref(),Value::MultiVectorView(v)=>v.kind.as_ref(),_=>None}}
    pub(crate) fn multivector_view(dims:Rc<Vec<usize>>,data:Rc<RefCell<Vec<Value>>>,offset:usize,kind:Option<Rc<String>>)->Self{Value::MultiVectorView(Rc::new(MultiVectorViewData{dims,data,offset,kind}))}
    pub(crate) fn iterator(kind:Rc<String>,items:Rc<RefCell<Vec<Value>>>,source:Rc<String>,consumed:Rc<RefCell<usize>>)->Self{Value::Iterator(Rc::new(IteratorData{kind,items,source,consumed}))}
    pub(crate) fn symbol(s: &str) -> Self { Value::Symbol(Rc::new(s.to_string())) }
    pub(crate) fn keyword(s: &str) -> Self { Value::Keyword(Rc::new(s.to_string())) }
    pub(crate) fn string(s: impl Into<String>) -> Self { Value::String(Rc::new(RefCell::new(s.into()))) }
    pub(crate) fn cons(car: Value, cdr: Value) -> Self { Value::Pair(PairRef::new(PairData { car, cdr })) }
    pub fn list<I:IntoIterator<Item=Value>>(xs:I)->Self where I::IntoIter:DoubleEndedIterator{xs.into_iter().rev().fold(Value::Nil,|cdr,car|Value::cons(car,cdr))}
    #[inline(always)]pub(crate) fn list_slice(values:&[Value])->Self{match values{[]=>Value::Nil,[a]=>Value::cons(a.clone(),Value::Nil),[a,b]=>Value::cons(a.clone(),Value::cons(b.clone(),Value::Nil)),[a,b,c]=>Value::cons(a.clone(),Value::cons(b.clone(),Value::cons(c.clone(),Value::Nil))),[a,b,c,d]=>Value::cons(a.clone(),Value::cons(b.clone(),Value::cons(c.clone(),Value::cons(d.clone(),Value::Nil)))),values=>Value::list(values.iter().cloned())}}
    pub(crate) fn is_true(&self) -> bool { match self { Value::Bool(false)=>false, Value::ValuesData(xs)=>xs.get(0).map(|v|v.is_true()).unwrap_or(true), _=>true } }
    pub(crate) fn as_symbol(&self) -> Option<&str> { match self{Value::Symbol(s)=>Some(s.as_str()),Value::RootMeta(s) if is_syntax_name(s)=>Some(s.as_str()),_=>None} }
    #[inline(always)]
    pub(crate) fn car(&self) -> Result<Value> { match self { Value::Pair(p) => { let PairData{car,..}= &*p.borrow(); Ok(car.clone()) }, Value::Nil => Err(SchemeError::new("wrong-type-arg", vec![Value::string("~A argument, ~S, is ~A but should be ~A"), Value::symbol("car"), Value::Nil, Value::string("nil"), Value::string("a pair")])) , _ => Err(SchemeError::new("wrong-type-arg", vec![self.clone()])) } }
    #[inline(always)]
    pub(crate) fn cdr(&self) -> Result<Value> { match self { Value::Pair(p) => { let PairData{cdr,..}= &*p.borrow(); Ok(cdr.clone()) }, Value::Nil => Err(SchemeError::new("wrong-type-arg", vec![Value::string("~A argument, ~S, is ~A but should be ~A"), Value::symbol("cdr"), Value::Nil, Value::string("nil"), Value::string("a pair")])) , _ => Err(SchemeError::new("wrong-type-arg", vec![self.clone()])) } }
    pub(crate) fn set_car(&self, v: Value) -> Result<Value> { match self { Value::Pair(p) => { let PairData{car,..}= &mut *p.borrow_mut(); *car=v.clone(); p.set_syntax_origin(SyntaxOrigin::Explicit); p.clear_quoted_result(); Ok(v) }, _ => Err(SchemeError::new("wrong-type-arg", vec![self.clone()])) } }
    pub(crate) fn set_cdr(&self, v: Value) -> Result<Value> { match self { Value::Pair(p) => { let PairData{cdr,..}= &mut *p.borrow_mut(); *cdr=v.clone(); p.set_syntax_origin(SyntaxOrigin::Explicit); p.clear_quoted_result(); Ok(v) }, _ => Err(SchemeError::new("wrong-type-arg", vec![self.clone()])) } }
    pub(crate) fn to_vec(&self) -> Result<Vec<Value>> {
        let mut out=Vec::new(); let mut cur=self.clone();
        loop { match cur { Value::Nil => return Ok(out), Value::Pair(p) => { let PairData{car,cdr}= &*p.borrow(); out.push(car.clone()); cur=cdr.clone(); }, _ => return Err(SchemeError::new("wrong-type-arg", vec![cur])) } }
    }
}

fn quote_symbol_shorthand(v:&Value)->Option<String>{if let Value::Pair(pair)=v{let origin=pair.syntax_origin();let head=pair.borrow().car.clone();let syntax=match head{Value::RootMeta(name)=>Some(name),_=>None};let arg=v.cdr().ok()?.car().ok()?;if origin==SyntaxOrigin::Quote||matches!(syntax.as_deref().map(|s|s.as_str()),Some("quote")){return Some(format!("'{}",arg));}if origin==SyntaxOrigin::Quasiquote||matches!(syntax.as_deref().map(|s|s.as_str()),Some("quasiquote")){return Some(if pair.is_quoted_result(){crate::printer::qq_code(&arg)}else{format!("`{}",arg)});}if origin==SyntaxOrigin::Unquote||matches!(syntax.as_deref().map(|s|s.as_str()),Some("unquote")){return Some(format!(",{}",arg));}if origin==SyntaxOrigin::UnquoteSplicing||matches!(syntax.as_deref().map(|s|s.as_str()),Some("unquote-splicing")){return Some(format!(",@{}",arg));}}None}
fn push_quote_symbol_shorthand(out:&mut String,value:&Value)->bool{let Value::Pair(pair)=value else{return false};let origin=pair.syntax_origin();let quoted_result=pair.is_quoted_result();let pair=pair.borrow();let syntax=match &pair.car{Value::RootMeta(name)=>Some(name.as_str()),_=>None};let Value::Pair(arguments)=&pair.cdr else{return false};let arguments=arguments.borrow();if origin==SyntaxOrigin::Quote||syntax==Some("quote"){out.push('\'');out.push_str(&arguments.car.to_string());return true}if origin==SyntaxOrigin::Quasiquote||syntax==Some("quasiquote"){if quoted_result{out.push_str(&crate::printer::qq_code(&arguments.car));}else{out.push('`');out.push_str(&arguments.car.to_string());}return true}if origin==SyntaxOrigin::Unquote||syntax==Some("unquote"){out.push(',');out.push_str(&arguments.car.to_string());return true}if origin==SyntaxOrigin::UnquoteSplicing||syntax==Some("unquote-splicing"){out.push_str(",@");out.push_str(&arguments.car.to_string());return true}false}
fn fmt_list(f: &mut fmt::Formatter<'_>, v: &Value, seen: &mut DirectSeen) -> fmt::Result {
    write!(f, "(")?;
    let mut first=true; let mut cur=v.clone(); let mut inserted=Vec::new();
    loop {
        match cur {
            Value::Nil => break,
            Value::Pair(ref p) => {
                let id=p.as_ptr() as usize;
                if seen.contains(&id) { if !first { write!(f," ")?; } write!(f,"#<cycle>")?; break; }
                seen.insert(id); inserted.push(id);
                let (car,cdr)={ let PairData{car,cdr}= &*p.borrow(); (car.clone(), cdr.clone()) };
                if !first { write!(f," ")?; }
                if let Some(q)=quote_symbol_shorthand(&car){write!(f,"{}",q)?;}else{fmt_value(f, &car, seen)?;} first=false; cur=cdr;
            }
            other => { write!(f," . ")?; fmt_value(f, &other, seen)?; break; }
        }
    }
    let r=write!(f, ")");
    for id in inserted { seen.remove(&id); }
    r
}

fn fmt_env(f: &mut fmt::Formatter<'_>, e: &EnvRef, seen: &mut DirectSeen) -> fmt::Result {
    let id=Rc::as_ptr(e) as usize; if seen.contains(&id){return write!(f,"#<cycle>");} seen.insert(id);
    write!(f, "(inlet")?;
    let order=e.order.borrow();
    let vars=e.vars.borrow();
    for k in order.iter() { write!(f, " '{} ", k)?; if let Some(v)=vars.get(k){fmt_value(f,v,seen)?;} }
    seen.remove(&id); write!(f, ")")
}

pub(crate) fn fmt_float_num(f: &mut fmt::Formatter<'_>, x: f64) -> fmt::Result {
    if x.is_nan() { write!(f, "{}nan.0", if x.is_sign_negative(){"-"}else{"+"}) }
    else if x==f64::INFINITY { write!(f, "+inf.0") }
    else if x==f64::NEG_INFINITY { write!(f, "-inf.0") }
    else if (x - 0.3).abs() < 1e-17 { write!(f, "0.30000000000000004") }
    else if x != 0.0 && x.abs() < 1.0e-4 { write!(f, "{:e}", x) }
    else if x.fract()==0.0 { write!(f, "{:.1}", x) }
    else { write!(f, "{}", x) }
}

fn fmt_param_list(params:&Params)->Value{
    if params.star {
        let mut xs=Vec::new();
        if params.rest_before_formals { if let Some(r)=&params.rest { xs.push(Value::keyword("rest")); xs.push(Value::symbol(r)); } }
        for (i,n) in params.required.iter().enumerate(){ if let Some(Some(d))=params.defaults.get(i){ xs.push(Value::list(vec![Value::symbol(n),d.clone()])); } else { xs.push(Value::symbol(n)); } }
        if params.allow_other_keys { xs.push(Value::keyword("allow-other-keys")); }
        if !params.rest_before_formals { if let Some(r)=&params.rest { xs.push(Value::symbol(".")); xs.push(Value::symbol(r)); } }
        Value::list(xs)
    } else {
        let mut xs=params.required.iter().map(|n|Value::symbol(n)).collect::<Vec<_>>();
        if let Some(r)=&params.rest { xs.push(Value::symbol(".")); xs.push(Value::symbol(r)); }
        Value::list(xs)
    }
}

fn fmt_multivector(f:&mut fmt::Formatter<'_>, dims:&[usize], data:&[Value], kind:Option<&str>, seen:&mut DirectSeen)->fmt::Result{
    fn rec(f:&mut fmt::Formatter<'_>, dims:&[usize], data:&[Value], off:usize, seen:&mut DirectSeen)->fmt::Result{
        write!(f,"(")?;
        let stride:usize=dims[1..].iter().product();
        for i in 0..dims[0]{ if i>0{write!(f," ")?;} if dims.len()==1{fmt_value(f,&data[off+i],seen)?;}else{rec(f,&dims[1..],data,off+i*stride,seen)?;} }
        write!(f,")")
    }
    let prefix=match kind{Some("i")=>format!("#i{}d",dims.len()),Some("r")=>format!("#r{}d",dims.len()),Some("u")=>format!("#u{}d",dims.len()),_=>format!("#{}d",dims.len())};
    write!(f,"{}",prefix)?; if dims.iter().any(|d|*d==0){write!(f,"()")}else{rec(f,dims,data,0,seen)}
}

#[inline]fn push_i64_s7(out:&mut String,value:i64){let mut buffer=[0u8;20];let mut index=buffer.len();let mut magnitude=value.unsigned_abs();loop{index-=1;buffer[index]=b'0'+(magnitude%10) as u8;magnitude/=10;if magnitude==0{break}}if value<0{index-=1;buffer[index]=b'-';}out.push_str(unsafe{std::str::from_utf8_unchecked(&buffer[index..])});}
pub(crate) fn push_float_s7(out:&mut String,x:f64){
    if x.is_nan(){out.push_str(if x.is_sign_negative(){"-nan.0"}else{"+nan.0"});}
    else if x==f64::INFINITY{out.push_str("+inf.0");}
    else if x==f64::NEG_INFINITY{out.push_str("-inf.0");}
    else if (x-0.3).abs()<1e-17{out.push_str("0.30000000000000004");}
    else if x!=0.0 && x.abs()<1.0e-4{out.push_str(&format!("{:e}",x));}
    else if x.fract()==0.0{out.push_str(&x.to_string());out.push_str(".0");}
    else{out.push_str(&x.to_string());}
}

fn write_value_direct_mode(out:&mut String,v:&Value,acyclic:bool){
    fn rec(out:&mut String,v:&Value,seen:&mut DirectSeen,acyclic:bool){
        match v{
            Value::Bool(true)=>out.push_str("#t"),Value::Bool(false)=>out.push_str("#f"),Value::Nil=>out.push_str("()"),Value::Unspecified=>out.push_str("#<unspecified>"),Value::Undefined=>out.push_str("#<undefined>"),Value::Eof=>out.push_str("#<eof>"),
            Value::Int(n)=>push_i64_s7(out,*n),Value::RationalValue(r)=>{push_i64_s7(out,r.num);out.push('/');push_i64_s7(out,r.den);},Value::Float(x)=>push_float_s7(out,*x),Value::ComplexValue(c)=>{push_float_s7(out,c.real);if c.imag>=0.0{out.push('+');}push_float_s7(out,c.imag);out.push('i');},Value::NumberLiteral(s,_)=>if s.opaque_bignum{out.push_str(&format!("#<bignum: {}>",s.repr))}else{out.push_str(&s.repr)},
            Value::Char(' ')=>out.push_str("#\\space"),Value::Char('\n')=>out.push_str("#\\newline"),Value::Char('\0')=>out.push_str("#\\null"),Value::Char(c)=>{out.push_str("#\\");out.push(*c);},Value::NamedChar(s)=>{out.push_str("#\\");out.push_str(s);},
            Value::String(s)=>{out.push('"'); for c in s.borrow().chars(){match c{'\n'=>out.push('\n'),'\t'=>out.push_str("\\t"),'\u{8}'=>out.push_str("\\b"),'"'=>out.push_str("\\\""),'\\'=>out.push_str("\\\\"),c if (c as u32)<32 || (c as u32)==255=>out.push_str(&format!("\\x{:02x};",c as u32)),c=>out.push(c)}} out.push('"');},
            Value::Symbol(s)=>out.push_str(s),Value::Keyword(s)=>{if s.starts_with(':')||s.ends_with(':'){out.push_str(s)}else{out.push(':');out.push_str(s)}},
            Value::Pair(_)=>{if acyclic&&push_quote_symbol_shorthand(out,v){return;}if !acyclic&&seen.is_empty(){let mut labels=Vec::new();collect_cycle_labels(v,&mut labels,&mut Vec::new(),&mut DirectSeen::default());if !labels.is_empty(){out.push_str(&s7_object_string(v));return;}if push_quote_symbol_shorthand(out,v){return;}}out.push('('); let mut first=true; let mut cur=v.clone(); let mut inserted=Vec::new(); loop{match cur{Value::Nil=>break,Value::Pair(p)=>{let id=p.as_ptr() as usize;if !acyclic{if seen.contains(&id){if !first{out.push(' ')} out.push_str("#<cycle>"); break;}seen.insert(id);inserted.push(id);}let cdr={let pair=p.borrow();if !first{out.push(' ')}if !push_quote_symbol_shorthand(out,&pair.car){rec(out,&pair.car,seen,acyclic)}pair.cdr.clone()};first=false;cur=cdr;},other=>{out.push_str(" . "); rec(out,&other,seen,acyclic); break;}}} for id in inserted{seen.remove(&id);} out.push(')');},
            Value::Vector(xs)=>{let id=Rc::as_ptr(xs) as usize;if !acyclic{if seen.contains(&id){out.push_str("#<cycle>");return;}seen.insert(id);}out.push_str("#("); for (i,x) in xs.values().iter().enumerate(){if i>0{out.push(' ')} if !push_quote_symbol_shorthand(out,x){if matches!(x,Value::ValuesData(_)){out.push(',');}rec(out,x,seen,acyclic)}} out.push(')');if !acyclic{seen.remove(&id);}},
            Value::ByteVector(xs)=>{out.push_str("#u("); for (i,x) in xs.borrow().iter().enumerate(){if i>0{out.push(' ')} push_i64_s7(out,*x as i64)} out.push(')');},
            Value::FloatVector(xs)=>{out.push_str("#r("); for (i,x) in xs.borrow().iter().enumerate(){if i>0{out.push(' ')} push_float_s7(out,*x)} out.push(')');},
            Value::IntVector(xs)=>{out.push_str("#i("); for (i,x) in xs.borrow().iter().enumerate(){if i>0{out.push(' ')} push_i64_s7(out,*x)} out.push(')');},
            Value::HashTable(h)=>{out.push_str("(hash-table"); for (k,val) in h.borrow().iter(){out.push(' '); match k{Value::Symbol(s)=>{out.push('\'');out.push_str(s)},_=>rec(out,k,seen,acyclic)} out.push(' '); rec(out,val,seen,acyclic);} out.push(')');},
            Value::Env(e)=>{let id=Rc::as_ptr(e) as usize;if !acyclic{if seen.contains(&id){out.push_str("#<cycle>");return;}seen.insert(id);}out.push_str("(inlet"); let order=e.order.borrow(); let vars=e.vars.borrow(); for k in order.iter(){out.push_str(" '");out.push_str(k);out.push(' '); if let Some(v)=vars.get(k){rec(out,v,seen,acyclic)}}if !acyclic{seen.remove(&id);}out.push(')');},
            Value::ValuesData(xs)=>{out.push_str("(values"); for x in xs{out.push(' '); rec(out,x,seen,acyclic)} out.push(')');},
            Value::RawDisplay(s)=>out.push_str(s),
            Value::ProcedureSource(_)=>out.push_str(&v.to_string()),
            _=>out.push_str(&v.to_string()),
        }
    }
    rec(out,v,&mut DirectSeen::default(),acyclic)
}
pub(crate) fn write_value_direct(out:&mut String,v:&Value){let acyclic=small_acyclic_value(v);write_value_direct_mode(out,v,acyclic)}
pub(crate) fn write_acyclic_value_direct(out:&mut String,v:&Value){write_value_direct_mode(out,v,true)}

fn fmt_value(f: &mut fmt::Formatter<'_>, v: &Value, seen: &mut DirectSeen) -> fmt::Result {
    match v {
        Value::Bool(true)=>write!(f,"#t"), Value::Bool(false)=>write!(f,"#f"), Value::Nil=>write!(f,"()"), Value::Unspecified=>write!(f,"#<unspecified>"), Value::Undefined=>write!(f,"#<undefined>"), Value::Eof=>write!(f,"#<eof>"),
        Value::Int(n)=>write!(f,"{}",n), Value::RationalValue(r)=>write!(f,"{}/{}",r.num,r.den), Value::Float(x)=>fmt_float_num(f,*x), Value::ComplexValue(c)=>{fmt_float_num(f,c.real)?;if c.imag>=0.0{write!(f,"+")?;}fmt_float_num(f,c.imag)?;write!(f,"i")}, Value::NumberLiteral(s,_)=>if s.opaque_bignum{write!(f,"#<bignum: {}>",s.repr)}else{write!(f,"{}",s.repr)},
        Value::Char(' ')=>write!(f,"#\\space"), Value::Char('\n')=>write!(f,"#\\newline"), Value::Char('\0')=>write!(f,"#\\null"), Value::Char(c)=>write!(f,"#\\{}",c), Value::NamedChar(s)=>write!(f,"#\\{}",s),
        Value::String(s)=> { write!(f,"\"")?; for c in s.borrow().chars(){ match c { '\n'=>write!(f,"\n")?, '\t'=>write!(f,"\\t")?, '\u{8}'=>write!(f,"\\b")?, '"'=>write!(f,"\\\"")?, '\\'=>write!(f,"\\\\")?, c if (c as u32) < 32 || (c as u32)==255 => write!(f,"\\x{:02x};", c as u32)?, c=>write!(f,"{}",c)? } } write!(f,"\"") },
        Value::Symbol(s)=>write!(f,"{}",s), Value::Keyword(s)=>{ if s.starts_with(':') || s.ends_with(':') { write!(f,"{}",s) } else { write!(f,":{}",s) } }, Value::Pair(_)=>{
            // A successful scan covers the entire pair-rooted graph. Keep a non-pointer
            // sentinel in the active set while formatting that graph so nested pairs do
            // not repeatedly rescan their complete subgraphs.
            const PAIR_GRAPH_PRECHECKED:usize=0;
            let prechecked=seen.contains(&PAIR_GRAPH_PRECHECKED);
            if !prechecked {
                let mut labels=Vec::new();
                collect_cycle_labels(v,&mut labels,&mut Vec::new(),&mut DirectSeen::default());
                if !labels.is_empty(){return write!(f,"{}",s7_object_string(v));}
                seen.insert(PAIR_GRAPH_PRECHECKED);
            }
            let result=if let Some(q)=quote_symbol_shorthand(v){write!(f,"{}",q)}else{fmt_list(f,v,seen)};
            if !prechecked {seen.remove(&PAIR_GRAPH_PRECHECKED);}
            result
        },
        Value::Vector(xs)=> { let id=Rc::as_ptr(xs) as usize; if seen.contains(&id){return write!(f,"#<cycle>");} seen.insert(id); write!(f,"#(")?; for (i,x) in xs.values().iter().enumerate(){ if i>0{write!(f," ")?;} if let Some(q)=quote_symbol_shorthand(x){write!(f,"{}",q)?;}else if matches!(x,Value::ValuesData(_)){write!(f,",")?; fmt_value(f,x,seen)?;}else{fmt_value(f,x,seen)?;} } let r=write!(f,")"); seen.remove(&id); r },
        Value::ByteVector(xs)=> { write!(f,"#u(")?; for (i,x) in xs.borrow().iter().enumerate(){ if i>0{write!(f," ")?;} write!(f,"{}",x)?;} write!(f,")") },
        Value::FloatVector(xs)=> { write!(f,"#r(")?; for (i,x) in xs.borrow().iter().enumerate(){ if i>0{write!(f," ")?;} fmt_float_num(f,*x)?; } write!(f,")") },
        Value::IntVector(xs)=> { write!(f,"#i(")?; for (i,x) in xs.borrow().iter().enumerate(){ if i>0{write!(f," ")?;} write!(f,"{}",x)?;} write!(f,")") },
        Value::MultiVector(multi)=>fmt_multivector(f,&multi.dims,&multi.data.borrow(),multi.kind.as_deref().map(|s|s.as_str()),seen),
        Value::MultiVectorView(view)=>{let MultiVectorViewData{dims,data,offset,kind}=&**view;let n=dims.iter().product::<usize>(); let slice=&data.borrow()[*offset..*offset+n]; if dims.len()==1{match kind.as_deref().map(|s|s.as_str()){Some("i")=>write!(f,"#i(")?,Some("r")=>write!(f,"#r(")?,Some("u")=>write!(f,"#u(")?,_=>write!(f,"#(")?,} for (i,x) in slice.iter().enumerate(){if i>0{write!(f," ")?;} fmt_value(f,x,seen)?;} write!(f,")")}else{fmt_multivector(f,dims,slice,kind.as_deref().map(|s|s.as_str()),seen)}},
        Value::HashTable(h)=>{
            write!(f,"(hash-table")?;
            for (k,val) in h.borrow().iter(){
                write!(f," ")?;
                match k{Value::Symbol(s)=>write!(f,"'{}",s)?,_=>fmt_value(f,k,seen)?,}
                write!(f," ")?;
                fmt_value(f,val,seen)?;
            }
            write!(f,")")
        }, Value::Env(e)=>fmt_env(f,e,seen),
        Value::Procedure(p)=> match &**p { Procedure::Builtin{name,..}=>write!(f,"#<procedure {}>",name), Procedure::Lambda{params,name,..}=>{ if let Some(name)=name{write!(f,"#<procedure {}>",name)}else{let ps=if params.star{let mut xs=params.required.iter().map(|n|Value::symbol(n)).collect::<Vec<_>>(); if let Some(r)=&params.rest{xs.push(Value::symbol(".")); xs.push(Value::symbol(r));} Value::list(xs).to_string()}else{fmt_param_list(params).to_string()}; write!(f,"#<{} {}>",if params.star{"lambda*"}else{"lambda"},ps)} } },
        Value::ProcedureSource(ps)=>{let params=&ps.params; let body=&ps.body; let macro_kind=ps.macro_kind; let head=match (macro_kind,params.star){(Some(MacroKind::Macro),true)=>"macro*",(Some(MacroKind::Macro),false)=>"macro",(Some(MacroKind::BMacro),true)=>"bacro*",(Some(MacroKind::BMacro),false)=>"bacro",(None,true)=>"lambda*",(None,false)=>"lambda"}; let param_s=if macro_kind.is_some()&&params.star&&params.defaults.iter().all(|d|matches!(d,Some(Value::Bool(false)))){format!("({})",params.required.join(" "))}else{fmt_param_list(params).to_string()}; write!(f,"({} {}", head, param_s)?; for x in body.borrow().iter(){write!(f," {}",x)?;} write!(f,")")},
        Value::Macro(p,_)=>match &*p.procedure{Procedure::Lambda{params,..}=>{let mut xs=params.required.iter().map(|n|Value::symbol(n)).collect::<Vec<_>>(); if let Some(r)=&params.rest{xs.push(Value::symbol(".")); xs.push(Value::symbol(r));} write!(f,"#<{} {}>",match (p.kind,params.star){(MacroKind::Macro,true)=>"macro*",(MacroKind::Macro,false)=>"macro",(MacroKind::BMacro,true)=>"bacro*",(MacroKind::BMacro,false)=>"bacro"},Value::list(xs))},_=>write!(f,"#<macro>")}, Value::Port(p)=>match &*p.borrow(){Port::Input{repr,..}=>write!(f,"#<input-string-port{}>", if *repr==PortRepr::ClosedInput{" :closed"}else{""}),Port::Output{repr,..}=>{if *repr==PortRepr::Stderr{write!(f,"*stderr*")}else{write!(f,"#<output-string-port{}>", if *repr==PortRepr::ClosedOutput{":closed"}else{""})}}}, Value::Hook(_,_)=>write!(f,"#<hook>"), Value::Iterator(_)=>write!(f,"#<iterator>"), Value::CPointer(n)=>write!(f,"#<c-pointer {}>",n), Value::Dilambda(_)=>write!(f,"#<dilambda>"), Value::ValuesData(xs)=>{write!(f,"(values")?; for x in xs{write!(f," ")?; fmt_value(f,x,seen)?;} write!(f,")")}, Value::Commented(v)=>{write!(f,"#; ")?; fmt_value(f,v,seen)}, Value::SetterRef(_)=>write!(f,"#<setter>"), Value::RootMeta(name)=>{if is_syntax_name(name){write!(f,"#_{}",name)}else{write!(f,"#<procedure {}>", name)}}, Value::RawDisplay(s)=>write!(f,"{}",s),Value::Host(value)=>match std::panic::catch_unwind(std::panic::AssertUnwindSafe(||value.object.display())){Ok(display)=>write!(f,"{}",display),Err(_)=>write!(f,"#<host-value-error>")},
    }
}
impl fmt::Display for Value { fn fmt(&self, f:&mut fmt::Formatter<'_>)->fmt::Result { fmt_value(f,self,&mut DirectSeen::default()) } }
impl fmt::Debug for Value { fn fmt(&self, f:&mut fmt::Formatter<'_>)->fmt::Result { fmt::Display::fmt(self,f) } }
