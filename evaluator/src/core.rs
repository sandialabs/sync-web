use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::fmt;
use std::hash::{BuildHasherDefault, Hasher};
use std::rc::Rc;

use super::{collect_cycle_labels, s7_object_string, Evaluator};

#[derive(Debug, Clone)]
pub struct SchemeError {
    pub tag: String,
    pub args: Vec<Value>,
}

impl SchemeError {
    #[cold]
    #[inline(never)]
    pub(crate) fn new(tag: impl Into<String>, args: Vec<Value>) -> Self { Self { tag: tag.into(), args } }
    pub fn to_scheme(&self) -> String {
        Value::list(vec![Value::symbol("error"), Value::list(vec![Value::symbol(&self.tag), Value::list(self.args.clone())])]).to_string()
    }
}

pub(crate) type Result<T> = std::result::Result<T, SchemeError>;

pub(crate) type EnvRef = Rc<Env>;
pub(crate) type ObjRef = Rc<RefCell<PairData>>;

type EnvMap = HashMap<String, Value, BuildHasherDefault<FnvHasher>>;

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
    pub(crate) fn slice_values(&self, start: usize, end: usize) -> Vec<Value> { self.slots.borrow()[start..end].iter().map(|slot| slot.borrow().clone()).collect() }
    pub(crate) fn any_value(&self, mut f: impl FnMut(&Value) -> bool) -> bool { self.slots.borrow().iter().any(|slot| f(&slot.borrow())) }
}

#[derive(Clone)]
pub enum Value {
    Bool(bool),
    Nil,
    Unspecified,
    Undefined,
    Eof,
    Int(i64),
    Rational(i64, i64),
    Float(f64),
    Complex(f64, f64),
    NumberLiteral(Rc<String>, f64),
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
    MultiVector { dims: Rc<Vec<usize>>, data: Rc<RefCell<Vec<Value>>>, kind: Option<Rc<String>> },
    MultiVectorView { dims: Rc<Vec<usize>>, data: Rc<RefCell<Vec<Value>>>, offset: usize, kind: Option<Rc<String>> },
    HashTable(Rc<RefCell<Vec<(Value, Value)>>>),
    Env(EnvRef),
    Procedure(Rc<Procedure>),
    ProcedureSource(Rc<ProcedureSourceData>),
    Macro(Rc<Procedure>, MacroKind),
    Port(Rc<RefCell<Port>>),
    Hook(Rc<RefCell<Vec<Value>>>, i64),
    Iterator { kind: Rc<String>, items: Rc<RefCell<Vec<Value>>>, source: Rc<String>, consumed: Rc<RefCell<usize>> },
    CPointer(i64),
    Dilambda(Rc<(Value, Value)>),
    Values(Vec<Value>),
    Commented(Box<Value>),
    SetterRef(usize),
    RootMeta(Rc<String>),
    RawDisplay(Rc<String>),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MacroKind { Macro, BMacro }

pub struct PairData { pub(crate) car: Value, pub(crate) cdr: Value }

#[derive(Clone)]
pub struct ProcedureSourceData { pub(crate) params: Params, pub(crate) body: Rc<RefCell<Vec<Value>>>, pub(crate) macro_kind: Option<MacroKind> }

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
    pub(crate) fn remove(&mut self, k:&str)->Option<Value> { match self { Self::Small(v)=>v.iter().position(|(kk,_)|kk==k).map(|i|v.remove(i).1), Self::Large(m)=>m.remove(k) } }
}

pub struct Env {
    pub(crate) parent: RefCell<Option<EnvRef>>,
    pub(crate) vars: RefCell<EnvVars>,
    pub(crate) order: RefCell<Vec<String>>,
    pub(crate) open: RefCell<bool>,
}

thread_local!{static IMMUTABLE_IDS: RefCell<HashSet<usize>>=RefCell::new(HashSet::new());}

pub(crate) fn object_id(v:&Value)->Option<usize>{match v{Value::Pair(p)=>Some(Rc::as_ptr(p) as usize),Value::Vector(x)=>Some(Rc::as_ptr(x) as usize),Value::String(x)=>Some(Rc::as_ptr(x) as usize),Value::ByteVector(x)=>Some(Rc::as_ptr(x) as usize),Value::FloatVector(x)=>Some(Rc::as_ptr(x) as usize),Value::IntVector(x)=>Some(Rc::as_ptr(x) as usize),Value::HashTable(x)=>Some(Rc::as_ptr(x) as usize),Value::Env(x)=>Some(Rc::as_ptr(x) as usize),_=>None}}
pub(crate) fn mark_immutable(v:&Value){if let Some(id)=object_id(v){IMMUTABLE_IDS.with(|s|{s.borrow_mut().insert(id);});}}
pub(crate) fn is_marked_immutable(v:&Value)->bool{object_id(v).map(|id|IMMUTABLE_IDS.with(|s|s.borrow().contains(&id))).unwrap_or(false)}
pub(crate) fn immutable_error(op:&str,v:&Value)->SchemeError{SchemeError::new("immutable-error",vec![Value::string("can't ~S ~S (it is immutable)"),Value::symbol(op),v.clone()])}

pub(crate) fn is_syntax_name(k: &str) -> bool {
    matches!(k, "if"|"begin"|"define"|"define*"|"set!"|"lambda"|"lambda*"|"let"|"let*"|"letrec"|"letrec*"|"let-temporarily"|"cond"|"case"|"when"|"unless"|"do"|"and"|"or"|"quote"|"quasiquote"|"unquote"|"unquote-splicing"|"catch"|"throw"|"define-macro"|"define-macro*"|"define-bacro"|"define-bacro*"|"macro"|"macro*"|"bacro"|"bacro*"|"macroexpand"|"with-let")
}

impl Env {
    pub(crate) fn new(parent: Option<EnvRef>) -> EnvRef {
        Rc::new(Self { parent: RefCell::new(parent), vars: RefCell::new(EnvVars::new()), order: RefCell::new(Vec::new()), open: RefCell::new(false) })
    }
    pub(crate) fn with_capacity(parent: Option<EnvRef>, cap: usize) -> EnvRef {
        Rc::new(Self { parent: RefCell::new(parent), vars: RefCell::new(EnvVars::with_capacity(cap)), order: RefCell::new(Vec::with_capacity(cap)), open: RefCell::new(false) })
    }
    pub(crate) fn define(&self, k: impl Into<String>, v: Value) { let k=k.into(); if !self.vars.borrow().contains_key(&k){ self.order.borrow_mut().push(k.clone()); } self.vars.borrow_mut().insert(k, v); }
    pub(crate) fn define_fresh(&self, k: String, v: Value) { self.order.borrow_mut().push(k.clone()); self.vars.borrow_mut().insert(k, v); }
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
        if let Some(slot)=self.vars.borrow_mut().get_mut(k){*slot=v; return true;}
        if let Some(p) = self.parent.borrow().as_ref() { p.set(k, v) } else { false }
    }
    pub(crate) fn set_local_existing_many(&self, pairs: impl IntoIterator<Item=(String, Value)>) {
        let mut vars=self.vars.borrow_mut();
        for (k,v) in pairs { vars.insert(k,v); }
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
    pub(crate) fn symbol(s: &str) -> Self { Value::Symbol(Rc::new(s.to_string())) }
    pub(crate) fn keyword(s: &str) -> Self { Value::Keyword(Rc::new(s.to_string())) }
    pub(crate) fn string(s: impl Into<String>) -> Self { Value::String(Rc::new(RefCell::new(s.into()))) }
    pub(crate) fn cons(car: Value, cdr: Value) -> Self { Value::Pair(Rc::new(RefCell::new(PairData { car, cdr }))) }
    pub fn list(xs: Vec<Value>) -> Self { xs.into_iter().rev().fold(Value::Nil, |cdr, car| Value::cons(car, cdr)) }
    pub(crate) fn is_true(&self) -> bool { match self { Value::Bool(false)=>false, Value::Values(xs)=>xs.get(0).map(|v|v.is_true()).unwrap_or(true), _=>true } }
    pub(crate) fn as_symbol(&self) -> Option<&str> { if let Value::Symbol(s)=self { Some(s.as_str()) } else { None } }
    #[inline(always)]
    pub(crate) fn car(&self) -> Result<Value> { match self { Value::Pair(p) => { let PairData{car,..}= &*p.borrow(); Ok(car.clone()) }, Value::Nil => Err(SchemeError::new("wrong-type-arg", vec![Value::string("~A argument, ~S, is ~A but should be ~A"), Value::symbol("car"), Value::Nil, Value::string("nil"), Value::string("a pair")])) , _ => Err(SchemeError::new("wrong-type-arg", vec![self.clone()])) } }
    #[inline(always)]
    pub(crate) fn cdr(&self) -> Result<Value> { match self { Value::Pair(p) => { let PairData{cdr,..}= &*p.borrow(); Ok(cdr.clone()) }, Value::Nil => Err(SchemeError::new("wrong-type-arg", vec![Value::string("~A argument, ~S, is ~A but should be ~A"), Value::symbol("cdr"), Value::Nil, Value::string("nil"), Value::string("a pair")])) , _ => Err(SchemeError::new("wrong-type-arg", vec![self.clone()])) } }
    pub(crate) fn set_car(&self, v: Value) -> Result<Value> { match self { Value::Pair(p) => { let PairData{car,..}= &mut *p.borrow_mut(); *car=v.clone(); Ok(v) }, _ => Err(SchemeError::new("wrong-type-arg", vec![self.clone()])) } }
    pub(crate) fn set_cdr(&self, v: Value) -> Result<Value> { match self { Value::Pair(p) => { let PairData{cdr,..}= &mut *p.borrow_mut(); *cdr=v.clone(); Ok(v) }, _ => Err(SchemeError::new("wrong-type-arg", vec![self.clone()])) } }
    pub(crate) fn to_vec(&self) -> Result<Vec<Value>> {
        let mut out=Vec::new(); let mut cur=self.clone();
        loop { match cur { Value::Nil => return Ok(out), Value::Pair(p) => { let PairData{car,cdr}= &*p.borrow(); out.push(car.clone()); cur=cdr.clone(); }, _ => return Err(SchemeError::new("wrong-type-arg", vec![cur])) } }
    }
}

fn quote_symbol_shorthand(v:&Value)->Option<String>{if let Value::Pair(_)=v{let head=v.car().ok().and_then(|x|x.as_symbol().map(|s|s.to_string())); if head.as_deref()==Some("quote")||head.as_deref()==Some("quasiquote"){let arg=v.cdr().ok()?.car().ok()?; if let Value::Symbol(s)=arg{return Some(format!("'{}",s));}} if head.as_deref()==Some("unquote"){let arg=v.cdr().ok()?.car().ok()?; return Some(format!(",{}",arg));}} None}
fn fmt_list(f: &mut fmt::Formatter<'_>, v: &Value, seen: &mut HashSet<usize>) -> fmt::Result {
    write!(f, "(")?;
    let mut first=true; let mut cur=v.clone(); let mut inserted=Vec::new();
    loop {
        match cur {
            Value::Nil => break,
            Value::Pair(ref p) => {
                let id=Rc::as_ptr(p) as usize;
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

fn fmt_env(f: &mut fmt::Formatter<'_>, e: &EnvRef, seen: &mut HashSet<usize>) -> fmt::Result {
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

fn fmt_multivector(f:&mut fmt::Formatter<'_>, dims:&[usize], data:&[Value], kind:Option<&str>, seen:&mut HashSet<usize>)->fmt::Result{
    fn rec(f:&mut fmt::Formatter<'_>, dims:&[usize], data:&[Value], off:usize, seen:&mut HashSet<usize>)->fmt::Result{
        write!(f,"(")?;
        let stride:usize=dims[1..].iter().product();
        for i in 0..dims[0]{ if i>0{write!(f," ")?;} if dims.len()==1{fmt_value(f,&data[off+i],seen)?;}else{rec(f,&dims[1..],data,off+i*stride,seen)?;} }
        write!(f,")")
    }
    let prefix=match kind{Some("i")=>format!("#i{}d",dims.len()),Some("r")=>format!("#r{}d",dims.len()),Some("u")=>format!("#u{}d",dims.len()),_=>format!("#{}d",dims.len())};
    write!(f,"{}",prefix)?; if dims.iter().any(|d|*d==0){write!(f,"()")}else{rec(f,dims,data,0,seen)}
}

fn push_float_s7(out:&mut String,x:f64){
    if x.is_nan(){out.push_str(if x.is_sign_negative(){"-nan.0"}else{"+nan.0"});}
    else if x==f64::INFINITY{out.push_str("+inf.0");}
    else if x==f64::NEG_INFINITY{out.push_str("-inf.0");}
    else if (x-0.3).abs()<1e-17{out.push_str("0.30000000000000004");}
    else if x!=0.0 && x.abs()<1.0e-4{out.push_str(&format!("{:e}",x));}
    else if x.fract()==0.0{out.push_str(&format!("{:.1}",x));}
    else{out.push_str(&x.to_string());}
}

pub(crate) fn write_value_direct(out:&mut String,v:&Value){
    fn rec(out:&mut String,v:&Value,seen:&mut HashSet<usize>){
        match v{
            Value::Bool(true)=>out.push_str("#t"),Value::Bool(false)=>out.push_str("#f"),Value::Nil=>out.push_str("()"),Value::Unspecified=>out.push_str("#<unspecified>"),Value::Undefined=>out.push_str("#<undefined>"),Value::Eof=>out.push_str("#<eof>"),
            Value::Int(n)=>out.push_str(&n.to_string()),Value::Rational(n,d)=>{out.push_str(&n.to_string());out.push('/');out.push_str(&d.to_string());},Value::Float(x)=>push_float_s7(out,*x),Value::Complex(re,im)=>{push_float_s7(out,*re); if *im>=0.0{out.push('+');} push_float_s7(out,*im); out.push('i');},Value::NumberLiteral(s,_)=>out.push_str(s),
            Value::Char(' ')=>out.push_str("#\\space"),Value::Char('\n')=>out.push_str("#\\newline"),Value::Char('\0')=>out.push_str("#\\null"),Value::Char(c)=>{out.push_str("#\\");out.push(*c);},Value::NamedChar(s)=>{out.push_str("#\\");out.push_str(s);},
            Value::String(s)=>{out.push('"'); for c in s.borrow().chars(){match c{'\n'=>out.push('\n'),'\t'=>out.push_str("\\t"),'\u{8}'=>out.push_str("\\b"),'"'=>out.push_str("\\\""),'\\'=>out.push_str("\\\\"),c if (c as u32)<32 || (c as u32)==255=>out.push_str(&format!("\\x{:02x};",c as u32)),c=>out.push(c)}} out.push('"');},
            Value::Symbol(s)=>out.push_str(s),Value::Keyword(s)=>{if s.starts_with(':')||s.ends_with(':'){out.push_str(s)}else{out.push(':');out.push_str(s)}},
            Value::Pair(_)=>{let mut labels=Vec::new(); collect_cycle_labels(v,&mut labels,&mut Vec::new(),&mut HashSet::new()); if !labels.is_empty(){out.push_str(&s7_object_string(v)); return;} out.push('('); let mut first=true; let mut cur=v.clone(); let mut inserted=Vec::new(); loop{match cur{Value::Nil=>break,Value::Pair(p)=>{let id=Rc::as_ptr(&p) as usize; if seen.contains(&id){if !first{out.push(' ')} out.push_str("#<cycle>"); break;} seen.insert(id); inserted.push(id); let (car,cdr)={let PairData{car,cdr}= &*p.borrow(); (car.clone(),cdr.clone())}; if !first{out.push(' ')} if let Some(q)=quote_symbol_shorthand(&car){out.push_str(&q)}else{rec(out,&car,seen)} first=false; cur=cdr;},other=>{out.push_str(" . "); rec(out,&other,seen); break;}}} for id in inserted{seen.remove(&id);} out.push(')');},
            Value::Vector(xs)=>{let id=Rc::as_ptr(xs) as usize; if seen.contains(&id){out.push_str("#<cycle>");return;} seen.insert(id); out.push_str("#("); for (i,x) in xs.values().iter().enumerate(){if i>0{out.push(' ')} if let Some(q)=quote_symbol_shorthand(x){out.push_str(&q)}else{if matches!(x,Value::Values(_)){out.push(',');} rec(out,x,seen)}} out.push(')'); seen.remove(&id);},
            Value::ByteVector(xs)=>{out.push_str("#u("); for (i,x) in xs.borrow().iter().enumerate(){if i>0{out.push(' ')} out.push_str(&x.to_string())} out.push(')');},
            Value::FloatVector(xs)=>{out.push_str("#r("); for (i,x) in xs.borrow().iter().enumerate(){if i>0{out.push(' ')} push_float_s7(out,*x)} out.push(')');},
            Value::IntVector(xs)=>{out.push_str("#i("); for (i,x) in xs.borrow().iter().enumerate(){if i>0{out.push(' ')} out.push_str(&x.to_string())} out.push(')');},
            Value::HashTable(h)=>{out.push_str("(hash-table"); for (k,val) in h.borrow().iter(){out.push(' '); match k{Value::Symbol(s)=>{out.push('\'');out.push_str(s)},_=>rec(out,k,seen)} out.push(' '); rec(out,val,seen);} out.push(')');},
            Value::Env(e)=>{let id=Rc::as_ptr(e) as usize; if seen.contains(&id){out.push_str("#<cycle>");return;} seen.insert(id); out.push_str("(inlet"); let order=e.order.borrow(); let vars=e.vars.borrow(); for k in order.iter(){out.push_str(" '");out.push_str(k);out.push(' '); if let Some(v)=vars.get(k){rec(out,v,seen)}} seen.remove(&id); out.push(')');},
            Value::Values(xs)=>{out.push_str("(values"); for x in xs{out.push(' '); rec(out,x,seen)} out.push(')');},
            Value::RawDisplay(s)=>out.push_str(s),
            Value::ProcedureSource(_)=>out.push_str(&v.to_string()),
            _=>out.push_str(&v.to_string()),
        }
    }
    rec(out,v,&mut HashSet::new())
}

fn fmt_value(f: &mut fmt::Formatter<'_>, v: &Value, seen: &mut HashSet<usize>) -> fmt::Result {
    match v {
        Value::Bool(true)=>write!(f,"#t"), Value::Bool(false)=>write!(f,"#f"), Value::Nil=>write!(f,"()"), Value::Unspecified=>write!(f,"#<unspecified>"), Value::Undefined=>write!(f,"#<undefined>"), Value::Eof=>write!(f,"#<eof>"),
        Value::Int(n)=>write!(f,"{}",n), Value::Rational(n,d)=>write!(f,"{}/{}",n,d), Value::Float(x)=>fmt_float_num(f,*x), Value::Complex(re,im)=>{fmt_float_num(f,*re)?; if *im>=0.0{write!(f,"+")?;} fmt_float_num(f,*im)?; write!(f,"i")}, Value::NumberLiteral(s,_)=>write!(f,"{}",s),
        Value::Char(' ')=>write!(f,"#\\space"), Value::Char('\n')=>write!(f,"#\\newline"), Value::Char('\0')=>write!(f,"#\\null"), Value::Char(c)=>write!(f,"#\\{}",c), Value::NamedChar(s)=>write!(f,"#\\{}",s),
        Value::String(s)=> { write!(f,"\"")?; for c in s.borrow().chars(){ match c { '\n'=>write!(f,"\n")?, '\t'=>write!(f,"\\t")?, '\u{8}'=>write!(f,"\\b")?, '"'=>write!(f,"\\\"")?, '\\'=>write!(f,"\\\\")?, c if (c as u32) < 32 || (c as u32)==255 => write!(f,"\\x{:02x};", c as u32)?, c=>write!(f,"{}",c)? } } write!(f,"\"") },
        Value::Symbol(s)=>write!(f,"{}",s), Value::Keyword(s)=>{ if s.starts_with(':') || s.ends_with(':') { write!(f,"{}",s) } else { write!(f,":{}",s) } }, Value::Pair(_)=>{let mut labels=Vec::new(); collect_cycle_labels(v,&mut labels,&mut Vec::new(),&mut HashSet::new()); if labels.is_empty(){fmt_list(f,v,seen)}else{write!(f,"{}",s7_object_string(v))}},
        Value::Vector(xs)=> { let id=Rc::as_ptr(xs) as usize; if seen.contains(&id){return write!(f,"#<cycle>");} seen.insert(id); write!(f,"#(")?; for (i,x) in xs.values().iter().enumerate(){ if i>0{write!(f," ")?;} if let Some(q)=quote_symbol_shorthand(x){write!(f,"{}",q)?;}else if matches!(x,Value::Values(_)){write!(f,",")?; fmt_value(f,x,seen)?;}else{fmt_value(f,x,seen)?;} } let r=write!(f,")"); seen.remove(&id); r },
        Value::ByteVector(xs)=> { write!(f,"#u(")?; for (i,x) in xs.borrow().iter().enumerate(){ if i>0{write!(f," ")?;} write!(f,"{}",x)?;} write!(f,")") },
        Value::FloatVector(xs)=> { write!(f,"#r(")?; for (i,x) in xs.borrow().iter().enumerate(){ if i>0{write!(f," ")?;} fmt_float_num(f,*x)?; } write!(f,")") },
        Value::IntVector(xs)=> { write!(f,"#i(")?; for (i,x) in xs.borrow().iter().enumerate(){ if i>0{write!(f," ")?;} write!(f,"{}",x)?;} write!(f,")") },
        Value::MultiVector{dims,data,kind}=>fmt_multivector(f,dims,&data.borrow(),kind.as_deref().map(|s|s.as_str()),seen),
        Value::MultiVectorView{dims,data,offset,kind}=>{let n=dims.iter().product::<usize>(); let slice=&data.borrow()[*offset..*offset+n]; if dims.len()==1{match kind.as_deref().map(|s|s.as_str()){Some("i")=>write!(f,"#i(")?,Some("r")=>write!(f,"#r(")?,Some("u")=>write!(f,"#u(")?,_=>write!(f,"#(")?,} for (i,x) in slice.iter().enumerate(){if i>0{write!(f," ")?;} fmt_value(f,x,seen)?;} write!(f,")")}else{fmt_multivector(f,dims,slice,kind.as_deref().map(|s|s.as_str()),seen)}},
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
        Value::Macro(p,k)=>match &**p{Procedure::Lambda{params,..}=>{let mut xs=params.required.iter().map(|n|Value::symbol(n)).collect::<Vec<_>>(); if let Some(r)=&params.rest{xs.push(Value::symbol(".")); xs.push(Value::symbol(r));} write!(f,"#<{} {}>",match (k,params.star){(MacroKind::Macro,true)=>"macro*",(MacroKind::Macro,false)=>"macro",(MacroKind::BMacro,true)=>"bacro*",(MacroKind::BMacro,false)=>"bacro"},Value::list(xs))},_=>write!(f,"#<macro>")}, Value::Port(p)=>match &*p.borrow(){Port::Input{repr,..}=>write!(f,"#<input-string-port{}>", if *repr==PortRepr::ClosedInput{" :closed"}else{""}),Port::Output{repr,..}=>{if *repr==PortRepr::Stderr{write!(f,"*stderr*")}else{write!(f,"#<output-string-port{}>", if *repr==PortRepr::ClosedOutput{":closed"}else{""})}}}, Value::Hook(_,_)=>write!(f,"#<hook>"), Value::Iterator{..}=>write!(f,"#<iterator>"), Value::CPointer(n)=>write!(f,"#<c-pointer {}>",n), Value::Dilambda(_)=>write!(f,"#<dilambda>"), Value::Values(xs)=>{write!(f,"(values")?; for x in xs{write!(f," ")?; fmt_value(f,x,seen)?;} write!(f,")")}, Value::Commented(v)=>{write!(f,"#; ")?; fmt_value(f,v,seen)}, Value::SetterRef(_)=>write!(f,"#<setter>"), Value::RootMeta(name)=>{if is_syntax_name(name){write!(f,"#_{}",name)}else{write!(f,"#<procedure {}>", name)}}, Value::RawDisplay(s)=>write!(f,"{}",s),
    }
}
impl fmt::Display for Value { fn fmt(&self, f:&mut fmt::Formatter<'_>)->fmt::Result { fmt_value(f,self,&mut HashSet::new()) } }
impl fmt::Debug for Value { fn fmt(&self, f:&mut fmt::Formatter<'_>)->fmt::Result { fmt::Display::fmt(self,f) } }
