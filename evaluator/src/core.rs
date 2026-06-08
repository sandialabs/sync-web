use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::fmt;
use std::rc::Rc;

use super::{collect_cycle_labels, s7_object_string, Evaluator};

#[derive(Debug, Clone)]
pub struct SchemeError {
    pub tag: String,
    pub args: Vec<Value>,
}

impl SchemeError {
    pub(crate) fn new(tag: impl Into<String>, args: Vec<Value>) -> Self { Self { tag: tag.into(), args } }
    pub fn to_scheme(&self) -> String {
        Value::list(vec![Value::symbol("error"), Value::list(vec![Value::symbol(&self.tag), Value::list(self.args.clone())])]).to_string()
    }
}

pub(crate) type Result<T> = std::result::Result<T, SchemeError>;

pub(crate) type EnvRef = Rc<Env>;
pub(crate) type ObjRef = Rc<RefCell<Object>>;

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
    Vector(Rc<RefCell<Vec<Value>>>),
    ByteVector(Rc<RefCell<Vec<u8>>>),
    FloatVector(Rc<RefCell<Vec<f64>>>),
    IntVector(Rc<RefCell<Vec<i64>>>),
    MultiVector { dims: Vec<usize>, data: Rc<RefCell<Vec<Value>>>, kind: Option<Rc<String>> },
    MultiVectorView { dims: Vec<usize>, data: Rc<RefCell<Vec<Value>>>, offset: usize, kind: Option<Rc<String>> },
    HashTable(Rc<RefCell<Vec<(Value, Value)>>>),
    Env(EnvRef),
    Procedure(Rc<Procedure>),
    ProcedureSource { params: Params, body: Rc<RefCell<Vec<Value>>> },
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

pub enum Object {
    Pair { car: Value, cdr: Value },
}

#[derive(Clone)]
pub enum Procedure {
    Builtin { name: &'static str, func: fn(&mut Evaluator, &[Value]) -> Result<Value>, min: usize, max: Option<usize>, doc: &'static str },
    Lambda { params: Params, body: Rc<RefCell<Vec<Value>>>, env: EnvRef, name: Option<String> },
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

pub struct Env {
    pub(crate) parent: RefCell<Option<EnvRef>>,
    pub(crate) vars: RefCell<HashMap<String, Value>>,
    pub(crate) order: RefCell<Vec<String>>,
    pub(crate) open: RefCell<bool>,
}

pub(crate) fn is_syntax_name(k: &str) -> bool {
    matches!(k, "if"|"begin"|"define"|"define*"|"set!"|"lambda"|"lambda*"|"let"|"let*"|"letrec"|"letrec*"|"let-temporarily"|"cond"|"case"|"when"|"unless"|"do"|"and"|"or"|"quote"|"quasiquote"|"unquote"|"unquote-splicing"|"catch"|"throw"|"define-macro"|"define-macro*"|"define-bacro"|"define-bacro*"|"macro"|"macro*"|"bacro"|"bacro*"|"macroexpand"|"with-let")
}

impl Env {
    pub(crate) fn new(parent: Option<EnvRef>) -> EnvRef {
        Rc::new(Self { parent: RefCell::new(parent), vars: RefCell::new(HashMap::new()), order: RefCell::new(Vec::new()), open: RefCell::new(true) })
    }
    pub(crate) fn define(&self, k: impl Into<String>, v: Value) { let k=k.into(); if !self.vars.borrow().contains_key(&k){ self.order.borrow_mut().push(k.clone()); } self.vars.borrow_mut().insert(k, v); }
    pub(crate) fn get(&self, k: &str) -> Option<Value> {
        if let Some(v) = self.vars.borrow().get(k) { return Some(v.clone()); }
        if is_syntax_name(k) || k=="sync-eval" { return Some(Value::RootMeta(Rc::new(k.to_string()))); }
        self.parent.borrow().as_ref().and_then(|p| p.get(k))
    }
    pub(crate) fn set(&self, k: &str, v: Value) -> bool {
        if self.vars.borrow().contains_key(k) { self.vars.borrow_mut().insert(k.to_string(), v); true }
        else if let Some(p) = self.parent.borrow().as_ref() { p.set(k, v) }
        else { false }
    }
}

impl Value {
    pub(crate) fn symbol(s: &str) -> Self { Value::Symbol(Rc::new(s.to_string())) }
    pub(crate) fn keyword(s: &str) -> Self { Value::Keyword(Rc::new(s.to_string())) }
    pub(crate) fn string(s: impl Into<String>) -> Self { Value::String(Rc::new(RefCell::new(s.into()))) }
    pub(crate) fn cons(car: Value, cdr: Value) -> Self { Value::Pair(Rc::new(RefCell::new(Object::Pair { car, cdr }))) }
    pub fn list(xs: Vec<Value>) -> Self { xs.into_iter().rev().fold(Value::Nil, |cdr, car| Value::cons(car, cdr)) }
    pub(crate) fn is_true(&self) -> bool { match self { Value::Bool(false)=>false, Value::Values(xs)=>xs.get(0).map(|v|v.is_true()).unwrap_or(true), _=>true } }
    pub(crate) fn as_symbol(&self) -> Option<&str> { if let Value::Symbol(s)=self { Some(s.as_str()) } else { None } }
    pub(crate) fn car(&self) -> Result<Value> { match self { Value::Pair(p) => { let Object::Pair{car,..}= &*p.borrow(); Ok(car.clone()) }, Value::Nil => Err(SchemeError::new("wrong-type-arg", vec![Value::string("~A argument, ~S, is ~A but should be ~A"), Value::symbol("car"), Value::Nil, Value::string("nil"), Value::string("a pair")])) , _ => Err(SchemeError::new("wrong-type-arg", vec![self.clone()])) } }
    pub(crate) fn cdr(&self) -> Result<Value> { match self { Value::Pair(p) => { let Object::Pair{cdr,..}= &*p.borrow(); Ok(cdr.clone()) }, Value::Nil => Err(SchemeError::new("wrong-type-arg", vec![Value::string("~A argument, ~S, is ~A but should be ~A"), Value::symbol("cdr"), Value::Nil, Value::string("nil"), Value::string("a pair")])) , _ => Err(SchemeError::new("wrong-type-arg", vec![self.clone()])) } }
    pub(crate) fn set_car(&self, v: Value) -> Result<Value> { match self { Value::Pair(p) => { let Object::Pair{car,..}= &mut *p.borrow_mut(); *car=v.clone(); Ok(v) }, _ => Err(SchemeError::new("wrong-type-arg", vec![self.clone()])) } }
    pub(crate) fn set_cdr(&self, v: Value) -> Result<Value> { match self { Value::Pair(p) => { let Object::Pair{cdr,..}= &mut *p.borrow_mut(); *cdr=v.clone(); Ok(v) }, _ => Err(SchemeError::new("wrong-type-arg", vec![self.clone()])) } }
    pub(crate) fn to_vec(&self) -> Result<Vec<Value>> {
        let mut out=Vec::new(); let mut cur=self.clone();
        loop { match cur { Value::Nil => return Ok(out), Value::Pair(p) => { let Object::Pair{car,cdr}= &*p.borrow(); out.push(car.clone()); cur=cdr.clone(); }, _ => return Err(SchemeError::new("wrong-type-arg", vec![cur])) } }
    }
}

fn quote_symbol_shorthand(v:&Value)->Option<String>{if let Value::Pair(_)=v{let head=v.car().ok().and_then(|x|x.as_symbol().map(|s|s.to_string())); if head.as_deref()==Some("quote"){let arg=v.cdr().ok()?.car().ok()?; if let Value::Symbol(s)=arg{return Some(format!("'{}",s));}} if head.as_deref()==Some("unquote"){let arg=v.cdr().ok()?.car().ok()?; return Some(format!(",{}",arg));}} None}
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
                let (car,cdr)={ let Object::Pair{car,cdr}= &*p.borrow(); (car.clone(), cdr.clone()) };
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
    let keys=e.order.borrow().clone();
    for k in keys { write!(f, " '{} ", k)?; if let Some(v)=e.vars.borrow().get(&k){fmt_value(f,v,seen)?;} }
    seen.remove(&id); write!(f, ")")
}

pub(crate) fn fmt_float_num(f: &mut fmt::Formatter<'_>, x: f64) -> fmt::Result {
    if x.is_nan() { write!(f, "+nan.0") }
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
        for (i,n) in params.required.iter().enumerate(){ if let Some(Some(d))=params.defaults.get(i){ xs.push(Value::list(vec![Value::symbol(n),d.clone()])); } else { xs.push(Value::symbol(n)); } }
        if params.allow_other_keys { xs.push(Value::keyword("allow-other-keys")); }
        if let Some(r)=&params.rest { xs.push(Value::symbol(".")); xs.push(Value::symbol(r)); }
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

fn fmt_value(f: &mut fmt::Formatter<'_>, v: &Value, seen: &mut HashSet<usize>) -> fmt::Result {
    match v {
        Value::Bool(true)=>write!(f,"#t"), Value::Bool(false)=>write!(f,"#f"), Value::Nil=>write!(f,"()"), Value::Unspecified=>write!(f,"#<unspecified>"), Value::Undefined=>write!(f,"#<undefined>"), Value::Eof=>write!(f,"#<eof>"),
        Value::Int(n)=>write!(f,"{}",n), Value::Rational(n,d)=>write!(f,"{}/{}",n,d), Value::Float(x)=>fmt_float_num(f,*x), Value::Complex(re,im)=>{fmt_float_num(f,*re)?; if *im>=0.0{write!(f,"+")?;} fmt_float_num(f,*im)?; write!(f,"i")}, Value::NumberLiteral(s,_)=>write!(f,"{}",s),
        Value::Char(' ')=>write!(f,"#\\space"), Value::Char('\n')=>write!(f,"#\\newline"), Value::Char('\0')=>write!(f,"#\\null"), Value::Char(c)=>write!(f,"#\\{}",c), Value::NamedChar(s)=>write!(f,"#\\{}",s),
        Value::String(s)=> { write!(f,"\"")?; for c in s.borrow().chars(){ match c { '\n'=>write!(f,"\n")?, '\t'=>write!(f,"\\t")?, '\u{8}'=>write!(f,"\\b")?, '"'=>write!(f,"\\\"")?, '\\'=>write!(f,"\\\\")?, c if (c as u32) < 32 || (c as u32)==255 => write!(f,"\\x{:02x};", c as u32)?, c=>write!(f,"{}",c)? } } write!(f,"\"") },
        Value::Symbol(s)=>write!(f,"{}",s), Value::Keyword(s)=>{ if s.starts_with(':') || s.ends_with(':') { write!(f,"{}",s) } else { write!(f,":{}",s) } }, Value::Pair(_)=>{let mut labels=Vec::new(); collect_cycle_labels(v,&mut labels,&mut Vec::new(),&mut HashSet::new()); if labels.is_empty(){fmt_list(f,v,seen)}else{write!(f,"{}",s7_object_string(v))}},
        Value::Vector(xs)=> { let id=Rc::as_ptr(xs) as usize; if seen.contains(&id){return write!(f,"#<cycle>");} seen.insert(id); write!(f,"#(")?; for (i,x) in xs.borrow().iter().enumerate(){ if i>0{write!(f," ")?;} if let Some(q)=quote_symbol_shorthand(x){write!(f,"{}",q)?;}else if matches!(x,Value::Values(_)){write!(f,",")?; fmt_value(f,x,seen)?;}else{fmt_value(f,x,seen)?;} } let r=write!(f,")"); seen.remove(&id); r },
        Value::ByteVector(xs)=> { write!(f,"#u(")?; for (i,x) in xs.borrow().iter().enumerate(){ if i>0{write!(f," ")?;} write!(f,"{}",x)?;} write!(f,")") },
        Value::FloatVector(xs)=> { write!(f,"#r(")?; for (i,x) in xs.borrow().iter().enumerate(){ if i>0{write!(f," ")?;} fmt_float_num(f,*x)?; } write!(f,")") },
        Value::IntVector(xs)=> { write!(f,"#i(")?; for (i,x) in xs.borrow().iter().enumerate(){ if i>0{write!(f," ")?;} write!(f,"{}",x)?;} write!(f,")") },
        Value::MultiVector{dims,data,kind}=>fmt_multivector(f,dims,&data.borrow(),kind.as_deref().map(|s|s.as_str()),seen),
        Value::MultiVectorView{dims,data,offset,kind}=>{let n=dims.iter().product::<usize>(); let slice=&data.borrow()[*offset..*offset+n]; if dims.len()==1{match kind.as_deref().map(|s|s.as_str()){Some("i")=>write!(f,"#i(")?,Some("r")=>write!(f,"#r(")?,Some("u")=>write!(f,"#u(")?,_=>write!(f,"#(")?,} for (i,x) in slice.iter().enumerate(){if i>0{write!(f," ")?;} fmt_value(f,x,seen)?;} write!(f,")")}else{fmt_multivector(f,dims,slice,kind.as_deref().map(|s|s.as_str()),seen)}},
        Value::HashTable(h)=>{let mut parts=Vec::new(); for (k,val) in h.borrow().iter(){parts.push(match k{Value::Symbol(s)=>format!("'{}",s),_=>k.to_string()}); parts.push(val.to_string());} write!(f,"(hash-table{})", if parts.is_empty(){String::new()}else{format!(" {}",parts.join(" "))})}, Value::Env(e)=>fmt_env(f,e,seen),
        Value::Procedure(p)=> match &**p { Procedure::Builtin{name,..}=>write!(f,"#<procedure {}>",name), Procedure::Lambda{params,name,..}=>{ if let Some(name)=name{write!(f,"#<procedure {}>",name)}else{write!(f,"#<lambda {}>",fmt_param_list(params))} } },
        Value::ProcedureSource{params,body}=>{write!(f,"(lambda{} {}", if params.star{"*"}else{""}, fmt_param_list(params))?; for x in body.borrow().iter(){write!(f," {}",x)?;} write!(f,")")},
        Value::Macro(_,_)=>write!(f,"#<macro>"), Value::Port(p)=>match &*p.borrow(){Port::Input{repr,..}=>write!(f,"#<input-string-port{}>", if *repr==PortRepr::ClosedInput{" :closed"}else{""}),Port::Output{repr,..}=>write!(f,"#<output-string-port{}>", if *repr==PortRepr::ClosedOutput{":closed"}else{""})}, Value::Hook(_,_)=>write!(f,"#<hook>"), Value::Iterator{..}=>write!(f,"#<iterator>"), Value::CPointer(n)=>write!(f,"#<c-pointer {}>",n), Value::Dilambda(_)=>write!(f,"#<dilambda>"), Value::Values(xs)=>{write!(f,"(values")?; for x in xs{write!(f," ")?; fmt_value(f,x,seen)?;} write!(f,")")}, Value::Commented(v)=>{write!(f,"#; ")?; fmt_value(f,v,seen)}, Value::SetterRef(_)=>write!(f,"#<setter>"), Value::RootMeta(name)=>{if is_syntax_name(name){write!(f,"#_{}",name)}else{write!(f,"#<procedure {}>", name)}}, Value::RawDisplay(s)=>write!(f,"{}",s),
    }
}
impl fmt::Display for Value { fn fmt(&self, f:&mut fmt::Formatter<'_>)->fmt::Result { fmt_value(f,self,&mut HashSet::new()) } }
impl fmt::Debug for Value { fn fmt(&self, f:&mut fmt::Formatter<'_>)->fmt::Result { fmt::Display::fmt(self,f) } }
