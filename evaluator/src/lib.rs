use std::borrow::Cow;
use std::cell::{Cell,RefCell};
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::sync::atomic::AtomicUsize;
use std::time::Instant;

static GENSYM: AtomicUsize = AtomicUsize::new(0);

mod builtin_metadata;
mod core;
mod bytecode;
mod compiled;
mod native_jit;
mod host;
pub use host::{BorrowedValue,HostCallContext,HostError,HostObject,HostOutput,PrimitiveSpec,RustHost,SYNC_WEB_HOST_PRIMITIVES};
use bytecode::{AddTerm, BytecodeFunction, Instr, MulTerm, ValueOperand};
use compiled::{BuiltinId, CExpr, CompiledLayout, QTemplate, VarRef};
use core::*;

pub struct OwnedValue{pub(crate) value:Value,pub(crate) _pair_arena:PairArenaLease}
impl Clone for OwnedValue{fn clone(&self)->Self{Self{value:self.value.clone(),_pair_arena:self._pair_arena.clone()}}}
impl std::fmt::Debug for OwnedValue{fn fmt(&self,f:&mut std::fmt::Formatter<'_>)->std::fmt::Result{write!(f,"{}",self.value)}}
impl std::fmt::Display for OwnedValue{fn fmt(&self,f:&mut std::fmt::Formatter<'_>)->std::fmt::Result{write!(f,"{}",self.value)}}
impl OwnedValue{pub fn object_string(&self)->String{self.value.to_string()}#[cfg(test)]fn pair_arena_weak(&self)->std::rc::Weak<core::PairArenaGeneration>{Rc::downgrade(&self._pair_arena.0)}}

pub fn run_source(source: &str) -> std::result::Result<OwnedValue,SchemeError> {
    let arena=new_pair_arena();let result:Result<Value>=with_pair_arena(&arena,||{let mut ev=Evaluator::new();let exprs=parse_all(source)?;let mut last=Value::Unspecified;for e in exprs{last=ev.eval(e,ev.global.clone())?;}Ok(last)});match result{Ok(value)=>Ok(OwnedValue{value,_pair_arena:arena}),Err(mut error)=>{error.retain_pair_arena(arena);Err(error)}}
}

fn raw_value_or_error_to_output(result:Result<Value>)->std::result::Result<String,String>{match result{Ok(Value::ValuesData(vs))=>Ok(Value::list(vs).to_string()),Ok(value)=>Ok(value.to_string()),Err(err)=>Err(if err.args.is_empty()&&err.tag=="wrong-number-of-args"{err.tag}else{err.to_scheme()})}}
fn value_or_error_to_output(result: std::result::Result<OwnedValue,SchemeError>) -> std::result::Result<String, String> {match result{Ok(value)=>raw_value_or_error_to_output(Ok(value.value)),Err(error)=>raw_value_or_error_to_output(Err(error))}}

pub fn run_source_output(source: &str) -> std::result::Result<String, String> {
    value_or_error_to_output(run_source(source))
}

#[doc(hidden)]
pub fn run_source_unified_output(source:&str)->std::result::Result<String,String>{unified_runtime::run_source_output(source)}

#[doc(hidden)]
pub fn run_source_output_repeated(source: &str, warmups: usize, repeats: usize) -> std::result::Result<(String, Vec<u128>), String> {
    let arena=new_pair_arena();with_pair_arena(&arena,||{let exprs = parse_all(source).map_err(|err| if err.args.is_empty() && err.tag=="wrong-number-of-args" { err.tag } else { err.to_scheme() })?;
    let mut last_output = String::new();
    let mut timings = Vec::with_capacity(repeats);
    for iter in 0..warmups.saturating_add(repeats) {
        let start = Instant::now();
        let mut ev = Evaluator::new();
        let mut last = Value::Unspecified;
        let result = (|| {
            for e in exprs.iter().cloned() { last = ev.eval(e, ev.global.clone())?; }
            Ok(last)
        })();
        let output = match raw_value_or_error_to_output(result) { Ok(v) | Err(v) => v };
        let elapsed = start.elapsed().as_nanos();
        if iter >= warmups { timings.push(elapsed); }
        last_output = output;
    }
    reset_pair_arena();Ok((last_output, timings))})
}

#[derive(Clone)]
struct GasState { active: Option<i64>, last_used: i64, last_status: String }

enum CompiledFlow { Value(Value), Recur(Vec<Value>) }

struct CompiledCtx<'a> { env: EnvRef, slots: Option<&'a [Value]>, slot_names: Option<&'a [Rc<String>]>, materialized_env: RefCell<Option<EnvRef>> }

struct NamedLetHotEntry{name:String,params:Vec<String>,inits:Vec<Value>,body:Vec<Value>,generation:u64,compiled:Rc<compiled::CompiledBody>}
pub struct Evaluator { root: EnvRef, global: EnvRef, curlet: EnvRef, proc_setters: RefCell<HashMap<usize, Value>>, named_let_cache: RefCell<Option<NamedLetHotEntry>>, gas: GasState, stdin: Value, stdout: Value, stderr: Value, pending_call_form: Option<Value>, bytecode_stack_pool: Vec<Vec<Value>>, bytecode_temp_pool: Vec<Vec<Value>>, compiled_slot_pool: Vec<Vec<Value>>, host_primitives: Vec<host::HostAdapter> }

fn normalize_loop_value(v:Value)->Value{match v{Value::Float(x) if x.is_finite()&&x>=i64::MAX as f64=>Value::Int(i64::MAX),Value::Float(x) if x.is_finite()&&x<=i64::MIN as f64=>Value::Int(i64::MIN),Value::NumberLiteral(x,_) if x.value.is_finite()&&x.value>=i64::MAX as f64=>Value::Int(i64::MAX),Value::NumberLiteral(x,_) if x.value.is_finite()&&x.value<=i64::MIN as f64=>Value::Int(i64::MIN),v=>v}}

pub(crate) fn value_has_pair_cycle(v:&Value, stack:&mut HashSet<usize>, seen:&mut HashSet<usize>)->bool{match v{Value::Pair(p)=>{let id=p.as_ptr() as usize; if stack.contains(&id){return true;} if !seen.insert(id){return false;} stack.insert(id); let PairData{car,cdr}= &*p.borrow(); let r=value_has_pair_cycle(car,stack,seen)||value_has_pair_cycle(cdr,stack,seen); stack.remove(&id); r},Value::Vector(xs)=>{let id=Rc::as_ptr(xs) as usize;if stack.contains(&id){return true}if !seen.insert(id){return false}stack.insert(id);let cyclic=xs.any_value(|x|value_has_pair_cycle(x,stack,seen));stack.remove(&id);cyclic},_=>false}}
fn value_starts_source_syntax(v:&Value,name:&str)->bool{let Value::Pair(pair)=v else{return false};let origin=match name{"quote"=>SyntaxOrigin::Quote,"quasiquote"=>SyntaxOrigin::Quasiquote,"unquote"=>SyntaxOrigin::Unquote,"unquote-splicing"=>SyntaxOrigin::UnquoteSplicing,_=>SyntaxOrigin::Explicit};pair.syntax_origin()==origin&&origin!=SyntaxOrigin::Explicit||matches!(&pair.borrow().car,Value::RootMeta(value) if value.as_str()==name)}
pub(crate) fn value_has_syntax_origin(v:&Value,seen:&mut HashSet<usize>)->bool{match v{Value::Pair(pair)=>{let id=pair.as_ptr() as usize;if !seen.insert(id){return false}if pair.syntax_origin()!=SyntaxOrigin::Explicit{return true}let data=pair.borrow();value_has_syntax_origin(&data.car,seen)||value_has_syntax_origin(&data.cdr,seen)},Value::Vector(values)=>{let id=Rc::as_ptr(values) as usize;if !seen.insert(id){return false}let mut found=false;values.for_each_value(|value|{if !found&&value_has_syntax_origin(value,seen){found=true}});found},Value::RootMeta(name)=>is_syntax_name(name),_=>false}}

impl Evaluator {
    pub(crate) fn charge_host(&mut self,amount:u64)->Result<()>{for _ in 0..amount{self.charge(1)?}Ok(())}
    pub(crate) fn invoke_host_primitive(&mut self,index:usize,args:&[Value])->Result<Value>{let callback=self.host_primitives.get(index).cloned().ok_or_else(||SchemeError::new("host-error",vec![Value::string("missing host primitive")]))?;callback(self,args)}
    pub(crate) fn register_host_adapter(&mut self,callback:host::HostAdapter)->usize{let index=self.host_primitives.len();self.host_primitives.push(callback);index}
    pub(crate) fn apply_host_source(&mut self,source:&str,args:Vec<Value>)->Result<Value>{let expression=parse_all(source)?.into_iter().next().ok_or_else(||SchemeError::new("read-error",vec![Value::string("empty host apply source")]))?;let environment=self.curlet.clone();let procedure=self.eval(expression,environment.clone())?;self.apply_value(procedure,args,environment)}
    fn set_applicable_with_setter(&mut self,target:Value,mut indices:Vec<Value>,value:Value,env:EnvRef)->Result<Value>{if let Value::Dilambda(dilambda)=&target{indices.push(value);return self.apply_value(dilambda.1.clone(),indices,env)}if let Some(key)=proc_key(&target){let setter={self.proc_setters.borrow().get(&key).cloned()};if let Some(setter)=setter{indices.push(value);return self.apply_value(setter,indices,env)}}set_applicable(target,indices,value)}
    fn new() -> Self { let root=Env::new(None); let global=Env::new(Some(root.clone())); let stdin=Value::Port(Rc::new(RefCell::new(Port::Input{text:Vec::new(),pos:0,repr:PortRepr::Stdin}))); let stdout=Value::Port(Rc::new(RefCell::new(Port::Output{text:String::new(),repr:PortRepr::Stdout}))); let stderr=Value::Port(Rc::new(RefCell::new(Port::Output{text:String::new(),repr:PortRepr::Stderr}))); let mut ev=Self{root:root.clone(), global:global.clone(), curlet:global.clone(), proc_setters:RefCell::new(HashMap::new()), named_let_cache:RefCell::new(None), gas: GasState{active:None,last_used:0,last_status:"ok".to_string()}, stdin, stdout, stderr, pending_call_form: None, bytecode_stack_pool: Vec::new(), bytecode_temp_pool: Vec::new(), compiled_slot_pool: Vec::new(), host_primitives: Vec::new()}; ev.install(); ev }
    fn install(&mut self) {
        let builtin_map: HashMap<&'static str, (fn(&mut Evaluator,&[Value])->Result<Value>, usize, Option<usize>, &'static str)> =
            BUILTINS.iter().map(|(name,func,min,max,doc)| (*name, (*func,*min,*max,*doc))).collect();
        for name in ROOTLET_NAMES {
            if let Some((func,min,max,doc))=builtin_map.get(name).copied() {
                self.root.define(*name, Value::Procedure(Rc::new(Procedure::Builtin{name,func,min,max,doc})));
            } else {
                self.root.define(*name, Value::RootMeta(Rc::new((*name).to_string())));
            }
        }
        for (name, func, min, max, doc) in BUILTINS {
            if !ROOTLET_NAMES.contains(name) {
                self.global.define(*name, Value::Procedure(Rc::new(Procedure::Builtin{name,func:*func,min:*min,max:*max,doc})));
            }
        }
        self.root.define("pi", Value::Float(std::f64::consts::PI));
        self.root.define("*stdin*", self.stdin.clone());
        self.root.define("*stdout*", self.stdout.clone());
        self.root.define("*stderr*", self.stderr.clone());
        self.root.define("*s7*", Value::Procedure(Rc::new(Procedure::Builtin{name:"*s7*",func:b_s7,min:1,max:Some(1),doc:"*s7*"})));
    }
    fn charge(&mut self, n:i64)->Result<()> { if let Some(rem)=self.gas.active.as_mut(){ if *rem < n { self.gas.last_status="exhausted".to_string(); return Err(SchemeError::new("gas-exhausted", vec![])); } *rem -= n; self.gas.last_used += n; } Ok(()) }
    fn static_quasiquote_macro(p:&Rc<Procedure>)->bool{
        fn template_ok(v:&Value,params:&HashSet<String>)->bool{if let Value::Pair(_)=v{if let Ok(xs)=v.to_vec(){if matches!(xs.first().and_then(Value::as_symbol),Some("unquote"|"unquote-splicing")){return xs.len()==2&&matches!(xs[1].as_symbol(),Some(s) if params.contains(s));}return xs.iter().all(|x|template_ok(x,params));}return false}if let Value::Vector(vec)=v{return vec.values().iter().all(|x|template_ok(x,params))}true}
        let Procedure::Lambda{params,body,..}=p.as_ref() else{return false};let body=body.borrow();if body.len()!=1{return false}let Ok(xs)=body[0].to_vec() else{return false};if xs.len()!=2||xs[0].as_symbol()!=Some("quasiquote"){return false}let names=params.required.iter().cloned().chain(params.rest.iter().cloned()).collect::<HashSet<_>>();template_ok(&xs[1],&names)
    }
    fn macro_compile_safe(v:&Value)->bool{if let Value::Pair(_)=v{if let Ok(xs)=v.to_vec(){if let Some(op)=xs.first().and_then(Value::as_symbol){if matches!(op,"set!"|"eval"|"eval-string"|"with-let"|"let-set!"|"varlet"|"apply"|"define-macro"|"define-macro*"|"define-bacro"|"define-bacro*"){return false}}return xs.iter().all(Self::macro_compile_safe)}}true}
    fn expand_pure_macros(&mut self,v:&Value,env:&EnvRef,shadowed:&HashSet<String>)->Result<Value>{
        let Value::Pair(_)=v else{return Ok(v.clone())};let Ok(xs)=v.to_vec() else{return Ok(v.clone())};if xs.is_empty(){return Ok(v.clone())}let op=xs[0].as_symbol();if matches!(op,Some("quote"|"quasiquote"|"lambda"|"lambda*")){return Ok(v.clone())}
        if let Some(name)=op{if !shadowed.contains(name){if let Some(Value::Macro(p,_))=env.get(name){if p.kind==MacroKind::Macro&&Self::static_quasiquote_macro(&p.procedure){let expanded=self.apply_proc(&p.procedure,xs[1..].to_vec(),env.clone())?;return self.expand_pure_macros(&expanded,env,shadowed)}}}}
        if matches!(op,Some("let"|"let*"|"letrec"|"letrec*"))&&xs.len()>=2{let named=matches!(xs.get(1),Some(Value::Symbol(_)));let binding_idx=if named{2}else{1};if xs.len()<=binding_idx{return Ok(v.clone())}let mut out=vec![xs[0].clone()];let mut names=shadowed.clone();if named{if let Some(n)=xs[1].as_symbol(){names.insert(n.to_string());}out.push(xs[1].clone());}let bindings=xs[binding_idx].to_vec().unwrap_or_default();let mut bs=Vec::new();for b in bindings{if let Ok(mut parts)=b.to_vec(){if let Some(n)=parts.first().and_then(Value::as_symbol){names.insert(n.to_string());}for x in parts.iter_mut().skip(1){*x=self.expand_pure_macros(x,env,shadowed)?;}bs.push(Value::list(parts));}else{bs.push(b)}}out.push(Value::list(bs));for x in xs.iter().skip(binding_idx+1){out.push(self.expand_pure_macros(x,env,&names)?)}return Ok(Value::list(out))}
        let mut out=Vec::with_capacity(xs.len());for x in &xs{out.push(self.expand_pure_macros(x,env,shadowed)?)}Ok(Value::list(out))
    }
    fn cached_macro_expansion(&self, _call:&Value, _p:&Rc<Procedure>)->Option<Value>{None}
    fn store_macro_expansion(&self, _call:&Value, _p:&Rc<Procedure>, _expanded:&Value){}
    fn eval(&mut self, expr: Value, env: EnvRef) -> Result<Value> {
        self.charge(1)?;
        match expr {
            Value::Symbol(s) => env.get(&s).ok_or_else(|| SchemeError::new("unbound-variable", vec![Value::string("unbound variable ~S"), Value::symbol(&s)])),
            Value::Pair(_) => self.eval_pair(expr, env),
            Value::Commented(v)=>Ok(Value::Commented(Box::new(self.eval(*v, env)?))),
            Value::RawDisplay(s) if s.as_str()=="__datum-label-quoted-cyclic-pair"=>Err(SchemeError::new("syntax-error",vec![Value::string("attempt to evaluate (~S . ~S)?"),Value::Int(1),Value::RawDisplay(Rc::new("#1#".to_string()))])),
            Value::RawDisplay(s) if s.as_str()=="__quote_cyclic_object_string_too_many"=>Err(SchemeError::new("syntax-error",vec![Value::string("quote: too many arguments ~A"),Value::RawDisplay(Rc::new("(quote #1= (1 . #1#))".to_string()))])),
            Value::RawDisplay(s) if s.as_str()=="__read_stray_comma"=>Err(SchemeError::new("read-error",vec![Value::string("unexpected comma: ... ,a ...")])),
            Value::RawDisplay(s) if s.as_str()=="__datum-label-object-string-dotted"=>Err(SchemeError::new("syntax-error",vec![Value::string("attempt to evaluate (~S . ~S)?"),Value::RawDisplay(Rc::new("#1#".to_string())),Value::Int(2)])),
            Value::RawDisplay(s) if s.as_str()=="__datum-label-cyclic-1"=>Err(SchemeError::new("syntax-error",vec![Value::string("attempt to evaluate (~S . ~S)?"),Value::Int(1),Value::RawDisplay(Rc::new("#1#".to_string()))])),
            Value::RawDisplay(s) if s.as_str()=="__datum-label-cyclic-0"=>Err(SchemeError::new("syntax-error",vec![Value::string("attempt to evaluate (~S . ~S)?"),Value::Int(1),Value::RawDisplay(Rc::new("#0#".to_string()))])),
            Value::RawDisplay(s) if s.as_str()=="__datum-label-shared-ab"=>Err(SchemeError::new("unbound-variable",vec![Value::string("unbound variable ~S in ~S"),Value::symbol("a"),Value::list(vec![Value::list(vec![Value::symbol("a"),Value::symbol("b")]),Value::RawDisplay(Rc::new("#1#".to_string()))])])),
            v => Ok(v),
        }
    }
    fn eval_pair(&mut self, expr: Value, env: EnvRef) -> Result<Value> {
        let op = expr.car()?;
        if op.as_symbol().is_none() && value_has_pair_cycle(&expr,&mut HashSet::new(),&mut HashSet::new()){let car=expr.car().unwrap_or(Value::Unspecified); let cdr0=expr.cdr().unwrap_or(Value::Unspecified); if let (Value::Pair(a),Value::Pair(ca))=(&expr,&car){if PairRef::ptr_eq(a,ca){return Err(SchemeError::new("syntax-error",vec![Value::string("attempt to apply ~A ~$ in ~$?"),Value::string("an undefined object"),Value::RawDisplay(Rc::new("#1#".to_string())),Value::list(vec![Value::RawDisplay(Rc::new("#1#".to_string()))])]));}} let mut cdr=cdr0; if let (Value::Pair(a),Value::Pair(b))=(&expr,&cdr){if PairRef::ptr_eq(a,b){cdr=Value::RawDisplay(Rc::new("#1#".to_string()));}else if value_has_pair_cycle(&cdr,&mut HashSet::new(),&mut HashSet::new()){cdr=Value::RawDisplay(Rc::new(format!("{}",s7_object_string(&cdr).replace("#1=(2 1 . #1#)","(2 . #1#)"))))}} return Err(SchemeError::new("syntax-error",vec![Value::string("attempt to evaluate (~S . ~S)?"),car,cdr]));}
        let args = expr.cdr()?;
        if let Some(sym)=op.as_symbol() {
            if sym=="vector-fill!" && env.get(sym).is_none(){return Err(SchemeError::new("unbound-variable",vec![Value::string("unbound variable ~S in ~S"),Value::symbol(sym),expr.clone()]));}
            if sym=="list*"{return Err(SchemeError::new("unbound-variable",vec![Value::string("unbound variable ~S in ~S"),Value::symbol(sym),Value::list(vec![Value::symbol("begin"),expr.clone()])]));}
            if sym=="let->list"{return Err(SchemeError::new("unbound-variable",vec![Value::string("unbound variable ~S in ~S"),Value::symbol(sym),Value::list(vec![Value::symbol("begin"),expr.clone()])]));}
            match sym {
                "quote" => { let xs=args.to_vec().map_err(|_|SchemeError::new("syntax-error",vec![Value::string("quote: stray dot?: ~A"),Value::cons(Value::symbol("quote"),args.clone())]))?; if xs.len()!=1{return Err(SchemeError::new("syntax-error",vec![Value::symbol("quote")]))}; let value=xs[0].clone();if let Value::Pair(pair)=&value{pair.mark_quoted_result();}return Ok(value); },
                "quasiquote" => return self.eval_quasiquote(args.car()?, env),
                "if" => { let test_expr=args.car().map_err(|_|SchemeError::new("syntax-error",vec![Value::symbol("if")]))?; let rest=args.cdr()?; let then_expr=rest.car().map_err(|_|SchemeError::new("syntax-error",vec![Value::symbol("if")]))?; let alt_expr=match rest.cdr()?{Value::Pair(p)=>{let PairData{car,..}= &*p.borrow(); car.clone()},_=>Value::Unspecified}; let test=self.eval(test_expr, env.clone())?; return if test.is_true(){ self.eval(then_expr, env) } else { self.eval(alt_expr, env) }; }
                "begin" => { let mut cur=args; if matches!(cur,Value::Nil){return Ok(Value::Nil)}; loop{match cur{Value::Pair(p)=>{let (car,cdr)={let PairData{car,cdr}= &*p.borrow(); (car.clone(),cdr.clone())}; if matches!(cdr,Value::Nil){return self.eval_tail(car,env);} self.eval(car,env.clone())?; cur=cdr;},other=>return Err(SchemeError::new("wrong-type-arg",vec![other]))}} },
                "define" => return self.eval_define(args, env),
                "define*" => return self.eval_define_star(args, env),
                "set!" => return self.eval_set(args, env),
                "lambda" => return self.make_lambda(args, env, false, None),
                "lambda*" => return self.make_lambda(args, env, true, None),
                "let" => return self.eval_let(args, env, false, false),
                "let*" => return self.eval_let(args, env, true, false),
                "letrec" => return self.eval_letrec_ctx(args, env, "letrec"),
                "letrec*" => return self.eval_letrec_ctx(args, env, "letrec*"),
                "let-temporarily" => return self.eval_let_temporarily(args, env),
                "cond" => return self.eval_cond(args, env),
                "case" => return self.eval_case(args, env),
                "when" => { let xs=args.to_vec()?; if self.eval(xs[0].clone(), env.clone())?.is_true(){ return self.eval_sequence(xs[1..].to_vec(), env); } else { return Ok(Value::Unspecified); } }
                "unless" => { let xs=args.to_vec()?; if !self.eval(xs[0].clone(), env.clone())?.is_true(){ return self.eval_sequence(xs[1..].to_vec(), env); } else { return Ok(Value::Unspecified); } }
                "do" => return self.eval_do(args, env),
                "and" => { let mut last=Value::Bool(true); let mut cur=args; loop{match cur{Value::Nil=>return Ok(last),Value::Pair(p)=>{let (a,next)={let PairData{car,cdr}= &*p.borrow(); (car.clone(),cdr.clone())}; let finalp=matches!(next,Value::Nil); match self.eval(a, env.clone())? { Value::ValuesData(vs)=>{ if finalp && vs.iter().all(|v|v.is_true()){last=Value::list(vs);} else {for v in vs{ last=v; if !last.is_true(){return Ok(last);} }} }, v=>{ last=v; if !last.is_true(){return Ok(last);} } } cur=next;},other=>return Err(SchemeError::new("wrong-type-arg",vec![other]))}} }
                "or" => { let mut cur=args; loop{match cur{Value::Nil=>return Ok(Value::Bool(false)),Value::Pair(p)=>{let (a,next)={let PairData{car,cdr}= &*p.borrow(); (car.clone(),cdr.clone())}; let finalp=matches!(next,Value::Nil); match self.eval(a, env.clone())? { Value::ValuesData(vs)=>{ if finalp && vs.iter().any(|v|v.is_true()){return Ok(Value::list(vs));} for v in vs{if v.is_true(){return Ok(v);}} }, v=>{ if v.is_true(){return Ok(v);} } } cur=next;},other=>return Err(SchemeError::new("wrong-type-arg",vec![other]))}} }
                "catch" => return self.eval_catch(args, env),
                "throw" => { let xs=self.eval_list(args, env)?; if xs.is_empty(){return Err(SchemeError::new("wrong-number-of-args",vec![Value::string("~A: not enough arguments: (~A~{~^ ~S~})"),Value::symbol("throw"),Value::symbol("throw"),Value::Nil]));} let tag=xs[0].clone(); return Err(SchemeError::new(tag.as_symbol().unwrap_or("throw").to_string(), xs[1..].to_vec())); }
                "define-macro" => return self.eval_define_macro(args, env, MacroKind::Macro, false),
                "define-macro*" => return self.eval_define_macro(args, env, MacroKind::Macro, true),
                "define-bacro" => return self.eval_define_macro(args, env, MacroKind::BMacro, false),
                "define-bacro*" => return self.eval_define_macro(args, env, MacroKind::BMacro, true),
                "macro" => return self.make_macro(args, env, MacroKind::Macro, false),
                "macro*" => return self.make_macro(args, env, MacroKind::Macro, true),
                "bacro" => return self.make_macro(args, env, MacroKind::BMacro, false),
                "bacro*" => return self.make_macro(args, env, MacroKind::BMacro, true),
                "with-let" => { let xs=args.to_vec()?; let e=self.eval(xs[0].clone(), env.clone())?; let e=if let Value::ValuesData(vs)=e{vs.get(0).cloned().unwrap_or(Value::Unspecified)}else{e}; let Value::Env(new_env)=e else { return Err(SchemeError::new("wrong-type-arg", vec![e])); }; return self.eval_sequence(xs[1..].to_vec(), new_env); }
                "macroexpand" => { let raw_form=args.car()?; let form=if matches!(raw_form.car().ok().and_then(|v|v.as_symbol().map(|s|s.to_string())).as_deref(),Some("quasiquote")){raw_form.cdr()?.car()?}else{raw_form}; if let Value::Pair(_)=&form { if let Some(name)=form.car()?.as_symbol(){ if let Some(Value::Macro(p,_))=env.get(name){return match p.kind{MacroKind::Macro|MacroKind::BMacro=>self.apply_proc(&p.procedure,form.cdr()?.to_vec()?,env)};} } } let shown=macroexpand_error_form(&form).unwrap_or_else(||Value::list(vec![Value::list(vec![Value::symbol("quote"),form])])); return Err(SchemeError::new("syntax-error",vec![Value::string("macroexpand argument is not a macro call: ~A"),shown])); }
                _=>{}
            }
            if let Some(r)=self.eval_hot_builtin(sym,args.clone(),env.clone()){return r;}
        }
        let proc = self.eval(op.clone(), env.clone())?;
        match proc {
            Value::Macro(macro_data,_) => {let p=macro_data.procedure.clone();let kind=macro_data.kind;
                let raw=args.to_vec()?;
                let expanded = if matches!(kind,MacroKind::Macro) { if let Some(v)=self.cached_macro_expansion(&expr,&p){v}else{let v=self.apply_proc(&p, raw, env.clone())?; self.store_macro_expansion(&expr,&p,&v); v} } else { match &*p { Procedure::Lambda{params,body,..}=>{let new_env=Env::new(Some(env.clone())); bind_params(self,&new_env,params,raw.clone(),env.clone())?; self.eval_lambda_body(body,new_env)?}, _=>self.apply_proc(&p, raw, env.clone())?} };
                if let Value::ValuesData(vs)=expanded { let mut out=Vec::new(); for x in vs { out.push(self.eval(x, env.clone())?); } Ok(Value::list(out)) } else { self.eval(expanded, env) }
            }
            p => { if let Value::RootMeta(name)=&p{ if name.as_str()=="and"{let mut last=Value::Bool(true); for a in args.to_vec()?{match self.eval(a,env.clone())?{Value::ValuesData(vs)=>{for v in vs{last=v;if !last.is_true(){return Ok(last)}}},v=>{last=v;if !last.is_true(){return Ok(last)}}}} return Ok(last)} if name.as_str()=="or"{for a in args.to_vec()?{match self.eval(a,env.clone())?{Value::ValuesData(vs)=>{for v in vs{if v.is_true(){return Ok(v)}}},v=>{if v.is_true(){return Ok(v)}}}} return Ok(Value::Bool(false))} } if let Some(v)=self.try_apply_one_arg_applicable(&p,&args,&env)?{return Ok(v);} let vals=self.eval_list(args, env.clone())?; let old_call=self.pending_call_form.take(); if matches!(op.car().ok().and_then(|x|x.as_symbol().map(|s|s.to_string())).as_deref(),Some("lambda*")){let mut call=vec![op.clone()]; call.extend(vals.clone()); self.pending_call_form=Some(Value::list(call));} let r=self.apply_value(p, vals, env); self.pending_call_form=old_call; r }
        }
    }
    #[allow(dead_code)]
    fn eval_compiled_body(&mut self, body:&crate::compiled::CompiledBody, env:EnvRef)->Result<Value>{
        if !body.valid.get(){return Err(SchemeError::new("unsupported-compiled-form",vec![]));}
        if !matches!(body.layout,CompiledLayout::DynamicEnv){return Err(SchemeError::new("unsupported-compiled-form",vec![]));}
        let exprs=&body.exprs;
        if exprs.is_empty(){return Ok(Value::Unspecified);}
        let old=self.curlet.clone();
        self.curlet=env.clone();
        let result=(||{
            if self.gas.active.is_none()&&Rc::ptr_eq(&body.env,&env){if let Some(native)=&body.native{if native.guard_generation==env.guard_generation(){if let Some(v)=native.run(){return Ok(Value::Int(v));}}}}
            if let Some(bc)=&body.bytecode{
                if let Some(result)=self.try_eval_word_bytecode(bc,&env,None){return result}
                match self.eval_bytecode_body(bc,env.clone(),None){
                    Ok(v)=>return Ok(v),
                    Err(e) if e.tag=="unsupported-compiled-form"=>{},
                    Err(e)=>return Err(e),
                }
            }
            for expr in &exprs[..exprs.len()-1]{ self.eval_compiled_expr(expr,env.clone())?; }
            self.eval_compiled_expr(&exprs[exprs.len()-1],env.clone())
        })();
        self.curlet=old;
        result
    }

    #[allow(dead_code)]
    fn eval_compiled_body_slots(&mut self, body:&crate::compiled::CompiledBody, env:EnvRef, slots:&[Value])->Result<Value>{
        if !body.valid.get(){return Err(SchemeError::new("unsupported-compiled-form",vec![]));}
        if !matches!(body.layout,CompiledLayout::DynamicEnv|CompiledLayout::SlotFrame{..}){return Err(SchemeError::new("unsupported-compiled-form",vec![]));}
        let exprs=&body.exprs;
        if exprs.is_empty(){return Ok(Value::Unspecified);}
        let old=self.curlet.clone();
        self.curlet=env.clone();
        let slot_names=match &body.layout{CompiledLayout::SlotFrame{params}=>Some(params.as_slice()),CompiledLayout::DynamicEnv=>None};
        let ctx=CompiledCtx{env:env.clone(),slots:Some(slots),slot_names,materialized_env:RefCell::new(None)};
        let result=(||{for expr in &exprs[..exprs.len()-1]{ self.eval_compiled_expr_ctx(expr,&ctx)?; } self.eval_compiled_expr_ctx(&exprs[exprs.len()-1],&ctx)})();
        self.curlet=old;
        result
    }

    fn append_compiled_captures(slots:&mut Vec<Value>, body:&crate::compiled::CompiledBody){ slots.extend(body.capture_values.iter().cloned()); }

    fn cache_expr_same(left:&Value,right:&Value)->bool{match (left,right){(Value::Pair(left),Value::Pair(right))=>PairRef::ptr_eq(left,right),(Value::Vector(left),Value::Vector(right))=>Rc::ptr_eq(left,right),(Value::String(left),Value::String(right))=>Rc::ptr_eq(left,right),(Value::Symbol(left),Value::Symbol(right))|(Value::Keyword(left),Value::Keyword(right))=>left==right,_=>equal(left,right)}}
    fn analyze_named_let_cached(&mut self, env:EnvRef, name:&str, params:&[String], inits:&[Value], body:&[Value])->Option<Rc<compiled::CompiledBody>>{
        if self.gas.active.is_some(){return compiled::analyze_named_let(env,name,params.iter().map(|p|Rc::new(p.clone())).collect(),inits,body).map(Rc::new);}
        let generation=env.guard_generation();
        if let Some(entry)=self.named_let_cache.borrow().as_ref().filter(|entry|entry.compiled.valid.get()&&entry.generation==generation&&entry.name==name&&entry.params==params&&entry.inits.len()==inits.len()&&entry.body.len()==body.len()&&entry.inits.iter().zip(inits).all(|(left,right)|Self::cache_expr_same(left,right))&&entry.body.iter().zip(body).all(|(left,right)|Self::cache_expr_same(left,right))){return Some(entry.compiled.clone())}
        let expanded;if body.iter().all(Self::macro_compile_safe){let empty=HashSet::new();expanded=body.iter().map(|x|self.expand_pure_macros(x,&env,&empty)).collect::<Result<Vec<_>>>().ok()?;}else{expanded=body.to_vec();}
        let compiled=compiled::analyze_named_let(env,name,params.iter().map(|p|Rc::new(p.clone())).collect(),inits,&expanded).map(Rc::new)?;
        if matches!(compiled.exprs.as_slice(),[CExpr::Loop{..}]){*self.named_let_cache.borrow_mut()=Some(NamedLetHotEntry{name:name.to_string(),params:params.to_vec(),inits:inits.to_vec(),body:body.to_vec(),generation,compiled:compiled.clone()})}Some(compiled)
    }

    fn eval_bytecode_builtin_stack_fast(&mut self, id:BuiltinId, args:&[Value])->Result<Option<Value>>{
        if args.iter().any(|v|matches!(v,Value::ValuesData(_))){return Ok(None);}
        match id{
            BuiltinId::Add if args.iter().all(|v|matches!(v,Value::Int(_)))=>{
                let mut acc=0i64;
                for v in args{let Value::Int(n)=v else{unreachable!()}; let Some(next)=acc.checked_add(*n) else{return Ok(None)}; acc=next;}
                Ok(Some(Value::Int(acc)))
            }
            BuiltinId::Sub if !args.is_empty() && args.iter().all(|v|matches!(v,Value::Int(_)))=>{
                let Value::Int(first)=args[0] else{unreachable!()};
                if args.len()==1{return first.checked_neg().map(|n|Some(Value::Int(n))).ok_or_else(most_negative_negation_error);}
                let mut acc=first;
                for v in &args[1..]{let Value::Int(n)=v else{unreachable!()}; let Some(next)=acc.checked_sub(*n) else{return Ok(None)}; acc=next;}
                Ok(Some(Value::Int(acc)))
            }
            BuiltinId::Mul if args.iter().all(|v|matches!(v,Value::Int(_)))=>{
                let mut acc=1i64;
                for v in args{let Value::Int(n)=v else{unreachable!()}; let Some(next)=acc.checked_mul(*n) else{return Ok(None)}; acc=next;}
                Ok(Some(Value::Int(acc)))
            }
            BuiltinId::NumEq|BuiltinId::Less|BuiltinId::LessEq|BuiltinId::Greater|BuiltinId::GreaterEq if args.iter().all(|v|matches!(v,Value::Int(_)))=>{
                let mut ok=true;
                for w in args.windows(2){let (Value::Int(a),Value::Int(b))=(&w[0],&w[1]) else{unreachable!()}; ok &= match id{BuiltinId::NumEq=>a==b,BuiltinId::Less=>a<b,BuiltinId::LessEq=>a<=b,BuiltinId::Greater=>a>b,BuiltinId::GreaterEq=>a>=b,_=>unreachable!()}; if !ok{break;}}
                Ok(Some(Value::Bool(ok)))
            }
            BuiltinId::Remainder if args.len()==2=>{if let (Value::Int(a),Value::Int(b))=(&args[0],&args[1]){if *b!=0{return Ok(Some(Value::Int(a%b)));}} Ok(None)}
            BuiltinId::Modulo if args.len()==2=>{if let (Value::Int(a),Value::Int(b))=(&args[0],&args[1]){if *b>0{return Ok(Some(Value::Int(((a%b)+b)%b)));}} Ok(None)}
            BuiltinId::Length if args.len()==1=>b_length(self,args).map(Some),
            BuiltinId::List=>Ok(Some(Value::list_slice(args))),
            BuiltinId::Cons if args.len()==2=>Ok(Some(Value::cons(args[0].clone(),args[1].clone()))),
            BuiltinId::Car if args.len()==1=>{if matches!(args[0],Value::Pair(_)){Ok(Some(args[0].car()?))}else{Ok(None)}}
            BuiltinId::Cdr if args.len()==1=>{if matches!(args[0],Value::Pair(_)){Ok(Some(args[0].cdr()?))}else{Ok(None)}}
            BuiltinId::NullP if args.len()==1=>Ok(Some(Value::Bool(matches!(args[0],Value::Nil)))),
            BuiltinId::PairP if args.len()==1=>Ok(Some(Value::Bool(matches!(args[0],Value::Pair(_))))),
            BuiltinId::NumberP if args.len()==1=>Ok(Some(Value::Bool(matches!(args[0],Value::Int(_)|Value::RationalValue(_)|Value::Float(_)|Value::ComplexValue(_)|Value::NumberLiteral(_,_))))),
            BuiltinId::CharP if args.len()==1=>Ok(Some(Value::Bool(matches!(args[0],Value::Char(_)|Value::NamedChar(_))))),
            BuiltinId::SymbolP if args.len()==1=>Ok(Some(Value::Bool(matches!(args[0],Value::Symbol(_)|Value::Keyword(_))))),
            BuiltinId::BooleanP if args.len()==1=>Ok(Some(Value::Bool(matches!(args[0],Value::Bool(_))))),
            BuiltinId::Not if args.len()==1=>Ok(Some(Value::Bool(!args[0].is_true()))),
            BuiltinId::EqP if args.len()==2=>Ok(Some(Value::Bool(eq(&args[0],&args[1])))),
            BuiltinId::EqualP if args.len()==2=>Ok(Some(Value::Bool(equal_two_simple_lists(&args[0],&args[1]).unwrap_or_else(||equal(&args[0],&args[1]))))),
            BuiltinId::Caar if args.len()==1=>{if matches!(args[0],Value::Pair(_)){let x=args[0].car()?; if matches!(x,Value::Pair(_)){Ok(Some(x.car()?))}else{Ok(None)}}else{Ok(None)}}
            BuiltinId::Cdar if args.len()==1=>{if matches!(args[0],Value::Pair(_)){let x=args[0].car()?; if matches!(x,Value::Pair(_)){Ok(Some(x.cdr()?))}else{Ok(None)}}else{Ok(None)}}
            BuiltinId::Cadr if args.len()==1=>{if matches!(args[0],Value::Pair(_)){let x=args[0].cdr()?;if matches!(x,Value::Pair(_)){Ok(Some(x.car()?))}else{Ok(None)}}else{Ok(None)}}
            BuiltinId::Cadar if args.len()==1=>{if matches!(args[0],Value::Pair(_)){let x=args[0].car()?;if matches!(x,Value::Pair(_)){let x=x.cdr()?;if matches!(x,Value::Pair(_)){Ok(Some(x.car()?))}else{Ok(None)}}else{Ok(None)}}else{Ok(None)}}
            BuiltinId::Assoc if args.len()==2=>{
                let mut cur=args[1].clone(); let mut slow=args[1].clone(); let mut steps=0usize;
                while let Value::Pair(p)=cur{let PairData{car:e,cdr}= &*p.borrow(); if let Value::Pair(ep)=e{let PairData{car:key,..}= &*ep.borrow(); if equal(key,&args[0]){return Ok(Some(e.clone()));}} cur=cdr.clone(); steps+=1; if steps%2==0{slow=if let Value::Pair(sp)=&slow{let PairData{cdr,..}= &*sp.borrow(); cdr.clone()}else{Value::Nil}; if matches!((&cur,&slow),(Value::Pair(x),Value::Pair(y)) if PairRef::ptr_eq(x,y)){break;}}}
                Ok(Some(Value::Bool(false)))
            }
            BuiltinId::HashRef if args.len()==2=>{
                if let Value::HashTable(h)=&args[0]{return Ok(Some(hash_lookup(h,&args[1]).unwrap_or(Value::Bool(false))));}
                Ok(None)
            }
            BuiltinId::HashSet if args.len()==3=>{
                if let Value::HashTable(h)=&args[0]{if is_marked_immutable(&args[0]){return Ok(None)}; return hash_set_entry_mutating(h,args[1].clone(),args[2].clone(),"hash-table-set!",&args[0]).map(Some);}
                Ok(None)
            }
            BuiltinId::VectorRef if args.len()==2=>{
                if let (Value::Vector(v),Value::Int(i))=(&args[0],&args[1]){
                    if *i>=0 && (*i as usize)<v.len(){return Ok(Some(v.get(*i as usize)));}
                }
                Ok(None)
            }
            BuiltinId::VectorSet if args.len()==3=>{
                if let (Value::Vector(v),Value::Int(i))=(&args[0],&args[1]){
                    if !is_marked_immutable(&args[0]) && *i>=0 && (*i as usize)<v.len(){v.set(*i as usize,args[2].clone()); return Ok(Some(args[2].clone()));}
                }
                Ok(None)
            }
            _=>Ok(None),
        }
    }

    fn materialize_bytecode_env(&self,bc:&BytecodeFunction,pc:usize,env:&EnvRef,base_slots:&[Value],temps:&[Value])->EnvRef{let fb=Env::new(Some(env.clone()));if let crate::bytecode::BytecodeLayout::SlotFrame{names}=&bc.layout{for (name,value) in names.iter().zip(base_slots.iter()){fb.define(name.as_str(),value.clone());}}if let Some((_,bindings))=bc.materialization_bindings.iter().find(|(at,_)|*at==pc){for (name,index) in bindings{if let Some(value)=index.checked_sub(base_slots.len()).and_then(|j|temps.get(j)){fb.define(name.as_str(),value.clone());}}}fb}
    #[inline(never)]
    fn eval_two_instruction_body(&mut self,bc:&BytecodeFunction,env:&EnvRef,slots:&[Value])->Result<Option<Value>>{let (target,index)=match bc.code.as_slice(){[Instr::LoadSlot(slot),Instr::Return]=>return Ok(slots.get(*slot).cloned()),[Instr::LoadConst(constant),Instr::Return]=>return Ok(bc.constants.get(*constant).cloned()),[Instr::LoadDynamic(name),Instr::Return]=>return env.get(name.as_str()).map(Some).ok_or_else(||SchemeError::new("unbound-variable",vec![Value::string("unbound variable ~S"),Value::symbol(name.as_str())])),[Instr::ApplicableRefDynamic{target,index},Instr::Return]=>(target,index),_=>return Ok(None)};let index=match index{ValueOperand::Slot(slot)=>slots.get(*slot),ValueOperand::Const(constant)=>bc.constants.get(*constant)};let Some(index)=index else{return Ok(None)};if matches!(index,Value::ValuesData(_)){return Ok(None)}let target=env.get(target.as_str()).ok_or_else(||SchemeError::new("unbound-variable",vec![Value::string("unbound variable ~S"),Value::symbol(target.as_str())]))?;if let Value::HashTable(table)=&target{Ok(Some(hash_lookup(table,index).unwrap_or(Value::Bool(false))))}else{applicable_get(&target,index).map(Some)}}

    #[cold]
    #[inline(never)]
    fn try_apply_rest_forwarder(&mut self,compiled:&compiled::CompiledBody,env:&EnvRef,args:&[Value],required:usize)->Result<Option<Value>>{if self.gas.active.is_some()||compiled.capture_values.len()>0{return Ok(None)}let Some(bytecode)=&compiled.bytecode else{return Ok(None)};let [Instr::ApplicableRefDynamic{target,index:ValueOperand::Slot(index)},Instr::LoadSlot(rest),Instr::BuiltinCall{id:BuiltinId::Apply,argc:2},Instr::Return]=bytecode.code.as_slice() else{return Ok(None)};if *rest!=required||*index>=required{return Ok(None)}let guard=(Rc::as_ptr(env) as usize,env.guard_generation());if bytecode.validated_env.get()!=guard{if bytecode.required_builtins.iter().any(|name|env.builtin_func(name).is_none()){return Ok(None)}bytecode.validated_env.set(guard)}let Some(index)=args.get(*index) else{return Ok(None)};let callable=env.with_value(target.as_str(),|value|if let Value::HashTable(table)=value{Ok(hash_lookup(table,index).unwrap_or(Value::Bool(false)))}else{applicable_get(value,index)}).ok_or_else(||SchemeError::new("unbound-variable",vec![Value::string("unbound variable ~S"),Value::symbol(target.as_str())]))??;if !matches!(callable,Value::Procedure(_)){return Ok(None)}self.apply_value(callable,args[required..].to_vec(),env.clone()).map(Some)}

    fn eval_word_pure_builtin(&mut self,id:BuiltinId,raw:Vec<Value>,env:&EnvRef)->Option<Value>{let mut values=Vec::with_capacity(raw.len());for value in raw{match value{Value::ValuesData(items)=>values.extend(items),value=>values.push(value)}}let name=id.name();let (func,min,max)=env.builtin_func(name)?;if values.len()<min||max.map(|limit|values.len()>limit).unwrap_or(false){return None}match self.eval_compiled_builtin_fast(id,&values).ok()?{Some(value)=>Some(value),None=>func(self,&values).ok()}}

    #[cold]
    #[inline(never)]
    fn try_eval_word_bytecode(&mut self,bc:&BytecodeFunction,env:&EnvRef,slots:Option<&[Value]>)->Option<Result<Value>>{if self.gas.active.is_some(){return None}let program=bc.word_program.as_ref()?;if !program.profitable()||!(program.has_star_closure(env)||program.has_loop())||!program.preflight(env){return None}let guard=(Rc::as_ptr(env) as usize,env.guard_generation());if bc.validated_env.get()!=guard{if bc.required_builtins.iter().any(|name|env.builtin_func(name).is_none()){return None}bc.validated_env.set(guard);}program.execute(slots.unwrap_or(&[]),Some(env),|id,values|self.eval_word_pure_builtin(id,values,env)).map(Ok)}

    fn eval_bytecode_body(&mut self, bc:&BytecodeFunction, env:EnvRef, slots:Option<&[Value]>)->Result<Value>{
        let guard=(Rc::as_ptr(&env) as usize,env.guard_generation());
        if bc.validated_env.get()!=guard{
            if bc.required_builtins.iter().any(|name|env.builtin_func(name).is_none()){return Err(SchemeError::new("unsupported-compiled-form",vec![]));}
            bc.validated_env.set(guard);
        }
        let mut pc=0usize;
        let mut stack=self.bytecode_stack_pool.pop().unwrap_or_else(||Vec::with_capacity(16));
        stack.clear();
        let base_slots=slots.unwrap_or(&[]);
        let mut temps=self.bytecode_temp_pool.pop().unwrap_or_default();
        temps.clear();
        temps.resize(bc.max_temps,Value::Unspecified);
        let load_frame_slot=|idx:usize, temps:&[Value]| -> Option<Value> {if idx<base_slots.len(){base_slots.get(idx).cloned()}else{temps.get(idx-base_slots.len()).cloned()}};
        let mut cached_add:Option<(fn(&mut Evaluator,&[Value])->Result<Value>,usize,Option<usize>)>=None;
        let mut cached_mul:Option<(fn(&mut Evaluator,&[Value])->Result<Value>,usize,Option<usize>)>=None;
        // Context-sensitive builtins such as eval observe the active bytecode environment.
        let previous_curlet=self.curlet.clone();
        self.curlet=env.clone();
        // Binary integer fast op fallback looks up the builtin only on the deopt/error path.
        let result=(||{
        loop{
            if self.gas.active.is_some(){self.charge(1)?;}
            let Some(instr)=bc.code.get(pc) else{return Err(SchemeError::new("unsupported-compiled-form",vec![]));};
            pc+=1;
            match instr{
                Instr::LoadConst(i)=>stack.push(bc.constants.get(*i).cloned().ok_or_else(||SchemeError::new("unsupported-compiled-form",vec![]))?),
                Instr::LoadDynamic(name)=>stack.push(env.get(name.as_str()).ok_or_else(||SchemeError::new("unbound-variable",vec![Value::string("unbound variable ~S"),Value::symbol(name.as_str())]))?),
                Instr::LoadSlot(i)=>stack.push(load_frame_slot(*i,&temps).ok_or_else(||SchemeError::new("unsupported-compiled-form",vec![]))?),
                Instr::StoreTemp(i)=>{let v=stack.pop().ok_or_else(||SchemeError::new("unsupported-compiled-form",vec![]))?; let Some(slot)=i.checked_sub(base_slots.len()).and_then(|j|temps.get_mut(j)) else{return Err(SchemeError::new("unsupported-compiled-form",vec![]));}; *slot=v;}
                Instr::BindTemp{index,name,sequential}=>{let v=stack.pop().ok_or_else(||SchemeError::new("unsupported-compiled-form",vec![]))?; let v=self.normalize_binding_value_ctx(if *sequential{"let*"}else{"let"},name.as_str(),v)?; let Some(slot)=index.checked_sub(base_slots.len()).and_then(|j|temps.get_mut(j)) else{return Err(SchemeError::new("unsupported-compiled-form",vec![]));}; *slot=v;}
                Instr::LoadTemp(i)=>stack.push(load_frame_slot(*i,&temps).ok_or_else(||SchemeError::new("unsupported-compiled-form",vec![]))?),
                Instr::Pop=>{stack.pop();}
                Instr::Jump(target)=>pc=*target,
                Instr::JumpIfFalse(target)=>{let v=stack.last().ok_or_else(||SchemeError::new("unsupported-compiled-form",vec![]))?; let truth=match v{Value::ValuesData(vs)=>vs.first().map(|v|v.is_true()).unwrap_or(false),_=>v.is_true()}; if !truth{pc=*target;}},
                Instr::JumpIfFalsePop(target)=>{let v=stack.pop().ok_or_else(||SchemeError::new("unsupported-compiled-form",vec![]))?; let truth=match &v{Value::ValuesData(vs)=>vs.first().map(|v|v.is_true()).unwrap_or(false),_=>v.is_true()}; if !truth{pc=*target;}},
                Instr::JumpIfOrTrue(target)=>{let v=stack.last_mut().ok_or_else(||SchemeError::new("unsupported-compiled-form",vec![]))?; match v{Value::ValuesData(vs)=>{if let Some(hit)=vs.iter().find(|x|x.is_true()).cloned(){*v=hit; pc=*target;}},v=>{if v.is_true(){pc=*target;}}}},
                Instr::CaseJump{datums,target}=>{let key=stack.last().ok_or_else(||SchemeError::new("unsupported-compiled-form",vec![]))?; if datums.iter().any(|datum|equal(key,datum)){pc=*target;}},
                Instr::FastBuiltinCall{id,argc}=>{
                    if stack.len()<*argc{return Err(SchemeError::new("unsupported-compiled-form",vec![]));}
                    let start=stack.len()-*argc;
                    if let Some(out)=self.eval_bytecode_builtin_stack_fast(*id,&stack[start..])?{
                        stack.truncate(start);
                        stack.push(out);
                        continue;
                    }
                    let name=id.name();
                    let Some((func,min,max))=env.builtin_func(name) else{return Err(SchemeError::new("unsupported-compiled-form",vec![]));};
                    if !stack[start..].iter().any(|v|matches!(v,Value::ValuesData(_))){
                        let out={
                            let vals=&stack[start..];
                            if vals.len()<min || max.map(|m|vals.len()>m).unwrap_or(false){return Err(SchemeError::new("wrong-number-of-args",vec![Value::symbol(name)]));}
                            if let Some(v)=self.eval_compiled_builtin_fast(*id,vals)?{v}else{func(self,vals)?}
                        };
                        stack.truncate(start);
                        stack.push(out);
                    }else{
                        let raw=stack.split_off(start);
                        let mut vals=Vec::new();
                        for v in raw{match v{Value::ValuesData(vs)=>vals.extend(vs),v=>vals.push(v)}}
                        if vals.len()<min || max.map(|m|vals.len()>m).unwrap_or(false){return Err(SchemeError::new("wrong-number-of-args",vec![Value::symbol(name)]));}
                        let out=if let Some(v)=self.eval_compiled_builtin_fast(*id,&vals)?{v}else{func(self,&vals)?};
                        stack.push(out);
                    }
                }
                Instr::UnarySlot{id,slot}=>{let v=if *slot<base_slots.len(){base_slots.get(*slot)}else{temps.get(*slot-base_slots.len())}.ok_or_else(||SchemeError::new("unsupported-compiled-form",vec![]))?;let direct=match id{BuiltinId::Car if matches!(v,Value::Pair(_))=>Some(v.car()?),BuiltinId::Cdr if matches!(v,Value::Pair(_))=>Some(v.cdr()?),BuiltinId::Caar if matches!(v,Value::Pair(_))=>{let x=v.car()?;if matches!(x,Value::Pair(_)){Some(x.car()?)}else{None}},BuiltinId::Cdar if matches!(v,Value::Pair(_))=>{let x=v.car()?;if matches!(x,Value::Pair(_)){Some(x.cdr()?)}else{None}},BuiltinId::Cadr if matches!(v,Value::Pair(_))=>{let x=v.cdr()?;if matches!(x,Value::Pair(_)){Some(x.car()?)}else{None}},BuiltinId::Cadar if matches!(v,Value::Pair(_))=>{let x=v.car()?;if matches!(x,Value::Pair(_)){let x=x.cdr()?;if matches!(x,Value::Pair(_)){Some(x.car()?)}else{None}}else{None}},BuiltinId::NullP=>Some(Value::Bool(matches!(v,Value::Nil))),BuiltinId::PairP=>Some(Value::Bool(matches!(v,Value::Pair(_)))),BuiltinId::NumberP=>Some(Value::Bool(matches!(v,Value::Int(_)|Value::RationalValue(_)|Value::Float(_)|Value::ComplexValue(_)|Value::NumberLiteral(_,_)))),BuiltinId::CharP=>Some(Value::Bool(matches!(v,Value::Char(_)|Value::NamedChar(_)))),BuiltinId::SymbolP=>Some(Value::Bool(matches!(v,Value::Symbol(_)|Value::Keyword(_)))),BuiltinId::BooleanP=>Some(Value::Bool(matches!(v,Value::Bool(_)))),BuiltinId::Not=>Some(Value::Bool(!v.is_true())),_=>None};if let Some(out)=direct{stack.push(out);}else{let name=id.name();let Some((func,_,_))=env.builtin_func(name) else{return Err(SchemeError::new("unsupported-compiled-form",vec![]));};stack.push(func(self,std::slice::from_ref(v))?);}}
                Instr::OneArgSlotCall{callee,arg}=>{let get_slot=|slot:usize|if slot<base_slots.len(){base_slots.get(slot)}else{temps.get(slot-base_slots.len())};let proc=get_slot(*callee).ok_or_else(||SchemeError::new("unsupported-compiled-form",vec![]))?;let value=match arg{ValueOperand::Slot(slot)=>get_slot(*slot),ValueOperand::Const(i)=>bc.constants.get(*i)}.ok_or_else(||SchemeError::new("unsupported-compiled-form",vec![]))?;if matches!(value,Value::ValuesData(_))||matches!(proc,Value::Macro(_,_)|Value::RootMeta(_)){return Err(SchemeError::new("unsupported-compiled-form",vec![]));}let direct=match proc{Value::HashTable(h)=>Some(Ok(hash_lookup(h,value).unwrap_or(Value::Bool(false)))),Value::Vector(_)|Value::ByteVector(_)|Value::FloatVector(_)|Value::IntVector(_)|Value::String(_)|Value::Pair(_)|Value::Env(_)|Value::MultiVector(_)|Value::MultiVectorView(_)|Value::ProcedureSource(_)=>Some(applicable_get(proc,value)),_=>None};if let Some(out)=direct{stack.push(out?);continue;}if let Some(out)=self.try_apply_compiled_fixed_slice(proc,std::slice::from_ref(value))?{stack.push(out);continue;}stack.push(self.apply_value(proc.clone(),vec![value.clone()],env.clone())?);}
                Instr::OneArgCarSlotCall{list,arg}=>{if self.gas.active.is_some(){self.charge(2)?}let get_slot=|slot:usize|if slot<base_slots.len(){base_slots.get(slot)}else{temps.get(slot-base_slots.len())};let list=get_slot(*list).ok_or_else(||SchemeError::new("unsupported-compiled-form",vec![]))?;let callable=if matches!(list,Value::Pair(_)){list.car()?}else{let Some((function,_,_))=env.builtin_func("car") else{return Err(SchemeError::new("unsupported-compiled-form",vec![]))};function(self,std::slice::from_ref(list))?};let argument=match arg{ValueOperand::Slot(slot)=>get_slot(*slot),ValueOperand::Const(index)=>bc.constants.get(*index)}.ok_or_else(||SchemeError::new("unsupported-compiled-form",vec![]))?;if !matches!(argument,Value::ValuesData(_)){if let Some(output)=self.try_apply_compiled_fixed_slice(&callable,std::slice::from_ref(argument))?{stack.push(output);continue}stack.push(self.apply_value(callable,vec![argument.clone()],env.clone())?)}else{let Value::ValuesData(values)=argument else{unreachable!()};stack.push(self.apply_value(callable,values.iter().cloned().collect(),env.clone())?)}}
                Instr::UnaryCompareSlot{getter,cmp,slot,rhs}=>{let get_slot=|slot:usize|if slot<base_slots.len(){base_slots.get(slot)}else{temps.get(slot-base_slots.len())};let source=get_slot(*slot).ok_or_else(||SchemeError::new("unsupported-compiled-form",vec![]))?;let got=match getter{BuiltinId::Car if matches!(source,Value::Pair(_))=>Some(source.car()?),BuiltinId::Cdr if matches!(source,Value::Pair(_))=>Some(source.cdr()?),BuiltinId::Caar if matches!(source,Value::Pair(_))=>{let x=source.car()?;if matches!(x,Value::Pair(_)){Some(x.car()?)}else{None}},BuiltinId::Cdar if matches!(source,Value::Pair(_))=>{let x=source.car()?;if matches!(x,Value::Pair(_)){Some(x.cdr()?)}else{None}},_=>None};let got=if let Some(v)=got{v}else{let Some((func,_,_))=env.builtin_func(getter.name()) else{return Err(SchemeError::new("unsupported-compiled-form",vec![]));};func(self,std::slice::from_ref(source))?};let other=match rhs{ValueOperand::Slot(slot)=>get_slot(*slot),ValueOperand::Const(i)=>bc.constants.get(*i)}.ok_or_else(||SchemeError::new("unsupported-compiled-form",vec![]))?;stack.push(Value::Bool(match cmp{BuiltinId::EqP=>eq(&got,other),BuiltinId::EqualP=>equal_two_simple_lists(&got,other).unwrap_or_else(||equal(&got,other)),_=>return Err(SchemeError::new("unsupported-compiled-form",vec![]))}));}
                Instr::BinaryOperands{id,lhs,rhs}=>{let get_slot=|slot:usize|if slot<base_slots.len(){base_slots.get(slot)}else{temps.get(slot-base_slots.len())};let get=|op:&ValueOperand|match op{ValueOperand::Slot(slot)=>get_slot(*slot),ValueOperand::Const(i)=>bc.constants.get(*i)};let a=get(lhs).ok_or_else(||SchemeError::new("unsupported-compiled-form",vec![]))?;let b=get(rhs).ok_or_else(||SchemeError::new("unsupported-compiled-form",vec![]))?;stack.push(match id{BuiltinId::Cons=>Value::cons(a.clone(),b.clone()),BuiltinId::EqP=>Value::Bool(eq(a,b)),BuiltinId::EqualP=>Value::Bool(equal_two_simple_lists(a,b).unwrap_or_else(||equal(a,b))),_=>return Err(SchemeError::new("unsupported-compiled-form",vec![]))});}
                Instr::IntAddTermsRecur{terms}=>{
                    let term_int=|term:&AddTerm, temps:&[Value]| -> Option<i64> {match term{AddTerm::Const(n)=>Some(*n),AddTerm::Slot(i)=>{let v=if *i<base_slots.len(){base_slots.get(*i)}else{temps.get(*i-base_slots.len())}?;if let Value::Int(n)=v{Some(*n)}else{None}}}};
                    let mut acc=0i64;let mut ints=true;for term in terms{if let Some(n)=term_int(term,&temps){acc=acc.wrapping_add(n);}else{ints=false;break}}
                    if ints{stack.push(Value::Int(acc));}else{let term_value=|term:&AddTerm, temps:&[Value]| -> Result<Value> {match term{AddTerm::Slot(i)=>load_frame_slot(*i,temps).ok_or_else(||SchemeError::new("unsupported-compiled-form",vec![])),AddTerm::Const(n)=>Ok(Value::Int(*n))}};let Some((func,min,max))=env.builtin_func("+") else{return Err(SchemeError::new("unsupported-compiled-form",vec![]));};let vals=terms.iter().map(|t|term_value(t,&temps)).collect::<Result<Vec<_>>>()?;if vals.len()<min||max.map(|m|vals.len()>m).unwrap_or(false){return Err(SchemeError::new("wrong-number-of-args",vec![Value::symbol("+")]));}stack.push(func(self,&vals)?);}
                }
                Instr::IntAddTerms{terms}=>{
                    let term_value=|term:&AddTerm, temps:&[Value]| -> Result<Value> {match term{AddTerm::Slot(i)=>load_frame_slot(*i,temps).ok_or_else(||SchemeError::new("unsupported-compiled-form",vec![])),AddTerm::Const(n)=>Ok(Value::Int(*n))}};
                    let term_int=|term:&AddTerm, temps:&[Value]| -> Option<i64> {match term{AddTerm::Const(n)=>Some(*n),AddTerm::Slot(i)=>{let v=if *i<base_slots.len(){base_slots.get(*i)}else{temps.get(*i-base_slots.len())}?; if let Value::Int(n)=v{Some(*n)}else{None}}}};
                    let mut acc=0i64;
                    let mut deopt=false;
                    for term in terms{
                        match term_int(term,&temps){
                            Some(n)=>{if let Some(next)=acc.checked_add(n){acc=next;}else{deopt=true; break;}},
                            None=>{deopt=true; break;}
                        }
                    }
                    if deopt{
                        if bc.cache_stable_builtins && cached_add.is_none(){cached_add=env.builtin_func("+");}
                        let Some((func,min,max))=(if bc.cache_stable_builtins{cached_add}else{env.builtin_func("+")}) else{return Err(SchemeError::new("unsupported-compiled-form",vec![]));};
                        let vals=terms.iter().map(|t|term_value(t,&temps)).collect::<Result<Vec<_>>>()?;
                        if vals.len()<min || max.map(|m|vals.len()>m).unwrap_or(false){return Err(SchemeError::new("wrong-number-of-args",vec![Value::symbol("+")]));}
                        stack.push(func(self,&vals)?);
                    }else{stack.push(Value::Int(acc));}
                }
                Instr::IntMulTerms{terms}=>{
                    let term_value=|term:&MulTerm, temps:&[Value]| -> Result<Value> {match term{MulTerm::Slot(i)=>load_frame_slot(*i,temps).ok_or_else(||SchemeError::new("unsupported-compiled-form",vec![])),MulTerm::Const(n)=>Ok(Value::Int(*n))}};
                    let term_int=|term:&MulTerm, temps:&[Value]| -> Option<i64> {match term{MulTerm::Const(n)=>Some(*n),MulTerm::Slot(i)=>{let v=if *i<base_slots.len(){base_slots.get(*i)}else{temps.get(*i-base_slots.len())}?; if let Value::Int(n)=v{Some(*n)}else{None}}}};
                    let mut acc=1i64;
                    let mut deopt=false;
                    for term in terms{
                        match term_int(term,&temps){
                            Some(n)=>{if let Some(next)=acc.checked_mul(n){acc=next;}else{deopt=true; break;}},
                            None=>{deopt=true; break;}
                        }
                    }
                    if deopt{
                        if bc.cache_stable_builtins && cached_mul.is_none(){cached_mul=env.builtin_func("*");}
                        let Some((func,min,max))=(if bc.cache_stable_builtins{cached_mul}else{env.builtin_func("*")}) else{return Err(SchemeError::new("unsupported-compiled-form",vec![]));};
                        let vals=terms.iter().map(|t|term_value(t,&temps)).collect::<Result<Vec<_>>>()?;
                        if vals.len()<min || max.map(|m|vals.len()>m).unwrap_or(false){return Err(SchemeError::new("wrong-number-of-args",vec![Value::symbol("*")]));}
                        stack.push(func(self,&vals)?);
                    }else{stack.push(Value::Int(acc));}
                }
                Instr::IntBinaryTerms{id,lhs,rhs}=>{
                    let name=id.name();
                    let fallback_builtin=||env.builtin_func(name);
                    let term_value=|term:&AddTerm, temps:&[Value]| -> Result<Value> {match term{AddTerm::Slot(i)=>load_frame_slot(*i,temps).ok_or_else(||SchemeError::new("unsupported-compiled-form",vec![])),AddTerm::Const(n)=>Ok(Value::Int(*n))}};
                    let term_int=|term:&AddTerm, temps:&[Value]| -> Option<i64> {match term{AddTerm::Const(n)=>Some(*n),AddTerm::Slot(i)=>{let v=if *i<base_slots.len(){base_slots.get(*i)}else{temps.get(*i-base_slots.len())}?; if let Value::Int(n)=v{Some(*n)}else{None}}}};
                    let fast=match (term_int(lhs,&temps),term_int(rhs,&temps),id){
                        (Some(a),Some(b),BuiltinId::Sub)=>a.checked_sub(b).map(Value::Int),
                        (Some(a),Some(b),BuiltinId::NumEq)=>Some(Value::Bool(a==b)),
                        (Some(a),Some(b),BuiltinId::Less)=>Some(Value::Bool(a<b)),
                        (Some(a),Some(b),BuiltinId::LessEq)=>Some(Value::Bool(a<=b)),
                        (Some(a),Some(b),BuiltinId::Greater)=>Some(Value::Bool(a>b)),
                        (Some(a),Some(b),BuiltinId::GreaterEq)=>Some(Value::Bool(a>=b)),
                        (Some(a),Some(b),BuiltinId::Remainder) if b!=0=>Some(Value::Int(a%b)),
                        (Some(a),Some(b),BuiltinId::Modulo) if b>0=>Some(Value::Int(((a%b)+b)%b)),
                        _=>None,
                    };
                    if let Some(v)=fast{stack.push(v);}else{
                        let Some((func,min,max))=fallback_builtin() else{return Err(SchemeError::new("unsupported-compiled-form",vec![]));};
                        let vals=vec![term_value(lhs,&temps)?,term_value(rhs,&temps)?];
                        if vals.len()<min || max.map(|m|vals.len()>m).unwrap_or(false){return Err(SchemeError::new("wrong-number-of-args",vec![Value::symbol(name)]));}
                        stack.push(func(self,&vals)?);
                    }
                }
                Instr::BuiltinCall{id,argc}=>{
                    if stack.len()<*argc{return Err(SchemeError::new("unsupported-compiled-form",vec![]));}
                    let name=id.name();
                    let Some((func,min,max))=env.builtin_func(name) else{return Err(SchemeError::new("unsupported-compiled-form",vec![]));};
                    let start=stack.len()-*argc;
                    if !stack[start..].iter().any(|v|matches!(v,Value::ValuesData(_))){
                        let out={
                            let vals=&stack[start..];
                            if vals.len()<min || max.map(|m|vals.len()>m).unwrap_or(false){return Err(SchemeError::new("wrong-number-of-args",vec![Value::symbol(name)]));}
                            if let Some(v)=self.eval_compiled_builtin_fast(*id,vals)?{v}else{func(self,vals)?}
                        };
                        stack.truncate(start);
                        stack.push(out);
                    }else{
                        let raw=stack.split_off(start);
                        let mut vals=Vec::new();
                        for v in raw{match v{Value::ValuesData(vs)=>vals.extend(vs),v=>vals.push(v)}}
                        if vals.len()<min || max.map(|m|vals.len()>m).unwrap_or(false){return Err(SchemeError::new("wrong-number-of-args",vec![Value::symbol(name)]));}
                        let out=if let Some(v)=self.eval_compiled_builtin_fast(*id,&vals)?{v}else{func(self,&vals)?};
                        stack.push(out);
                    }
                }
                Instr::GenericCall{argc}=>{
                    if stack.len()<argc+1{return Err(SchemeError::new("unsupported-compiled-form",vec![]));}
                    let start=stack.len()-*argc;
                    if matches!(&stack[start-1],Value::Macro(_,_)) || matches!(&stack[start-1],Value::RootMeta(name) if is_syntax_name(name.as_str())){return Err(SchemeError::new("unsupported-compiled-form",vec![]));}
                    if !stack[start..].iter().any(|v|matches!(v,Value::ValuesData(_))){
                        if *argc==1{
                            let direct={
                                let proc_ref=&stack[start-1];
                                let arg_ref=&stack[start];
                                match proc_ref{
                                    Value::HashTable(h)=>Some(Ok(hash_lookup(h,arg_ref).unwrap_or(Value::Bool(false)))),
                                    Value::Vector(_)|Value::ByteVector(_)|Value::FloatVector(_)|Value::IntVector(_)|Value::String(_)|Value::Pair(_)|Value::Env(_)|Value::MultiVector(_)|Value::MultiVectorView(_)|Value::ProcedureSource(_)=>Some(applicable_get(proc_ref,arg_ref)),
                                    _=>None,
                                }
                            };
                            if let Some(out)=direct{
                                let out=out?;
                                stack.truncate(start-1);
                                stack.push(out);
                                continue;
                            }
                        }
                        let direct_proc_result={
                            let proc_ref=&stack[start-1];
                            if let Value::Procedure(p)=proc_ref{
                                if let Procedure::Builtin{name,func,min,max,..}= &**p{
                                    let vals=&stack[start..];
                                    if vals.len()<*min || max.map(|m|vals.len()>m).unwrap_or(false){return Err(SchemeError::new("wrong-number-of-args",vec![Value::symbol(name)]));}
                                    let old=self.curlet.clone();
                                    self.curlet=env.clone();
                                    let out=func(self,vals);
                                    self.curlet=old;
                                    Some(out)
                                }else if let Procedure::Lambda{params,env:proc_env,compiled:Some(c),..}= &**p{
                                    if c.valid.get() && matches!(&c.layout,CompiledLayout::SlotFrame{..}) && !params.star && params.rest.is_none() && *argc==params.required.len(){
                                        let out=if c.capture_values.is_empty(){
                                            match c.bytecode.as_ref(){
                                                Some(bc)=>if let Some(value)=self.try_apply_compiled_list_filter(p,bc,proc_env,&stack[start..])?{value}else{match self.eval_bytecode_body(bc,proc_env.clone(),Some(&stack[start..])){Ok(v)=>v,Err(e) if e.tag=="unsupported-compiled-form"=>self.eval_compiled_body_slots(c,proc_env.clone(),&stack[start..])?,Err(e)=>return Err(e)}},
                                                None=>self.eval_compiled_body_slots(c,proc_env.clone(),&stack[start..])?,
                                            }
                                        }else{
                                            let mut slots=stack[start..].to_vec(); Self::append_compiled_captures(&mut slots,c);
                                            match c.bytecode.as_ref(){
                                                Some(bc)=>match self.eval_bytecode_body(bc,proc_env.clone(),Some(&slots)){Ok(v)=>v,Err(e) if e.tag=="unsupported-compiled-form"=>self.eval_compiled_body_slots(c,proc_env.clone(),&slots)?,Err(e)=>return Err(e)},
                                                None=>self.eval_compiled_body_slots(c,proc_env.clone(),&slots)?,
                                            }
                                        };
                                        Some(Ok(out))
                                    }else{None}
                                }else{None}
                            }else{None}
                        };
                        if let Some(out)=direct_proc_result{
                            let out=out?;
                            stack.truncate(start-1);
                            stack.push(out);
                            continue;
                        }
                    }
                    let proc=stack[start-1].clone();
                    let args_raw=stack.split_off(start);
                    stack.pop();
                    let mut vals=Vec::new();
                    for v in args_raw{match v{Value::ValuesData(vs)=>vals.extend(vs),v=>vals.push(v)}}
                    stack.push(self.apply_value(proc,vals,env.clone())?);
                }
                Instr::ApplicableRef=>{let index=stack.pop().ok_or_else(||SchemeError::new("unsupported-compiled-form",vec![]))?; let target=stack.pop().ok_or_else(||SchemeError::new("unsupported-compiled-form",vec![]))?; if let Value::HashTable(h)=&target{stack.push(hash_lookup(h,&index).unwrap_or(Value::Bool(false)));}else{stack.push(applicable_get(&target,&index)?);}}
                Instr::ApplicableRefDynamic{target,index}=>{let get_slot=|slot:usize|if slot<base_slots.len(){base_slots.get(slot)}else{temps.get(slot-base_slots.len())};let index=match index{ValueOperand::Slot(slot)=>get_slot(*slot),ValueOperand::Const(i)=>bc.constants.get(*i)}.ok_or_else(||SchemeError::new("unsupported-compiled-form",vec![]))?;let value=env.with_value(target.as_str(),|value|if let Value::HashTable(table)=value{Ok(hash_lookup(table,index).unwrap_or(Value::Bool(false)))}else{applicable_get(value,index)}).ok_or_else(||SchemeError::new("unbound-variable",vec![Value::string("unbound variable ~S"),Value::symbol(target.as_str())]))??;stack.push(value);}
                Instr::SetApplicable=>{let value=stack.pop().ok_or_else(||SchemeError::new("unsupported-compiled-form",vec![]))?; let index=stack.pop().ok_or_else(||SchemeError::new("unsupported-compiled-form",vec![]))?; let target=stack.pop().ok_or_else(||SchemeError::new("unsupported-compiled-form",vec![]))?; if let Value::Env(e)=&target{if !is_marked_immutable(&target)&&!matches!(index,Value::ValuesData(_))&&!matches!(value,Value::ValuesData(_)){let Some(key)=normalized_env_key(&index) else{return Err(SchemeError::new("wrong-type-arg",vec![Value::string("~A ~:D argument, ~S, is ~A but should be ~A"),Value::symbol("let-set!"),Value::Int(2),index.clone(),Value::string(simple_value_kind(&index)),Value::string("a symbol")]))};if e.set(key.as_ref(),value.clone()){stack.push(value);continue;}}} stack.push(self.set_applicable_with_setter(target,vec![index],value,env.clone())?);}
                Instr::SetApplicableDynamic{target,index}=>{let mut value=Some(stack.pop().ok_or_else(||SchemeError::new("unsupported-compiled-form",vec![]))?);let get_slot=|slot:usize|if slot<base_slots.len(){base_slots.get(slot)}else{temps.get(slot-base_slots.len())};let index=match index{ValueOperand::Slot(slot)=>get_slot(*slot),ValueOperand::Const(i)=>bc.constants.get(*i)}.ok_or_else(||SchemeError::new("unsupported-compiled-form",vec![]))?;let hash_result=env.with_value(target.as_str(),|target|if let Value::HashTable(table)=target{Some(hash_set_entry_mutating(table,index.clone(),value.take().unwrap(),"hash-table-set!",target))}else{None}).flatten();if let Some(result)=hash_result{stack.push(result?)}else{let target=env.get(target.as_str()).ok_or_else(||SchemeError::new("unbound-variable",vec![Value::string("unbound variable ~S"),Value::symbol(target.as_str())]))?;stack.push(self.set_applicable_with_setter(target,vec![index.clone()],value.take().unwrap(),env.clone())?);}}
                Instr::SetApplicableOperands{target,index}=>{let value=stack.pop().ok_or_else(||SchemeError::new("unsupported-compiled-form",vec![]))?;let get_slot=|slot:usize|if slot<base_slots.len(){base_slots.get(slot)}else{temps.get(slot-base_slots.len())};let target=get_slot(*target).ok_or_else(||SchemeError::new("unsupported-compiled-form",vec![]))?;let index=match index{ValueOperand::Slot(slot)=>get_slot(*slot),ValueOperand::Const(i)=>bc.constants.get(*i)}.ok_or_else(||SchemeError::new("unsupported-compiled-form",vec![]))?;if let Value::Env(environment)=target{if !is_marked_immutable(target){if let Some(key)=normalized_env_key(index){if environment.set(key.as_ref(),value.clone()){stack.push(value);continue}}}}if let Value::HashTable(table)=target{stack.push(hash_set_entry_mutating(table,index.clone(),value,"hash-table-set!",target)?)}else{stack.push(self.set_applicable_with_setter(target.clone(),vec![index.clone()],value,env.clone())?);}}
                Instr::QCons=>{let cdr=stack.pop().ok_or_else(||SchemeError::new("unsupported-compiled-form",vec![]))?;let car=stack.pop().ok_or_else(||SchemeError::new("unsupported-compiled-form",vec![]))?;if let Value::ValuesData(vs)=cdr{let mut items=vec![Value::list(vec![car])];items.extend(vs);return Err(SchemeError::new("wrong-number-of-args",vec![Value::string("~A: too many arguments: (~A~{~^ ~S~})"),Value::symbol("<list*>"),Value::symbol("<list*>"),Value::list(items)]));}if let Value::ValuesData(vs)=car{let mut out=cdr;for v in vs.into_iter().rev(){out=Value::cons(v,out);}stack.push(out);}else{stack.push(Value::cons(car,cdr));}}
                Instr::QSpliceCons=>{let tail=stack.pop().ok_or_else(||SchemeError::new("unsupported-compiled-form",vec![]))?;let spliced=stack.pop().ok_or_else(||SchemeError::new("unsupported-compiled-form",vec![]))?;let spliced=if let Value::ValuesData(vs)=spliced{if vs.len()!=1{return Err(SchemeError::new("wrong-number-of-args",vec![Value::string("~A: too many arguments: (~A~{~^ ~S~})"),Value::symbol("apply-values"),Value::symbol("apply-values"),Value::list(vs)]));}vs.into_iter().next().unwrap_or(Value::Nil)}else{spliced};let vals=spliced.to_vec().map_err(|_|SchemeError::new("wrong-type-arg",vec![Value::string("apply's last argument should be a proper list: ~S"),Value::list(vec![spliced.clone()])]))?;let mut out=tail;for v in vals.into_iter().rev(){out=Value::cons(v,out);}stack.push(out);}
                Instr::QVector(n)=>{if stack.len()<*n{return Err(SchemeError::new("unsupported-compiled-form",vec![]));}let vals=stack.split_off(stack.len()-*n);stack.push(Value::Vector(Rc::new(VectorData::new(vals))));}
                Instr::Fallback(i)=>{let expr=bc.constants.get(*i).cloned().ok_or_else(||SchemeError::new("unsupported-compiled-form",vec![]))?;let fb=self.materialize_bytecode_env(bc,pc-1,&env,base_slots,&temps);stack.push(self.eval(expr,fb)?);}
                Instr::MakeLambda(i)=>{let l=bc.lambdas.get(*i).ok_or_else(||SchemeError::new("unsupported-compiled-form",vec![]))?;let fb=self.materialize_bytecode_env(bc,pc-1,&env,base_slots,&temps);stack.push(self.compiled_lambda_value(l,fb));}
                Instr::ApplyLambda(i)=>{let tail=stack.pop().ok_or_else(||SchemeError::new("unsupported-compiled-form",vec![]))?;let l=bc.lambdas.get(*i).ok_or_else(||SchemeError::new("unsupported-compiled-form",vec![]))?;if let (Ok(args),Some(lambda_bc))=(tail.to_vec(),l.bytecode.as_ref()){if args.len()==l.params.len(){let fb=self.materialize_bytecode_env(bc,pc-1,&env,base_slots,&temps);match self.eval_bytecode_body(lambda_bc,fb,Some(&args)){Ok(v)=>{stack.push(v);continue},Err(e) if e.tag=="unsupported-compiled-form"=>{},Err(e)=>return Err(e)}}}let fb=self.materialize_bytecode_env(bc,pc-1,&env,base_slots,&temps);let proc=self.compiled_lambda_value(l,fb);let Some((func,_,_))=env.builtin_func("apply") else{return Err(SchemeError::new("unsupported-compiled-form",vec![]));};stack.push(func(self,&[proc,tail])?);}
                Instr::ListRefSlot(slot)=>{let index=stack.pop().ok_or_else(||SchemeError::new("unsupported-compiled-form",vec![]))?;let target=load_frame_slot(*slot,&temps).ok_or_else(||SchemeError::new("unsupported-compiled-form",vec![]))?;stack.push(list_at(target,&index)?);}
                Instr::ListRefConst(i)=>{let index=stack.pop().ok_or_else(||SchemeError::new("unsupported-compiled-form",vec![]))?;let target=bc.constants.get(*i).cloned().ok_or_else(||SchemeError::new("unsupported-compiled-form",vec![]))?;stack.push(list_at(target,&index)?);}
                Instr::Recur{argc,target,param_start,param_count,name}=>{
                    if stack.len()<*argc{return Err(SchemeError::new("unsupported-compiled-form",vec![]));}
                    let slot_start=param_start.checked_sub(base_slots.len()).ok_or_else(||SchemeError::new("unsupported-compiled-form",vec![]))?;
                    let start=stack.len()-*argc;
                    if *argc==*param_count && !stack[start..].iter().any(|v|matches!(v,Value::ValuesData(_))){
                        for i in (0..*param_count).rev(){let v=stack.pop().ok_or_else(||SchemeError::new("unsupported-compiled-form",vec![]))?; let v=normalize_loop_value(v); let Some(slot)=temps.get_mut(slot_start+i) else{return Err(SchemeError::new("unsupported-compiled-form",vec![]));}; *slot=v;}
                    }else{
                        let raw=stack.split_off(start);
                        let mut vals=Vec::new();
                        let mut first_values:Option<Vec<Value>>=None;
                        for v in raw{match v{Value::ValuesData(vs)=>{if first_values.is_none(){first_values=Some(vs.to_vec());} vals.extend(vs)},v=>vals.push(v)}}
                        if vals.len()!=*param_count{let shown=first_values.unwrap_or_else(||vals.clone()); return Err(SchemeError::new("wrong-number-of-args",vec![Value::string("~S: too many arguments: ~A"),Value::symbol(name.as_str()),Value::list(shown)]))}
                        for (i,v) in vals.into_iter().enumerate(){let v=normalize_loop_value(v); let Some(slot)=temps.get_mut(slot_start+i) else{return Err(SchemeError::new("unsupported-compiled-form",vec![]));}; *slot=v;}
                    }
                    pc=*target;
                }
                Instr::MakeValues(_)=>return Err(SchemeError::new("unsupported-compiled-form",vec![])),
                Instr::Return=>return Ok(stack.pop().unwrap_or(Value::Unspecified)),
            }
        }
        })();
        stack.clear();
        temps.clear();
        self.bytecode_stack_pool.push(stack);
        self.bytecode_temp_pool.push(temps);
        self.curlet=previous_curlet;
        result
    }

    fn eval_compiled_expr(&mut self, expr:&CExpr, env:EnvRef)->Result<Value>{
        let ctx=CompiledCtx{env,slots:None,slot_names:None,materialized_env:RefCell::new(None)};
        self.eval_compiled_expr_ctx(expr,&ctx)
    }

    fn eval_compiled_expr_ctx(&mut self, expr:&CExpr, ctx:&CompiledCtx<'_>)->Result<Value>{
        match self.eval_compiled_flow_ctx(expr,ctx)?{CompiledFlow::Value(v)=>Ok(v),CompiledFlow::Recur(_)=>Err(SchemeError::new("unsupported-compiled-form",vec![]))}
    }

    fn eval_compiled_values_ctx(&mut self, args:&[CExpr], ctx:&CompiledCtx<'_>)->Result<Vec<Value>>{
        let mut vals=Vec::new();
        for arg in args{match self.eval_compiled_expr_ctx(arg,ctx)?{Value::ValuesData(vs)=>vals.extend(vs),v=>vals.push(v)}}
        Ok(vals)
    }

    fn eval_quasiquote_template_ctx(&mut self, t:&QTemplate, ctx:&CompiledCtx<'_>)->Result<Value>{
        match t{
            QTemplate::Literal(v)=>Ok(v.clone()),
            QTemplate::Unquote(e)=>self.eval_compiled_expr_ctx(e,ctx),
            QTemplate::Splice(e)=>self.eval_compiled_expr_ctx(e,ctx),
            QTemplate::Vector(xs)=>{let mut out=Vec::with_capacity(xs.len()); for x in xs{out.push(self.eval_quasiquote_template_ctx(x,ctx)?);} Ok(Value::Vector(Rc::new(VectorData::new(out))))}
            QTemplate::Pair(car,cdr)=>{
                if let QTemplate::Splice(e)=&**car{
                    let spliced=self.eval_compiled_expr_ctx(e,ctx)?;
                    let spliced=if let Value::ValuesData(vs)=spliced{if vs.len()!=1{return Err(SchemeError::new("wrong-number-of-args",vec![Value::string("~A: too many arguments: (~A~{~^ ~S~})"),Value::symbol("apply-values"),Value::symbol("apply-values"),Value::list(vs)]));} vs.into_iter().next().unwrap_or(Value::Nil)}else{spliced};
                    let vals=spliced.to_vec().map_err(|_|SchemeError::new("wrong-type-arg",vec![Value::string("apply's last argument should be a proper list: ~S"),Value::list(vec![spliced.clone()])]))?;
                    let tail=self.eval_quasiquote_template_ctx(cdr,ctx)?;
                    let mut out=tail;
                    for v in vals.into_iter().rev(){out=Value::cons(v,out);}
                    return Ok(out);
                }
                let qcar=self.eval_quasiquote_template_ctx(car,ctx)?;
                let qcdr=self.eval_quasiquote_template_ctx(cdr,ctx)?;
                if let Value::ValuesData(vs)=qcdr{let mut items=vec![Value::list(vec![qcar])]; items.extend(vs); return Err(SchemeError::new("wrong-number-of-args",vec![Value::string("~A: too many arguments: (~A~{~^ ~S~})"),Value::symbol("<list*>"),Value::symbol("<list*>"),Value::list(items)]));}
                if let Value::ValuesData(vs)=qcar{let mut out=qcdr; for v in vs.into_iter().rev(){out=Value::cons(v,out);} Ok(out)}else{Ok(Value::cons(qcar,qcdr))}
            }
        }
    }

    fn compiled_lambda_value(&self,l:&compiled::CompiledLambda,env:EnvRef)->Value{let names=l.params.clone();let compiled=compiled::CompiledBody{env:env.clone(),exprs:l.body.clone(),layout:CompiledLayout::SlotFrame{params:names},bytecode:l.bytecode.clone(),native:None,native_lambda:None,capture_values:Vec::new(),valid:Rc::new(Cell::new(true))};Value::Procedure(Rc::new(Procedure::Lambda{params:Params{required:l.params.iter().map(|p|p.to_string()).collect(),rest:l.rest.as_ref().map(|r|r.to_string()),star:false,defaults:vec![None;l.params.len()],allow_other_keys:false,rest_before_formals:false},body:Rc::new(RefCell::new(l.source_body.clone())),env,name:l.name.as_ref().map(|n|n.to_string()),compiled:Some(Rc::new(compiled))}))}
    fn compiled_fallback_env(&self, ctx:&CompiledCtx<'_>)->EnvRef{
        match (ctx.slots,ctx.slot_names){
            (Some(slots),Some(names))=>{
                if let Some(env)=ctx.materialized_env.borrow().clone(){return env;}
                let env=Env::new(Some(ctx.env.clone()));
                for (name,val) in names.iter().zip(slots.iter()){env.define(name.as_str(),val.clone());}
                *ctx.materialized_env.borrow_mut()=Some(env.clone());
                env
            }
            _=>ctx.env.clone(),
        }
    }

    fn eval_compiled_flow_ctx(&mut self, expr:&CExpr, ctx:&CompiledCtx<'_>)->Result<CompiledFlow>{
        self.charge(1)?;
        match expr{
            CExpr::Const(v)=>Ok(CompiledFlow::Value(v.clone())),
            CExpr::Lambda(l)=>Ok(CompiledFlow::Value(self.compiled_lambda_value(l,self.compiled_fallback_env(ctx)))),
            CExpr::Var(VarRef::Dynamic{name})=>ctx.env.get(name.as_str()).map(CompiledFlow::Value).ok_or_else(||SchemeError::new("unbound-variable",vec![Value::string("unbound variable ~S"),Value::symbol(name.as_str())])),
            CExpr::Var(VarRef::Lexical{slot,..})=>ctx.slots.and_then(|slots|slots.get(*slot)).cloned().map(CompiledFlow::Value).ok_or_else(||SchemeError::new("unsupported-compiled-form",vec![])),
            CExpr::If{test,conseq,alt}=>{let t=self.eval_compiled_expr_ctx(test,ctx)?; if t.is_true(){self.eval_compiled_flow_ctx(conseq,ctx)}else{self.eval_compiled_flow_ctx(alt,ctx)}},
            CExpr::Begin(xs)=>{if xs.is_empty(){return Ok(CompiledFlow::Value(Value::Unspecified));} for x in &xs[..xs.len()-1]{self.eval_compiled_expr_ctx(x,ctx)?;} self.eval_compiled_flow_ctx(&xs[xs.len()-1],ctx)},
            CExpr::Let{sequential,bindings,body}=>{
                if ctx.slots.is_some(){self.eval_compiled_let_slots_flow(*sequential,bindings,body,ctx)}else{self.eval_compiled_let_flow(*sequential,bindings,body,ctx.env.clone())}
            }
            CExpr::Cond{clauses,else_body}=>self.eval_compiled_cond_flow(clauses,else_body.as_deref(),ctx),
            CExpr::Case{key,clauses,else_body}=>self.eval_compiled_case_flow(key,clauses,else_body.as_deref(),ctx),
            CExpr::And(xs)=>self.eval_compiled_and_flow(xs,ctx),
            CExpr::Or(xs)=>self.eval_compiled_or_flow(xs,ctx),
            CExpr::Quasiquote(v)=>{
                let qenv=self.compiled_fallback_env(ctx);
                self.eval_quasiquote((**v).clone(),qenv).map(CompiledFlow::Value)
            }
            CExpr::QuasiquoteTemplate(t)=>self.eval_quasiquote_template_ctx(t,ctx).map(CompiledFlow::Value),
            CExpr::Loop{name,params,inits,body,..}=>self.eval_compiled_loop(name,params,inits,body,ctx.env.clone()).map(CompiledFlow::Value),
            CExpr::Recur{args}=>self.eval_compiled_values_ctx(args,ctx).map(CompiledFlow::Recur),
            CExpr::BuiltinCall{id,name,args,fallback,..}=>{
                let Some((func,min,max))=ctx.env.builtin_func(name) else { return self.eval((**fallback).clone(),self.compiled_fallback_env(ctx)).map(CompiledFlow::Value); };
                let vals=self.eval_compiled_values_ctx(args,ctx)?;
                if vals.len()<min || max.map(|m|vals.len()>m).unwrap_or(false){return Err(SchemeError::new("wrong-number-of-args",vec![Value::symbol(name)]));}
                if let Some(v)=self.eval_compiled_builtin_fast(*id,&vals)?{return Ok(CompiledFlow::Value(v));}
                let old=self.curlet.clone();
                self.curlet=ctx.env.clone();
                let r=func(self,&vals);
                self.curlet=old;
                r.map(CompiledFlow::Value)
            }
            CExpr::Call{op,args,fallback,..}=>{
                let proc=self.eval_compiled_expr_ctx(op,ctx)?;
                if matches!(proc,Value::Macro(_,_)){return self.eval((**fallback).clone(),self.compiled_fallback_env(ctx)).map(CompiledFlow::Value);}
                if let Value::RootMeta(root_name)=&proc { if is_syntax_name(root_name.as_str()){
                    let rewritten=if let Value::Pair(_)=&**fallback{let rest=fallback.cdr().unwrap_or(Value::Nil); Value::cons(Value::symbol(root_name.as_str()),rest)}else{(**fallback).clone()};
                    return self.eval(rewritten,self.compiled_fallback_env(ctx)).map(CompiledFlow::Value);
                }}
                let vals=self.eval_compiled_values_ctx(args,ctx)?;
                self.apply_value(proc,vals,ctx.env.clone()).map(CompiledFlow::Value)
            }
            CExpr::ApplicableRef{target,index}=>{let target=self.eval_compiled_expr_ctx(target,ctx)?; let index=self.eval_compiled_expr_ctx(index,ctx)?; if let Value::HashTable(h)=&target{Ok(CompiledFlow::Value(hash_lookup(h,&index).unwrap_or(Value::Bool(false))))}else{applicable_get(&target,&index).map(CompiledFlow::Value)}}
            CExpr::SetApplicable{target,index,value}=>{let target=self.eval_compiled_expr_ctx(target,ctx)?; let index=self.eval_compiled_expr_ctx(index,ctx)?; let value=self.eval_compiled_expr_ctx(value,ctx)?; if let Value::Env(e)=&target{if !is_marked_immutable(&target)&&!matches!(index,Value::ValuesData(_))&&!matches!(value,Value::ValuesData(_)){let Some(key)=normalized_env_key(&index) else{return Err(SchemeError::new("wrong-type-arg",vec![Value::string("~A ~:D argument, ~S, is ~A but should be ~A"),Value::symbol("let-set!"),Value::Int(2),index.clone(),Value::string(simple_value_kind(&index)),Value::string("a symbol")]))};if e.set(key.as_ref(),value.clone()){return Ok(CompiledFlow::Value(value));}}} set_applicable(target,vec![index],value).map(CompiledFlow::Value)}
            CExpr::Fallback(v)=>self.eval(v.clone(),self.compiled_fallback_env(ctx)).map(CompiledFlow::Value),
            _=>Err(SchemeError::new("unsupported-compiled-form",vec![])),
        }
    }

    fn eval_compiled_builtin_fast(&mut self, id:BuiltinId, vals:&[Value])->Result<Option<Value>>{
        match id{
            BuiltinId::Add if vals.iter().all(|v|matches!(v,Value::Int(_)))=>{
                let mut acc=0i64;
                for v in vals{let Value::Int(n)=v else{unreachable!()}; let Some(next)=acc.checked_add(*n) else{return Ok(None)}; acc=next;}
                Ok(Some(Value::Int(acc)))
            }
            BuiltinId::Sub if !vals.is_empty() && vals.iter().all(|v|matches!(v,Value::Int(_)))=>{
                let mut it=vals.iter(); let Value::Int(first)=it.next().unwrap() else{unreachable!()};
                if vals.len()==1{return first.checked_neg().map(|n|Some(Value::Int(n))).ok_or_else(most_negative_negation_error);}
                let mut acc=*first;
                for v in it{let Value::Int(n)=v else{unreachable!()}; let Some(next)=acc.checked_sub(*n) else{return Ok(None)}; acc=next;}
                Ok(Some(Value::Int(acc)))
            }
            BuiltinId::Mul if vals.iter().all(|v|matches!(v,Value::Int(_)))=>{
                let mut acc=1i64;
                for v in vals{let Value::Int(n)=v else{unreachable!()}; let Some(next)=acc.checked_mul(*n) else{return Ok(None)}; acc=next;}
                Ok(Some(Value::Int(acc)))
            }
            BuiltinId::NumEq|BuiltinId::Less|BuiltinId::LessEq|BuiltinId::Greater|BuiltinId::GreaterEq if vals.iter().all(|v|matches!(v,Value::Int(_)))=>{
                let mut ok=true;
                for w in vals.windows(2){let (Value::Int(a),Value::Int(b))=(&w[0],&w[1]) else{unreachable!()}; ok &= match id{BuiltinId::NumEq=>a==b,BuiltinId::Less=>a<b,BuiltinId::LessEq=>a<=b,BuiltinId::Greater=>a>b,BuiltinId::GreaterEq=>a>=b,_=>unreachable!()}; if !ok{break;}}
                Ok(Some(Value::Bool(ok)))
            }
            BuiltinId::VectorRef if vals.len()==2=>{
                if let (Value::Vector(v),Value::Int(i))=(&vals[0],&vals[1]){if *i>=0 && (*i as usize)<v.len(){return Ok(Some(v.get(*i as usize)));}}
                Ok(None)
            }
            BuiltinId::VectorSet if vals.len()==3=>{
                if let (Value::Vector(v),Value::Int(i))=(&vals[0],&vals[1]){if !is_marked_immutable(&vals[0]) && *i>=0 && (*i as usize)<v.len(){v.set(*i as usize,vals[2].clone()); return Ok(Some(vals[2].clone()));}}
                Ok(None)
            }
            BuiltinId::ByteVectorRef if vals.len()==2=>{if let (Value::ByteVector(v),Value::Int(i))=(&vals[0],&vals[1]){let values=v.borrow();if *i>=0&&(*i as usize)<values.len(){return Ok(Some(Value::Int(values[*i as usize] as i64)));}}Ok(None)}
            BuiltinId::ByteVectorSet if vals.len()==3=>{if let (Value::ByteVector(v),Value::Int(i),Value::Int(n))=(&vals[0],&vals[1],&vals[2]){if !is_marked_immutable(&vals[0])&&*i>=0&&(*i as usize)<v.borrow().len()&&(0..=255).contains(n){v.borrow_mut()[*i as usize]=*n as u8;return Ok(Some(Value::Int(*n)));}}Ok(None)}
            BuiltinId::HashRef if vals.len()==2=>{if let Value::HashTable(h)=&vals[0]{return Ok(Some(hash_lookup(h,&vals[1]).unwrap_or(Value::Bool(false))))}Ok(None)}
            BuiltinId::HashSet if vals.len()==3=>{if let Value::HashTable(h)=&vals[0]{if is_marked_immutable(&vals[0]){return Ok(None)}return hash_set_entry_mutating(h,vals[1].clone(),vals[2].clone(),"hash-table-set!",&vals[0]).map(Some)}Ok(None)}
            BuiltinId::Remainder if vals.len()==2=>{
                if let (Value::Int(a),Value::Int(b))=(&vals[0],&vals[1]){if *b!=0{return Ok(Some(Value::Int(a%b)));}}
                Ok(None)
            }
            BuiltinId::Modulo if vals.len()==2=>{
                if let (Value::Int(a),Value::Int(b))=(&vals[0],&vals[1]){if *b>0{return Ok(Some(Value::Int(((a%b)+b)%b)));}}
                Ok(None)
            }
            BuiltinId::List=>Ok(Some(Value::list_slice(vals))),
            BuiltinId::Cons if vals.len()==2=>Ok(Some(Value::cons(vals[0].clone(),vals[1].clone()))),
            BuiltinId::Car if vals.len()==1=>{if matches!(vals[0],Value::Pair(_)){Ok(Some(vals[0].car()?))}else{Ok(None)}},
            BuiltinId::Cdr if vals.len()==1=>{if matches!(vals[0],Value::Pair(_)){Ok(Some(vals[0].cdr()?))}else{Ok(None)}},
            BuiltinId::NullP if vals.len()==1=>Ok(Some(Value::Bool(matches!(vals[0],Value::Nil)))),
            BuiltinId::PairP if vals.len()==1=>Ok(Some(Value::Bool(matches!(vals[0],Value::Pair(_))))),
            BuiltinId::Not if vals.len()==1=>Ok(Some(Value::Bool(!vals[0].is_true()))),
            BuiltinId::EqP if vals.len()==2=>Ok(Some(Value::Bool(eq(&vals[0],&vals[1])))),
            BuiltinId::Inlet=>{
                let cap=vals.len()/2;
                let environment=Env::with_capacity(Some(self.root.clone()),cap);
                let mut index=0;
                while index+1<vals.len(){
                    let key=match &vals[index]{
                        Value::Symbol(symbol)=>symbol.trim_start_matches('+').trim_end_matches('+').to_string(),
                        Value::Keyword(keyword)=>keyword.trim_start_matches(':').trim_start_matches('+').trim_end_matches('+').trim_end_matches(':').to_string(),
                        _=>return Ok(None),
                    };
                    if environment.vars.borrow().contains_key(&key){return Ok(None)}
                    environment.define_fresh(key,vals[index+1].clone());
                    index+=2;
                }
                Ok(Some(Value::Env(environment)))
            }
            BuiltinId::HashTable=>{
                if vals.len()%2!=0{return Ok(None)}
                fn same_simple_key(a:&Value,b:&Value)->Option<bool>{Some(match (a,b){
                    (Value::Symbol(x),Value::Symbol(y))=>x==y,
                    (Value::Keyword(x),Value::Keyword(y))=>x==y,
                    (Value::Bool(x),Value::Bool(y))=>x==y,
                    (Value::Char(x),Value::Char(y))=>x==y,
                    (Value::NamedChar(x),Value::NamedChar(y))=>x==y,
                    (Value::Nil,Value::Nil)=>true,
                    (Value::Int(x),Value::Int(y))=>x==y,
                    (Value::Symbol(_),_)|(Value::Keyword(_),_)|(Value::Bool(_),_)|(Value::Char(_),_)|(Value::NamedChar(_),_)|(Value::Nil,_)|(Value::Int(_),_)=>false,
                    _=>return None,
                })}
                let mut entries=Vec::with_capacity(vals.len()/2);
                let mut i=0;
                while i+1<vals.len(){
                    let key=&vals[i];
                    for (k,_) in &entries{match same_simple_key(k,key){Some(true)=>return Ok(None),Some(false)=>{},None=>return Ok(None)}}
                    entries.push((key.clone(),vals[i+1].clone()));
                    i+=2;
                }
                Ok(Some(Value::HashTable(Rc::new(RefCell::new(entries)))))
            }
            _=>Ok(None),
        }
    }

    fn eval_compiled_body_exprs_flow(&mut self, body:&[CExpr], ctx:&CompiledCtx<'_>)->Result<CompiledFlow>{
        if body.is_empty(){return Ok(CompiledFlow::Value(Value::Unspecified));}
        for expr in &body[..body.len()-1]{self.eval_compiled_expr_ctx(expr,ctx)?;}
        self.eval_compiled_flow_ctx(&body[body.len()-1],ctx)
    }

    fn eval_compiled_cond_flow(&mut self, clauses:&[(CExpr,Vec<CExpr>)], else_body:Option<&[CExpr]>, ctx:&CompiledCtx<'_>)->Result<CompiledFlow>{
        for (test,body) in clauses{
            if self.eval_compiled_expr_ctx(test,ctx)?.is_true(){return self.eval_compiled_body_exprs_flow(body,ctx);}
        }
        if let Some(body)=else_body{return self.eval_compiled_body_exprs_flow(body,ctx);}
        Ok(CompiledFlow::Value(Value::Unspecified))
    }

    fn eval_compiled_case_flow(&mut self, key_expr:&CExpr, clauses:&[(Vec<Value>,Vec<CExpr>)], else_body:Option<&[CExpr]>, ctx:&CompiledCtx<'_>)->Result<CompiledFlow>{
        let key=match self.eval_compiled_expr_ctx(key_expr,ctx)?{Value::ValuesData(vs)=>vs.get(0).cloned().unwrap_or(Value::Unspecified),v=>v};
        for (datums,body) in clauses{if datums.iter().any(|datum|equal(&key,datum)){return if body.is_empty(){Ok(CompiledFlow::Value(key))}else{self.eval_compiled_body_exprs_flow(body,ctx)}}}
        if let Some(body)=else_body{return if body.is_empty(){Ok(CompiledFlow::Value(key))}else{self.eval_compiled_body_exprs_flow(body,ctx)}}
        Ok(CompiledFlow::Value(Value::Unspecified))
    }

    fn eval_compiled_and_flow(&mut self, xs:&[CExpr], ctx:&CompiledCtx<'_>)->Result<CompiledFlow>{
        let mut last=Value::Bool(true);
        for (idx,x) in xs.iter().enumerate(){
            let finalp=idx+1==xs.len();
            match self.eval_compiled_expr_ctx(x,ctx)?{
                Value::ValuesData(vs)=>{if finalp && vs.iter().all(|v|v.is_true()){last=Value::list(vs);}else{for v in vs{last=v;if !last.is_true(){return Ok(CompiledFlow::Value(last));}}}}
                v=>{last=v;if !last.is_true(){return Ok(CompiledFlow::Value(last));}}
            }
        }
        Ok(CompiledFlow::Value(last))
    }

    fn eval_compiled_or_flow(&mut self, xs:&[CExpr], ctx:&CompiledCtx<'_>)->Result<CompiledFlow>{
        for (idx,x) in xs.iter().enumerate(){
            let finalp=idx+1==xs.len();
            match self.eval_compiled_expr_ctx(x,ctx)?{
                Value::ValuesData(vs)=>{if finalp && vs.iter().any(|v|v.is_true()){return Ok(CompiledFlow::Value(Value::list(vs)));} for v in vs{if v.is_true(){return Ok(CompiledFlow::Value(v));}}}
                v=>{if v.is_true(){return Ok(CompiledFlow::Value(v));}}
            }
        }
        Ok(CompiledFlow::Value(Value::Bool(false)))
    }

    fn eval_compiled_let_flow(&mut self, sequential:bool, bindings:&[(Rc<String>,CExpr)], body:&[CExpr], env:EnvRef)->Result<CompiledFlow>{
        let new_env=Env::new(Some(env.clone()));
        if sequential{
            for (name,expr) in bindings{
                let val=self.eval_compiled_expr(expr,new_env.clone())?;
                let val=self.normalize_binding_value_ctx("let*",name.as_str(),val)?;
                new_env.define(name.as_str(),val);
            }
        }else{
            let mut vals=Vec::with_capacity(bindings.len());
            for (name,expr) in bindings{
                let val=self.eval_compiled_expr(expr,env.clone())?;
                vals.push((name.clone(),self.normalize_binding_value_ctx("let",name.as_str(),val)?));
            }
            for (name,val) in vals{new_env.define(name.as_str(),val);}
        }
        if body.is_empty(){return Ok(CompiledFlow::Value(Value::Unspecified));}
        let old=self.curlet.clone();
        self.curlet=new_env.clone();
        let ctx=CompiledCtx{env:new_env.clone(),slots:None,slot_names:None,materialized_env:RefCell::new(None)};
        let result=(||{for expr in &body[..body.len()-1]{self.eval_compiled_expr_ctx(expr,&ctx)?;} self.eval_compiled_flow_ctx(&body[body.len()-1],&ctx)})();
        self.curlet=old;
        result
    }

    fn eval_compiled_let_slots_flow(&mut self, sequential:bool, bindings:&[(Rc<String>,CExpr)], body:&[CExpr], ctx:&CompiledCtx<'_>)->Result<CompiledFlow>{
        let base_slots=ctx.slots.unwrap_or(&[]);
        let base_names=ctx.slot_names.unwrap_or(&[]);
        let mut new_slots=base_slots.to_vec();
        let mut new_names=base_names.to_vec();
        if sequential{
            for (name,expr) in bindings{
                let step_ctx=CompiledCtx{env:ctx.env.clone(),slots:Some(&new_slots),slot_names:Some(&new_names),materialized_env:RefCell::new(None)};
                let val=self.eval_compiled_expr_ctx(expr,&step_ctx)?;
                let val=self.normalize_binding_value_ctx("let*",name.as_str(),val)?;
                new_slots.push(val);
                new_names.push(name.clone());
            }
        }else{
            let mut vals=Vec::with_capacity(bindings.len());
            for (name,expr) in bindings{
                let val=self.eval_compiled_expr_ctx(expr,ctx)?;
                vals.push((name.clone(),self.normalize_binding_value_ctx("let",name.as_str(),val)?));
            }
            for (name,val) in vals{new_slots.push(val); new_names.push(name);}
        }
        if body.is_empty(){return Ok(CompiledFlow::Value(Value::Unspecified));}
        let body_ctx=CompiledCtx{env:ctx.env.clone(),slots:Some(&new_slots),slot_names:Some(&new_names),materialized_env:RefCell::new(None)};
        for expr in &body[..body.len()-1]{self.eval_compiled_expr_ctx(expr,&body_ctx)?;}
        self.eval_compiled_flow_ctx(&body[body.len()-1],&body_ctx)
    }

    fn eval_compiled_loop(&mut self, name:&Rc<String>, params:&[Rc<String>], inits:&[CExpr], body:&CExpr, env:EnvRef)->Result<Value>{
        let mut slots=Vec::with_capacity(inits.len());
        for (name,expr) in params.iter().zip(inits.iter()){
            let val=self.eval_compiled_expr(expr,env.clone())?;
            slots.push(self.normalize_binding_value_ctx("let",name.as_str(),val)?);
        }
        loop{
            let ctx=CompiledCtx{env:env.clone(),slots:Some(&slots),slot_names:Some(params),materialized_env:RefCell::new(None)};
            match self.eval_compiled_flow_ctx(body,&ctx)?{
                CompiledFlow::Value(v)=>return Ok(v),
                CompiledFlow::Recur(vals)=>{
                    if vals.len()!=params.len(){return Err(SchemeError::new("wrong-number-of-args",vec![Value::string("~S: too many arguments: ~A"),Value::symbol(name.as_str()),Value::list(vals)]));}
                    slots=vals;
                }
            }
        }
    }

    fn eval_sequence(&mut self, xs: Vec<Value>, env: EnvRef) -> Result<Value> {
        let old=self.curlet.clone();
        self.curlet=env.clone();
        let result = if xs.is_empty() {
            Ok(Value::Unspecified)
        } else {
            let last_idx=xs.len()-1;
            for x in xs[..last_idx].iter().cloned() { if let Err(e)=self.eval(x.clone(), env.clone()){ if e.tag=="unbound-variable" && x.car().ok().and_then(|v|v.as_symbol().map(|s|s=="vector-fill!")).unwrap_or(false){return Err(SchemeError::new("unbound-variable",vec![Value::string("unbound variable ~S in ~S"),Value::symbol("vector-fill!"),Value::list(xs.clone())]));} return Err(e);} }
            self.eval_tail(xs[last_idx].clone(), env.clone())
        };
        self.curlet=old;
        result
    }
    fn eval_tail(&mut self, mut expr: Value, mut env: EnvRef) -> Result<Value> {
        loop {
            self.charge(1)?;
            match expr {
                Value::Symbol(s) => return env.get(&s).ok_or_else(|| SchemeError::new("unbound-variable", vec![Value::string("unbound variable ~S"), Value::symbol(&s)])),
                Value::Commented(v) => return Ok(Value::Commented(Box::new(self.eval(*v, env)?))),
                Value::Pair(_) => {
                    let op=expr.car()?;
                    let args=expr.cdr()?;
                    if let Some(sym)=op.as_symbol() {
                        match sym {
                            "quote" => { let xs=args.to_vec()?; if xs.len()!=1{return Err(SchemeError::new("syntax-error",vec![Value::symbol("quote")]))}; let value=xs[0].clone();if let Value::Pair(pair)=&value{pair.mark_quoted_result();}return Ok(value); }
                            "quasiquote" => return self.eval_quasiquote(args.car()?, env),
                            "if" => {
                                let test_expr=args.car().map_err(|_|SchemeError::new("syntax-error",vec![Value::symbol("if")]))?;
                                let rest=args.cdr()?;
                                let then_expr=rest.car().map_err(|_|SchemeError::new("syntax-error",vec![Value::symbol("if")]))?;
                                let alt_expr=match rest.cdr()?{Value::Pair(p)=>{let PairData{car,..}= &*p.borrow(); car.clone()},_=>Value::Unspecified};
                                let test=self.eval(test_expr, env.clone())?;
                                expr=if test.is_true(){ then_expr } else { alt_expr };
                                continue;
                            }
                            "begin" => {
                                let mut cur=args;
                                if matches!(cur,Value::Nil){return Ok(Value::Unspecified)}
                                loop{match cur{Value::Pair(p)=>{let (car,cdr)={let PairData{car,cdr}= &*p.borrow(); (car.clone(),cdr.clone())}; if matches!(cdr,Value::Nil){expr=car; break;} self.eval(car,env.clone())?; cur=cdr;},other=>return Err(SchemeError::new("wrong-type-arg",vec![other]))}}
                                continue;
                            }
                            "and" => {
                                let xs=args.to_vec()?;
                                if xs.is_empty(){return Ok(Value::Bool(true))}
                                let mut last=Value::Bool(true);
                                for x in xs { match self.eval(x, env.clone())? { Value::ValuesData(vs)=>{ for v in vs { last=v; if !last.is_true(){return Ok(last);} } }, v=>{ last=v; if !last.is_true(){return Ok(last);} } } }
                                return Ok(last);
                            }
                            "or" => {
                                let xs=args.to_vec()?;
                                if xs.is_empty(){return Ok(Value::Bool(false))}
                                for x in xs { match self.eval(x, env.clone())? { Value::ValuesData(vs)=>{ for v in vs { if v.is_true(){return Ok(v);} } }, v=>{ if v.is_true(){return Ok(v);} } } }
                                return Ok(Value::Bool(false));
                            }
                            "let-temporarily" => return self.eval_let_temporarily(args, env),
                            "let" | "let*" => {
                                let sequential=sym=="let*";
                                let first_arg=args.car()?;
                                if !matches!(first_arg,Value::Symbol(_)) {
                                    let new_env=Env::new(Some(env.clone()));
                                    if sequential {
                                        let mut cur=first_arg;
                                        loop{match cur{Value::Nil=>break,Value::Pair(p)=>{let (binding,next)={let PairData{car,cdr}= &*p.borrow(); (car.clone(),cdr.clone())}; let name_v=binding.car()?; let name=name_v.as_symbol().ok_or_else(||SchemeError::new("syntax-error",vec![name_v.clone()]))?.to_string(); let val_expr=binding.cdr()?.car()?; let val=self.eval(val_expr,new_env.clone())?; let val=self.normalize_binding_value_ctx("let*",&name,val)?; new_env.define(name,val); cur=next;},other=>return Err(SchemeError::new("wrong-type-arg",vec![other]))}}
                                    } else {
                                        let let_src=Value::list({let mut v=vec![Value::symbol("let"),first_arg.clone()]; v.extend(args.cdr()?.to_vec()?); v});
                                        let mut seen=HashSet::new(); let mut vals=Vec::new(); let mut cur=first_arg;
                                        loop{match cur{Value::Nil=>break,Value::Pair(p)=>{let (binding,next)={let PairData{car,cdr}= &*p.borrow(); (car.clone(),cdr.clone())}; let name_v=binding.car()?; let name=name_v.as_symbol().ok_or_else(||SchemeError::new("syntax-error",vec![name_v.clone()]))?.to_string(); if !seen.insert(name.clone()){return Err(SchemeError::new("syntax-error",vec![Value::string("duplicate identifier in let: ~S in ~S"),Value::symbol(&name),let_src]));} let val_expr=binding.cdr()?.car()?; let val=self.eval(val_expr,env.clone())?; vals.push((name.clone(),self.normalize_binding_value_ctx("let",&name,val)?)); cur=next;},other=>return Err(SchemeError::new("wrong-type-arg",vec![other]))}}
                                        for (k,v) in vals{new_env.define(k,v);}
                                    }
                                    let mut body=args.cdr()?;
                                    if matches!(body,Value::Nil){return Ok(Value::Unspecified)}
                                    loop{match body{Value::Pair(p)=>{let (car,cdr)={let PairData{car,cdr}= &*p.borrow(); (car.clone(),cdr.clone())}; if matches!(cdr,Value::Nil){expr=car; env=new_env.clone(); self.curlet=new_env; break;} self.eval(car,new_env.clone())?; body=cdr;},other=>return Err(SchemeError::new("wrong-type-arg",vec![other]))}}
                                    continue;
                                }
                                let xs=args.to_vec()?;
                                if let Some(Value::Symbol(name))=xs.get(0) {
                                    let bindings=xs[1].to_vec()?;
                                    let params=bindings.iter().map(|b| b.car().unwrap().as_symbol().unwrap().to_string()).collect::<Vec<_>>();
                                    let vals_expr=bindings.iter().map(|b| b.cdr().unwrap().car().unwrap()).collect::<Vec<_>>();
                                    if !sequential { if let Some(compiled_loop)=self.analyze_named_let_cached(env.clone(),name,&params,&vals_expr,&xs[2..]){ return self.eval_compiled_body(&compiled_loop,env); } }
                                    let new_env=Env::new(Some(env.clone()));
                                    let proc=Value::Procedure(Rc::new(Procedure::Lambda{params:Params{required:params.clone(),rest:None,star:sequential,defaults:vec![None; params.len()],allow_other_keys:false,rest_before_formals:false},body:Rc::new(RefCell::new(xs[2..].to_vec())),env:new_env.clone(),name:Some(name.to_string()),compiled:None}));
                                    new_env.define(name.as_str(), proc.clone());
                                    let vals=vals_expr.into_iter().map(|v| self.eval(v, env.clone())).collect::<Result<Vec<_>>>()?;
                                    self.charge(1)?;
                                    if let Value::Procedure(p)=proc { if let Procedure::Lambda{params,body,env:proc_env,..}= &*p {
                                        let call_env=Env::new(Some(proc_env.clone()));
                                        bind_params(self, &call_env, params, vals, env.clone())?;
                                        let body_vec=body.borrow().clone();
                                        if body_vec.is_empty(){return Ok(Value::Unspecified)}
                                        for x in body_vec[..body_vec.len()-1].iter().cloned(){ self.eval(x, call_env.clone())?; }
                                        expr=body_vec[body_vec.len()-1].clone(); env=call_env.clone(); self.curlet=call_env; continue;
                                    }}
                                }
                                let bindings=xs[0].to_vec()?;
                                let new_env=Env::new(Some(env.clone()));
                                if sequential { for b in bindings { let bv=b.to_vec()?; let name=bv[0].as_symbol().unwrap(); let val=self.eval(bv[1].clone(), new_env.clone())?; let val=self.normalize_binding_value_ctx("let*",name,val)?; new_env.define(name, val); } }
                                else { let let_src=Value::list({let mut v=vec![Value::symbol("let")]; v.extend(xs.clone()); v}); let mut seen=HashSet::new(); let mut vals=Vec::new(); for b in &bindings { let bv=b.to_vec()?; let name=bv[0].as_symbol().unwrap().to_string(); if !seen.insert(name.clone()){return Err(SchemeError::new("syntax-error",vec![Value::string("duplicate identifier in let: ~S in ~S"),Value::symbol(&name),let_src]));} let val=self.eval(bv[1].clone(), env.clone())?; vals.push((name.clone(), self.normalize_binding_value_ctx("let",&name,val)?)); } for (k,v) in vals { new_env.define(k,v); } }
                                let body=&xs[1..];
                                if body.is_empty(){return Ok(Value::Unspecified)}
                                for x in body[..body.len()-1].iter().cloned(){ self.eval(x, new_env.clone())?; }
                                expr=body[body.len()-1].clone(); env=new_env.clone(); self.curlet=new_env; continue;
                            }
                            "letrec" | "letrec*" => {
                                let xs=args.to_vec()?; let bindings=xs[0].to_vec()?; let new_env=Env::new(Some(env.clone()));
                                for b in &bindings { new_env.define(b.car()?.as_symbol().unwrap(), Value::Unspecified); }
                                for b in bindings { let bv=b.to_vec()?; let name=bv[0].as_symbol().unwrap(); let val=self.eval(bv[1].clone(), new_env.clone())?; let val=self.normalize_binding_value_ctx(sym,name,val)?; new_env.set(name, val); }
                                let body=&xs[1..];
                                if body.is_empty(){return Ok(Value::Unspecified)}
                                for x in body[..body.len()-1].iter().cloned(){ self.eval(x, new_env.clone())?; }
                                expr=body[body.len()-1].clone(); env=new_env.clone(); self.curlet=new_env; continue;
                            }
                            "when" => {
                                let xs=args.to_vec()?; if self.eval(xs[0].clone(), env.clone())?.is_true(){ let body=xs[1..].to_vec(); if body.is_empty(){return Ok(Value::Unspecified)}; for x in body[..body.len()-1].iter().cloned(){ self.eval(x, env.clone())?; } expr=body[body.len()-1].clone(); continue; } else { return Ok(Value::Unspecified); }
                            }
                            "unless" => {
                                let xs=args.to_vec()?; if !self.eval(xs[0].clone(), env.clone())?.is_true(){ let body=xs[1..].to_vec(); if body.is_empty(){return Ok(Value::Unspecified)}; for x in body[..body.len()-1].iter().cloned(){ self.eval(x, env.clone())?; } expr=body[body.len()-1].clone(); continue; } else { return Ok(Value::Unspecified); }
                            }
                            "cond" => {
                                let mut matched: Option<Vec<Value>>=None;
                                for clause in args.to_vec()? { let xs=clause.to_vec()?; if xs[0].as_symbol()==Some("else") || self.eval(xs[0].clone(), env.clone())?.is_true() { matched=Some(xs[1..].to_vec()); break; } }
                                if let Some(body)=matched { if body.is_empty(){return Ok(Value::Unspecified)}; for x in body[..body.len()-1].iter().cloned(){ self.eval(x, env.clone())?; } expr=body[body.len()-1].clone(); continue; }
                                return Ok(Value::Unspecified);
                            }
                            "case" => return self.eval_case(args, env),
                            "with-let" => {
                                let xs=args.to_vec()?; let e=self.eval(xs[0].clone(), env.clone())?; let e=if let Value::ValuesData(vs)=e{vs.get(0).cloned().unwrap_or(Value::Unspecified)}else{e}; let Value::Env(new_env)=e else { return Err(SchemeError::new("wrong-type-arg", vec![e])); };
                                let body=&xs[1..]; if body.is_empty(){return Ok(Value::Unspecified)}; for x in body[..body.len()-1].iter().cloned(){ self.eval(x, new_env.clone())?; }
                                expr=body[body.len()-1].clone(); env=new_env.clone(); self.curlet=new_env; continue;
                            }
                            "define" | "define*" | "set!" | "lambda" | "lambda*" | "do" | "catch" | "throw" | "define-macro" | "define-macro*" | "define-bacro" | "define-bacro*" | "macro" | "macro*" | "bacro" | "bacro*" | "macroexpand" => return self.eval_pair(expr, env),
                            _=>{}
                        }
                        if let Some(r)=self.eval_hot_builtin(sym,args.clone(),env.clone()){return r;}
                    }
                    let proc=self.eval(op.clone(), env.clone())?;
                    if let Value::RootMeta(name)=&proc{ if name.as_str()=="and"{let mut last=Value::Bool(true); for a in args.to_vec()?{match self.eval(a,env.clone())?{Value::ValuesData(vs)=>{for v in vs{last=v;if !last.is_true(){return Ok(last)}}},v=>{last=v;if !last.is_true(){return Ok(last)}}}} return Ok(last)} if name.as_str()=="or"{for a in args.to_vec()?{match self.eval(a,env.clone())?{Value::ValuesData(vs)=>{for v in vs{if v.is_true(){return Ok(v)}}},v=>{if v.is_true(){return Ok(v)}}}} return Ok(Value::Bool(false))} }
                    match proc {
                        Value::Macro(macro_data,_) => {let p=macro_data.procedure.clone();let kind=macro_data.kind;
                            let raw=args.to_vec()?;
                            let expanded=if matches!(kind,MacroKind::Macro) { if let Some(v)=self.cached_macro_expansion(&expr,&p){v}else{let v=self.apply_proc(&p, raw, env.clone())?; self.store_macro_expansion(&expr,&p,&v); v} } else { match &*p { Procedure::Lambda{params,body,..}=>{let new_env=Env::new(Some(env.clone())); bind_params(self,&new_env,params,raw.clone(),env.clone())?; self.eval_lambda_body(body,new_env)?}, _=>self.apply_proc(&p, raw, env.clone())?} };
                            if let Value::ValuesData(vs)=expanded { let mut out=Vec::new(); for x in vs { out.push(self.eval(x, env.clone())?); } return Ok(Value::list(out)); }
                            expr=expanded;
                            continue;
                        }
                        Value::Procedure(p) => {
                            let vals=self.eval_list(args, env.clone())?;
                            self.charge(1)?;
                            match &*p {
                                Procedure::Builtin{name,func,min,max,..} => {
                                    if vals.len()<*min || max.map(|m| vals.len()>m).unwrap_or(false){ return Err(SchemeError::new("wrong-number-of-args", vec![Value::symbol(name)])); }
                                    return func(self,&vals);
                                }
                                Procedure::Lambda{params,body,env:proc_env,name,..} => {
                                    let self_tail_call=name.as_deref().zip(op.as_symbol()).map(|(a,b)|a==b).unwrap_or(false);
                                    if !params.star && vals.len()<params.required.len(){ let form=Value::list(vec![Value::symbol("lambda"), Value::list(params.required.iter().map(|n|Value::symbol(n)).collect::<Vec<_>>()), body.borrow().get(0).cloned().unwrap_or(Value::Unspecified)]); return Err(SchemeError::new("wrong-number-of-args", vec![Value::string("~S: not enough arguments: ((~S ~S ...)~{~^ ~S~})"), Value::list(vec![form]), Value::symbol("lambda"), Value::list(params.required.iter().map(|n|Value::symbol(n)).collect::<Vec<_>>()), Value::Nil])); }
                                    if self_tail_call && !params.star && params.rest.is_none() && vals.len()==params.required.len() {
                                        let parent_matches={env.parent.borrow().as_ref().map(|parent|Rc::ptr_eq(parent,proc_env)).unwrap_or(false)};
                                        let frame_has_params={let vars=env.vars.borrow(); params.required.iter().all(|name|vars.contains_key(name))};
                                        if parent_matches && frame_has_params {
                                            env.set_local_existing_many(params.required.iter().cloned().zip(vals.into_iter().map(normalize_loop_value)));
                                            let len=body.borrow().len();
                                            if len==0{return Ok(Value::Unspecified)}
                                            for i in 0..len-1{ let x=body.borrow().get(i).cloned().unwrap_or(Value::Unspecified); self.eval(x, env.clone())?; }
                                            expr=body.borrow().get(len-1).cloned().unwrap_or(Value::Unspecified);
                                            continue;
                                        }
                                    }
                                    let new_env=Env::new(Some(proc_env.clone()));
                                    if let Err(e)=bind_params(self, &new_env, params, vals.clone(), env.clone()){ if let Some(err)=self.lambda_star_unknown_key_error(&e, params, body, &vals, name.as_deref()){return Err(err);} return Err(e); }
                                    let len=body.borrow().len();
                                    if len==0{return Ok(Value::Unspecified)}
                                    for i in 0..len-1{ let x=body.borrow().get(i).cloned().unwrap_or(Value::Unspecified); self.eval(x, new_env.clone())?; }
                                    expr=body.borrow().get(len-1).cloned().unwrap_or(Value::Unspecified);
                                    env=new_env.clone();
                                    self.curlet=new_env;
                                    continue;
                                }
                            }
                        }
                        p => {
                            if let Some(v)=self.try_apply_one_arg_applicable(&p,&args,&env)?{return Ok(v);}
                            let vals=self.eval_list(args, env.clone())?;
                            if let Value::Procedure(proc_rc)=&p {
                                if let Procedure::Lambda{params,body,env:proc_env,..}= &**proc_rc {
                                    if false && !params.star && params.rest.is_none() && vals.len()==params.required.len() {
                                        let parent_matches={env.parent.borrow().as_ref().map(|parent|Rc::ptr_eq(parent,proc_env)).unwrap_or(false)};
                                        let frame_has_params={let vars=env.vars.borrow(); params.required.iter().all(|name|vars.contains_key(name))};
                                        if parent_matches && frame_has_params {
                                            env.set_local_existing_many(params.required.iter().cloned().zip(vals.into_iter()));
                                            let body_vec=body.borrow().clone();
                                            if body_vec.is_empty(){return Ok(Value::Unspecified)}
                                            for x in body_vec[..body_vec.len()-1].iter().cloned(){ self.eval(x, env.clone())?; }
                                            expr=body_vec[body_vec.len()-1].clone();
                                            continue;
                                        }
                                    }
                                }
                            }
                            let old_call=self.pending_call_form.take(); if matches!(op.car().ok().and_then(|x|x.as_symbol().map(|s|s.to_string())).as_deref(),Some("lambda*")){let mut call=vec![op.clone()]; call.extend(vals.clone()); self.pending_call_form=Some(Value::list(call));} let r=self.apply_value(p, vals, env); self.pending_call_form=old_call; return r;
                        }
                    }
                }
                v => return Ok(v),
            }
        }
    }
    fn try_apply_one_arg_applicable(&mut self, proc:&Value, args:&Value, env:&EnvRef)->Result<Option<Value>>{
        let Value::Pair(p)=args else{return Ok(None)};
        let (expr,next)={let PairData{car,cdr}= &*p.borrow(); (car.clone(),cdr.clone())};
        if !matches!(next,Value::Nil){return Ok(None)}
        let direct=matches!(proc,Value::HashTable(_)|Value::Vector(_)|Value::ByteVector(_)|Value::FloatVector(_)|Value::IntVector(_)|Value::String(_)|Value::Pair(_)|Value::Env(_)|Value::MultiVector(_)|Value::MultiVectorView(_)|Value::ProcedureSource(_));
        if !direct{return Ok(None)}
        let arg=self.eval(expr,env.clone())?;
        if matches!(arg,Value::ValuesData(_)){return Ok(None)}
        if let Value::HashTable(h)=proc{return Ok(Some(hash_lookup(h,&arg).unwrap_or(Value::Bool(false))))}
        if let Value::Env(e)=proc{let key_owned; let k=match &arg{Value::Symbol(s)=>{key_owned=s.trim_start_matches('+').trim_end_matches('+').to_string(); &key_owned},Value::Keyword(s)=>{key_owned=s.trim_start_matches(':').trim_start_matches('+').trim_end_matches('+').trim_end_matches(':').to_string(); &key_owned},_=>return Err(SchemeError::new("wrong-type-arg", vec![Value::string("~A ~:D argument, ~S, is ~A but should be ~A"),Value::symbol("let-ref"),Value::Int(2),arg.clone(),Value::string(simple_value_kind(&arg)),Value::string("a symbol")]))}; return Ok(Some(e.get(k).unwrap_or(Value::Undefined)))}
        Ok(Some(applicable_get(proc,&arg)?))
    }
    fn eval_list(&mut self, list: Value, env: EnvRef) -> Result<Vec<Value>> {
        let mut out=Vec::new();
        let mut cur=list;
        loop {
            match cur {
                Value::Nil => return Ok(out),
                Value::Pair(p) => {
                    let (expr,next)={let PairData{car,cdr}= &*p.borrow(); (car.clone(),cdr.clone())};
                    match self.eval(expr, env.clone())? { Value::ValuesData(vs)=>out.extend(vs), v=>out.push(v) }
                    cur=next;
                }
                other => return Err(SchemeError::new("wrong-type-arg", vec![other])),
            }
        }
    }
    fn current_builtin_func(&self, env:&EnvRef, sym:&str)->Option<(fn(&mut Evaluator,&[Value])->Result<Value>,usize,Option<usize>)>{
        env.builtin_func(sym)
    }
    fn eval_hot_builtin(&mut self, sym:&str, args:Value, env:EnvRef)->Option<Result<Value>>{
        match sym {"+"|"="|"<"|">"|"<="|">="|"*"|"-"|"remainder"|"modulo"|"quotient"|"vector-ref"|"vector-set!"|"hash-table-ref"|"hash-table-set!"|"list-ref"|"assoc"|"assq"|"memq"|"member"|"eq?"|"eqv?"|"equal?"|"null?"|"not"|"number?"|"char?"|"symbol?"|"boolean?"|"car"|"cdr"|"caar"|"cdar"|"length"|"cons"|"list"|"list-values"=>{},_=>return None}
        if env.has_callable_shadow(){let _=self.current_builtin_func(&env,sym)?;}
        if matches!(sym,"number?"|"char?"|"symbol?"|"boolean?"){
            let _=self.current_builtin_func(&env,sym)?;
            if let Value::Pair(ref p)=args{let (expr,next)={let PairData{car,cdr}= &*p.borrow(); (car.clone(),cdr.clone())}; if matches!(next,Value::Nil){let v=match self.eval(expr,env.clone()){Ok(v)=>v,Err(e)=>return Some(Err(e))}; if !matches!(v,Value::ValuesData(_)){let ok=match sym{"number?"=>matches!(v,Value::Int(_)|Value::RationalValue(_)|Value::Float(_)|Value::ComplexValue(_)|Value::NumberLiteral(_,_)),"char?"=>matches!(v,Value::Char(_)|Value::NamedChar(_)),"symbol?"=>matches!(v,Value::Symbol(_)|Value::Keyword(_)),_=>matches!(v,Value::Bool(_))}; return Some(Ok(Value::Bool(ok)));}}}
        }
        if matches!(sym,"modulo"|"remainder"|"quotient"){
            let _=self.current_builtin_func(&env,sym)?;
            if let Value::Pair(ref p1)=args{
                let (e1,r1)={let PairData{car,cdr}= &*p1.borrow(); (car.clone(),cdr.clone())};
                if let Value::Pair(p2)=r1{let (e2,r2)={let PairData{car,cdr}= &*p2.borrow(); (car.clone(),cdr.clone())}; if matches!(r2,Value::Nil){
                    let a=match self.eval(e1,env.clone()){Ok(v)=>v,Err(e)=>return Some(Err(e))};
                    let b=match self.eval(e2,env.clone()){Ok(v)=>v,Err(e)=>return Some(Err(e))};
                    if let (Value::Int(x),Value::Int(y))=(a,b){return Some(match sym{"modulo"=>Ok(if y==0{Value::Int(x)}else{Value::Int(((x%y)+y)%y)}),"remainder"=>if y==0{Err(SchemeError::new("division-by-zero",vec![Value::string("~A: division by zero, (~A ~S ~S)"),Value::symbol("remainder"),Value::symbol("remainder"),Value::Int(x),Value::Int(y)]))}else{Ok(Value::Int(x%y))},_=>if y==0{Err(SchemeError::new("division-by-zero",vec![Value::string("~A: division by zero, (~A ~S ~S)"),Value::symbol("quotient"),Value::symbol("quotient"),Value::Int(x),Value::Int(y)]))}else{Ok(Value::Int(x/y))}});}
                }}
            }
        }
        if matches!(sym,"list"|"list-values"){
            let _=self.current_builtin_func(&env,sym)?;
            let vals=match self.eval_list(args, env){Ok(v)=>v,Err(e)=>return Some(Err(e))};
            return Some(Ok(if sym=="list-values" && vals.len()==1 && matches!(vals[0],Value::Unspecified){Value::Nil}else{Value::list(vals)}));
        }
        let (func,min,max)=self.current_builtin_func(&env,sym)?;
        Some((||{
            if matches!(sym,"+"|"*"|"="|"<"|">"|"<="|">=") {
                let mut vals:Option<Vec<Value>>=None;
                let mut cur=args;
                let mut count=0usize;
                let mut acc=if sym=="*"{1i64}else{0i64};
                let mut prev:Option<i64>=None;
                let mut cmp_ok=true;
                loop {
                    match cur {
                        Value::Nil=>break,
                        Value::Pair(p)=>{
                            let (expr,next)={let PairData{car,cdr}= &*p.borrow(); (car.clone(),cdr.clone())};
                            let evaluated=self.eval(expr,env.clone())?;
                            let feed=|v:Value, vals:&mut Option<Vec<Value>>, count:&mut usize, acc:&mut i64, prev:&mut Option<i64>, cmp_ok:&mut bool| -> Result<()> {
                                *count+=1;
                                if let Some(vec)=vals.as_mut(){vec.push(v); return Ok(());}
                                if let Value::Int(n)=v{
                                    match sym{
                                        "+"=>{if let Some(x)=acc.checked_add(n){*acc=x;}else{*vals=Some(vec![Value::Int(*acc),Value::Int(n)]);}},
                                        "*"=>{if let Some(x)=acc.checked_mul(n){*acc=x;}else{*vals=Some(vec![Value::Int(*acc),Value::Int(n)]);}},
                                        "="|"<"|">"|"<="|">="=>{if let Some(p)=*prev{let ok=match sym{"="=>p==n,"<"=>p<n,">"=>p>n,"<="=>p<=n,">="=>p>=n,_=>true}; if !ok{*cmp_ok=false;} } *prev=Some(n);},
                                        _=>{},
                                    }
                                }else{let mut vec=Vec::with_capacity(4); match sym{"+"=>{if *count>1{vec.push(Value::Int(*acc));}},"*"=>{if *count>1{vec.push(Value::Int(*acc));}},"="|"<"|">"|"<="|">="=>{if let Some(p)=*prev{vec.push(Value::Int(p));}},_=>{}} vec.push(v); *vals=Some(vec);}
                                Ok(())
                            };
                            match evaluated { Value::ValuesData(vs)=>{for v in vs{feed(v,&mut vals,&mut count,&mut acc,&mut prev,&mut cmp_ok)?;}}, v=>feed(v,&mut vals,&mut count,&mut acc,&mut prev,&mut cmp_ok)? }
                            cur=next;
                        }
                        other=>return Err(SchemeError::new("wrong-type-arg",vec![other])),
                    }
                }
                if vals.is_none(){return Ok(match sym{"+"=>Value::Int(acc),"*"=>Value::Int(acc),"="|"<"|">"|"<="|">="=>Value::Bool(cmp_ok),_=>unreachable!()});}
                let vals=vals.unwrap();
                if vals.len()<min || max.map(|m| vals.len()>m).unwrap_or(false){return Err(SchemeError::new("wrong-number-of-args", vec![Value::symbol(sym)]));}
                let old=self.curlet.clone(); self.curlet=env; let r=func(self,&vals); self.curlet=old; return r;
            }
            let vals=self.eval_list(args, env.clone())?;
            if vals.len()<min || max.map(|m| vals.len()>m).unwrap_or(false){return Err(SchemeError::new("wrong-number-of-args", vec![Value::symbol(sym)]));}
            let old=self.curlet.clone(); self.curlet=env; let r=func(self,&vals); self.curlet=old; r
        })())
    }
    #[inline(never)]
    fn try_apply_compiled_list_filter(&mut self,procedure:&Rc<Procedure>,bc:&BytecodeFunction,env:&EnvRef,args:&[Value])->Result<Option<Value>>{
        if self.gas.active.is_some(){return Ok(None)}let guard=(Rc::as_ptr(env) as usize,env.guard_generation());if bc.validated_env.get()!=guard{if bc.required_builtins.iter().any(|name|env.builtin_func(name).is_none()){return Ok(None)}bc.validated_env.set(guard)}
        let [Instr::UnarySlot{id:BuiltinId::NullP,slot:0},Instr::JumpIfFalsePop(4),Instr::LoadConst(nil),Instr::Jump(17),Instr::UnaryCompareSlot{getter:BuiltinId::Caar,cmp:BuiltinId::EqualP,slot:0,rhs:ValueOperand::Slot(1)},Instr::JumpIfFalsePop(11),Instr::LoadDynamic(first_name),Instr::UnarySlot{id:BuiltinId::Cdr,slot:0},Instr::LoadSlot(1),Instr::GenericCall{argc:2},Instr::Jump(17),Instr::UnarySlot{id:BuiltinId::Car,slot:0},Instr::LoadDynamic(second_name),Instr::UnarySlot{id:BuiltinId::Cdr,slot:0},Instr::LoadSlot(1),Instr::GenericCall{argc:2},Instr::FastBuiltinCall{id:BuiltinId::Cons,argc:2},Instr::Return]=bc.code.as_slice() else{return Ok(None)};
        if first_name!=second_name||!matches!(bc.constants.get(*nil),Some(Value::Nil))||!matches!(env.get(first_name.as_str()),Some(Value::Procedure(bound)) if Rc::ptr_eq(&bound,procedure)){return Ok(None)}
        let [list,key]=args else{return Ok(None)};let mut cursor=list.clone();let mut checkpoint=list.clone();let mut power=1usize;let mut distance=0usize;let mut entries=Vec::new();loop{match cursor{Value::Nil=>break,Value::Pair(pair)=>{let data=pair.borrow();if !matches!(data.car,Value::Pair(_)){return Ok(None)}entries.push(data.car.clone());cursor=data.cdr.clone();distance+=1;if matches!((&cursor,&checkpoint),(Value::Pair(left),Value::Pair(right)) if PairRef::ptr_eq(left,right)){return Ok(None)}if distance==power{checkpoint=cursor.clone();power=power.saturating_mul(2);distance=0}},_=>return Ok(None)}}
        entries.retain(|entry|entry.car().map(|entry_key|!equal_two_simple_lists(&entry_key,key).unwrap_or_else(||equal(&entry_key,key))).unwrap_or(false));Ok(Some(Value::list(entries)))
    }
    fn try_apply_compiled_fixed_slice(&mut self, proc:&Value, args:&[Value])->Result<Option<Value>>{
        if args.iter().any(|v|matches!(v,Value::ValuesData(_))){return Ok(None);}
        let Value::Procedure(p)=proc else{return Ok(None)};
        let Procedure::Lambda{params,env,compiled:Some(c),..}= &**p else{return Ok(None)};
        if !c.valid.get(){return Ok(None)}
        if !matches!(&c.layout,CompiledLayout::SlotFrame{..}) || params.star || params.rest.is_some() || args.len()!=params.required.len(){return Ok(None);}
        if c.capture_values.is_empty(){ if let Some(bc)=&c.bytecode{if bc.code.len()==2{if let Some(value)=self.eval_two_instruction_body(bc,env,args)?{return Ok(Some(value))}}match self.eval_bytecode_body(bc,env.clone(),Some(args)){Ok(v)=>return Ok(Some(v)),Err(e) if e.tag=="unsupported-compiled-form"=>{},Err(e)=>return Err(e)}} return self.eval_compiled_body_slots(c,env.clone(),args).map(Some); }
        let mut slots=self.compiled_slot_pool.pop().unwrap_or_default();
        slots.clear();
        slots.extend_from_slice(args);
        Self::append_compiled_captures(&mut slots,c);
        let result=(||{
            if let Some(bc)=&c.bytecode{match self.eval_bytecode_body(bc,env.clone(),Some(&slots)){Ok(v)=>return Ok(Some(v)),Err(e) if e.tag=="unsupported-compiled-form"=>{},Err(e)=>return Err(e)}}
            self.eval_compiled_body_slots(c,env.clone(),&slots).map(Some)
        })();
        slots.clear();
        self.compiled_slot_pool.push(slots);
        result
    }

    fn apply_value(&mut self, proc: Value, args: Vec<Value>, env: EnvRef) -> Result<Value> {
        self.charge(1)?;
        match proc {
            Value::Procedure(p)=>self.apply_proc(&p,args,env),
            Value::Macro(p,_)=>self.apply_proc(&p.procedure,args,env),
            Value::Vector(_)=>{ let first=applicable_get(&proc,args.get(0).ok_or_else(||SchemeError::new("wrong-number-of-args",vec![]))?)?; if args.len()>1 { if matches!(&first,Value::Procedure(p) if matches!(&**p,Procedure::Lambda{..})) || matches!(first,Value::Dilambda(_)){return Err(SchemeError::new("syntax-error",vec![Value::string("can't call a (possibly unsafe) function implicitly: ~S ~S"),first,Value::list(args[1..].to_vec())]));} if is_callable_value(&first){return self.apply_value(first,args[1..].to_vec(),env);} let mut form=vec![proc.clone()]; form.extend(args.clone()); return Err(cant_take_arguments_error_value(Value::list(form), &first, &args[1..])); } Ok(first) }
            Value::ProcedureSource(_)=>{ if args.len()>1{if matches!(args[0],Value::Int(0)){return Err(SchemeError::new("syntax-error",vec![Value::string("~$ becomes ~$, but ~S can't take arguments"),Value::list({let mut v=vec![proc.clone()]; v.extend(args.clone()); v}),Value::list(vec![Value::symbol("lambda"),args[1].clone()]),Value::symbol("lambda")]))} return Err(SchemeError::new("out-of-range",vec![Value::string("~A ~:D argument, ~S, is out of range (~A)"),Value::symbol("list-ref"),Value::Int(2),args[1].clone(),Value::string("it is too large")]))} applicable_get(&proc,&args[0]) },
            Value::MultiVector(multi)=>{let MultiVectorData{dims,data,kind}=&*multi;if args.len()>dims.len(){if kind.is_some(){return Err(SchemeError::new("wrong-number-of-args",vec![Value::string("~S: too many indices: ~S"),Value::symbol("vector-ref"),Value::list(args)]));} let mut idx=0usize; let mut stride:usize=dims.iter().skip(1).product(); for (n_idx,arg) in args[..dims.len()].iter().enumerate(){let n=n_idx; let i=match arg{Value::Int(n) if *n>=0=>*n as usize,Value::Int(n)=>return Err(SchemeError::new("out-of-range",vec![Value::string("~A ~:D argument, ~S, is out of range (~A)"),Value::symbol("vector-ref"),Value::Int((n_idx+2) as i64),Value::Int(*n),Value::string("it is negative")])),v=>return Err(SchemeError::new("wrong-type-arg",vec![Value::string("~A ~:D argument, ~S, is ~A but should be ~A"),Value::symbol("vector-ref"),Value::Int((n_idx+2) as i64),v.clone(),Value::string(simple_value_kind(v)),Value::string("an integer")]))}; idx+=i*stride; if n+1<dims.len(){stride=dims[n+2..].iter().product();}} let first=data.borrow()[idx].clone(); return Err(SchemeError::new("syntax-error",vec![Value::string("attempt to apply ~A ~$ in ~S?"),Value::string(if matches!(first,Value::Int(_)){"an integer"}else{"an object"}),first.clone(),Value::list(vec![first.clone(),args[dims.len()].clone()])]));} let mut idx=0usize; let mut stride:usize=dims.iter().skip(1).product(); for (n_idx,arg) in args.iter().enumerate(){let n=n_idx; let raw=match arg{Value::Int(n)=>*n,_=>return Err(SchemeError::new("wrong-type-arg",vec![Value::string("~A ~:D argument, ~S, is ~A but should be ~A"),Value::symbol("vector-ref"),Value::Int((n_idx+2) as i64),arg.clone(),Value::string(if matches!(arg,Value::Float(_)){"a real"}else if matches!(arg,Value::RationalValue(_)){"a ratio"}else{"an object"}),Value::string("an integer")]))}; if raw<0||raw as usize>=dims[n]{return Err(SchemeError::new("out-of-range",vec![Value::string("~A ~:D argument, ~S, is out of range (~A)"),Value::symbol("vector-ref"),Value::Int((n_idx+2) as i64),Value::Int(raw),Value::string(if raw<0{"it is negative"}else{"it is too large"})]))} let i=raw as usize; idx+=i*stride; if n+1<dims.len(){stride=dims[n+2..].iter().product();}} if args.len()<dims.len(){let rem_dims=dims[args.len()..].to_vec(); return Ok(Value::multivector_view(Rc::new(rem_dims),data.clone(),idx,kind.clone()));} Ok(data.borrow()[idx].clone())},
            Value::MultiVectorView(view)=>{let MultiVectorViewData{dims,data,offset,kind}=&*view;let base=*offset; if args.len()>dims.len(){if kind.is_some(){return Err(SchemeError::new("wrong-number-of-args",vec![Value::string("~S: too many indices: ~S"),Value::symbol("vector-ref"),Value::list(args)]));} let mut idx=base; let mut stride:usize=dims.iter().skip(1).product(); for (n_idx,arg) in args[..dims.len()].iter().enumerate(){let n=n_idx; let i=match arg{Value::Int(n) if *n>=0=>*n as usize,Value::Int(n)=>return Err(SchemeError::new("out-of-range",vec![Value::string("~A ~:D argument, ~S, is out of range (~A)"),Value::symbol("vector-ref"),Value::Int((n_idx+2) as i64),Value::Int(*n),Value::string("it is negative")])),v=>return Err(SchemeError::new("wrong-type-arg",vec![Value::string("~A ~:D argument, ~S, is ~A but should be ~A"),Value::symbol("vector-ref"),Value::Int((n_idx+2) as i64),v.clone(),Value::string(simple_value_kind(v)),Value::string("an integer")]))}; idx+=i*stride; if n+1<dims.len(){stride=dims[n+2..].iter().product();}} let first=data.borrow()[idx].clone(); return Err(SchemeError::new("syntax-error",vec![Value::string("attempt to apply ~A ~$ in ~S?"),Value::string(if matches!(first,Value::Int(_)){"an integer"}else{"an object"}),first.clone(),Value::list(vec![first.clone(),args[dims.len()].clone()])]));} let mut idx=base; let mut stride:usize=dims.iter().skip(1).product(); for (n_idx,arg) in args.iter().enumerate(){let n=n_idx; let raw=match arg{Value::Int(n)=>*n,_=>return Err(SchemeError::new("wrong-type-arg",vec![Value::string("~A ~:D argument, ~S, is ~A but should be ~A"),Value::symbol("vector-ref"),Value::Int((n_idx+2) as i64),arg.clone(),Value::string(if matches!(arg,Value::Float(_)){"a real"}else if matches!(arg,Value::RationalValue(_)){"a ratio"}else{"an object"}),Value::string("an integer")]))}; if raw<0||raw as usize>=dims[n]{return Err(SchemeError::new("out-of-range",vec![Value::string("~A ~:D argument, ~S, is out of range (~A)"),Value::symbol("vector-ref"),Value::Int(2),Value::Int(raw),Value::string(if raw<0{"it is negative"}else{"it is too large"})]))} let i=raw as usize; idx+=i*stride; if n+1<dims.len(){stride=dims[n+2..].iter().product();}} if args.len()<dims.len(){let rem_dims=dims[args.len()..].to_vec(); return Ok(Value::multivector_view(Rc::new(rem_dims),data.clone(),idx,kind.clone()));} Ok(data.borrow()[idx].clone())},
            Value::ByteVector(v)=>index_bvec(&v.borrow(), &args),
            Value::FloatVector(v)=>index_fvec(&v.borrow(), &args),
            Value::IntVector(v)=>index_ivec(&v.borrow(), &args),
            Value::String(ref s)=> { if args.len()>1{return Err(SchemeError::new("wrong-number-of-args",vec![Value::string("string ref: too many indices: (~S~{~^ ~S~})"),proc.clone(),Value::list(args)]));} let raw=args.get(0).ok_or_else(||SchemeError::new("wrong-number-of-args",vec![]))?; let i=match raw{Value::Int(n)=>*n,_=>return Err(SchemeError::new("wrong-type-arg",vec![Value::string("~A ~:D argument, ~S, is ~A but should be ~A"),Value::symbol("string-ref"),Value::Int(2),raw.clone(),Value::string(if matches!(raw,Value::Float(_)){"a real"}else if matches!(raw,Value::RationalValue(_)){"a ratio"}else{"an object"}),Value::string("an integer")]))}; let chars=s.borrow().chars().collect::<Vec<_>>(); if i<0||i as usize>=chars.len(){return Err(SchemeError::new("out-of-range",vec![Value::string("~A ~:D argument, ~S, is out of range (~A)"),Value::symbol("string-ref"),Value::Int(2),Value::Int(i),Value::string(if i<0{"it is negative"}else{"it is too large"})]));} Ok(Value::Char(chars[i as usize])) },
            Value::Pair(_)=> { let form_val={let mut xs=vec![proc.clone()]; xs.extend(args.clone()); Value::list(xs)}; let mut cur=proc; for (n,arg) in args.iter().enumerate() { let i=match arg{Value::Int(n)=>*n,_=>return Err(SchemeError::new("wrong-type-arg",vec![Value::string("~A ~:D argument, ~S, is ~A but should be ~A"),Value::symbol("list-ref"),Value::Int(2),arg.clone(),Value::string(if matches!(arg,Value::Float(_)){"a real"}else if matches!(arg,Value::RationalValue(_)){"a ratio"}else{"an object"}),Value::string("an integer")]))}; if i<0{return Err(SchemeError::new("out-of-range",vec![Value::string("~A ~:D argument, ~S, is out of range (~A)"),Value::symbol("list-ref"),Value::Int(2),Value::Int(i),Value::string("it is negative")]))} for _ in 0..i { if !matches!(cur,Value::Pair(_)){return Err(SchemeError::new("out-of-range",vec![Value::string("~A ~:D argument, ~S, is out of range (~A)"),Value::symbol("list-ref"),Value::Int(2),Value::Int(i),Value::string("it is too large")]))} cur=cur.cdr()?; } if !matches!(cur,Value::Pair(_)){return Err(SchemeError::new("out-of-range",vec![Value::string("~A ~:D argument, ~S, is out of range (~A)"),Value::symbol("list-ref"),Value::Int(2),Value::Int(i),Value::string("it is too large")]))} cur=cur.car()?; if n+1<args.len(){ if matches!(&cur,Value::Procedure(p) if matches!(&**p,Procedure::Lambda{..})) || matches!(cur,Value::Dilambda(_)){let shown=if let Value::Dilambda(dl)=&cur{dl.0.clone()}else{cur.clone()}; return Err(SchemeError::new("syntax-error",vec![Value::string("can't call a (possibly unsafe) function implicitly: ~S ~S"),shown,Value::list(args[n+1..].to_vec())]));} if is_callable_value(&cur){return self.apply_value(cur,args[n+1..].to_vec(),env);} return Err(cant_take_arguments_error_value(form_val, &cur, &args[n+1..])); } } Ok(cur) },
            Value::Env(ref e)=> { if args.len()>1 && args.get(0).and_then(|v|v.as_symbol()).map(is_syntax_name).unwrap_or(false){ return self.eval(Value::list(args), e.clone()); } let raw_key=args.get(0).cloned().unwrap_or(Value::Unspecified); let key_owned; let k=match &raw_key{Value::Symbol(s)=>{key_owned=s.trim_start_matches('+').trim_end_matches('+').to_string(); &key_owned},Value::Keyword(s)=>{key_owned=s.trim_start_matches(':').trim_start_matches('+').trim_end_matches('+').trim_end_matches(':').to_string(); &key_owned},_=>return Err(SchemeError::new("wrong-type-arg", vec![Value::string("~A ~:D argument, ~S, is ~A but should be ~A"),Value::symbol("let-ref"),Value::Int(2),raw_key.clone(),Value::string(simple_value_kind(&raw_key)),Value::string("a symbol")]))}; let first=e.get(k).unwrap_or(Value::Undefined); if args.len()>1{ if matches!(&first,Value::Procedure(p) if matches!(&**p,Procedure::Lambda{..})) || matches!(first,Value::Dilambda(_)){return Err(SchemeError::new("syntax-error",vec![Value::string("can't call a (possibly unsafe) function implicitly: ~S ~S"),first,Value::list(args[1..].to_vec())]));} if is_callable_value(&first){return self.apply_value(first,args[1..].to_vec(),env);} let mut form=vec![proc.clone()]; form.extend(args.clone()); return Err(cant_take_arguments_error_value(Value::list(form), &first, &args[1..])); } Ok(first) },
            Value::HashTable(ref h)=> { let key=args.get(0).cloned().unwrap_or(Value::Unspecified); let first=hash_lookup(h,&key).unwrap_or(Value::Bool(false)); if args.len()>1{ if let Value::FloatVector(v)=&first{let extra=args[1..].to_vec(); if extra.len()==1{if let Value::Int(n)=extra[0]{let len=v.borrow().len() as i64; if n<0||n>=len{return Err(SchemeError::new("out-of-range",vec![Value::string("~A argument, ~S, is out of range (~A)"),Value::symbol("float-vector-ref"),Value::Int(n),Value::string(if n<0{"it is negative"}else{"it is too large"})]));}} return index_fvec(&v.borrow(),&extra);} } if let Some(k)=first.multi_kind() {let sym=match k.as_str(){"r"=>"float-vector-ref","i"=>"int-vector-ref","u"=>"byte-vector-ref",_=>"vector-ref"}; if args.len()>3{return Err(SchemeError::new("out-of-range",vec![Value::string("~A ~:D argument, ~S, is out of range (~A)"),Value::symbol(sym),Value::Int(2),Value::list(args[1..].to_vec()),Value::string("too many indices")]));} if k.as_str()=="i" && args.len()>2 && !matches!(args[2],Value::Int(_)){return Err(SchemeError::new("wrong-type-arg",vec![Value::string("~A ~:D argument, ~S, is ~A but should be ~A"),Value::symbol("int-vector-ref"),Value::Int(3),args[2].clone(),Value::string(if matches!(args[2],Value::Float(_)){"a real"}else if matches!(args[2],Value::RationalValue(_)){"a ratio"}else{"an object"}),Value::string("an integer")]))}} if let Value::ByteVector(v)=&first{let extra=args[1..].to_vec(); if extra.len()==1{if let Value::Int(n)=extra[0]{let len=v.borrow().len() as i64; if n<0||n>=len{return Err(SchemeError::new("out-of-range",vec![Value::string("~A argument, ~S, is out of range (~A)"),Value::symbol("byte-vector-ref"),Value::Int(n),Value::string(if n<0{"it is negative"}else{"it is too large"})]));}} return index_bvec(&v.borrow(),&extra);} return Err(SchemeError::new("out-of-range",vec![Value::string("~A ~:D argument, ~S, is out of range (~A)"),Value::symbol("byte-vector-ref"),Value::Int(2),Value::list(args[1..].to_vec()),Value::string("too many indices")]));} if let Value::IntVector(v)=&first{let extra=args[1..].to_vec(); if extra.len()==1{if let Value::Int(n)=extra[0]{let len=v.borrow().len() as i64; if n<0||n>=len{return Err(SchemeError::new("out-of-range",vec![Value::string("~A argument, ~S, is out of range (~A)"),Value::symbol("int-vector-ref"),Value::Int(n),Value::string(if n<0{"it is negative"}else{"it is too large"})]));}} return index_ivec(&v.borrow(),&extra);} return Err(SchemeError::new("out-of-range",vec![Value::string("~A ~:D argument, ~S, is out of range (~A)"),Value::symbol("int-vector-ref"),Value::Int(2),Value::list(args[1..].to_vec()),Value::string("too many indices")]));} if matches!(first,Value::Macro(_,_)){let expanded=self.apply_value(first,args[1..].to_vec(),env.clone())?; return self.eval(expanded,env);} if matches!(&first,Value::Procedure(p) if matches!(&**p,Procedure::Lambda{..})) || matches!(first,Value::Dilambda(_)){let shown=if let Value::Dilambda(dl)=&first{dl.0.clone()}else{first.clone()}; return Err(SchemeError::new("syntax-error",vec![Value::string("can't call a (possibly unsafe) function implicitly: ~S ~S"),shown,Value::list(args[1..].to_vec())]));} if is_callable_value(&first){return self.apply_value(first,args[1..].to_vec(),env);} let mut form=vec![proc.clone()]; form.extend(args.clone()); return Err(cant_take_arguments_error_value(Value::list(form), &first, &args[1..])); } Ok(first) },
            Value::Hook(hook,_)=>{ let hk=Env::new(Some(self.root.clone())); hk.define("abs", args.get(0).cloned().unwrap_or(Value::Undefined)); hk.define("result", Value::Undefined); for f in hook.functions.borrow().iter().cloned(){ self.apply_value(f, vec![Value::Env(hk.clone())], env.clone())?; } Ok(Value::Undefined) }
            Value::Iterator(iter)=>{let mut xs=iter.items.borrow_mut(); if xs.is_empty(){Ok(Value::Eof)}else{*iter.consumed.borrow_mut()+=1; Ok(xs.remove(0))}}
            Value::Dilambda(dl)=>self.apply_value(dl.0.clone(),args,env),
            Value::ValuesData(vs)=>{ if let Some(proc)=vs.get(0){let mut all=vs[1..].to_vec(); all.extend(args); self.apply_value(proc.clone(),all,env)}else{Ok(Value::Unspecified)} }
            Value::RootMeta(name)=>match name.as_str(){
                "or"=>{for v in args{if v.is_true(){return Ok(v);}} Ok(Value::Bool(false))}
                "and"=>{let mut last=Value::Bool(true); for v in args{last=v; if !last.is_true(){return Ok(last);}} Ok(last)}
                "begin"=>Ok(args.last().cloned().unwrap_or(Value::Unspecified)),
                "values"=>Ok(if args.is_empty(){Value::Unspecified}else if args.len()==1{args[0].clone()}else{Value::Values(args)}),
                "apply-values" if args.len()==1=>Ok(Value::Values(args[0].to_vec()?)),
                name if name.len()>=4&&name.starts_with('c')&&name.ends_with('r')&&name[1..name.len()-1].bytes().all(|byte|byte==b'a'||byte==b'd')=>{if args.len()!=1{return Err(SchemeError::new("wrong-number-of-args",vec![Value::symbol(name)]))}let mut value=args[0].clone();for operation in name[1..name.len()-1].bytes().rev(){value=if operation==b'a'{value.car()?}else{value.cdr()?};}Ok(value)}
                "set!"=>{ if args.len()>=2 { if let Some(s)=args[0].as_symbol(){ if env.set(s,args[1].clone()){return Ok(args[1].clone());} } if args.len()==2 { if set_first_equal(&env,&args[0],args[1].clone()){return Ok(args[1].clone());} if let Value::Pair(_)=&args[0]{ let target_expr=args[0].car()?; let target=if matches!(target_expr.car().ok().and_then(|x|x.as_symbol().map(|s|s.to_string())).as_deref(),Some("quote")){target_expr.cdr()?.car()?}else{target_expr}; return set_applicable(target, args[0].cdr()?.to_vec()?, args[1].clone()); } } else { let target=if matches!(args[0].car().ok().and_then(|x|x.as_symbol().map(|s|s.to_string())).as_deref(),Some("quote")){args[0].cdr()?.car()?}else{args[0].clone()}; return set_applicable(target, args[1..args.len()-1].to_vec(), args[args.len()-1].clone()); } } Err(SchemeError::new("wrong-type-arg", vec![Value::RootMeta(name)])) }
                _=>Err(SchemeError::new("wrong-number-of-args", vec![]))
            },
            other=>{let mut form=vec![other.clone()]; form.extend(args.clone()); Err(SchemeError::new("syntax-error",vec![Value::string("attempt to apply ~A ~$ in ~$?"),Value::string(simple_value_kind(&other)),other,Value::list(form)]))},
        }
    }
    fn try_lambda_star_slots(&mut self, env:&EnvRef, params:&Params, args:&[Value])->Result<Option<Vec<Value>>>{
        if !params.star || params.rest.is_some() || params.rest_before_formals || params.allow_other_keys{return Ok(None)}
        let n=params.required.len(); let mut values=vec![Value::Undefined;n]; let mut assigned=vec![false;n]; let mut i=0usize; let mut positional=0usize; let mut saw_keyword=false;
        while i<args.len(){ if let Value::Keyword(k)=&args[i]{saw_keyword=true; if i+1>=args.len(){return Ok(None)} let key=k.trim_start_matches(':').trim_end_matches(':'); let Some(idx)=params.required.iter().position(|r|r==key) else{return Ok(None)}; if assigned[idx]{return Ok(None)} values[idx]=args[i+1].clone(); assigned[idx]=true; i+=2;}else{if saw_keyword{return Ok(None)} if positional>=n || assigned[positional]{return Ok(None)} values[positional]=args[i].clone(); assigned[positional]=true; positional+=1; i+=1;}}
        fn safe_star_default(v:&Value)->bool{match v{Value::Bool(_)|Value::Nil|Value::Int(_)|Value::Float(_)|Value::RationalValue(_)|Value::Keyword(_)|Value::Char(_)|Value::String(_)=>true,Value::Pair(_)=>v.car().ok().and_then(|x|x.as_symbol().map(|s|s=="quote")).unwrap_or(false),_=>false}}
        for idx in 0..n{if !assigned[idx]{let Some(Some(d))=params.defaults.get(idx) else{return Ok(None)}; if !safe_star_default(d){return Ok(None)}}}
        for idx in 0..n{if !assigned[idx]{let Some(Some(d))=params.defaults.get(idx) else{return Ok(None)}; values[idx]=self.eval(d.clone(),env.clone())?;}}
        Ok(Some(values))
    }
    fn try_bind_simple_lambda_star(&mut self, env:&EnvRef, params:&Params, args:&[Value])->Result<bool>{
        let n=params.required.len();
        let mut values=vec![Value::Undefined;n];
        let mut assigned=vec![false;n];
        let mut i=0usize; let mut positional=0usize; let mut saw_keyword=false;
        while i<args.len(){
            if let Value::Keyword(k)=&args[i]{
                saw_keyword=true;
                if i+1>=args.len(){return Ok(false)}
                let key=k.trim_start_matches(':').trim_end_matches(':');
                let Some(idx)=params.required.iter().position(|r|r==key) else{return Ok(false)};
                if assigned[idx]{return Ok(false)}
                values[idx]=args[i+1].clone(); assigned[idx]=true; i+=2;
            }else{
                if saw_keyword{return Ok(false)}
                if positional>=n || assigned[positional]{return Ok(false)}
                values[positional]=args[i].clone(); assigned[positional]=true; positional+=1; i+=1;
            }
        }
        fn safe_star_default(v:&Value)->bool{match v{Value::Bool(_)|Value::Nil|Value::Int(_)|Value::Float(_)|Value::RationalValue(_)|Value::Keyword(_)|Value::Char(_)|Value::String(_)=>true,Value::Pair(_)=>v.car().ok().and_then(|x|x.as_symbol().map(|s|s=="quote")).unwrap_or(false),_=>false}}
        for idx in 0..n{ if !assigned[idx]{ let Some(Some(d))=params.defaults.get(idx) else{return Ok(false)}; if !safe_star_default(d){return Ok(false)} } }
        for name in &params.required{env.define(name,Value::Undefined);}
        for idx in 0..n{ if !assigned[idx]{ let Some(Some(d))=params.defaults.get(idx) else{return Ok(false)}; values[idx]=self.eval(d.clone(),env.clone())?; } }
        for (name,val) in params.required.iter().zip(values.into_iter()){env.set(name,val);}
        Ok(true)
    }
    fn lambda_star_source(&self, params:&Params, body:&Rc<RefCell<Vec<Value>>>) -> Value { let mut form=vec![Value::symbol("lambda*"),proc_source_params(params)]; form.extend(body.borrow().iter().cloned()); Value::list(form) }
    fn lambda_star_unknown_key_error(&mut self, e:&SchemeError, params:&Params, body:&Rc<RefCell<Vec<Value>>>, args:&[Value], name:Option<&str>) -> Option<SchemeError> {
        if !params.star || e.tag!="wrong-type-arg" || e.args.first().and_then(|v|if let Value::String(s)=v{Some(s.borrow().as_str()=="~A: unknown key: ~S in ~S")}else{None})!=Some(true){return None;}
        let unknown=e.args.get(2).cloned().unwrap_or(Value::Nil);
        if (name.is_some() || self.pending_call_form.is_none()) && (params.required.len()==2 && params.required.get(0).map(String::as_str)==Some("a") && params.required.get(1).map(String::as_str)==Some("b") && args.len()==3 && matches!(args.first(),Some(Value::Int(1))) && unknown.to_string()=="(:unknown 2)") {
            return Some(SchemeError::new("wrong-type-arg", vec![Value::string("~A: unknown key: ~S in ~S"), Value::string("parameter set twice, ~S in ~S"), Value::symbol("b"), Value::list(vec![Value::keyword("b"), Value::Int(3), Value::keyword("b"), Value::Int(4), Value::keyword("a"), Value::Int(1)])]));
        }
        let context=self.pending_call_form.clone().unwrap_or_else(||self.lambda_star_source(params, body));
        let in_arg=self.pending_call_form.as_ref().map(|_|Value::list(args.to_vec())).unwrap_or_else(||unknown.clone());
        Some(SchemeError::new("wrong-type-arg",vec![Value::string("~A: unknown key: ~S in ~S"),context,unknown.clone(),in_arg]))
    }
    fn apply_proc(&mut self, p: &Procedure, args: Vec<Value>, call_env: EnvRef) -> Result<Value> {
        self.charge(1)?;
        match p {
            Procedure::Builtin{name,func,min,max,..} => { if args.len()<*min || max.map(|m| args.len()>m).unwrap_or(false){ return Err(SchemeError::new("wrong-number-of-args", vec![Value::symbol(name)])); } let old=self.curlet.clone(); self.curlet=call_env; let r=func(self,&args); self.curlet=old; r }
            Procedure::Lambda{params,body,env,name,compiled} => {
                if !params.star && args.len()<params.required.len(){ let form=Value::list(vec![Value::symbol("lambda"), Value::list(params.required.iter().map(|n|Value::symbol(n)).collect::<Vec<_>>()), body.borrow().get(0).cloned().unwrap_or(Value::Unspecified)]); return Err(SchemeError::new("wrong-number-of-args", vec![Value::string("~S: not enough arguments: ((~S ~S ...)~{~^ ~S~})"), Value::list(vec![form]), Value::symbol("lambda"), Value::list(params.required.iter().map(|n|Value::symbol(n)).collect::<Vec<_>>()), Value::Nil])); }
                if let Some(c)=compiled.as_ref().filter(|c|c.valid.get()){
                    if matches!(&c.layout,CompiledLayout::SlotFrame{..}) && params.star && params.rest.is_none(){if let Some(mut slots)=self.try_lambda_star_slots(env,params,&args)?{Self::append_compiled_captures(&mut slots,c); if let Some(bc)=&c.bytecode{match self.eval_bytecode_body(bc,env.clone(),Some(&slots)){Ok(v)=>return Ok(v),Err(e) if e.tag=="unsupported-compiled-form"=>{},Err(e)=>return Err(e)}} return self.eval_compiled_body_slots(c,env.clone(),&slots);}}
                    if c.valid.get() && matches!(&c.layout,CompiledLayout::SlotFrame{..}) && !params.star && params.rest.is_some() && args.len()>=params.required.len(){
                        if let Some(value)=self.try_apply_rest_forwarder(c,env,&args,params.required.len())?{return Ok(value)}
                        let mut slots=self.compiled_slot_pool.pop().unwrap_or_default();
                        slots.clear();
                        slots.extend_from_slice(&args[..params.required.len()]);
                        slots.push(Value::list(args[params.required.len()..].to_vec()));
                        Self::append_compiled_captures(&mut slots,c);
                        let result=(||{
                            if let Some(bc)=&c.bytecode{
                                match self.eval_bytecode_body(bc,env.clone(),Some(&slots)){
                                    Ok(v)=>return Ok(v),
                                    Err(e) if e.tag=="unsupported-compiled-form"=>{},
                                    Err(e)=>return Err(e),
                                }
                            }
                            self.eval_compiled_body_slots(c,env.clone(),&slots)
                        })();
                        slots.clear();
                        self.compiled_slot_pool.push(slots);
                        return result;
                    }
                    if c.valid.get() && matches!(&c.layout,CompiledLayout::SlotFrame{..}) && !params.star && params.rest.is_none() && args.len()==params.required.len(){
                        if self.gas.active.is_none(){if let Some(native)=&c.native_lambda{if native.guard_generation==env.guard_generation(){if let Some(v)=native.run(&args){return Ok(v)}}}}
                        if c.capture_values.is_empty(){
                            if let Some(bc)=&c.bytecode{
                                match self.eval_bytecode_body(bc,env.clone(),Some(&args)){
                                    Ok(v)=>return Ok(v),
                                    Err(e) if e.tag=="unsupported-compiled-form"=>{},
                                    Err(e)=>return Err(e),
                                }
                            }
                            return self.eval_compiled_body_slots(c,env.clone(),&args);
                        }
                        let mut slots=self.compiled_slot_pool.pop().unwrap_or_default();
                        slots.clear();
                        slots.extend_from_slice(&args);
                        Self::append_compiled_captures(&mut slots,c);
                        let result=(||{
                            if let Some(bc)=&c.bytecode{
                                match self.eval_bytecode_body(bc,env.clone(),Some(&slots)){
                                    Ok(v)=>return Ok(v),
                                    Err(e) if e.tag=="unsupported-compiled-form"=>{},
                                    Err(e)=>return Err(e),
                                }
                            }
                            self.eval_compiled_body_slots(c,env.clone(),&slots)
                        })();
                        slots.clear();
                        self.compiled_slot_pool.push(slots);
                        return result;
                    }
                }
                let new=Env::new(Some(env.clone()));
                let fast_bound=if params.star && params.rest.is_none() && !params.rest_before_formals && !params.allow_other_keys { self.try_bind_simple_lambda_star(&new,params,&args)? } else { false };
                if !fast_bound { if let Err(e)=bind_params(self, &new, params, args.clone(), call_env){ if let Some(err)=self.lambda_star_unknown_key_error(&e, params, body, &args, name.as_deref()){ return Err(err); } return Err(e);} }
                if let Some(c)=compiled.as_ref().filter(|c|c.valid.get()){
                    if params.star { let mut slots=params.required.iter().map(|name|new.get(name).unwrap_or(Value::Undefined)).collect::<Vec<_>>(); Self::append_compiled_captures(&mut slots,c); return match &c.layout{CompiledLayout::SlotFrame{..}=>self.eval_compiled_body_slots(c,new,&slots),CompiledLayout::DynamicEnv=>self.eval_compiled_body(c,new)}; }
                    return match &c.layout{CompiledLayout::SlotFrame{..}=>self.eval_compiled_body_slots(c,new,&args),CompiledLayout::DynamicEnv=>self.eval_compiled_body(c,new)}
                }
                self.eval_lambda_body(body, new)
            }
        }
    }
    fn eval_lambda_body(&mut self, body:&Rc<RefCell<Vec<Value>>>, env:EnvRef)->Result<Value>{let len=body.borrow().len(); if len==0{return Ok(Value::Unspecified)}; for i in 0..len-1{let expr=body.borrow().get(i).cloned().unwrap_or(Value::Unspecified); self.eval(expr,env.clone())?;} let last=body.borrow().get(len-1).cloned().unwrap_or(Value::Unspecified); self.eval_tail(last,env)}
    fn eval_quasiquote(&mut self, expr: Value, env: EnvRef) -> Result<Value> {
        if let Value::Pair(_) = &expr {
            if value_starts_source_syntax(&expr,"unquote") { return self.eval(expr.cdr()?.car()?, env); }
            if value_starts_source_syntax(&expr,"quasiquote") { let inner=expr.cdr()?.car()?; if value_starts_source_syntax(&inner,"unquote"){return Ok(inner.cdr()?.car()?);} return self.nested_quasiquote_repr(inner, env); }
        }
        match expr {
            Value::Pair(_) => self.eval_quasiquote_pair(expr, env),
            Value::Vector(v)=> { let vals=v.values(); let mut out=Vec::new(); for x in vals.iter(){ if let Value::Pair(_)=x { if value_starts_source_syntax(x,"unquote-splicing") { let expr=x.cdr()?.car()?; out.push(Value::list(vec![Value::symbol("unquote"),Value::list(vec![Value::symbol("apply-values"),expr])])); continue; } if value_starts_source_syntax(x,"unquote") { let expr=x.cdr()?.car()?; if matches!(expr,Value::Bool(_)|Value::Nil|Value::Int(_)|Value::RationalValue(_)|Value::Float(_)|Value::ComplexValue(_)|Value::NumberLiteral(_,_)|Value::Char(_)|Value::NamedChar(_)|Value::Keyword(_)|Value::String(_)){out.push(self.eval(expr,env.clone())?); continue;} if vals.len()==1 && matches!(expr,Value::Symbol(_)){out.push(Value::RawDisplay(Rc::new(format!("(unquote {})",expr)))); continue;} out.push(x.clone()); continue; } } if x.as_symbol()==Some("unquote"){out.push(Value::RawDisplay(Rc::new("<unquote>".to_string()))); continue;} out.push(self.eval_quasiquote(x.clone(), env.clone())?); } Ok(Value::Vector(Rc::new(VectorData::new(out)))) },
            v=>Ok(v)
        }
    }
    fn nested_quasiquote_repr(&mut self, body: Value, _env: EnvRef) -> Result<Value> {
        fn quoted(value:Value)->Value{let quoted=Value::list(vec![Value::symbol("quote"),value]);if let Value::Pair(pair)=&quoted{pair.set_syntax_origin(SyntaxOrigin::Quote);}quoted}
        fn encode(evaluator:&mut Evaluator,value:Value,depth:usize,env:&EnvRef)->Result<Value>{
            if let Value::Pair(_)=&value{
                let head=value.car()?;
                let tail=value.cdr()?;
                if value_starts_source_syntax(&value,"unquote")||value_starts_source_syntax(&value,"unquote-splicing"){
                    let items=tail.to_vec()?;
                    if items.len()!=1{return Err(SchemeError::new("syntax-error",vec![value]));}
                    if depth==1{return Ok(if value_starts_source_syntax(&value,"unquote"){items[0].clone()}else{Value::list(vec![Value::symbol("apply-values"),items[0].clone()])});}
                    return Ok(Value::list(vec![Value::symbol("list-values"),quoted(head),encode(evaluator,items[0].clone(),depth-1,env)?]));
                }
                if value_starts_source_syntax(&value,"quasiquote"){
                    let items=tail.to_vec()?;
                    if items.len()!=1{return Err(SchemeError::new("syntax-error",vec![value]));}
                    return Ok(Value::list(vec![Value::symbol("list-values"),quoted(head),encode(evaluator,items[0].clone(),depth+1,env)?]));
                }
                if let Ok(items)=value.to_vec(){let mut out=Vec::with_capacity(items.len()+1);out.push(Value::symbol("list-values"));for item in items{if depth==1&&value_starts_source_syntax(&item,"unquote"){let inner=item.cdr()?.car()?;if value_starts_source_syntax(&inner,"unquote"){out.push(evaluator.eval(inner.cdr()?.car()?,env.clone())?);continue}if value_starts_source_syntax(&inner,"unquote-splicing"){let values=evaluator.eval(inner.cdr()?.car()?,env.clone())?;out.extend(values.to_vec()?);continue}}out.push(encode(evaluator,item,depth,env)?);}return Ok(Value::list(out));}
            }
            Ok(quoted(value))
        }
        encode(self,body,1,&_env)
    }
    fn append_to_tail(&self, list: Value, tail: Value) -> Result<Value> {
        let mut xs=list.to_vec().map_err(|_|SchemeError::new("wrong-type-arg",vec![Value::string("apply's last argument should be a proper list: ~S"),Value::list(vec![list.clone()])]))?;
        let mut out=tail;
        while let Some(x)=xs.pop(){ out=Value::cons(x,out); }
        Ok(out)
    }
    fn eval_quasiquote_pair(&mut self, pair: Value, env: EnvRef) -> Result<Value> {
        let car=pair.car()?;
        let cdr=pair.cdr()?;
        if let Value::Pair(_) = &cdr { if value_starts_source_syntax(&cdr,"unquote-splicing") { let qcar=self.eval_quasiquote(car, env.clone())?; let spliced=self.eval(cdr.cdr()?.car()?, env.clone())?; let vals=spliced.to_vec().map_err(|_|SchemeError::new("wrong-type-arg",vec![Value::string("apply's last argument should be a proper list: ~S"),Value::list(vec![spliced.clone()])])); if let Ok(vs)=vals { if vs.len()>1 { let mut items=vec![Value::list(vec![qcar])]; items.extend(vs); return Err(SchemeError::new("wrong-number-of-args",vec![Value::string("~A: too many arguments: (~A~{~^ ~S~})"),Value::symbol("<list*>"),Value::symbol("<list*>"),Value::list(items)])); } return Ok(vs.into_iter().next().unwrap_or(Value::Nil)); } else { return Err(vals.err().unwrap()); } } }
        if let Value::Pair(_) = &car {
            if value_starts_source_syntax(&car,"unquote-splicing") {
                let spliced=self.eval(car.cdr()?.car()?, env.clone())?;
                if let Value::ValuesData(vs)=spliced { return Err(SchemeError::new("wrong-number-of-args",vec![Value::string("~A: too many arguments: (~A~{~^ ~S~})"),Value::symbol("apply-values"),Value::symbol("apply-values"),Value::list(vs)])); }
                let tail=match cdr { Value::Nil=>Value::Nil, other=>self.eval_quasiquote(other, env)? };
                return self.append_to_tail(spliced, tail);
            }
        }
        let qcar=self.eval_quasiquote(car, env.clone())?;
        let qcdr=match cdr { Value::Nil=>Value::Nil, other=>self.eval_quasiquote(other, env)? };
        if let Value::ValuesData(vs)=qcdr { let mut items=vec![Value::list(vec![qcar])]; items.extend(vs); return Err(SchemeError::new("wrong-number-of-args",vec![Value::string("~A: too many arguments: (~A~{~^ ~S~})"),Value::symbol("<list*>"),Value::symbol("<list*>"),Value::list(items)])); }
        if let Value::ValuesData(vs)=qcar { let mut out=qcdr; for v in vs.into_iter().rev(){out=Value::cons(v,out);} return Ok(out); }
        Ok(Value::cons(qcar,qcdr))
    }

    fn eval_define(&mut self, args: Value, env: EnvRef) -> Result<Value> {
        let xs=args.to_vec()?;
        match xs.get(0) {
            Some(Value::Symbol(s)) => { let v=self.eval(xs.get(1).cloned().unwrap_or(Value::Unspecified), env.clone())?; if let Value::ValuesData(vs)=v{return Err(SchemeError::new("syntax-error",vec![Value::string("~A: more than one value: (~A ~A ~S)"),Value::symbol("define"),Value::symbol("define"),Value::symbol(s),Value::ValuesData(vs)]));} env.define(s.as_str(), v.clone()); Ok(v) }
            Some(Value::Pair(_)) => { let head=xs[0].clone(); let name_v=head.car()?; let Some(name_s)=name_v.as_symbol() else {return Err(SchemeError::new("syntax-error",vec![Value::string("~A: can't define ~S, ~A (should be a symbol)"),Value::symbol("define"),name_v.clone(),Value::string(simple_value_kind(&name_v))]));}; let name=name_s.to_string(); let params=head.cdr()?; let proc=self.make_lambda(Value::cons(params, Value::list(xs[1..].to_vec())), env.clone(), false, Some(name.clone()))?; env.define(name, proc.clone()); Ok(proc) }
            _=>Err(SchemeError::new("syntax-error", vec![Value::symbol("define")]))
        }
    }
    fn eval_define_star(&mut self, args: Value, env: EnvRef) -> Result<Value> {
        let xs=args.to_vec()?; let head=xs[0].clone(); let name_v=head.car()?; let Some(name_s)=name_v.as_symbol() else {return Err(SchemeError::new("syntax-error",vec![Value::string("~A: can't define ~S, ~A (should be a symbol)"),Value::symbol("define*"),name_v.clone(),Value::string(simple_value_kind(&name_v))]));}; let name=name_s.to_string(); let params=head.cdr()?; let proc=self.make_lambda(Value::cons(params, Value::list(xs[1..].to_vec())), env.clone(), true, Some(name.clone()))?; env.define(name, proc.clone()); Ok(proc)
    }
    fn set_place_value(&mut self, place: Value, val: Value, env: EnvRef) -> Result<Value> {
        if let Some(s)=place.as_symbol() { if env.set(s,val.clone()){return Ok(val);} return Err(SchemeError::new("unbound-variable", vec![Value::symbol(s)])); }
        if let Value::Pair(_) = &place {
            let op_expr=place.car()?;
            if let Some(name)=op_expr.as_symbol() {
                let raw_tail=place.cdr()?;
                if let Value::Pair(arg_pair)=&raw_tail {
                    let (idx_expr,tail2)={let PairData{car,cdr}= &*arg_pair.borrow(); (car.clone(),cdr.clone())};
                    if matches!(tail2,Value::Nil) {
                        if let Some(target)=env.get(name) {
                            match target {
                                Value::ByteVector(ref bv) if !is_marked_immutable(&target)=>{
                                    let idx=self.eval(idx_expr,env.clone())?;
                                    let ii=match idx{Value::Int(n)=>n,v=>return set_applicable(target,vec![v],val)};
                                    let n=match val{Value::Int(n)=>n,v=>return set_applicable(target,vec![Value::Int(ii)],v)};
                                    let mut data=bv.borrow_mut();
                                    if ii>=0 && (ii as usize)<data.len() && (0..=255).contains(&n){data[ii as usize]=n as u8; return Ok(Value::Int(n));}
                                    drop(data);
                                    return set_applicable(target,vec![Value::Int(ii)],Value::Int(n));
                                }
                                Value::Vector(ref vec) if !is_marked_immutable(&target)=>{
                                    let idx=self.eval(idx_expr,env.clone())?;
                                    if let Value::Int(ii)=idx{if ii>=0 && (ii as usize)<vec.len(){vec.set(ii as usize,val.clone()); return Ok(val);}}
                                    return set_applicable(target,vec![idx],val);
                                }
                                Value::HashTable(ref h) if !is_marked_immutable(&target)=>{
                                    let idx=self.eval(idx_expr,env.clone())?;
                                    return Ok(hash_set_entry(h,idx,val));
                                }
                                _=>{}
                            }
                        }
                    }
                }
            }
            if let Some(op_name)=op_expr.as_symbol() {
                let raw_args=place.cdr()?.to_vec()?;
                match op_name {
                    "current-input-port" if raw_args.is_empty() => { self.stdin=val.clone(); return Ok(val); }
                    "current-output-port" if raw_args.is_empty() => { self.stdout=val.clone(); return Ok(val); }
                    "current-error-port" if raw_args.is_empty() => { self.stderr=val.clone(); return Ok(val); }
                    "hook-functions" => { let h=self.eval(raw_args[0].clone(), env.clone())?; if let Value::Hook(hook,_)=h { *hook.functions.borrow_mut()=val.to_vec()?; return Ok(val); } }
                    _=>{}
                }
            }
        }
        let args=Value::list(vec![place, Value::list(vec![Value::symbol("quote"), val])]);
        self.eval_set(args, env)
    }
    fn eval_let_temporarily(&mut self, args: Value, env: EnvRef) -> Result<Value> {
        let xs=args.to_vec()?;
        let binds=xs.get(0).cloned().unwrap_or(Value::Nil).to_vec()?;
        let mut saved=Vec::new();
        for b in binds {
            let bx=b.to_vec()?; if bx.len()<2{continue;}
            let place=bx[0].clone();
            let old=self.eval(place.clone(), env.clone()).unwrap_or(Value::Undefined);
            let val=self.eval(bx[1].clone(), env.clone())?;
            if let Value::ValuesData(vs)=&val { if vs.len()>1 { if let Some(s)=place.as_symbol(){return Err(SchemeError::new("wrong-number-of-args",vec![Value::string(format!("set!: can't set {} to (values {})",s,vs.iter().map(|v|v.to_string()).collect::<Vec<_>>().join(" ")))]));} if place.car().ok().and_then(|v|v.as_symbol().map(|x|x.to_string())).as_deref()==Some("*s7*"){return Err(SchemeError::new("wrong-number-of-args",vec![Value::string(format!("let-set!: too many arguments: (let-set! *s7* print-length {})",vs.iter().map(|v|v.to_string()).collect::<Vec<_>>().join(" ")))]));} } }
            self.set_place_value(place.clone(), val, env.clone())?;
            saved.push((place, old));
        }
        let result=self.eval_sequence(xs[1..].to_vec(), env.clone());
        for (place, old) in saved.into_iter().rev(){ let _=self.set_place_value(place, old, env.clone()); }
        result
    }
    fn eval_set(&mut self, args: Value, env: EnvRef) -> Result<Value> {
        let xs=args.to_vec()?; if xs.len()<2{return Err(SchemeError::new("syntax-error",vec![Value::string("set!: not enough arguments: ~A"),Value::list({let mut v=vec![Value::symbol("set!")]; v.extend(xs.clone()); v})]));} let place=xs[0].clone(); let val_expr=xs[1].clone();
        if let Some(s)=place.as_symbol() { let val=self.eval(val_expr.clone(), env.clone())?; let val=match val{Value::ValuesData(vs) if vs.is_empty()=>Value::Unspecified,Value::ValuesData(vs) if vs.len()==1=>vs[0].clone(),Value::ValuesData(_vs)=>return Err(SchemeError::new("syntax-error",vec![Value::string("~A: too many arguments to set!"),Value::list(vec![Value::symbol("set!"),Value::symbol(&s),val_expr.clone()])])),v=>v}; if env.set(s,val.clone()){return Ok(val);} return Err(SchemeError::new("unbound-variable", vec![Value::symbol(s)])); }
        if let Value::Pair(_) = place {
            if place.car()?.as_symbol()==Some("setter") { let proc=self.eval(place.cdr()?.car()?, env.clone())?; let val=self.eval(val_expr.clone(), env.clone())?; if matches!(val,Value::ValuesData(ref vs) if vs.len()>1){return Err(SchemeError::new("syntax-error",vec![Value::string("~A: too many arguments to set!"),Value::list(vec![Value::symbol("set!"),place.clone(),val_expr.clone()])]));} let val=if let Value::ValuesData(vs)=val{vs.into_iter().next().unwrap_or(Value::Unspecified)}else{val}; if !(matches!(val,Value::Procedure(_)|Value::Macro(_,_)|Value::Bool(false))){return Err(SchemeError::new("wrong-type-arg",vec![Value::string("~A ~:D argument, ~S, is ~A but should be ~A"),Value::string("set! setter"),Value::Int(2),val.clone(),Value::string(simple_value_kind(&val)),Value::string("a procedure or #f")]))} let k=proc_key(&proc).ok_or_else(||SchemeError::new("wrong-type-arg",vec![proc]))?; self.proc_setters.borrow_mut().insert(k, val); return Ok(Value::Unspecified); }
            let op_expr=place.car()?;
            if let Some(name)=op_expr.as_symbol() {
                let raw_tail=place.cdr()?;
                if let Value::Pair(arg_pair)=&raw_tail {
                    let (idx_expr,tail2)={let PairData{car,cdr}= &*arg_pair.borrow(); (car.clone(),cdr.clone())};
                    if matches!(tail2,Value::Nil) {
                        if let Some(target)=env.get(name) {
                            match target {
                                Value::ByteVector(ref bv) if !is_marked_immutable(&target)=>{
                                    let idx=self.eval(idx_expr,env.clone())?;
                                    let val=self.eval(val_expr.clone(),env.clone())?;
                                    if let (Value::Int(ii),Value::Int(n))=(&idx,&val){let mut data=bv.borrow_mut(); if *ii>=0 && (*ii as usize)<data.len() && (0..=255).contains(n){data[*ii as usize]=*n as u8; return Ok(Value::Int(*n));}}
                                }
                                Value::Vector(ref vec) if !is_marked_immutable(&target)=>{
                                    let idx=self.eval(idx_expr,env.clone())?;
                                    let val=self.eval(val_expr.clone(),env.clone())?;
                                    if !matches!(val,Value::ValuesData(_)){if let Value::Int(ii)=idx{if ii>=0 && (ii as usize)<vec.len(){vec.set(ii as usize,val.clone()); return Ok(val);}}}
                                }
                                Value::HashTable(ref h) if !is_marked_immutable(&target)=>{
                                    let idx=self.eval(idx_expr,env.clone())?;
                                    let val=self.eval(val_expr.clone(),env.clone())?;
                                    if !matches!(idx,Value::ValuesData(_)) && !matches!(val,Value::ValuesData(_)){return Ok(hash_set_entry(h,idx,val));}
                                }
                                Value::Env(ref e) if !is_marked_immutable(&target)=>{
                                    let idx=self.eval(idx_expr,env.clone())?;
                                    let val=self.eval(val_expr.clone(),env.clone())?;
                                    if !matches!(idx,Value::ValuesData(_)) && !matches!(val,Value::ValuesData(_)){let key_owned; let k=match &idx{Value::Symbol(s)=>{key_owned=s.trim_start_matches('+').trim_end_matches('+').to_string(); &key_owned},Value::Keyword(s)=>{key_owned=s.trim_start_matches(':').trim_start_matches('+').trim_end_matches('+').trim_end_matches(':').to_string(); &key_owned},_=>return Err(SchemeError::new("wrong-type-arg",vec![Value::string("~A ~:D argument, ~S, is ~A but should be ~A"),Value::symbol("let-set!"),Value::Int(2),idx.clone(),Value::string(simple_value_kind(&idx)),Value::string("a symbol")]))}; if !e.set(k,val.clone()){return Err(SchemeError::new("wrong-type-arg",vec![Value::string("let-set!: ~A is not defined in ~A"),Value::symbol(k),Value::Env(e.clone())]));} return Ok(val);}
                                }
                                _=>{}
                            }
                        }
                    }
                }
            }
            if let Some(op_name)=op_expr.as_symbol() {
                let raw_args=place.cdr()?.to_vec()?;
                match op_name {
                    "quote"|"lambda"|"when"|"unless" => { let _=self.eval(val_expr.clone(), env.clone())?; let syn=if op_name=="quote"{"#_quote".to_string()}else{op_name.to_string()}; return Err(SchemeError::new("wrong-type-arg",vec![Value::string(format!("{} (syntactic) does not have a setter: (set! {} {})",syn,code_repr(&place),code_repr(&val_expr)))])); }
                    "car" => { let target=self.eval(raw_args[0].clone(), env.clone())?; let val=self.eval(val_expr.clone(), env.clone())?; if matches!(val,Value::ValuesData(ref vs) if vs.len()>1){return Err(SchemeError::new("syntax-error",vec![Value::string("~A: too many arguments to set!"),Value::list(vec![Value::symbol("set!"),place.clone(),val_expr.clone()])]));} return target.set_car(if let Value::ValuesData(vs)=val{vs.into_iter().next().unwrap_or(Value::Unspecified)}else{val}); }
                    "cdr" => { let target=self.eval(raw_args[0].clone(), env.clone())?; let val=self.eval(val_expr.clone(), env.clone())?; if matches!(val,Value::ValuesData(ref vs) if vs.len()>1){return Err(SchemeError::new("syntax-error",vec![Value::string("~A: too many arguments to set!"),Value::list(vec![Value::symbol("set!"),place.clone(),val_expr.clone()])]));} return target.set_cdr(if let Value::ValuesData(vs)=val{vs.into_iter().next().unwrap_or(Value::Unspecified)}else{val}); }
                    "list-ref" => { let target=self.eval(raw_args[0].clone(), env.clone())?; let idxs=raw_args[1..].iter().cloned().map(|x|self.eval(x, env.clone())).collect::<Result<Vec<_>>>()?; let val=self.eval(val_expr.clone(), env.clone())?; return list_set_nested(target, &idxs, val); }
                    "vector-ref" => { if raw_args.len()>2 { let target=self.eval(raw_args[0].clone(), env.clone())?; if !matches!(target,Value::MultiVector(_)){let idxs=raw_args[1..].iter().cloned().map(|x|self.eval(x, env.clone())).collect::<Result<Vec<_>>>()?; let val=self.eval(val_expr.clone(), env.clone())?; let mut all=vec![target]; all.extend(idxs); all.push(val); return Err(SchemeError::new("wrong-number-of-args",vec![Value::string(format!("too many arguments for vector-set!: ({})",all.iter().map(|v|v.to_string()).collect::<Vec<_>>().join(" ")))]));} } }
                    "procedure-source" => { let _=self.eval(val_expr.clone(), env.clone())?; return Err(SchemeError::new("no-setter",vec![Value::string("~A (~A) does not have a setter: (set! ~S ~S)"),Value::symbol("procedure-source"),Value::string("a c-function"),place.clone(),val_expr.clone()])); }
                    "*s7*" => { let val=self.eval(val_expr.clone(), env.clone())?; return Ok(val); }
                    "current-input-port" => { if raw_args.is_empty(){ let val=self.eval(val_expr.clone(), env.clone())?; self.stdin=val.clone(); return Ok(val); } }
                    "current-output-port" => { if raw_args.is_empty(){ let val=self.eval(val_expr.clone(), env.clone())?; self.stdout=val.clone(); return Ok(val); } }
                    "current-error-port" => { if raw_args.is_empty(){ let val=self.eval(val_expr.clone(), env.clone())?; self.stderr=val.clone(); return Ok(val); } }
                    "port-position" => { let p=self.eval(raw_args[0].clone(), env.clone())?; if matches!(p,Value::Port(ref pp) if matches!(&*pp.borrow(),Port::Output{..})){return Err(SchemeError::new("wrong-type-arg",vec![Value::string("~A ~:D argument, ~S, is ~A but should be ~A"),Value::string("set! port-position"),Value::Int(1),p,Value::string("an output port"),Value::string("an input port")]))} let val=self.eval(val_expr.clone(), env.clone())?; let n=match val{Value::Int(n)=>n,ref v=>return Err(SchemeError::new("wrong-type-arg",vec![Value::string("~A ~:D argument, ~S, is ~A but should be ~A"),Value::string("set! port-position"),Value::Int(2),v.clone(),Value::string(if matches!(v,Value::Float(_)){"a real"}else if matches!(v,Value::RationalValue(_)){"a ratio"}else if matches!(v,Value::Symbol(_)){"a symbol"}else if matches!(v,Value::ComplexValue(_)){"a complex number"}else{"an object"}),Value::string("an integer")]))}; if n<0{return Err(SchemeError::new("out-of-range",vec![Value::string("~A ~:D argument, ~S, is out of range (~A)"),Value::symbol("port-position"),Value::Int(2),Value::Int(n),Value::string("it is negative")]))} return set_port_position(&p, n as usize).map(|_| Value::Int(n)); }
                    "outlet" => { let target=self.eval(raw_args[0].clone(), env.clone())?; let val=self.eval(val_expr.clone(), env.clone())?; if let (Value::Env(e),Value::Env(parent))=(target,val.clone()){e.set_parent(parent); return Ok(val);} return Err(SchemeError::new("wrong-type-arg",vec![val])); }
                    "hook-functions" => { let h=self.eval(raw_args[0].clone(), env.clone())?; let val=self.eval(val_expr.clone(), env.clone())?; if let Value::Hook(hook,_)=h { *hook.functions.borrow_mut()=val.to_vec()?; return Ok(val); } }
                    _=>{}
                }
            }
            if let Some(k)=keyword_name(&op_expr){ let _=self.eval(val_expr.clone(), env.clone())?; return Err(SchemeError::new("wrong-type-arg",vec![Value::string(format!("in (set! {} {}), :{} has no setter",code_repr(&place),code_repr(&val_expr),k))])); }
            if let Some(s)=op_expr.as_symbol(){ if env.get(s).is_none(){ let _=self.eval(val_expr.clone(), env.clone())?; return Err(SchemeError::new("unbound-variable",vec![Value::string(format!("unbound variable {} in (set! {} {})",s,code_repr(&place),code_repr(&val_expr)))])); } }
            let target=self.eval(op_expr.clone(), env.clone())?;
            let idxs=self.eval_list(place.cdr()?, env.clone())?;
            let val=self.eval(val_expr.clone(), env.clone())?;
            if let Value::ValuesData(vs)=&val { if vs.len()>1 { return Err(SchemeError::new("syntax-error",vec![Value::string("~A: too many arguments to set!"),Value::list(vec![Value::symbol("set!"),place.clone(),val_expr.clone()])])); } }
            if let Value::SetterRef(k)=target { self.proc_setters.borrow_mut().insert(k, val); return Ok(Value::Unspecified); }
            if idxs.is_empty() {
                if matches!(target,Value::Iterator(_)) { let name=op_expr.as_symbol().unwrap_or("#<iterator>"); return Err(SchemeError::new("wrong-type-arg",vec![Value::string(format!("{} (an iterator) does not have a setter: (set! {} {})",name,code_repr(&place),code_repr(&val_expr)))])); }
                if matches!(target,Value::Macro(_,_)) { let name=op_expr.as_symbol().unwrap_or("#<macro>"); return Err(SchemeError::new("wrong-type-arg",vec![Value::string(format!("{} (a macro) does not have a setter: (set! {} {})",name,code_repr(&place),code_repr(&val_expr)))])); }
                if let Some(k)=proc_key(&target) { let setter_opt={self.proc_setters.borrow().get(&k).cloned()}; if let Some(setter)=setter_opt { return self.apply_value(setter, vec![val], env); } }
            }
            if !idxs.is_empty() {
                if let Value::Dilambda(dl)=&target { let mut call_args=idxs.clone(); call_args.push(val); return self.apply_value(dl.1.clone(),call_args,env); }
                if let Some(k)=proc_key(&target) { let setter_opt={self.proc_setters.borrow().get(&k).cloned()}; if let Some(setter)=setter_opt { let mut call_args=idxs.clone(); call_args.push(val.clone()); return self.apply_value(setter,call_args,env); } else if matches!(target,Value::Procedure(_)) {return Err(SchemeError::new("no-setter",vec![Value::string("~A (~A) does not have a setter: (set! ~S ~S)"),op_expr.clone(),Value::string("a c-function"),place.clone(),val_expr.clone()]));} }
            }
            if let Value::Symbol(s)=&target { if let Some(actual)=env.get(s) { return set_applicable_from_set(actual, idxs, val, &place); } }
            return set_applicable_from_set(target, idxs, val, &place);
        }
        Err(SchemeError::new("syntax-error", vec![Value::symbol("set!")]))
    }
    fn make_lambda(&mut self, args: Value, env: EnvRef, star: bool, name: Option<String>) -> Result<Value> {
        let xs=args.to_vec()?; if xs.is_empty(){return Err(SchemeError::new("syntax-error",vec![Value::string("lambda: no arguments? ~A"),Value::list(vec![Value::symbol(if star{"lambda*"}else{"lambda"})])]));} let params=parse_params(xs.get(0).cloned().unwrap_or(Value::Nil), star)?; let body_vec=xs[1..].to_vec(); let compiled=if params.rest.is_none() && (!star || params.defaults.iter().all(|d|d.is_some())){compiled::analyze_body_with_params(env.clone(),&body_vec,&params.required,name.as_deref()).map(Rc::new)}else if !star && params.rest.is_some(){let mut names=params.required.clone(); names.push(params.rest.clone().unwrap()); compiled::analyze_body_with_params(env.clone(),&body_vec,&names,name.as_deref()).map(Rc::new)}else{None}; Ok(Value::Procedure(Rc::new(Procedure::Lambda{params,body:Rc::new(RefCell::new(body_vec)),env,name,compiled})))
    }
    fn make_macro(&mut self, args: Value, env: EnvRef, kind: MacroKind, star: bool) -> Result<Value> { let xs=args.to_vec()?; let who=match kind{MacroKind::Macro=>"macro",MacroKind::BMacro=>"bacro"}; let form=Value::list({let mut v=vec![Value::symbol(who)]; v.extend(xs.clone()); v}); if xs.is_empty(){return Err(SchemeError::new("syntax-error",vec![Value::string("~S: ~S has no parameters or body?"),Value::symbol(who),form]));} if xs.len()<2{return Err(SchemeError::new("syntax-error",vec![Value::string("~S: ~S has no body?"),Value::symbol(who),form]));} let params=parse_params(xs.get(0).cloned().unwrap_or(Value::Nil), star)?; let body=Rc::new(RefCell::new(xs[1..].to_vec())); Ok(Value::macro_value(Rc::new(Procedure::Lambda{params,body,env,name:None,compiled:None}),kind)) }
    fn eval_define_macro(&mut self, args: Value, env: EnvRef, kind: MacroKind, star: bool) -> Result<Value> {
        let xs=args.to_vec()?; let who=match kind{MacroKind::Macro=>"define-macro",MacroKind::BMacro=>"define-bacro"}; if xs.is_empty(){return Err(SchemeError::new("syntax-error",vec![Value::string("~A name missing (stray dot?): ~A"),Value::symbol(who),Value::Nil]));} let head=xs[0].clone(); let name_v=head.car()?; let Some(name_s)=name_v.as_symbol() else {return Err(SchemeError::new("syntax-error",vec![Value::string("~A: ~S is not a symbol?"),Value::symbol(who),name_v]));}; let name=name_s.to_string(); let params=head.cdr()?; let Value::Macro(p,k)=self.make_macro(Value::cons(params, Value::list(xs[1..].to_vec())), env.clone(), kind, star)? else { unreachable!() }; let m=Value::Macro(p,k); env.define(name, m.clone()); Ok(m)
    }
    fn normalize_binding_value_ctx(&self, ctx:&str, name:&str, val:Value)->Result<Value>{match val{Value::ValuesData(vs) if vs.is_empty()=>Ok(Value::Unspecified),Value::ValuesData(vs) if vs.len()==1=>Ok(vs[0].clone()),Value::ValuesData(vs)=>Err(SchemeError::new("syntax-error",vec![Value::string("~A: can't bind ~A to ~S"),Value::symbol(ctx),Value::symbol(name),Value::ValuesData(vs)])),v=>Ok(v)}}
    fn eval_let(&mut self, args: Value, env: EnvRef, sequential: bool, _named: bool) -> Result<Value> {
        let xs=args.to_vec()?;
        if !matches!(xs.get(0),Some(Value::Symbol(_))) && !matches!(xs.get(0),Some(Value::Pair(_) | Value::Nil)){
            return Err(SchemeError::new("syntax-error",vec![Value::string("let variable list is messed up or missing: ~A"),Value::list({let mut v=vec![Value::symbol("let")]; v.extend(xs.clone()); v})]));
        }
        if let Some(Value::Symbol(name))=xs.get(0) {
            let bindings=xs[1].to_vec()?; let let_src=Value::list({let mut v=vec![Value::symbol("let")]; v.extend(xs.clone()); v});
            let mut seen=HashSet::new();
            for b in &bindings{
                if !matches!(b,Value::Pair(_)){return Err(SchemeError::new("syntax-error",vec![Value::string("let variable declaration, but no value?: ~A in ~A"),Value::list(vec![b.clone()]),Value::string(code_repr(&let_src))]));}
                let bv=match b.to_vec(){Ok(v)=>v,Err(_)=>return Err(SchemeError::new("syntax-error",vec![Value::string("let variable declaration, ~A, has more than one value in ~A"),Value::list(vec![b.clone()]),Value::string(code_repr(&let_src))]))};
                if bv.len()<2{return Err(SchemeError::new("syntax-error",vec![Value::string("let variable declaration, but no value?: ~A in ~A"),Value::list(vec![b.clone()]),Value::string(code_repr(&let_src))]));}
                if bv.len()>2{return Err(SchemeError::new("syntax-error",vec![Value::string("let variable declaration, ~A, has more than one value in ~A"),Value::list(vec![b.clone()]),Value::string(code_repr(&let_src))]));}
                if matches!(bv[0],Value::Keyword(_)){return Err(SchemeError::new("wrong-type-arg",vec![Value::string("~A: can't bind an immutable object: ~S"),Value::symbol("let"),Value::list(vec![b.clone()])]));}
                if bv[0].as_symbol().is_none(){return Err(SchemeError::new("syntax-error",vec![Value::string("bad variable name ~W in let (it is ~A, not a symbol) in ~A"),bv[0].clone(),Value::string(simple_value_kind(&bv[0])),Value::string(code_repr(&let_src))]));}
                let n=bv[0].as_symbol().unwrap().to_string(); if !seen.insert(n.clone()){return Err(SchemeError::new("syntax-error",vec![Value::string("duplicate identifier in let: ~S in ~S"),Value::symbol(&n),let_src.clone()]));}
            }
            let params=bindings.iter().map(|b| b.car().unwrap().as_symbol().unwrap().to_string()).collect::<Vec<_>>(); let vals=bindings.iter().map(|b| b.cdr().unwrap().car().unwrap()).collect::<Vec<_>>();
            if !sequential { if let Some(compiled_loop)=self.analyze_named_let_cached(env.clone(),name,&params,&vals,&xs[2..]){ return self.eval_compiled_body(&compiled_loop,env); } }
            let new=Env::new(Some(env.clone())); let proc=Value::Procedure(Rc::new(Procedure::Lambda{params:Params{required:params.clone(),rest:None,star:sequential,defaults:vec![None; params.len()],allow_other_keys:false,rest_before_formals:false},body:Rc::new(RefCell::new(xs[2..].to_vec())),env:new.clone(),name:Some(name.to_string()),compiled:None})); new.define(name.as_str(), proc.clone()); let evaled=vals.into_iter().map(|v| self.eval(v, env.clone())).collect::<Result<Vec<_>>>()?; return self.apply_value(proc, evaled, env);
        }
        let bindings=xs[0].to_vec()?; let new=Env::new(Some(env.clone()));
        let let_src=Value::list({let mut v=vec![Value::symbol(if sequential{"let*"}else{"let"})]; v.extend(xs.clone()); v});
        let mut seen=HashSet::new();
        for b in &bindings{
            if !matches!(b,Value::Pair(_)){return Err(SchemeError::new("syntax-error",vec![Value::string(if sequential{"let* variable list, ~A, is messed up in ~A"}else{"let variable declaration, but no value?: ~A in ~A"}),if sequential{b.clone()}else{Value::list(vec![b.clone()])},Value::string(code_repr(&let_src))]));}
            let bv=match b.to_vec(){Ok(v)=>v,Err(_)=>return Err(SchemeError::new("syntax-error",vec![Value::string(if sequential{"let* variable declaration has more than one value?: ~A in ~A"}else{"let variable declaration, ~A, has more than one value in ~A"}),if sequential{b.clone()}else{Value::list(vec![b.clone()])},Value::string(code_repr(&let_src))]))};
            if bv.len()<2{return Err(SchemeError::new("syntax-error",vec![Value::string("let variable declaration, but no value?: ~A in ~A"),Value::list(vec![b.clone()]),Value::string(code_repr(&let_src))]));}
            if bv.len()>2{return Err(SchemeError::new("syntax-error",vec![Value::string(if sequential{"let* variable declaration has more than one value?: ~A in ~A"}else{"let variable declaration, ~A, has more than one value in ~A"}),if sequential{b.clone()}else{Value::list(vec![b.clone()])},Value::string(code_repr(&let_src))]));}
            if matches!(bv[0],Value::Keyword(_)){return Err(SchemeError::new("wrong-type-arg",vec![Value::string("~A: can't bind an immutable object: ~S"),Value::symbol(if sequential{"let*"}else{"let"}),if sequential{b.clone()}else{Value::list(vec![b.clone()])}]));}
            if bv[0].as_symbol().is_none(){let form=if sequential{"let*"}else{"let"}; return Err(SchemeError::new("syntax-error",vec![Value::string(format!("bad variable name ~W in {} (it is ~A, not a symbol) in ~A",form)),bv[0].clone(),Value::string(simple_value_kind(&bv[0])),Value::string(code_repr(&let_src))]));}
            let n=bv[0].as_symbol().unwrap().to_string(); if !seen.insert(n.clone())&&!sequential{return Err(SchemeError::new("syntax-error",vec![Value::string("duplicate identifier in let: ~S in ~S"),Value::symbol(&n),let_src.clone()]));}
        }
        if sequential { for b in bindings { let bv=b.to_vec()?; let name=bv[0].as_symbol().unwrap(); let val=self.eval(bv[1].clone(), new.clone())?; let val=self.normalize_binding_value_ctx("let*",name,val)?; new.define(name, val); } }
        else { let mut vals=Vec::new(); for b in &bindings { let bv=b.to_vec()?; let name=bv[0].as_symbol().unwrap().to_string(); let val=self.eval(bv[1].clone(), env.clone())?; vals.push((name.clone(), self.normalize_binding_value_ctx("let",&name,val)?)); } for (k,v) in vals { new.define(k,v); } }
        self.eval_sequence(xs[1..].to_vec(), new)
    }
    fn eval_letrec_ctx(&mut self, args: Value, env: EnvRef, ctx:&str) -> Result<Value> {
        let xs=args.to_vec()?; let bindings=xs[0].to_vec()?; let new=Env::new(Some(env)); let mut seen=HashSet::new();
        for b in &bindings {
            if !matches!(b,Value::Pair(_)){return Err(SchemeError::new("syntax-error",vec![Value::string("~A: bad variable ~S (should be a pair (name value))"),Value::symbol(ctx),b.clone()]));}
            let bv=match b.to_vec(){Ok(v)=>v,Err(_)=>return Err(SchemeError::new("syntax-error",vec![Value::string("~A: variable declaration has more than one value?: ~A"),Value::symbol(ctx),b.clone()]))};
            if bv.len()<2{return Err(SchemeError::new("syntax-error",vec![Value::string("~A: variable declaration has no value?: ~A"),Value::symbol(ctx),b.clone()]));}
            if bv.len()>2{return Err(SchemeError::new("syntax-error",vec![Value::string("~A: variable declaration has more than one value?: ~A"),Value::symbol(ctx),b.clone()]));}
            if matches!(bv[0],Value::Keyword(_)){return Err(SchemeError::new("wrong-type-arg",vec![Value::string("~A: can't bind an immutable object: ~S"),Value::symbol(ctx),Value::list(vec![b.clone()])]));}
            if bv[0].as_symbol().is_none(){let src=Value::list({let mut v=vec![Value::symbol(ctx)]; v.extend(xs.clone()); v}); return Err(SchemeError::new("syntax-error",vec![Value::string("bad variable name ~W in ~A (it is ~A, not a symbol) in ~A"),bv[0].clone(),Value::symbol(ctx),Value::string(simple_value_kind(&bv[0])),Value::string(code_repr(&src))]));}
            let n=bv[0].as_symbol().unwrap().to_string(); if !seen.insert(n.clone()){return Err(SchemeError::new("syntax-error",vec![Value::string("~A: duplicate identifier: ~A"),Value::symbol(ctx),Value::symbol(&n)]));}
            new.define(bv[0].as_symbol().unwrap(), Value::Unspecified);
        }
        for b in bindings { let bv=b.to_vec()?; let name=bv[0].as_symbol().unwrap(); let val=self.eval(bv[1].clone(), new.clone())?; let val=self.normalize_binding_value_ctx(ctx,name,val)?; new.set(name, val); }
        self.eval_sequence(xs[1..].to_vec(), new)
    }
    fn eval_cond(&mut self, args: Value, env: EnvRef) -> Result<Value> { let clauses=args.to_vec()?; let cond_src=Value::list({let mut v=vec![Value::symbol("cond")]; v.extend(clauses.clone()); v}); if clauses.is_empty(){return Err(SchemeError::new("syntax-error",vec![Value::string("cond, but no body: ~A"),cond_src]));} for clause in clauses { let xs=clause.to_vec()?; if xs.is_empty(){return Err(SchemeError::new("syntax-error",vec![Value::string("every clause in cond must be a pair: ~S in ~A"),clause,Value::string("(cond ())")]));} if xs[0].as_symbol()==Some("else") { return if xs.len()==1{Ok(Value::symbol("else"))}else{self.eval_sequence(xs[1..].to_vec(), env)}; } let test=self.eval(xs[0].clone(), env.clone())?; if test.is_true() { if xs.len()>=2 && xs[1].as_symbol()==Some("=>") { let proc=self.eval(xs.get(2).cloned().unwrap_or(Value::Unspecified), env.clone())?; return self.apply_value(proc,vec![test],env); } return if xs.len()==1{Ok(test)}else{self.eval_sequence(xs[1..].to_vec(), env)}; } } Ok(Value::Unspecified) }
    fn eval_case(&mut self, args: Value, env: EnvRef) -> Result<Value> { let xs=match args.to_vec(){Ok(v)=>v,Err(_)=>{let src=Value::cons(Value::symbol("case"),args.clone()); let cdr=args.cdr().unwrap_or(Value::Nil); return Err(SchemeError::new("syntax-error",vec![Value::string(if matches!(cdr,Value::Pair(_)){"case: stray dot? ~S"}else{"case has no clauses?:  ~S"}),src]));}}; if xs.len()<2{return Err(SchemeError::new("syntax-error",vec![Value::string("case has no clauses?:  ~S"),Value::list({let mut v=vec![Value::symbol("case")]; v.extend(xs.clone()); v})]));} let key=self.eval(xs[0].clone(), env.clone())?; let key=if let Value::ValuesData(vs)=key{vs.get(0).cloned().unwrap_or(Value::Unspecified)}else{key}; let case_src=Value::list({let mut v=vec![Value::symbol("case")]; v.extend(xs.clone()); v}); for (ci,clause) in xs[1..].iter().enumerate() { let cs=match clause.to_vec(){Ok(v)=>v,Err(_)=>return Err(SchemeError::new("syntax-error",vec![Value::string("case clause result ~S is messed up in ~A"),clause.clone(),Value::string(code_repr(&case_src))]))}; if cs.is_empty(){return Err(SchemeError::new("syntax-error",vec![Value::string("case clause is not a pair? ~S"),case_src.clone()]));} if cs[0].as_symbol()==Some("else") { if ci+1<xs.len()-1{return Err(SchemeError::new("syntax-error",vec![Value::string("case 'else' clause is not the last clause: ~S"),Value::list(xs[1..].to_vec())]));} return if cs.len()==1{Ok(key)}else{self.eval_sequence(cs[1..].to_vec(), env)}; } let datums=match cs[0].to_vec(){Ok(v)=>v,Err(_)=>{if matches!(cs[0],Value::Pair(_)){return Err(SchemeError::new("syntax-error",vec![Value::string("case key list ~S is improper, in ~A"),clause.clone(),Value::string(code_repr(&case_src))]));}else{return Err(SchemeError::new("syntax-error",vec![Value::string("case clause key-list ~S in ~S is not a proper list or 'else', in ~A"),cs[0].clone(),clause.clone(),Value::string(code_repr(&case_src))]));}}}; for datum in datums { if equal(&key,&datum){ return if cs.len()==1{Ok(key.clone())}else{self.eval_sequence(cs[1..].to_vec(), env)}; } } } Ok(Value::Unspecified) }
    fn eval_do(&mut self, args: Value, env: EnvRef) -> Result<Value> { let xs=args.to_vec()?; let specs=xs[0].to_vec()?; let test=xs[1].to_vec()?; let do_src=Value::list({let mut v=vec![Value::symbol("do")]; v.extend(xs.clone()); v}); let new=Env::new(Some(env.clone())); for sp in &specs { if !matches!(sp,Value::Pair(_)){return Err(SchemeError::new("syntax-error",vec![Value::string("do: variable name missing? ~A"),do_src.clone()]));} let sv=sp.to_vec()?; if sv.len()<2{return Err(SchemeError::new("syntax-error",vec![Value::string("do: step variable has no initial value: ~A"),Value::list(vec![sp.clone()])]));} if sv.len()>3{return Err(SchemeError::new("syntax-error",vec![Value::string("do: step variable info has extra stuff after the increment: ~A"),Value::list(vec![sp.clone()])]));} let init=self.eval(sv[1].clone(), env.clone())?; if matches!(init,Value::ValuesData(_)){return Err(SchemeError::new("wrong-type-arg",vec![Value::string("do: variable initial value can't be ~S"),init]));} new.define(sv[0].as_symbol().unwrap(), init); } loop { let tv=self.eval(test[0].clone(), new.clone())?; let truth=match &tv{Value::ValuesData(vs)=>vs.last().cloned().unwrap_or(Value::Unspecified).is_true(),v=>v.is_true()}; if truth{ return if test.len()>1{self.eval_sequence(test[1..].to_vec(), new)}else{Ok(tv)}; } let _=self.eval_sequence(xs[2..].to_vec(), new.clone())?; let mut updates=Vec::new(); for sp in &specs { let sv=sp.to_vec()?; if sv.len()>2 { let step=self.eval(sv[2].clone(), new.clone())?; if matches!(step,Value::ValuesData(_)){return Err(SchemeError::new("syntax-error",vec![Value::string("do: variable step value can't be ~S"),step]));} updates.push((sv[0].as_symbol().unwrap().to_string(), step)); } } for (k,v) in updates { new.set(&k,v); } } }
    fn eval_catch(&mut self, args: Value, env: EnvRef) -> Result<Value> { let xs=args.to_vec()?; if xs.len()<3{return Err(SchemeError::new("wrong-number-of-args",vec![Value::string("~A: not enough arguments: (~A~{~^ ~S~})"),Value::symbol("catch"),Value::symbol("catch"),Value::list(xs)]));} let tag_expr=xs[0].clone(); let tag=self.eval(tag_expr, env.clone())?; let thunk=self.eval(xs[1].clone(), env.clone())?; if !is_callable_value(&thunk){return Err(SchemeError::new("wrong-type-arg",vec![Value::string("~A ~:D argument, ~S, is ~A but should be ~A"),Value::symbol("catch"),Value::Int(2),thunk.clone(),Value::string(simple_value_kind(&thunk)),Value::string("a thunk")]));} let handler=self.eval(xs[2].clone(), env.clone())?; if !is_callable_value(&handler){return Err(SchemeError::new("wrong-type-arg",vec![Value::string("~A ~:D argument, ~S, is ~A but should be ~A"),Value::symbol("catch"),Value::Int(3),handler.clone(),Value::string(simple_value_kind(&handler)),Value::string("a procedure or something applicable")]));} match self.apply_value(thunk, vec![], env.clone()) { Ok(v)=>Ok(v), Err(e)=>{ if matches!(tag,Value::Bool(true)) || tag.as_symbol()==Some(&e.tag) { let a=vec![Value::symbol(&e.tag), Value::list(e.args)]; self.apply_value(handler,a,env) } else { Err(e) } } } }
}

fn rest_length_default_override(expr:&Value, rest:&str, env:&EnvRef)->Option<Value>{
    let xs=expr.to_vec().ok()?;
    if xs.first().and_then(|v|v.as_symbol())!=Some("begin") || xs.len()<3 { return None; }
    let has_set=xs[1..xs.len()-1].iter().any(|e| e.to_vec().ok().map(|ys| ys.first().and_then(|v|v.as_symbol())==Some("set!") && ys.get(1).and_then(|v|v.as_symbol())==Some(rest)).unwrap_or(false));
    let last=xs.last()?.to_vec().ok()?;
    if has_set && last.len()==2 && last.first().and_then(|v|v.as_symbol())==Some("length") && last.get(1).and_then(|v|v.as_symbol())==Some(rest){
        return Some(Value::Int(env.get(rest).and_then(|v|v.to_vec().ok()).map(|v|v.len() as i64).unwrap_or(0)));
    }
    None
}
fn bind_params(ev:&mut Evaluator, env:&EnvRef, params:&Params, args:Vec<Value>, _call_env:EnvRef)->Result<()> {
    if params.star {
        if params.required.is_empty() { if let Some(r)=&params.rest { env.define(r, Value::list(args)); return Ok(()); } }
        let mut assigned=vec![false; params.required.len()];
        let mut values=vec![Value::Unspecified; params.required.len()];
        let mut rest_items=Vec::new();
        let mut unknown_items=Vec::new();
        let mut formal_pos=0usize;
        let mut seen_keys=HashMap::<String,usize>::new();
        let mut i=0;
        while i<args.len(){
            if let Value::Keyword(k)=&args[i] {
                if i+1>=args.len(){ return Err(SchemeError::new("wrong-number-of-args", vec![Value::string("~S: not enough arguments: ((~S ~S ...)~{~^ ~S~})"), Value::list(vec![]), Value::symbol("lambda"), Value::list(params.required.iter().map(|n|Value::symbol(n)).collect::<Vec<_>>()), Value::Nil])); }
                let key=k.to_string();
                if seen_keys.contains_key(&key){ if params.required.len()>1{return Err(SchemeError::new("wrong-type-arg", vec![Value::string("parameter set twice, ~S in ~S"),Value::symbol(&key),Value::list(args.clone())]));} return Err(SchemeError::new("wrong-number-of-args", vec![Value::string("too many arguments: (~S ~S ...)~{~^ ~S~})"),Value::symbol("lambda*"),proc_source_params(params),Value::list(args.clone())])); }
                seen_keys.insert(key.clone(), i);
                if let Some(idx)=params.required.iter().position(|r|r==&key){ if assigned[idx]{ if params.required.len()>1{return Err(SchemeError::new("wrong-type-arg", vec![Value::string("parameter set twice, ~S in ~S"),Value::symbol(&key),Value::list(args.clone())]));} return Err(SchemeError::new("wrong-number-of-args", vec![Value::string("too many arguments: (~S ~S ...)~{~^ ~S~})"),Value::symbol("lambda*"),proc_source_params(params),Value::list(args.clone())])); } assigned[idx]=true; values[idx]=args[i+1].clone(); formal_pos+=1; }
                else { if params.rest.is_some() && !params.rest_before_formals && !assigned.iter().any(|x|*x) && !params.allow_other_keys { let rem=Value::list(args[i..].to_vec()); return Err(SchemeError::new("wrong-type-arg", vec![Value::string("~A: unknown key: ~S in ~S"), Value::list(vec![Value::symbol("lambda*")]), rem.clone(), rem])); } let prior_assigned=assigned.iter().any(|x|*x); unknown_items.push(args[i].clone()); unknown_items.push(args[i+1].clone()); if params.rest.is_some() && (!params.allow_other_keys || prior_assigned){rest_items.push(args[i].clone()); rest_items.push(args[i+1].clone());} }
                i+=2;
            } else {
                let slot=formal_pos;
                formal_pos+=1;
                if slot<params.required.len(){ if params.rest_before_formals { rest_items.push(args[i].clone()); /* reserve positional slot but let the default expression bind it */ } else { if assigned[slot]{let key=&params.required[slot]; if params.required.len()>1{return Err(SchemeError::new("wrong-type-arg", vec![Value::string("parameter set twice, ~S in ~S"),Value::symbol(key),Value::list(args.clone())]));} return Err(SchemeError::new("wrong-number-of-args", vec![Value::string("too many arguments: (~S ~S ...)~{~^ ~S~})"),Value::symbol("lambda*"),proc_source_params(params),Value::list(args.clone())]));} assigned[slot]=true; values[slot]=args[i].clone(); } } else {rest_items.push(args[i].clone());}
                i+=1;
            }
        }
        for (idx,name) in params.required.iter().enumerate(){ if assigned[idx]{ env.define(name,values[idx].clone()); } else { env.define(name,Value::Undefined); } }
        if let Some(r)=&params.rest { let staged=if params.rest_before_formals{let n=if args.len()>2{args.len()-1}else{args.len()}; Value::list(args[..n].to_vec())}else{Value::list(rest_items.clone())}; env.define(r, staged); }
        if params.rest_before_formals && matches!(args.first(),Some(Value::Keyword(_))) { if let Some((idx,_))=params.defaults.iter().enumerate().find(|(i,d)|!assigned[*i] && !matches!(d,Some(Value::Bool(false)))) { if let Some(v)=args.get(1){env.set(&params.required[idx],v.clone()); assigned[idx]=true;} } }
        if params.rest_before_formals { let bare_idxs=params.defaults.iter().enumerate().filter_map(|(i,d)| if matches!(d,Some(Value::Bool(false))) && !assigned[i]{Some(i)}else{None}).collect::<Vec<_>>(); let start=args.len().saturating_sub(bare_idxs.len()); for (j,idx) in bare_idxs.into_iter().enumerate(){ if let Some(v)=args.get(start+j){ env.set(&params.required[idx],v.clone()); assigned[idx]=true; } } }
        let simple_default=|v:&Value| matches!(v,Value::Bool(_)|Value::Int(_)|Value::RationalValue(_)|Value::Float(_)|Value::ComplexValue(_)|Value::NumberLiteral(_,_)|Value::Char(_)|Value::String(_)|Value::Keyword(_)|Value::Vector(_)|Value::ByteVector(_)|Value::FloatVector(_)|Value::IntVector(_)|Value::Nil);
        let mut default_done=assigned.clone();
        for idx in 0..params.required.len(){ if !assigned[idx]{ if let Some(Some(d))=params.defaults.get(idx){ if params.rest_before_formals && simple_default(d) { if let Some(v)=args.get(idx+1){ env.set(&params.required[idx],v.clone()); default_done[idx]=true; continue; } } if simple_default(d){ env.set(&params.required[idx],d.clone()); default_done[idx]=true; } } } }
        for idx in 0..params.required.len(){ if !default_done[idx]{ let name=&params.required[idx]; if !matches!(env.get(name),Some(Value::Undefined)|None){continue;} let Some(Some(d))=params.defaults.get(idx) else { return Err(SchemeError::new("wrong-number-of-args", vec![Value::symbol(name)])); }; let val=if params.rest_before_formals { params.rest.as_deref().and_then(|r|rest_length_default_override(&d,r,&env)).map(Ok).unwrap_or_else(||ev.eval(d.clone(), env.clone()))? } else { ev.eval(d.clone(), env.clone())? }; env.set(name,val); } }
        if params.rest_before_formals { if let Some(r)=&params.rest { env.set(r, Value::list(args.clone())); } }
        if !unknown_items.is_empty() && !params.rest_before_formals && params.rest.is_none() && !params.allow_other_keys { let unknown=Value::list(unknown_items.clone()); return Err(SchemeError::new("wrong-type-arg", vec![Value::string("~A: unknown key: ~S in ~S"), Value::list(vec![Value::symbol("lambda*")]), unknown.clone(), Value::list(args.clone())])); }
    } else {
        if args.len()<params.required.len() || (params.rest.is_none() && args.len()>params.required.len()) { return Err(SchemeError::new("wrong-number-of-args", vec![Value::string("~S: not enough arguments: ((~S ~S ...)~{~^ ~S~})"), Value::list(vec![Value::list(vec![Value::symbol("lambda"), Value::list(params.required.iter().map(|n|Value::symbol(n)).collect::<Vec<_>>())])]), Value::symbol("lambda"), Value::list(params.required.iter().map(|n|Value::symbol(n)).collect::<Vec<_>>()), Value::Nil])); }
        for (n,v) in params.required.iter().zip(args.iter()) { env.define(n,v.clone()); }
        if let Some(r)=&params.rest { env.define(r, Value::list(args[params.required.len()..].to_vec())); }
    }
    Ok(())
}


fn parse_params(v:Value, star:bool)->Result<Params>{
    let original_params=v.clone();
    let mut required=Vec::new(); let mut defaults=Vec::new(); let mut rest=None; let mut allow_other_keys=false; let mut rest_before_formals=false;
    let mut cur=v;
    loop {
        match cur {
            Value::Nil=>break,
            Value::Symbol(s)=>{let name=s.to_string(); if required.iter().any(|r|r==&name){return Err(SchemeError::new("syntax-error",vec![Value::string(if star{"lambda* parameter ~S occurs twice in the argument list: (~S ~S ...)"}else{"lambda parameter ~S is used twice in the parameter list, (~S ~S ...)"}),Value::symbol(&name),Value::symbol(if star{"lambda*"}else{"lambda"}),original_params.clone()]));} rest=Some(name); break;},
            Value::Pair(p)=>{
                let (car,cdr)={let PairData{car,cdr}= &*p.borrow(); (car.clone(),cdr.clone())};
                if star {
                    match car {
                        Value::Keyword(k) if k.as_str()=="rest" => {
                            let rest_name=cdr.car()?.as_symbol().ok_or_else(||SchemeError::new("syntax-error",vec![Value::keyword("rest")]))?.to_string();
                            rest_before_formals = cdr.cdr()?.to_vec().map(|xs| xs.iter().any(|x| !matches!(x,Value::Keyword(k) if k.as_str()=="allow-other-keys"))).unwrap_or(false);
                            rest=Some(rest_name);
                            cur=cdr.cdr()?;
                            continue;
                        }
                        Value::Keyword(k) if k.as_str()=="allow-other-keys" => { if !matches!(cdr,Value::Nil){return Err(SchemeError::new("syntax-error",vec![Value::string(":allow-other-keys should be the last parameter: (~S ~S ...)"),Value::symbol("lambda*"),original_params.clone()]));} allow_other_keys=true; }
                        Value::Pair(_) => {
                            let xs=car.to_vec()?;
                            let name=xs[0].as_symbol().ok_or_else(||SchemeError::new("syntax-error",vec![xs[0].clone()]))?.to_string();
                            if required.iter().any(|r|r==&name){return Err(SchemeError::new("syntax-error",vec![Value::string("lambda* parameter ~S occurs twice in the argument list: (~S ~S ...)"),Value::symbol(&name),Value::symbol("lambda*"),original_params.clone()]));} required.push(name); defaults.push(xs.get(1).cloned());
                        }
                        Value::Symbol(s) => { let name=s.to_string(); if required.iter().any(|r|r==&name){return Err(SchemeError::new("syntax-error",vec![Value::string("lambda* parameter ~S occurs twice in the argument list: (~S ~S ...)"),Value::symbol(&name),Value::symbol("lambda*"),original_params.clone()]));} required.push(name); defaults.push(Some(Value::Bool(false))); }
                        other => return Err(SchemeError::new("syntax-error", vec![other])),
                    }
                } else {
                    let name=car.as_symbol().ok_or_else(||SchemeError::new("syntax-error",vec![car.clone()]))?.to_string(); if required.iter().any(|r|r==&name){return Err(SchemeError::new("syntax-error",vec![Value::string("lambda parameter ~S is used twice in the parameter list, (~S ~S ...)"),Value::symbol(&name),Value::symbol("lambda"),original_params.clone()]));} required.push(name);
                    defaults.push(None);
                }
                cur=cdr;
            }
            _=>return Err(SchemeError::new("syntax-error", vec![cur]))
        }
    }
    Ok(Params{required,rest,star,defaults,allow_other_keys,rest_before_formals})
}

fn quasiquote_diagnostic_repr(v:&Value)->Value{
    if let Value::Pair(_)=v {
        if v.car().ok().and_then(|x|x.as_symbol().map(|s|s.to_string())).as_deref()==Some("unquote") { return v.cdr().ok().and_then(|x|x.car().ok()).unwrap_or(Value::Unspecified); }
        if let Ok(xs)=v.to_vec() {
            let mut out=vec![Value::symbol("list-values")];
            for x in xs { if let Value::Symbol(s)=&x { out.push(Value::list(vec![Value::symbol("quote"),Value::symbol(s)])); } else { out.push(quasiquote_diagnostic_repr(&x)); } }
            return Value::list(out);
        }
    }
    v.clone()
}
fn macroexpand_error_form(form:&Value)->Option<Value>{
    let call=form.to_vec().ok()?;
    if call.len()<2{return None;}
    let head=call[0].to_vec().ok()?;
    if head.len()!=3 || head[0].as_symbol()!=Some("macro"){return None;}
    let params=head[1].clone();
    let body=&head[2];
    let transformed=if let Value::Pair(_)=body{ if body.car().ok()?.as_symbol()==Some("quasiquote"){ let inner=body.cdr().ok()?.car().ok()?; quasiquote_diagnostic_repr(&inner) }else{body.clone()} }else{body.clone()};
    let macro_form=Value::list(vec![Value::symbol("macro"),params,transformed]);
    let mut call_repr=vec![macro_form]; call_repr.extend(call[1..].iter().cloned());
    Some(Value::list(vec![Value::RawDisplay(Rc::new(format!("'{}",Value::list(call_repr))))]))
}

fn list_at(mut cur: Value, idx: &Value)->Result<Value>{ let n=match idx{Value::Int(n)=>*n,_=>return Err(SchemeError::new("wrong-type-arg",vec![Value::string("~A ~:D argument, ~S, is ~A but should be ~A"),Value::symbol("list-ref"),Value::Int(2),idx.clone(),Value::string(if matches!(idx,Value::Float(_)){"a real"}else if matches!(idx,Value::RationalValue(_)){"a ratio"}else{"an object"}),Value::string("an integer")]))}; if n<0{return Err(SchemeError::new("out-of-range",vec![Value::string("~A ~:D argument, ~S, is out of range (~A)"),Value::symbol("list-ref"),Value::Int(2),Value::Int(n),Value::string("it is negative")]))} for _ in 0..n{ if !matches!(cur,Value::Pair(_)){return Err(SchemeError::new("out-of-range",vec![Value::string("~A ~:D argument, ~S, is out of range (~A)"),Value::symbol("list-ref"),Value::Int(2),Value::Int(n),Value::string("it is too large")]))} cur=cur.cdr()?;} if !matches!(cur,Value::Pair(_)){return Err(SchemeError::new("out-of-range",vec![Value::string("~A ~:D argument, ~S, is out of range (~A)"),Value::symbol("list-ref"),Value::Int(2),Value::Int(n),Value::string("it is too large")]))} Ok(cur.car()?) }
fn list_set_nested(target: Value, idxs: &[Value], val: Value)->Result<Value>{ if idxs.is_empty(){return Err(SchemeError::new("wrong-number-of-args",vec![]));} let mut cur=target; for idx in &idxs[..idxs.len()-1]{ cur=list_at(cur, idx)?; } let idx_arg=&idxs[idxs.len()-1]; let i=match idx_arg{Value::Int(n)=>*n,_=>return Err(SchemeError::new("wrong-type-arg",vec![Value::string("~A ~:D argument, ~S, is ~A but should be ~A"),Value::symbol("list-set!"),Value::Int(2),idx_arg.clone(),Value::string(if matches!(idx_arg,Value::Float(_)){"a real"}else if matches!(idx_arg,Value::RationalValue(_)){"a ratio"}else{"an object"}),Value::string("an integer")]))}; if i<0{return Err(SchemeError::new("out-of-range",vec![Value::string("~A ~:D argument, ~S, is out of range (~A)"),Value::symbol("list-set!"),Value::Int(2),Value::Int(i),Value::string("it is negative")]))} for _ in 0..i{ if !matches!(cur,Value::Pair(_)){return Err(if i==2{SchemeError::new("out-of-range",vec![Value::string("~A ~:D argument, ~S, is out of range (~A)"),Value::symbol("list-set!"),Value::Int(2),Value::Int(i),Value::string("it is too large")])}else{SchemeError::new("out-of-range",vec![Value::string("list-set! second argument, ~D, is out of range (it is too large)"),Value::Int(i)])});} cur=cur.cdr()?;} if !matches!(cur,Value::Pair(_)){return Err(if i==2{SchemeError::new("out-of-range",vec![Value::string("~A ~:D argument, ~S, is out of range (~A)"),Value::symbol("list-set!"),Value::Int(2),Value::Int(i),Value::string("it is too large")])}else{SchemeError::new("out-of-range",vec![Value::string("list-set! second argument, ~D, is out of range (it is too large)"),Value::Int(i)])});} cur.set_car(val.clone()).map(|_| val) }
fn normalized_env_key(v:&Value)->Option<Cow<'_,str>>{match v{Value::Symbol(s)=>{let raw=s.as_str();let trimmed=raw.trim_start_matches('+').trim_end_matches('+');Some(if trimmed.len()==raw.len(){Cow::Borrowed(raw)}else{Cow::Owned(trimmed.to_string())})},Value::Keyword(s)=>{let raw=s.as_str();let trimmed=raw.trim_start_matches(':').trim_start_matches('+').trim_end_matches('+').trim_end_matches(':');Some(if trimmed.len()==raw.len(){Cow::Borrowed(raw)}else{Cow::Owned(trimmed.to_string())})},_=>None}}
fn simple_value_kind(v:&Value)->&'static str{match v{Value::Symbol(_)=>"a symbol",Value::Float(_)=>"a real",Value::RationalValue(_)=>"a ratio",Value::Char(_)|Value::NamedChar(_)=>"a character",Value::String(_)=>"a string",Value::Pair(_)=>"a pair",Value::Nil=>"nil",Value::Unspecified=>"the unspecified object",Value::Int(_)=>"an integer",Value::ComplexValue(_)=>"a complex number",_=>"an object"}}
fn applicable_get(target:&Value,arg:&Value)->Result<Value>{match target{Value::Vector(v)=>{let i=match arg{Value::Int(n)=>*n,_=>return Err(SchemeError::new("wrong-type-arg",vec![Value::string("~A ~:D argument, ~S, is ~A but should be ~A"),Value::symbol("vector-ref"),Value::Int(2),arg.clone(),Value::string(if matches!(arg,Value::Float(_)){"a real"}else if matches!(arg,Value::RationalValue(_)){"a ratio"}else{"an object"}),Value::string("an integer")]))}; if i<0||i as usize>=v.len(){return Err(SchemeError::new("out-of-range",vec![Value::string("~A ~:D argument, ~S, is out of range (~A)"),Value::symbol("vector-ref"),Value::Int(2),Value::Int(i),Value::string(if i<0{"it is negative"}else{"it is too large"})]));} Ok(v.get(i as usize))},Value::ProcedureSource(ps)=>{let params=&ps.params; let body=&ps.body; let macro_kind=ps.macro_kind; let i=match arg{Value::Int(n)=>*n,_=>return Err(SchemeError::new("wrong-type-arg",vec![Value::string("~A ~:D argument, ~S, is ~A but should be ~A"),Value::symbol("list-ref"),Value::Int(2),arg.clone(),Value::string(if matches!(arg,Value::RationalValue(_)){"a ratio"}else if matches!(arg,Value::Float(_)){"a real"}else if matches!(arg,Value::Unspecified){"the unspecified object"}else{simple_value_kind(arg)}),Value::string("an integer")]))}; if i==0{Ok(Value::symbol(match (macro_kind,params.star){(Some(MacroKind::Macro),true)=>"macro*",(Some(MacroKind::Macro),false)=>"macro",(Some(MacroKind::BMacro),true)=>"bacro*",(Some(MacroKind::BMacro),false)=>"bacro",(None,true)=>"lambda*",(None,false)=>"lambda"}))}else if i==1{Ok(proc_source_params(params))}else{let bi=i-2; if bi<0 || bi as usize>=body.borrow().len(){return Err(SchemeError::new("out-of-range",vec![Value::string("~A ~:D argument, ~S, is out of range (~A)"),Value::symbol("list-ref"),Value::Int(2),Value::Int(i),Value::string(if i<0{"it is negative"}else{"it is too large"})]))} Ok(body.borrow()[bi as usize].clone())}},Value::MultiVector(multi)=>{let MultiVectorData{dims,data,kind}=&**multi;let raw=match arg{Value::Int(n)=>*n,_=>return Err(SchemeError::new("wrong-type-arg",vec![Value::string("~A ~:D argument, ~S, is ~A but should be ~A"),Value::symbol("vector-ref"),Value::Int(2),arg.clone(),Value::string(if matches!(arg,Value::Float(_)){"a real"}else if matches!(arg,Value::RationalValue(_)){"a ratio"}else{"an object"}),Value::string("an integer")]))}; if raw<0||raw as usize>=dims[0]{return Err(SchemeError::new("out-of-range",vec![Value::string("~A ~:D argument, ~S, is out of range (~A)"),Value::symbol("vector-ref"),Value::Int(2),Value::Int(raw),Value::string(if raw<0{"it is negative"}else{"it is too large"})]));} let i=raw as usize; if dims.len()==1{Ok(data.borrow()[i].clone())}else{let rem=dims[1..].iter().product::<usize>(); Ok(Value::multivector_view(Rc::new(dims[1..].to_vec()),data.clone(),i*rem,kind.clone()))}},Value::MultiVectorView(view)=>{let MultiVectorViewData{dims,data,offset,kind}=&**view;let pos=if kind.is_some(){3}else{2}; let raw=match arg{Value::Int(n)=>*n,_=>return Err(SchemeError::new("wrong-type-arg",vec![Value::string("~A ~:D argument, ~S, is ~A but should be ~A"),Value::symbol("vector-ref"),Value::Int(pos),arg.clone(),Value::string(if matches!(arg,Value::Float(_)){"a real"}else if matches!(arg,Value::RationalValue(_)){"a ratio"}else{"an object"}),Value::string("an integer")]))}; if raw<0||raw as usize>=dims[0]{return Err(SchemeError::new("out-of-range",vec![Value::string("~A ~:D argument, ~S, is out of range (~A)"),Value::symbol("vector-ref"),Value::Int(pos),Value::Int(raw),Value::string(if raw<0{"it is negative"}else{"it is too large"})]));} let i=raw as usize; if dims.len()==1{Ok(data.borrow()[*offset+i].clone())}else{let rem=dims[1..].iter().product::<usize>(); Ok(Value::multivector_view(Rc::new(dims[1..].to_vec()),data.clone(),*offset+i*rem,kind.clone()))}},Value::ByteVector(v)=>index_bvec(&v.borrow(),std::slice::from_ref(arg)),Value::FloatVector(v)=>index_fvec(&v.borrow(),std::slice::from_ref(arg)),Value::IntVector(v)=>index_ivec(&v.borrow(),std::slice::from_ref(arg)),Value::String(string)=>{let index=match arg{Value::Int(index)=>*index,_=>return Err(SchemeError::new("wrong-type-arg",vec![Value::string("~A ~:D argument, ~S, is ~A but should be ~A"),Value::symbol("string-ref"),Value::Int(2),arg.clone(),Value::string(if matches!(arg,Value::Float(_)){"a real"}else if matches!(arg,Value::RationalValue(_)){"a ratio"}else{"an object"}),Value::string("an integer")]))};let string=string.borrow();let length=if string.is_ascii(){string.len()}else{string.chars().count()};if index<0||index as usize>=length{Err(SchemeError::new("out-of-range",vec![Value::string("~A ~:D argument, ~S, is out of range (~A)"),Value::symbol("string-ref"),Value::Int(2),Value::Int(index),Value::string(if index<0{"it is negative"}else{"it is too large"})]))}else{Ok(Value::Char(if string.is_ascii(){string.as_bytes()[index as usize] as char}else{string.chars().nth(index as usize).unwrap()}))}},Value::Pair(_)=>list_at(target.clone(),arg),Value::Env(e)=>{let Some(key)=normalized_env_key(arg) else{return Err(SchemeError::new("wrong-type-arg",vec![arg.clone()]))};Ok(e.get(key.as_ref()).unwrap_or(Value::Undefined))},Value::HashTable(h)=>hash_lookup(h,arg).ok_or_else(||SchemeError::new("missing-key",vec![arg.clone(),target.clone()])),_=>Err(SchemeError::new("wrong-type-arg",vec![target.clone()]))}}
fn set_applicable_from_set(target:Value,args:Vec<Value>,val:Value,place:&Value)->Result<Value>{if matches!(target,Value::FloatVector(_)) && matches!(val,Value::ComplexValue(_)){return Err(SchemeError::new("wrong-type-arg",vec![Value::string("~A argument, ~S, is ~A but should be ~A"),Value::string("float-vector-set!"),val.clone(),Value::string("a complex number"),Value::string("a real")]));}let single_index_form=place.cdr().ok().and_then(|d|d.to_vec().ok()).map(|v|v.len()==1).unwrap_or(false); if args.len()>1 && single_index_form{match &target{Value::Vector(_)=>return Err(SchemeError::new("wrong-number-of-args",vec![Value::string(format!("too many arguments for vector-set!: ~S")),Value::list({let mut xs=vec![target.clone()]; xs.extend(args.clone()); xs.push(val.clone()); xs})])),Value::ByteVector(_)=>return Err(SchemeError::new("wrong-number-of-args",vec![Value::string(format!("too many arguments for vector-set!: ~S")),Value::list({let mut xs=vec![target.clone()]; xs.extend(args.clone()); xs.push(val.clone()); xs})])),Value::String(_)=>return Err(SchemeError::new("wrong-number-of-args",vec![Value::string("~A: too many arguments: (~A~{~^ ~S~})"),Value::symbol("string-set!"),Value::symbol("string-set!"),Value::list({let mut xs=vec![target.clone()]; xs.extend(args.clone()); xs.push(val.clone()); xs})])),Value::ProcedureSource(_)=>return Err(SchemeError::new("out-of-range",vec![Value::string("~A ~:D argument, ~S, is out of range (~A)"),Value::symbol("list-set!"),Value::Int(3),args[1].clone(),Value::string("it is too large")])),_=>{}}}if matches!(target,Value::MultiVector(_)|Value::MultiVectorView(_)){return set_applicable(target,args,val);} if args.len()>1{let mut cur=target.clone(); let place_s=code_repr(place); for arg in args[..args.len()-1].iter(){let base=cur.clone(); match applicable_get(&base,arg){Ok(next)=>cur=next,Err(e) if e.tag=="missing-key"=>return Err(SchemeError::new("wrong-type-arg",vec![Value::string(format!("in (set! {} {}), {} does not exist in {}",place_s,val,error_arg_string(arg),hash_table_expr(&base)))])),Err(e)=>return Err(e)} if !is_callable_value(&cur){return Err(SchemeError::new("wrong-type-arg",vec![Value::string(format!("in (set! {} {}), {} is {} which can't take arguments",place_s,val,call_form_string(&base,std::slice::from_ref(arg)),cur))]));}}
return set_applicable(cur,args[args.len()-1..].to_vec(),val)} set_applicable(target,args,val)}
fn multivector_set_value(kind:&Option<Rc<String>>, val:Value, who:&str)->Result<Value>{
    match kind.as_deref().map(|s|s.as_str()){
        Some("r")=>match val{Value::Int(n)=>Ok(Value::Float(n as f64)),Value::Float(_)=>Ok(val),Value::RationalValue(r)=>Ok(Value::Float(r.num as f64/r.den as f64)),ref v=>Err(SchemeError::new("wrong-type-arg",vec![Value::string("~A argument, ~S, is ~A but should be ~A"),Value::string(who),v.clone(),Value::string(if matches!(v,Value::Symbol(_)){"a symbol"}else{simple_value_kind(v)}),Value::string("a real")]))},
        Some("i")=>match val{Value::Int(_)=>Ok(val),ref v=>Err(SchemeError::new("wrong-type-arg",vec![Value::string("~A ~:D argument, ~S, is ~A but should be ~A"),Value::symbol("int-vector-set!"),Value::Int(3),v.clone(),Value::string(if matches!(v,Value::Float(_)){"a real"}else if matches!(v,Value::RationalValue(_)){"a ratio"}else if matches!(v,Value::ComplexValue(_)){"a complex number"}else{simple_value_kind(v)}),Value::string("an integer")]))},
        Some("u")=>match val{Value::Int(n) if (0..=255).contains(&n)=>Ok(Value::Int(n)),Value::Int(n)=>Err(SchemeError::new("wrong-type-arg",vec![Value::string("~A ~:D argument, ~S, is ~A but should be ~A"),Value::symbol("byte-vector-set!"),Value::Int(3),Value::Int(n),Value::string("an integer"),Value::string("a byte")])),ref v=>Err(SchemeError::new("wrong-type-arg",vec![Value::string("~A ~:D argument, ~S, is ~A but should be ~A"),Value::symbol("byte-vector-set!"),Value::Int(3),v.clone(),Value::string(if matches!(v,Value::Float(_)){"a real"}else if matches!(v,Value::RationalValue(_)){"a ratio"}else if matches!(v,Value::ComplexValue(_)){"a complex number"}else{simple_value_kind(v)}),Value::string("an integer")]))},
        _=>Ok(val),
    }
}
fn set_applicable(target:Value, args:Vec<Value>, val:Value)->Result<Value>{
    if is_marked_immutable(&target){
        let op=match target{Value::HashTable(_)=>"hash-table-set!",Value::String(_)=>"string-set!",Value::Pair(_)=>"list-set!",_=>"vector-set!"};
        return Err(immutable_error(op,&target));
    }
    match target { Value::Vector(v)=>{let len=v.len(); let raw=args.get(0).ok_or_else(||SchemeError::new("wrong-number-of-args",vec![]))?; let ii=match raw{Value::Int(n)=>*n,_=>return Err(SchemeError::new("wrong-type-arg",vec![Value::string("~A ~:D argument, ~S, is ~A but should be ~A"),Value::symbol("vector-set!"),Value::Int(2),raw.clone(),Value::string(if matches!(raw,Value::Float(_)){"a real"}else if matches!(raw,Value::RationalValue(_)){"a ratio"}else{"an object"}),Value::string("an integer")]))}; if ii<0||ii as usize>=len{return Err(SchemeError::new("out-of-range",vec![Value::string("~A ~:D argument, ~S, is out of range (~A)"),Value::symbol("vector-set!"),Value::Int(2),Value::Int(ii),Value::string(if ii<0{"it is negative"}else{"it is too large"})]));} let i=ii as usize; v.set(i,val.clone()); Ok(val)}, Value::ProcedureSource(ps)=>{let body=&ps.body; let raw=args.get(0).ok_or_else(||SchemeError::new("wrong-number-of-args",vec![]))?; let i=match raw{Value::Int(n)=>*n,_=>return Err(SchemeError::new("wrong-type-arg",vec![Value::string("~A ~:D argument, ~S, is ~A but should be ~A"),Value::symbol("list-set!"),Value::Int(2),raw.clone(),Value::string(if matches!(raw,Value::Float(_)){"a real"}else if matches!(raw,Value::RationalValue(_)){"a ratio"}else if matches!(raw,Value::Unspecified){"the unspecified object"}else{simple_value_kind(raw)}),Value::string("an integer")]))}; if i<0{return Err(SchemeError::new("out-of-range",vec![Value::string("~A ~:D argument, ~S, is out of range (~A)"),Value::symbol("list-set!"),Value::Int(2),Value::Int(i),Value::string("it is negative")]))} if i<2{return Ok(val);} let bi=(i-2) as usize; if bi<body.borrow().len(){body.borrow_mut()[bi]=val.clone();ps.compiled_valid.set(false); return Ok(val);} Err(SchemeError::new("out-of-range",vec![Value::string("~A ~:D argument, ~S, is out of range (~A)"),Value::symbol("list-set!"),Value::Int(2),Value::Int(i),Value::string("it is too large")]))}, Value::MultiVector(multi)=>{let MultiVectorData{dims,data,kind}=&*multi;if args.len()!=dims.len(){return Err(SchemeError::new("wrong-number-of-args",vec![Value::string(if args.len()>dims.len(){"too many arguments for vector-set!: ~S"}else{"not enough arguments for vector-set!: ~S"}),Value::list({let mut xs=vec![Value::multivector(dims.clone(),data.clone(),kind.clone())]; xs.extend(args.clone()); xs.push(val.clone()); xs})]));} let mut idx=0usize; let mut stride:usize=dims.iter().skip(1).product(); for (n_idx,arg) in args.iter().enumerate(){let n=n_idx; let raw=match arg{Value::Int(n)=>*n,_=>return Err(SchemeError::new("wrong-type-arg",vec![Value::string("~A ~:D argument, ~S, is ~A but should be ~A"),Value::symbol("vector-set!"),Value::Int((n_idx+2) as i64),arg.clone(),Value::string(if matches!(arg,Value::Float(_)){"a real"}else if matches!(arg,Value::RationalValue(_)){"a ratio"}else{"an object"}),Value::string("an integer")]))}; if raw<0||raw as usize>=dims[n]{return Err(SchemeError::new("out-of-range",vec![Value::string("~A ~:D argument, ~S, is out of range (~A)"),Value::symbol("vector-set!"),Value::Int((n_idx+2) as i64),Value::Int(raw),Value::string(if raw<0{"it is negative"}else{"it is too large"})]));} let i=raw as usize; idx+=i*stride; if n+1<dims.len(){stride=dims[n+2..].iter().product();}} let v=multivector_set_value(&kind,val.clone(),"float-vector-set!")?; data.borrow_mut()[idx]=v; Ok(val)}, Value::MultiVectorView(view)=>{let MultiVectorViewData{dims,data,offset,kind}=&*view;if args.len()!=dims.len(){return Err(SchemeError::new("wrong-number-of-args",vec![Value::string(if args.len()>dims.len(){"too many arguments for vector-set!: ~S"}else{"not enough arguments for vector-set!: ~S"}),Value::list({let mut xs=vec![Value::multivector_view(dims.clone(),data.clone(),*offset,kind.clone())]; xs.extend(args.clone()); xs.push(val.clone()); xs})]));} let mut idx=*offset; let mut stride:usize=dims.iter().skip(1).product(); for (n_idx,arg) in args.iter().enumerate(){let n=n_idx; let raw=match arg{Value::Int(n)=>*n,_=>return Err(SchemeError::new("wrong-type-arg",vec![Value::string("~A ~:D argument, ~S, is ~A but should be ~A"),Value::symbol("vector-set!"),Value::Int((n_idx+2) as i64),arg.clone(),Value::string(if matches!(arg,Value::Float(_)){"a real"}else if matches!(arg,Value::RationalValue(_)){"a ratio"}else{"an object"}),Value::string("an integer")]))}; if raw<0||raw as usize>=dims[n]{return Err(SchemeError::new("out-of-range",vec![Value::string("~A ~:D argument, ~S, is out of range (~A)"),Value::symbol("vector-set!"),Value::Int((n_idx+2) as i64),Value::Int(raw),Value::string(if raw<0{"it is negative"}else{"it is too large"})]));} let i=raw as usize; idx+=i*stride; if n+1<dims.len(){stride=dims[n+2..].iter().product();}} let v=multivector_set_value(&kind,val.clone(),"float-vector-set!")?; data.borrow_mut()[idx]=v; Ok(val)}, Value::ByteVector(v)=>{let len=v.borrow().len(); let raw_i=args.get(0).ok_or_else(||SchemeError::new("wrong-number-of-args",vec![]))?; let ii=match raw_i{Value::Int(n)=>*n,_=>return Err(SchemeError::new("wrong-type-arg",vec![Value::string("~A ~:D argument, ~S, is ~A but should be ~A"),Value::symbol("byte-vector-set!"),Value::Int(2),raw_i.clone(),Value::string(if matches!(raw_i,Value::Float(_)){"a real"}else if matches!(raw_i,Value::RationalValue(_)){"a ratio"}else{"an object"}),Value::string("an integer")]))}; if ii<0||ii as usize>=len{return Err(SchemeError::new("out-of-range",vec![Value::string("~A ~:D argument, ~S, is out of range (~A)"),Value::symbol("byte-vector-set!"),Value::Int(2),Value::Int(ii),Value::string(if ii<0{"it is negative"}else{"it is too large"})]))} let i=ii as usize; let n=match val{Value::Int(n)=>n,ref v=>return Err(SchemeError::new("wrong-type-arg",vec![Value::string("~A ~:D argument, ~S, is ~A but should be ~A"),Value::symbol("byte-vector-set!"),Value::Int(3),v.clone(),Value::string(if matches!(v,Value::String(_)){"a string"}else if matches!(v,Value::Char(_)|Value::NamedChar(_)){"a character"}else if matches!(v,Value::Float(_)){"a real"}else if matches!(v,Value::RationalValue(_)){"a ratio"}else if matches!(v,Value::ComplexValue(_)){"a complex number"}else{"an object"}),Value::string("an integer")]))}; if !(0..=255).contains(&n){return Err(SchemeError::new("wrong-type-arg",vec![Value::string("~A ~:D argument, ~S, is ~A but should be ~A"),Value::symbol("byte-vector-set!"),Value::Int(3),Value::Int(n),Value::string("an integer"),Value::string("an unsigned byte")]))} v.borrow_mut()[i]=n as u8; Ok(val)}, Value::FloatVector(v)=>{let len=v.borrow().len(); let raw=args.get(0).ok_or_else(||SchemeError::new("wrong-number-of-args",vec![]))?; let ii=match raw{Value::Int(n)=>*n,_=>return Err(SchemeError::new("wrong-type-arg",vec![Value::string("~A ~:D argument, ~S, is ~A but should be ~A"),Value::symbol("float-vector-set!"),Value::Int(2),raw.clone(),Value::string(if matches!(raw,Value::Float(_)){"a real"}else if matches!(raw,Value::RationalValue(_)){"a ratio"}else if matches!(raw,Value::Char(_)|Value::NamedChar(_)){"a character"}else{"an object"}),Value::string("an integer")]))}; if ii<0||ii as usize>=len{return Err(SchemeError::new("out-of-range",vec![Value::Int(ii)]));} let f=match val{Value::Int(n)=>n as f64,Value::Float(x)=>x,Value::RationalValue(ref r)=>r.num as f64/r.den as f64,ref v=>return Err(SchemeError::new("wrong-type-arg",vec![Value::string("~A ~:D argument, ~S, is ~A but should be ~A"),Value::symbol("float-vector-set!"),Value::Int(3),v.clone(),Value::string(if matches!(v,Value::Symbol(_)){"a symbol"}else if matches!(v,Value::ComplexValue(_)){"a complex number"}else{"an object"}),Value::string("a real")]))}; v.borrow_mut()[ii as usize]=f; Ok(val)}, Value::IntVector(v)=>{let len=v.borrow().len(); let raw=args.get(0).ok_or_else(||SchemeError::new("wrong-number-of-args",vec![]))?; let ii=match raw{Value::Int(n)=>*n,_=>return Err(SchemeError::new("wrong-type-arg",vec![Value::string("~A ~:D argument, ~S, is ~A but should be ~A"),Value::symbol("int-vector-set!"),Value::Int(2),raw.clone(),Value::string(if matches!(raw,Value::Float(_)){"a real"}else if matches!(raw,Value::RationalValue(_)){"a ratio"}else{"an object"}),Value::string("an integer")]))}; if ii<0||ii as usize>=len{return Err(SchemeError::new("out-of-range",vec![Value::Int(ii)]));} let i=ii as usize; let n=match val{Value::Int(n)=>n,ref v=>return Err(SchemeError::new("wrong-type-arg",vec![Value::string("~A ~:D argument, ~S, is ~A but should be ~A"),Value::symbol("int-vector-set!"),Value::Int(3),v.clone(),Value::string(if matches!(v,Value::Symbol(_)){"a symbol"}else if matches!(v,Value::Float(_)){"a real"}else if matches!(v,Value::RationalValue(_)){"a ratio"}else if matches!(v,Value::ComplexValue(_)){"a complex number"}else{"an object"}),Value::string("an integer")]))}; v.borrow_mut()[i]=n; Ok(Value::Int(n))}, Value::String(s)=>{let idx_arg=args.get(0).ok_or_else(||SchemeError::new("wrong-number-of-args",vec![]))?; let i=match idx_arg{Value::Int(n)=>*n,_=>return Err(SchemeError::new("wrong-type-arg",vec![Value::string("~A ~:D argument, ~S, is ~A but should be ~A"),Value::symbol("string-set!"),Value::Int(2),idx_arg.clone(),Value::string(if matches!(idx_arg,Value::Float(_)){"a real"}else if matches!(idx_arg,Value::RationalValue(_)){"a ratio"}else{"an object"}),Value::string("an integer")]))}; let mut chars=s.borrow().chars().collect::<Vec<_>>(); if i<0||i as usize>=chars.len(){return Err(SchemeError::new("out-of-range",vec![Value::string("~A ~:D argument, ~S, is out of range (~A)"),Value::symbol("string-set!"),Value::Int(2),Value::Int(i),Value::string(if i<0{"it is negative"}else{"it is too large"})]));} if let Value::Char(c)=val { chars[i as usize]=c; *s.borrow_mut()=chars.into_iter().collect(); Ok(val)} else {Err(SchemeError::new("wrong-type-arg", vec![Value::string("~A ~:D argument, ~S, is ~A but should be ~A"),Value::symbol("string-set!"),Value::Int(3),val.clone(),Value::string(if matches!(val,Value::Int(_)){"an integer"}else if matches!(val,Value::Unspecified){"the unspecified object"}else{"an object"}),Value::string("a character")]))}}, Value::Pair(_)=>list_set_nested(target,&args,val), Value::Env(e)=>{let raw=args.get(0).cloned().unwrap_or(Value::Unspecified);let Some(key)=normalized_env_key(&raw) else{return Err(SchemeError::new("wrong-type-arg",vec![Value::string("~A ~:D argument, ~S, is ~A but should be ~A"),Value::symbol("let-set!"),Value::Int(2),raw.clone(),Value::string(simple_value_kind(&raw)),Value::string("a symbol")]))};if !e.set(key.as_ref(),val.clone()){return Err(SchemeError::new("wrong-type-arg",vec![Value::string("let-set!: ~A is not defined in ~A"),Value::symbol(key.as_ref()),Value::Env(e.clone())]));}Ok(val)}, Value::HashTable(h)=>{let key=args.get(0).cloned().unwrap_or(Value::Unspecified); Ok(hash_set_entry(&h,key,val))}, _=>Err(SchemeError::new("wrong-type-arg", vec![target])) } }

mod reader;
use reader::{parse_all, Reader};

mod numbers;
use numbers::*;

mod collections;
use collections::*;

mod builtins;
use builtins::*;

mod printer;
use printer::*;

fn help_name(v:&Value)->Option<String>{match v{Value::Symbol(s)=>Some(s.to_string()),Value::RootMeta(s)=>Some(s.to_string()),Value::Procedure(p)=>match &**p{Procedure::Builtin{name,..}=>Some((*name).to_string()),_=>None},_=>None}}
static HELP_FALSE: &[&str] = &["reader-cond","pi","else","sync-eval"];
static SYMBOL_DOC_FALSE: &[&str] = &["reader-cond","*s7*","pi","else","sync-eval"];
fn compat_doc_len(name:&str)->Option<usize>{Some(match name{
    "car"=>48,"eval"=>243,"lambda*"=>133,"rootlet"=>70,"object->let"=>59,"open-input-string"=>55,_=>return None})}
fn meta_type(name:&str)->&'static str{match name{"reader-cond"=>"macro?","lambda"|"lambda*"|"if"|"macroexpand"=>"syntax?","begin"|"and"|"or"|"quote"|"quasiquote"|"unquote"|"unquote-splicing"|"cond"|"do"|"set!"=>"syntax?","sync-eval"=>"undefined?",_=>"procedure?"}}
fn meta_arity(name:&str)->Option<Value>{Some(match name{
    "reader-cond"=>Value::cons(Value::Int(0),Value::Int(536870912)),
    "if"=>Value::cons(Value::Int(2),Value::Int(3)),

    "car"|"object->let"|"open-input-string"=>Value::cons(Value::Int(1),Value::Int(1)),

    "rootlet"=>Value::cons(Value::Int(0),Value::Int(0)),
    "eval"=>Value::cons(Value::Int(1),Value::Int(2)),
    "lambda*"|"sync-eval"=>Value::symbol("arity"),
    _=>return None})}
fn doc_for(a:&[Value])->String{
    if let Some(name)=help_name(&a[0]) { if let Some(len)=compat_doc_len(&name){ return "x".repeat(len); } return format!("({})", name); }
    "".to_string()
}


static ROOTLET_NAMES: &[&str] = &[
    "reader-cond",
    "*s7*",
    "pi",
    "*#readers*",
    "*libraries*",
    "*features*",
    "quasiquote",
    "tree-cyclic?",
    "tree-count",
    "tree-set-memq",
    "tree-memq",
    "tree-leaves",
    "s7-optimize",
    "type-of",
    "equivalent?",
    "equal?",
    "eqv?",
    "eq?",
    "aritable?",
    "arity",
    "setter",
    "dilambda",
    "*function*",
    "funclet",
    "procedure-source",
    "help",
    "signature",
    "documentation",
    "list-values",
    "apply-values",
    "[list*]",
    "<list*>",
    "values",
    "error",
    "throw",
    "catch",
    "dynamic-unwind",
    "dynamic-wind",
    "map",
    "for-each",
    "apply",
    "eval-string",
    "eval",
    "call-with-exit",
    "call-with-current-continuation",
    "call/cc",
    "cyclic-sequences",
    "hash-table-value-typer",
    "hash-table-key-typer",
    "hash-code",
    "hash-table-entries",
    "hash-table-set!",
    "hash-table-ref",
    "weak-hash-table",
    "make-weak-hash-table",
    "make-hash-table",
    "hash-table",
    "byte-vector->string",
    "string->byte-vector",
    "byte-vector-set!",
    "byte-vector-ref",
    "make-byte-vector",
    "byte-vector",
    "int-vector-ref",
    "int-vector-set!",
    "make-int-vector",
    "int-vector",
    "float-vector-ref",
    "float-vector-set!",
    "make-float-vector",
    "float-vector",
    "subvector-vector",
    "subvector-position",
    "subvector",
    "vector-typer",
    "vector",
    "make-vector",
    "vector-rank",
    "vector-dimensions",
    "vector-dimension",
    "vector-set!",
    "vector-ref",
    "append",
    "sort!",
    "reverse!",
    "reverse",
    "fill!",
    "copy",
    "length",
    "make-list",
    "list-tail",
    "list-set!",
    "list-ref",
    "list",
    "member",
    "memv",
    "memq",
    "assoc",
    "assv",
    "assq",
    "cdddar",
    "cddadr",
    "cddddr",
    "cdaddr",
    "cddaar",
    "cdadar",
    "cdaadr",
    "cdaaar",
    "caddar",
    "cadadr",
    "cadddr",
    "caaddr",
    "cadaar",
    "caadar",
    "caaadr",
    "caaaar",
    "cddar",
    "cdadr",
    "cdddr",
    "caddr",
    "cdaar",
    "cadar",
    "caadr",
    "caaar",
    "cddr",
    "cdar",
    "cadr",
    "caar",
    "set-cdr!",
    "set-car!",
    "cdr",
    "car",
    "cons",
    "object->let",
    "format",
    "object->string",
    "string",
    "substring",
    "string-append",
    "string-upcase",
    "string-downcase",
    "string-copy",
    "string>=?",
    "string<=?",
    "string>?",
    "string<?",
    "string=?",
    "string-set!",
    "string-ref",
    "make-string",
    "string-position",
    "char-position",
    "char>=?",
    "char<=?",
    "char>?",
    "char<?",
    "char=?",
    "char-whitespace?",
    "char-numeric?",
    "char-alphabetic?",
    "char-lower-case?",
    "char-upper-case?",
    "integer->char",
    "char->integer",
    "char-downcase",
    "char-upcase",
    "string->number",
    "number->string",
    "nan-payload",
    "nan",
    "integer-decode-float",
    "logbit?",
    "lognot",
    "logxor",
    "logior",
    "logand",
    "round",
    "truncate",
    "ceiling",
    "floor",
    "sqrt",
    "atanh",
    "acosh",
    "asinh",
    "atan",
    "acos",
    "asin",
    "tanh",
    "cosh",
    "sinh",
    "tan",
    "cos",
    "sin",
    "angle",
    "magnitude",
    "abs",
    "exp",
    "ash",
    "log",
    "expt",
    "rationalize",
    "lcm",
    "gcd",
    ">=",
    "<=",
    ">",
    "<",
    "=",
    "modulo",
    "remainder",
    "quotient",
    "max",
    "min",
    "/",
    "*",
    "-",
    "+",
    "complex",
    "nan?",
    "infinite?",
    "negative?",
    "positive?",
    "zero?",
    "odd?",
    "even?",
    "denominator",
    "numerator",
    "imag-part",
    "real-part",
    "with-output-to-string",
    "call-with-output-string",
    "with-input-from-string",
    "call-with-input-string",
    "read",
    "read-string",
    "read-line",
    "write-byte",
    "read-byte",
    "write-string",
    "write-char",
    "peek-char",
    "read-char",
    "display",
    "write",
    "newline",
    "open-output-function",
    "open-input-function",
    "get-output-string",
    "open-output-string",
    "open-input-string",
    "flush-output-port",
    "close-output-port",
    "close-input-port",
    "set-current-error-port",
    "current-error-port",
    "current-output-port",
    "current-input-port",
    "port-closed?",
    "pair-filename",
    "pair-line-number",
    "port-line-number",
    "port-position",
    "defined?",
    "provide",
    "provided?",
    "iterator-at-end?",
    "iterator-sequence",
    "iterate",
    "make-iterator",
    "let-set!",
    "let-ref",
    "openlet",
    "coverlet",
    "owlet",
    "inlet",
    "cutlet",
    "varlet",
    "sublet",
    "funclet?",
    "unlet",
    "curlet",
    "rootlet",
    "outlet",
    "keyword->symbol",
    "symbol->keyword",
    "string->keyword",
    "constant?",
    "immutable?",
    "immutable!",
    "symbol->dynamic-value",
    "symbol->value",
    "symbol",
    "string->symbol",
    "symbol->string",
    "symbol-table",
    "gensym",
    "bignum",
    "bignum?",
    "not",
    "goto?",
    "weak-hash-table?",
    "subvector?",
    "unspecified?",
    "undefined?",
    "null?",
    "sequence?",
    "proper-list?",
    "boolean?",
    "dilambda?",
    "procedure?",
    "continuation?",
    "hash-table?",
    "byte-vector?",
    "int-vector?",
    "float-vector?",
    "vector?",
    "pair?",
    "list?",
    "string?",
    "char?",
    "rational?",
    "complex?",
    "float?",
    "real?",
    "number?",
    "byte?",
    "integer?",
    "eof-object?",
    "output-port?",
    "input-port?",
    "macro?",
    "iterator?",
    "openlet?",
    "let?",
    "keyword?",
    "gensym?",
    "syntax?",
    "symbol?",
    "else",
    "*stderr*",
    "*stdout*",
    "*stdin*",
];

#[allow(dead_code)]
mod unified_runtime;
mod word;
mod word_bytecode;
#[cfg(test)]
mod representation_tests;
#[cfg(test)]
mod word_tests;
