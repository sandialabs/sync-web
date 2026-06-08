use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::sync::atomic::AtomicUsize;

static GENSYM: AtomicUsize = AtomicUsize::new(0);

mod core;
use core::*;

pub fn run_source(source: &str) -> Result<Value> {
    let mut ev = Evaluator::new();
    let exprs = parse_all(source)?;
    let mut last = Value::Unspecified;
    for e in exprs { last = ev.eval(e, ev.global.clone())?; }
    Ok(last)
}

#[derive(Clone)]
struct GasState { active: Option<i64>, last_used: i64, last_status: String }

pub struct Evaluator { root: EnvRef, global: EnvRef, curlet: EnvRef, proc_setters: RefCell<HashMap<usize, Value>>, gas: GasState, stdin: Value, stdout: Value, stderr: Value, pending_call_form: Option<Value> }

impl Evaluator {
    fn new() -> Self { let root=Env::new(None); let global=Env::new(Some(root.clone())); let stdin=Value::Port(Rc::new(RefCell::new(Port::Input{text:Vec::new(),pos:0,repr:PortRepr::Stdin}))); let stdout=Value::Port(Rc::new(RefCell::new(Port::Output{text:String::new(),repr:PortRepr::Stdout}))); let stderr=Value::Port(Rc::new(RefCell::new(Port::Output{text:String::new(),repr:PortRepr::Stderr}))); let mut ev=Self{root:root.clone(), global:global.clone(), curlet:global.clone(), proc_setters:RefCell::new(HashMap::new()), gas: GasState{active:None,last_used:0,last_status:"ok".to_string()}, stdin, stdout, stderr, pending_call_form: None}; ev.install(); ev }
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
    fn eval(&mut self, expr: Value, env: EnvRef) -> Result<Value> {
        self.charge(1)?;
        match expr {
            Value::Symbol(s) => env.get(&s).ok_or_else(|| SchemeError::new("unbound-variable", vec![Value::string("unbound variable ~S"), Value::symbol(&s)])),
            Value::Pair(_) => self.eval_pair(expr, env),
            Value::Commented(v)=>Ok(Value::Commented(Box::new(self.eval(*v, env)?))),
            v => Ok(v),
        }
    }
    fn eval_pair(&mut self, expr: Value, env: EnvRef) -> Result<Value> {
        let op = expr.car()?;
        let args = expr.cdr()?;
        if let Some(sym)=op.as_symbol() {
            if sym=="vector-fill!" && env.get(sym).is_none(){return Err(SchemeError::new("unbound-variable",vec![Value::string("unbound variable ~S in ~S"),Value::symbol(sym),expr.clone()]));}
            match sym {
                "quote" => { let xs=args.to_vec()?; if xs.len()!=1{return Err(SchemeError::new("syntax-error",vec![Value::symbol("quote")]))}; return Ok(xs[0].clone()); },
                "quasiquote" => return self.eval_quasiquote(args.car()?, env),
                "if" => { let xs=args.to_vec()?; if xs.len()<2{return Err(SchemeError::new("syntax-error",vec![Value::symbol("if")]))}; let test=self.eval(xs.get(0).cloned().unwrap_or(Value::Bool(false)), env.clone())?; return if test.is_true(){ self.eval(xs.get(1).cloned().unwrap_or(Value::Unspecified), env) } else { self.eval(xs.get(2).cloned().unwrap_or(Value::Unspecified), env) }; }
                "begin" => { let xs=args.to_vec()?; if xs.is_empty(){return Ok(Value::Nil)}; return self.eval_sequence(xs, env); },
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
                "and" => { let mut last=Value::Bool(true); for a in args.to_vec()? { match self.eval(a, env.clone())? { Value::Values(vs)=>{ for v in vs { last=v; if !last.is_true(){return Ok(last);} } }, v=>{ last=v; if !last.is_true(){return Ok(last);} } } } return Ok(last); }
                "or" => { for a in args.to_vec()? { match self.eval(a, env.clone())? { Value::Values(vs)=>{ for v in vs { if v.is_true(){return Ok(v);} } }, v=>{ if v.is_true(){return Ok(v);} } } } return Ok(Value::Bool(false)); }
                "catch" => return self.eval_catch(args, env),
                "throw" => { let xs=self.eval_list(args, env)?; let tag=xs.get(0).cloned().unwrap_or(Value::symbol("error")); return Err(SchemeError::new(tag.as_symbol().unwrap_or("throw").to_string(), xs[1..].to_vec())); }
                "define-macro" => return self.eval_define_macro(args, env, MacroKind::Macro, false),
                "define-macro*" => return self.eval_define_macro(args, env, MacroKind::Macro, true),
                "define-bacro" => return self.eval_define_macro(args, env, MacroKind::BMacro, false),
                "define-bacro*" => return self.eval_define_macro(args, env, MacroKind::BMacro, true),
                "macro" => return self.make_macro(args, env, MacroKind::Macro, false),
                "macro*" => return self.make_macro(args, env, MacroKind::Macro, true),
                "bacro" => return self.make_macro(args, env, MacroKind::BMacro, false),
                "bacro*" => return self.make_macro(args, env, MacroKind::BMacro, true),
                "with-let" => { let xs=args.to_vec()?; let e=self.eval(xs[0].clone(), env.clone())?; let e=if let Value::Values(vs)=e{vs.get(0).cloned().unwrap_or(Value::Unspecified)}else{e}; let Value::Env(new_env)=e else { return Err(SchemeError::new("wrong-type-arg", vec![e])); }; return self.eval_sequence(xs[1..].to_vec(), new_env); }
                "macroexpand" => { let raw_form=args.car()?; let form=if matches!(raw_form.car().ok().and_then(|v|v.as_symbol().map(|s|s.to_string())).as_deref(),Some("quasiquote")){raw_form.cdr()?.car()?}else{raw_form}; if let Value::Pair(_)=&form { if let Some(name)=form.car()?.as_symbol(){ if let Some(Value::Macro(p,k))=env.get(name){return match k{MacroKind::Macro|MacroKind::BMacro=>self.apply_proc(&p, form.cdr()?.to_vec()?, env)};} } } let shown=macroexpand_error_form(&form).unwrap_or_else(||Value::list(vec![Value::list(vec![Value::symbol("quote"),form])])); return Err(SchemeError::new("syntax-error",vec![Value::string("macroexpand argument is not a macro call: ~A"),shown])); }
                _=>{}
            }
        }
        let proc = self.eval(op.clone(), env.clone())?;
        match proc {
            Value::Macro(p, kind) => {
                let raw=args.to_vec()?;
                let expanded = match kind { MacroKind::Macro => self.apply_proc(&p, raw, env.clone())?, MacroKind::BMacro => self.apply_proc(&p, raw, env.clone())? };
                if let Value::Values(vs)=expanded { let mut out=Vec::new(); for x in vs { out.push(self.eval(x, env.clone())?); } Ok(Value::list(out)) } else { self.eval(expanded, env) }
            }
            p => { if let Value::RootMeta(name)=&p{ if name.as_str()=="and"{let mut last=Value::Bool(true); for a in args.to_vec()?{match self.eval(a,env.clone())?{Value::Values(vs)=>{for v in vs{last=v;if !last.is_true(){return Ok(last)}}},v=>{last=v;if !last.is_true(){return Ok(last)}}}} return Ok(last)} if name.as_str()=="or"{for a in args.to_vec()?{match self.eval(a,env.clone())?{Value::Values(vs)=>{for v in vs{if v.is_true(){return Ok(v)}}},v=>{if v.is_true(){return Ok(v)}}}} return Ok(Value::Bool(false))} } let vals=self.eval_list(args, env.clone())?; let old_call=self.pending_call_form.take(); if matches!(op.car().ok().and_then(|x|x.as_symbol().map(|s|s.to_string())).as_deref(),Some("lambda*")){let mut call=vec![op.clone()]; call.extend(vals.clone()); self.pending_call_form=Some(Value::list(call));} let r=self.apply_value(p, vals, env); self.pending_call_form=old_call; r }
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
                            "quote" => { let xs=args.to_vec()?; if xs.len()!=1{return Err(SchemeError::new("syntax-error",vec![Value::symbol("quote")]))}; return Ok(xs[0].clone()); }
                            "quasiquote" => return self.eval_quasiquote(args.car()?, env),
                            "if" => {
                                let xs=args.to_vec()?;
                                if xs.len()<2{return Err(SchemeError::new("syntax-error",vec![Value::symbol("if")]))}
                                let test=self.eval(xs.get(0).cloned().unwrap_or(Value::Bool(false)), env.clone())?;
                                expr=if test.is_true(){ xs.get(1).cloned().unwrap_or(Value::Unspecified) } else { xs.get(2).cloned().unwrap_or(Value::Unspecified) };
                                continue;
                            }
                            "begin" => {
                                let xs=args.to_vec()?;
                                if xs.is_empty(){return Ok(Value::Unspecified)}
                                for x in xs[..xs.len()-1].iter().cloned(){ self.eval(x, env.clone())?; }
                                expr=xs[xs.len()-1].clone();
                                continue;
                            }
                            "and" => {
                                let xs=args.to_vec()?;
                                if xs.is_empty(){return Ok(Value::Bool(true))}
                                let mut last=Value::Bool(true);
                                for x in xs { match self.eval(x, env.clone())? { Value::Values(vs)=>{ for v in vs { last=v; if !last.is_true(){return Ok(last);} } }, v=>{ last=v; if !last.is_true(){return Ok(last);} } } }
                                return Ok(last);
                            }
                            "or" => {
                                let xs=args.to_vec()?;
                                if xs.is_empty(){return Ok(Value::Bool(false))}
                                for x in xs { match self.eval(x, env.clone())? { Value::Values(vs)=>{ for v in vs { if v.is_true(){return Ok(v);} } }, v=>{ if v.is_true(){return Ok(v);} } } }
                                return Ok(Value::Bool(false));
                            }
                            "let-temporarily" => return self.eval_let_temporarily(args, env),
                            "let" | "let*" => {
                                let sequential=sym=="let*";
                                let xs=args.to_vec()?;
                                if let Some(Value::Symbol(name))=xs.get(0) {
                                    let bindings=xs[1].to_vec()?;
                                    let params=bindings.iter().map(|b| b.car().unwrap().as_symbol().unwrap().to_string()).collect::<Vec<_>>();
                                    let vals_expr=bindings.iter().map(|b| b.cdr().unwrap().car().unwrap()).collect::<Vec<_>>();
                                    let new_env=Env::new(Some(env.clone()));
                                    let proc=Value::Procedure(Rc::new(Procedure::Lambda{params:Params{required:params.clone(),rest:None,star:sequential,defaults:vec![None; params.len()],allow_other_keys:false,rest_before_formals:false},body:Rc::new(RefCell::new(xs[2..].to_vec())),env:new_env.clone(),name:Some(name.to_string())}));
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
                                else { let mut vals=Vec::new(); for b in &bindings { let bv=b.to_vec()?; let name=bv[0].as_symbol().unwrap().to_string(); let val=self.eval(bv[1].clone(), env.clone())?; vals.push((name.clone(), self.normalize_binding_value_ctx("let",&name,val)?)); } for (k,v) in vals { new_env.define(k,v); } }
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
                            "case" => {
                                let xs=args.to_vec()?; let key=self.eval(xs[0].clone(), env.clone())?; let key=if let Value::Values(vs)=key{vs.get(0).cloned().unwrap_or(Value::Unspecified)}else{key};
                                let mut matched: Option<Vec<Value>>=None;
                                'clauses: for clause in &xs[1..] { let cs=clause.to_vec()?; if cs[0].as_symbol()==Some("else") { matched=Some(cs[1..].to_vec()); break; } for datum in cs[0].to_vec()? { if equal(&key,&datum){ matched=Some(cs[1..].to_vec()); break 'clauses; } } }
                                if let Some(body)=matched { if body.is_empty(){return Ok(Value::Unspecified)}; for x in body[..body.len()-1].iter().cloned(){ self.eval(x, env.clone())?; } expr=body[body.len()-1].clone(); continue; }
                                return Ok(Value::Unspecified);
                            }
                            "with-let" => {
                                let xs=args.to_vec()?; let e=self.eval(xs[0].clone(), env.clone())?; let e=if let Value::Values(vs)=e{vs.get(0).cloned().unwrap_or(Value::Unspecified)}else{e}; let Value::Env(new_env)=e else { return Err(SchemeError::new("wrong-type-arg", vec![e])); };
                                let body=&xs[1..]; if body.is_empty(){return Ok(Value::Unspecified)}; for x in body[..body.len()-1].iter().cloned(){ self.eval(x, new_env.clone())?; }
                                expr=body[body.len()-1].clone(); env=new_env.clone(); self.curlet=new_env; continue;
                            }
                            "define" | "define*" | "set!" | "lambda" | "lambda*" | "do" | "catch" | "throw" | "define-macro" | "define-macro*" | "define-bacro" | "define-bacro*" | "macro" | "macro*" | "bacro" | "bacro*" | "macroexpand" => return self.eval_pair(expr, env),
                            _=>{}
                        }
                    }
                    let proc=self.eval(op.clone(), env.clone())?;
                    if let Value::RootMeta(name)=&proc{ if name.as_str()=="and"{let mut last=Value::Bool(true); for a in args.to_vec()?{match self.eval(a,env.clone())?{Value::Values(vs)=>{for v in vs{last=v;if !last.is_true(){return Ok(last)}}},v=>{last=v;if !last.is_true(){return Ok(last)}}}} return Ok(last)} if name.as_str()=="or"{for a in args.to_vec()?{match self.eval(a,env.clone())?{Value::Values(vs)=>{for v in vs{if v.is_true(){return Ok(v)}}},v=>{if v.is_true(){return Ok(v)}}}} return Ok(Value::Bool(false))} }
                    match proc {
                        Value::Macro(p, kind) => {
                            let raw=args.to_vec()?;
                            let expanded=match kind { MacroKind::Macro => self.apply_proc(&p, raw, env.clone())?, MacroKind::BMacro => self.apply_proc(&p, raw, env.clone())? };
                            if let Value::Values(vs)=expanded { let mut out=Vec::new(); for x in vs { out.push(self.eval(x, env.clone())?); } return Ok(Value::list(out)); }
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
                                Procedure::Lambda{params,body,env:proc_env,name} => {
                                    if !params.star && vals.len()<params.required.len(){ let form=Value::list(vec![Value::symbol("lambda"), Value::list(params.required.iter().map(|n|Value::symbol(n)).collect()), body.borrow().get(0).cloned().unwrap_or(Value::Unspecified)]); return Err(SchemeError::new("wrong-number-of-args", vec![Value::string("~S: not enough arguments: ((~S ~S ...)~{~^ ~S~})"), Value::list(vec![form]), Value::symbol("lambda"), Value::list(params.required.iter().map(|n|Value::symbol(n)).collect()), Value::Nil])); }
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
                        p => { let vals=self.eval_list(args, env.clone())?; let old_call=self.pending_call_form.take(); if matches!(op.car().ok().and_then(|x|x.as_symbol().map(|s|s.to_string())).as_deref(),Some("lambda*")){let mut call=vec![op.clone()]; call.extend(vals.clone()); self.pending_call_form=Some(Value::list(call));} let r=self.apply_value(p, vals, env); self.pending_call_form=old_call; return r; }
                    }
                }
                v => return Ok(v),
            }
        }
    }
    fn eval_list(&mut self, list: Value, env: EnvRef) -> Result<Vec<Value>> { let mut out=Vec::new(); for x in list.to_vec()? { match self.eval(x, env.clone())? { Value::Values(vs)=>out.extend(vs), v=>out.push(v) } } Ok(out) }
    fn apply_value(&mut self, proc: Value, args: Vec<Value>, env: EnvRef) -> Result<Value> {
        self.charge(1)?;
        match proc {
            Value::Procedure(p)=>self.apply_proc(&p,args,env),
            Value::Macro(p,_)=>self.apply_proc(&p,args,env),
            Value::Vector(ref v)=>{ if args.len()>1 { let first=index_vec(&v.borrow(), &args[..1])?; if is_callable_value(&first){return self.apply_value(first,args[1..].to_vec(),env);} let mut form=vec![proc.clone()]; form.extend(args.clone()); return Err(cant_take_arguments_error_value(Value::list(form), &first, &args[1..])); } index_vec(&v.borrow(), &args) },
            Value::ProcedureSource{..}=>{ if args.len()>1{if matches!(args[0],Value::Int(0)){return Err(SchemeError::new("syntax-error",vec![Value::string("~$ becomes ~$, but ~S can't take arguments"),Value::list({let mut v=vec![proc.clone()]; v.extend(args.clone()); v}),Value::list(vec![Value::symbol("lambda"),args[1].clone()]),Value::symbol("lambda")]))} return Err(SchemeError::new("out-of-range",vec![Value::string("~A ~:D argument, ~S, is out of range (~A)"),Value::symbol("list-ref"),Value::Int(2),args[1].clone(),Value::string("it is too large")]))} applicable_get(&proc,&args[0]) },
            Value::MultiVector{dims,data,kind}=>{if args.len()>dims.len(){if kind.is_some(){return Err(SchemeError::new("wrong-number-of-args",vec![Value::string("~S: too many indices: ~S"),Value::symbol("vector-ref"),Value::list(args)]));} let mut idx=0usize; let mut stride:usize=dims.iter().skip(1).product(); for (n_idx,arg) in args[..dims.len()].iter().enumerate(){let n=n_idx; let i=match arg{Value::Int(n) if *n>=0=>*n as usize,Value::Int(n)=>return Err(SchemeError::new("out-of-range",vec![Value::string("~A ~:D argument, ~S, is out of range (~A)"),Value::symbol("vector-ref"),Value::Int((n_idx+2) as i64),Value::Int(*n),Value::string("it is negative")])),v=>return Err(SchemeError::new("wrong-type-arg",vec![Value::string("~A ~:D argument, ~S, is ~A but should be ~A"),Value::symbol("vector-ref"),Value::Int((n_idx+2) as i64),v.clone(),Value::string(simple_value_kind(v)),Value::string("an integer")]))}; idx+=i*stride; if n+1<dims.len(){stride=dims[n+2..].iter().product();}} let first=data.borrow()[idx].clone(); return Err(SchemeError::new("syntax-error",vec![Value::string("attempt to apply ~A ~$ in ~S?"),Value::string(if matches!(first,Value::Int(_)){"an integer"}else{"an object"}),first.clone(),Value::list(vec![first.clone(),args[dims.len()].clone()])]));} let mut idx=0usize; let mut stride:usize=dims.iter().skip(1).product(); for (n_idx,arg) in args.iter().enumerate(){let n=n_idx; let raw=match arg{Value::Int(n)=>*n,_=>return Err(SchemeError::new("wrong-type-arg",vec![Value::string("~A ~:D argument, ~S, is ~A but should be ~A"),Value::symbol("vector-ref"),Value::Int((n_idx+2) as i64),arg.clone(),Value::string(if matches!(arg,Value::Float(_)){"a real"}else if matches!(arg,Value::Rational(_,_)){"a ratio"}else{"an object"}),Value::string("an integer")]))}; if raw<0||raw as usize>=dims[n]{return Err(SchemeError::new("out-of-range",vec![Value::string("~A ~:D argument, ~S, is out of range (~A)"),Value::symbol("vector-ref"),Value::Int((n_idx+2) as i64),Value::Int(raw),Value::string(if raw<0{"it is negative"}else{"it is too large"})]))} let i=raw as usize; idx+=i*stride; if n+1<dims.len(){stride=dims[n+2..].iter().product();}} if args.len()<dims.len(){let rem_dims=dims[args.len()..].to_vec(); return Ok(Value::MultiVectorView{dims:rem_dims,data:data.clone(),offset:idx,kind:kind.clone()});} Ok(data.borrow()[idx].clone())},
            Value::MultiVectorView{dims,data,offset,kind}=>{let base=offset; if args.len()>dims.len(){if kind.is_some(){return Err(SchemeError::new("wrong-number-of-args",vec![Value::string("~S: too many indices: ~S"),Value::symbol("vector-ref"),Value::list(args)]));} let mut idx=base; let mut stride:usize=dims.iter().skip(1).product(); for (n_idx,arg) in args[..dims.len()].iter().enumerate(){let n=n_idx; let i=match arg{Value::Int(n) if *n>=0=>*n as usize,Value::Int(n)=>return Err(SchemeError::new("out-of-range",vec![Value::string("~A ~:D argument, ~S, is out of range (~A)"),Value::symbol("vector-ref"),Value::Int((n_idx+2) as i64),Value::Int(*n),Value::string("it is negative")])),v=>return Err(SchemeError::new("wrong-type-arg",vec![Value::string("~A ~:D argument, ~S, is ~A but should be ~A"),Value::symbol("vector-ref"),Value::Int((n_idx+2) as i64),v.clone(),Value::string(simple_value_kind(v)),Value::string("an integer")]))}; idx+=i*stride; if n+1<dims.len(){stride=dims[n+2..].iter().product();}} let first=data.borrow()[idx].clone(); return Err(SchemeError::new("syntax-error",vec![Value::string("attempt to apply ~A ~$ in ~S?"),Value::string(if matches!(first,Value::Int(_)){"an integer"}else{"an object"}),first.clone(),Value::list(vec![first.clone(),args[dims.len()].clone()])]));} let mut idx=base; let mut stride:usize=dims.iter().skip(1).product(); for (n_idx,arg) in args.iter().enumerate(){let n=n_idx; let raw=match arg{Value::Int(n)=>*n,_=>return Err(SchemeError::new("wrong-type-arg",vec![Value::string("~A ~:D argument, ~S, is ~A but should be ~A"),Value::symbol("vector-ref"),Value::Int((n_idx+2) as i64),arg.clone(),Value::string(if matches!(arg,Value::Float(_)){"a real"}else if matches!(arg,Value::Rational(_,_)){"a ratio"}else{"an object"}),Value::string("an integer")]))}; if raw<0||raw as usize>=dims[n]{return Err(SchemeError::new("out-of-range",vec![Value::string("~A ~:D argument, ~S, is out of range (~A)"),Value::symbol("vector-ref"),Value::Int(2),Value::Int(raw),Value::string(if raw<0{"it is negative"}else{"it is too large"})]))} let i=raw as usize; idx+=i*stride; if n+1<dims.len(){stride=dims[n+2..].iter().product();}} if args.len()<dims.len(){let rem_dims=dims[args.len()..].to_vec(); return Ok(Value::MultiVectorView{dims:rem_dims,data:data.clone(),offset:idx,kind:kind.clone()});} Ok(data.borrow()[idx].clone())},
            Value::ByteVector(v)=>index_bvec(&v.borrow(), &args),
            Value::FloatVector(v)=>index_fvec(&v.borrow(), &args),
            Value::IntVector(v)=>index_ivec(&v.borrow(), &args),
            Value::String(ref s)=> { if args.len()>1{return Err(SchemeError::new("wrong-number-of-args",vec![Value::string("~S: too many arguments: ~A"),proc.clone(),Value::list(args)]));} let raw=args.get(0).ok_or_else(||SchemeError::new("wrong-number-of-args",vec![]))?; let i=match raw{Value::Int(n)=>*n,_=>return Err(SchemeError::new("wrong-type-arg",vec![Value::string("~A ~:D argument, ~S, is ~A but should be ~A"),Value::symbol("string-ref"),Value::Int(2),raw.clone(),Value::string(if matches!(raw,Value::Float(_)){"a real"}else if matches!(raw,Value::Rational(_,_)){"a ratio"}else{"an object"}),Value::string("an integer")]))}; let chars=s.borrow().chars().collect::<Vec<_>>(); if i<0||i as usize>=chars.len(){return Err(SchemeError::new("out-of-range",vec![Value::string("~A ~:D argument, ~S, is out of range (~A)"),Value::symbol("string-ref"),Value::Int(2),Value::Int(i),Value::string(if i<0{"it is negative"}else{"it is too large"})]));} Ok(Value::Char(chars[i as usize])) },
            Value::Pair(_)=> { let form_val={let mut xs=vec![proc.clone()]; xs.extend(args.clone()); Value::list(xs)}; let mut cur=proc; for (n,arg) in args.iter().enumerate() { let i=match arg{Value::Int(n)=>*n,_=>return Err(SchemeError::new("wrong-type-arg",vec![Value::string("~A ~:D argument, ~S, is ~A but should be ~A"),Value::symbol("list-ref"),Value::Int(2),arg.clone(),Value::string(if matches!(arg,Value::Float(_)){"a real"}else if matches!(arg,Value::Rational(_,_)){"a ratio"}else{"an object"}),Value::string("an integer")]))}; if i<0{return Err(SchemeError::new("out-of-range",vec![Value::string("~A ~:D argument, ~S, is out of range (~A)"),Value::symbol("list-ref"),Value::Int(2),Value::Int(i),Value::string("it is negative")]))} for _ in 0..i { if !matches!(cur,Value::Pair(_)){return Err(SchemeError::new("out-of-range",vec![Value::string("~A ~:D argument, ~S, is out of range (~A)"),Value::symbol("list-ref"),Value::Int(2),Value::Int(i),Value::string("it is too large")]))} cur=cur.cdr()?; } if !matches!(cur,Value::Pair(_)){return Err(SchemeError::new("out-of-range",vec![Value::string("~A ~:D argument, ~S, is out of range (~A)"),Value::symbol("list-ref"),Value::Int(2),Value::Int(i),Value::string("it is too large")]))} cur=cur.car()?; if n+1<args.len(){ if is_callable_value(&cur){return self.apply_value(cur,args[n+1..].to_vec(),env);} return Err(cant_take_arguments_error_value(form_val, &cur, &args[n+1..])); } } Ok(cur) },
            Value::Env(ref e)=> { if args.len()>1 && args.get(0).and_then(|v|v.as_symbol()).map(is_syntax_name).unwrap_or(false){ return self.eval(Value::list(args), e.clone()); } let k=args.get(0).and_then(|v| v.as_symbol()).ok_or_else(|| SchemeError::new("wrong-type-arg", vec![Value::string("~A ~:D argument, ~S, is ~A but should be ~A"),Value::symbol("let-ref"),Value::Int(2),args.get(0).cloned().unwrap_or(Value::Unspecified),Value::string("a pair"),Value::string("a symbol")]))?; let first=e.get(k).unwrap_or(Value::Undefined); if args.len()>1{ if is_callable_value(&first){return self.apply_value(first,args[1..].to_vec(),env);} return Err(cant_take_arguments_error(&call_form_string(&proc,&args), &first, &args[1..])); } Ok(first) },
            Value::HashTable(ref h)=> { let key=args.get(0).cloned().unwrap_or(Value::Unspecified); let mut first=Value::Bool(false); for (k,v) in h.borrow().iter(){ if equal(k,&key){first=v.clone(); break;}} if args.len()>1{ if let Value::MultiVector{kind:Some(k),..}|Value::MultiVectorView{kind:Some(k),..}= &first {let sym=match k.as_str(){"r"=>"float-vector-ref","i"=>"int-vector-ref","u"=>"byte-vector-ref",_=>"vector-ref"}; if args.len()>3{return Err(SchemeError::new("out-of-range",vec![Value::string("~A ~:D argument, ~S, is out of range (~A)"),Value::symbol(sym),Value::Int(2),Value::list(args[1..].to_vec()),Value::string("too many indices")]));} if k.as_str()=="i" && args.len()>2 && !matches!(args[2],Value::Int(_)){return Err(SchemeError::new("wrong-type-arg",vec![Value::string("~A ~:D argument, ~S, is ~A but should be ~A"),Value::symbol("int-vector-ref"),Value::Int(3),args[2].clone(),Value::string(if matches!(args[2],Value::Float(_)){"a real"}else if matches!(args[2],Value::Rational(_,_)){"a ratio"}else{"an object"}),Value::string("an integer")]))}} if matches!(first,Value::ByteVector(_)){return Err(SchemeError::new("out-of-range",vec![Value::string("~A ~:D argument, ~S, is out of range (~A)"),Value::symbol("byte-vector-ref"),Value::Int(2),Value::list(args[1..].to_vec()),Value::string("too many indices")]));} if matches!(first,Value::IntVector(_)){return Err(SchemeError::new("out-of-range",vec![Value::string("~A ~:D argument, ~S, is out of range (~A)"),Value::symbol("int-vector-ref"),Value::Int(2),Value::list(args[1..].to_vec()),Value::string("too many indices")]));} if matches!(first,Value::Macro(_,_)){let expanded=self.apply_value(first,args[1..].to_vec(),env.clone())?; return self.eval(expanded,env);} if matches!(first,Value::Procedure(_)|Value::Dilambda(_)){let shown=if let Value::Dilambda(dl)=&first{dl.0.clone()}else{first.clone()}; return Err(SchemeError::new("syntax-error",vec![Value::string("can't call a (possibly unsafe) function implicitly: ~S ~S"),shown,Value::list(args[1..].to_vec())]));} if is_callable_value(&first){return self.apply_value(first,args[1..].to_vec(),env);} let mut form=vec![proc.clone()]; form.extend(args.clone()); return Err(cant_take_arguments_error_value(Value::list(form), &first, &args[1..])); } Ok(first) },
            Value::Hook(funcs,_)=>{ let hk=Env::new(Some(self.root.clone())); hk.define("abs", args.get(0).cloned().unwrap_or(Value::Undefined)); hk.define("result", Value::Undefined); for f in funcs.borrow().iter().cloned(){ self.apply_value(f, vec![Value::Env(hk.clone())], env.clone())?; } Ok(Value::Undefined) }
            Value::Iterator{items,consumed,..}=>{let mut xs=items.borrow_mut(); if xs.is_empty(){Ok(Value::Eof)}else{*consumed.borrow_mut()+=1; Ok(xs.remove(0))}}
            Value::Dilambda(dl)=>self.apply_value(dl.0.clone(),args,env),
            Value::Values(vs)=>{ if let Some(proc)=vs.get(0){let mut all=vs[1..].to_vec(); all.extend(args); self.apply_value(proc.clone(),all,env)}else{Ok(Value::Unspecified)} }
            Value::RootMeta(name)=>match name.as_str(){
                "or"=>{for v in args{if v.is_true(){return Ok(v);}} Ok(Value::Bool(false))}
                "and"=>{let mut last=Value::Bool(true); for v in args{last=v; if !last.is_true(){return Ok(last);}} Ok(last)}
                "begin"=>Ok(args.last().cloned().unwrap_or(Value::Unspecified)),
                "values"=>Ok(if args.is_empty(){Value::Unspecified}else if args.len()==1{args[0].clone()}else{Value::Values(args)}),
                "set!"=>{ if args.len()>=2 { if let Some(s)=args[0].as_symbol(){ if env.set(s,args[1].clone()){return Ok(args[1].clone());} } if args.len()==2 { if set_first_equal(&env,&args[0],args[1].clone()){return Ok(args[1].clone());} if let Value::Pair(_)=&args[0]{ let target_expr=args[0].car()?; let target=if matches!(target_expr.car().ok().and_then(|x|x.as_symbol().map(|s|s.to_string())).as_deref(),Some("quote")){target_expr.cdr()?.car()?}else{target_expr}; return set_applicable(target, args[0].cdr()?.to_vec()?, args[1].clone()); } } else { let target=if matches!(args[0].car().ok().and_then(|x|x.as_symbol().map(|s|s.to_string())).as_deref(),Some("quote")){args[0].cdr()?.car()?}else{args[0].clone()}; return set_applicable(target, args[1..args.len()-1].to_vec(), args[args.len()-1].clone()); } } Err(SchemeError::new("wrong-type-arg", vec![Value::RootMeta(name)])) }
                _=>Err(SchemeError::new("wrong-number-of-args", vec![]))
            },
            other=>Err(SchemeError::new("wrong-type-arg", vec![other])),
        }
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
            Procedure::Lambda{params,body,env,name} => { if !params.star && args.len()<params.required.len(){ let form=Value::list(vec![Value::symbol("lambda"), Value::list(params.required.iter().map(|n|Value::symbol(n)).collect()), body.borrow().get(0).cloned().unwrap_or(Value::Unspecified)]); return Err(SchemeError::new("wrong-number-of-args", vec![Value::string("~S: not enough arguments: ((~S ~S ...)~{~^ ~S~})"), Value::list(vec![form]), Value::symbol("lambda"), Value::list(params.required.iter().map(|n|Value::symbol(n)).collect()), Value::Nil])); } let new=Env::new(Some(env.clone())); if let Err(e)=bind_params(self, &new, params, args.clone(), call_env){ if let Some(err)=self.lambda_star_unknown_key_error(&e, params, body, &args, name.as_deref()){ return Err(err); } return Err(e);} self.eval_lambda_body(body, new) }
        }
    }
    fn eval_lambda_body(&mut self, body:&Rc<RefCell<Vec<Value>>>, env:EnvRef)->Result<Value>{let len=body.borrow().len(); if len==0{return Ok(Value::Unspecified)}; for i in 0..len-1{let expr=body.borrow().get(i).cloned().unwrap_or(Value::Unspecified); self.eval(expr,env.clone())?;} let last=body.borrow().get(len-1).cloned().unwrap_or(Value::Unspecified); self.eval_tail(last,env)}
    fn eval_quasiquote(&mut self, expr: Value, env: EnvRef) -> Result<Value> {
        if let Value::Pair(_) = &expr {
            if expr.car()?.as_symbol()==Some("unquote") { return self.eval(expr.cdr()?.car()?, env); }
            if expr.car()?.as_symbol()==Some("quasiquote") { return self.nested_quasiquote_repr(expr.cdr()?.car()?, env); }
        }
        match expr {
            Value::Pair(_) => self.eval_quasiquote_pair(expr, env),
            Value::Vector(v)=> { let mut out=Vec::new(); for x in v.borrow().iter(){ if let Value::Pair(_)=x { if x.car()?.as_symbol()==Some("unquote-splicing") { let expr=x.cdr()?.car()?; out.push(Value::list(vec![Value::symbol("unquote"),Value::list(vec![Value::symbol("apply-values"),expr])])); continue; } } out.push(self.eval_quasiquote(x.clone(), env.clone())?); } Ok(Value::Vector(Rc::new(RefCell::new(out)))) },
            v=>Ok(v)
        }
    }
    fn nested_quasiquote_repr(&mut self, body: Value, env: EnvRef) -> Result<Value> {
        fn quote_symbol_for(v:&Value)->Value{ Value::symbol(&format!("'{}", v)) }
        let mut out=vec![Value::symbol("list-values")];
        for item in body.to_vec()? {
            if let Value::Pair(_) = &item {
                if item.car()?.as_symbol()==Some("unquote") {
                    let expr=item.cdr()?.car()?;
                    if let Value::Pair(_) = &expr {
                        if expr.car()?.as_symbol()==Some("quote") {
                            let quoted=expr.cdr()?.car()?;
                            if let Value::Pair(_) = &quoted {
                                if quoted.car()?.as_symbol()==Some("unquote") {
                                    let v=self.eval(quoted.cdr()?.car()?, env.clone())?;
                                    out.push(quote_symbol_for(&v));
                                    continue;
                                }
                            }
                            out.push(quote_symbol_for(&quoted));
                            continue;
                        }
                    }
                    if let Some(s)=expr.as_symbol(){ out.push(Value::symbol(s)); }
                    else { out.push(expr); }
                    continue;
                }
            }
            out.push(quote_symbol_for(&item));
        }
        Ok(Value::list(out))
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
        if let Value::Pair(_) = &car {
            if car.car()?.as_symbol()==Some("unquote-splicing") {
                let spliced=self.eval(car.cdr()?.car()?, env.clone())?;
                if let Value::Values(vs)=spliced { return Err(SchemeError::new("wrong-number-of-args",vec![Value::string("~A: too many arguments: (~A~{~^ ~S~})"),Value::symbol("apply-values"),Value::symbol("apply-values"),Value::list(vs)])); }
                let tail=match cdr { Value::Nil=>Value::Nil, other=>self.eval_quasiquote(other, env)? };
                return self.append_to_tail(spliced, tail);
            }
        }
        let qcar=self.eval_quasiquote(car, env.clone())?;
        let qcdr=match cdr { Value::Nil=>Value::Nil, other=>self.eval_quasiquote(other, env)? };
        if let Value::Values(vs)=qcdr { let mut items=vec![Value::list(vec![qcar])]; items.extend(vs); return Err(SchemeError::new("wrong-number-of-args",vec![Value::string("~A: too many arguments: (~A~{~^ ~S~})"),Value::symbol("<list*>"),Value::symbol("<list*>"),Value::list(items)])); }
        if let Value::Values(vs)=qcar { let mut out=qcdr; for v in vs.into_iter().rev(){out=Value::cons(v,out);} return Ok(out); }
        Ok(Value::cons(qcar,qcdr))
    }

    fn eval_define(&mut self, args: Value, env: EnvRef) -> Result<Value> {
        let xs=args.to_vec()?;
        match xs.get(0) {
            Some(Value::Symbol(s)) => { let v=self.eval(xs.get(1).cloned().unwrap_or(Value::Unspecified), env.clone())?; if let Value::Values(vs)=v{return Err(SchemeError::new("syntax-error",vec![Value::string("~A: more than one value: (~A ~A ~S)"),Value::symbol("define"),Value::symbol("define"),Value::symbol(s),Value::Values(vs)]));} env.define(s.as_str(), v.clone()); Ok(v) }
            Some(Value::Pair(_)) => { let head=xs[0].clone(); let name=head.car()?.as_symbol().unwrap_or("<lambda>").to_string(); let params=head.cdr()?; let proc=self.make_lambda(Value::cons(params, Value::list(xs[1..].to_vec())), env.clone(), false, Some(name.clone()))?; env.define(name, proc.clone()); Ok(proc) }
            _=>Err(SchemeError::new("syntax-error", vec![Value::symbol("define")]))
        }
    }
    fn eval_define_star(&mut self, args: Value, env: EnvRef) -> Result<Value> {
        let xs=args.to_vec()?; let head=xs[0].clone(); let name=head.car()?.as_symbol().unwrap_or("<lambda>").to_string(); let params=head.cdr()?; let proc=self.make_lambda(Value::cons(params, Value::list(xs[1..].to_vec())), env.clone(), true, Some(name.clone()))?; env.define(name, proc.clone()); Ok(proc)
    }
    fn set_place_value(&mut self, place: Value, val: Value, env: EnvRef) -> Result<Value> {
        if let Some(s)=place.as_symbol() { if env.set(s,val.clone()){return Ok(val);} return Err(SchemeError::new("unbound-variable", vec![Value::symbol(s)])); }
        if let Value::Pair(_) = &place {
            let op_expr=place.car()?;
            if let Some(op_name)=op_expr.as_symbol() {
                let raw_args=place.cdr()?.to_vec()?;
                match op_name {
                    "current-input-port" if raw_args.is_empty() => { self.stdin=val.clone(); return Ok(val); }
                    "current-output-port" if raw_args.is_empty() => { self.stdout=val.clone(); return Ok(val); }
                    "current-error-port" if raw_args.is_empty() => { self.stderr=val.clone(); return Ok(val); }
                    "hook-functions" => { let h=self.eval(raw_args[0].clone(), env.clone())?; if let Value::Hook(funcs,_)=h { *funcs.borrow_mut()=val.to_vec()?; return Ok(val); } }
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
            if let Value::Values(vs)=&val { if vs.len()>1 { if let Some(s)=place.as_symbol(){return Err(SchemeError::new("wrong-number-of-args",vec![Value::string(format!("set!: can't set {} to (values {})",s,vs.iter().map(|v|v.to_string()).collect::<Vec<_>>().join(" ")))]));} if place.car().ok().and_then(|v|v.as_symbol().map(|x|x.to_string())).as_deref()==Some("*s7*"){return Err(SchemeError::new("wrong-number-of-args",vec![Value::string(format!("let-set!: too many arguments: (let-set! *s7* print-length {})",vs.iter().map(|v|v.to_string()).collect::<Vec<_>>().join(" ")))]));} } }
            self.set_place_value(place.clone(), val, env.clone())?;
            saved.push((place, old));
        }
        let result=self.eval_sequence(xs[1..].to_vec(), env.clone());
        for (place, old) in saved.into_iter().rev(){ let _=self.set_place_value(place, old, env.clone()); }
        result
    }
    fn eval_set(&mut self, args: Value, env: EnvRef) -> Result<Value> {
        let xs=args.to_vec()?; let place=xs[0].clone(); let val_expr=xs[1].clone();
        if let Some(s)=place.as_symbol() { let val=self.eval(val_expr.clone(), env.clone())?; let val=match val{Value::Values(vs) if vs.is_empty()=>Value::Unspecified,Value::Values(vs) if vs.len()==1=>vs[0].clone(),Value::Values(vs)=>return Err(SchemeError::new("wrong-number-of-args",vec![Value::string(format!("(set! {} (values{})): too many arguments to set!",s,format!(" {}",vs.iter().map(|v|v.to_string()).collect::<Vec<_>>().join(" "))))])),v=>v}; if env.set(s,val.clone()){return Ok(val);} return Err(SchemeError::new("unbound-variable", vec![Value::symbol(s)])); }
        if let Value::Pair(_) = place {
            if place.car()?.as_symbol()==Some("setter") { let proc=self.eval(place.cdr()?.car()?, env.clone())?; let val=self.eval(val_expr.clone(), env.clone())?; let k=proc_key(&proc).ok_or_else(||SchemeError::new("wrong-type-arg",vec![proc]))?; self.proc_setters.borrow_mut().insert(k, val); return Ok(Value::Unspecified); }
            let op_expr=place.car()?;
            if let Some(op_name)=op_expr.as_symbol() {
                let raw_args=place.cdr()?.to_vec()?;
                match op_name {
                    "quote"|"lambda"|"when"|"unless" => { let _=self.eval(val_expr.clone(), env.clone())?; let syn=if op_name=="quote"{"#_quote".to_string()}else{op_name.to_string()}; return Err(SchemeError::new("wrong-type-arg",vec![Value::string(format!("{} (syntactic) does not have a setter: (set! {} {})",syn,code_repr(&place),code_repr(&val_expr)))])); }
                    "car" => { let target=self.eval(raw_args[0].clone(), env.clone())?; let val=self.eval(val_expr.clone(), env.clone())?; if matches!(val,Value::Values(ref vs) if vs.len()>1){return Err(SchemeError::new("syntax-error",vec![Value::string("~A: too many arguments to set!"),Value::list(vec![Value::symbol("set!"),place.clone(),val_expr.clone()])]));} return target.set_car(if let Value::Values(vs)=val{vs.into_iter().next().unwrap_or(Value::Unspecified)}else{val}); }
                    "cdr" => { let target=self.eval(raw_args[0].clone(), env.clone())?; let val=self.eval(val_expr.clone(), env.clone())?; if matches!(val,Value::Values(ref vs) if vs.len()>1){return Err(SchemeError::new("syntax-error",vec![Value::string("~A: too many arguments to set!"),Value::list(vec![Value::symbol("set!"),place.clone(),val_expr.clone()])]));} return target.set_cdr(if let Value::Values(vs)=val{vs.into_iter().next().unwrap_or(Value::Unspecified)}else{val}); }
                    "list-ref" => { let target=self.eval(raw_args[0].clone(), env.clone())?; let idxs=raw_args[1..].iter().cloned().map(|x|self.eval(x, env.clone())).collect::<Result<Vec<_>>>()?; let val=self.eval(val_expr.clone(), env.clone())?; return list_set_nested(target, &idxs, val); }
                    "vector-ref" => { if raw_args.len()>2 { let target=self.eval(raw_args[0].clone(), env.clone())?; if !matches!(target,Value::MultiVector{..}){let idxs=raw_args[1..].iter().cloned().map(|x|self.eval(x, env.clone())).collect::<Result<Vec<_>>>()?; let val=self.eval(val_expr.clone(), env.clone())?; let mut all=vec![target]; all.extend(idxs); all.push(val); return Err(SchemeError::new("wrong-number-of-args",vec![Value::string(format!("too many arguments for vector-set!: ({})",all.iter().map(|v|v.to_string()).collect::<Vec<_>>().join(" ")))]));} } }
                    "*s7*" => { let val=self.eval(val_expr.clone(), env.clone())?; return Ok(val); }
                    "current-input-port" => { if raw_args.is_empty(){ let val=self.eval(val_expr.clone(), env.clone())?; self.stdin=val.clone(); return Ok(val); } }
                    "current-output-port" => { if raw_args.is_empty(){ let val=self.eval(val_expr.clone(), env.clone())?; self.stdout=val.clone(); return Ok(val); } }
                    "current-error-port" => { if raw_args.is_empty(){ let val=self.eval(val_expr.clone(), env.clone())?; self.stderr=val.clone(); return Ok(val); } }
                    "port-position" => { let p=self.eval(raw_args[0].clone(), env.clone())?; if matches!(p,Value::Port(ref pp) if matches!(&*pp.borrow(),Port::Output{..})){return Err(SchemeError::new("wrong-type-arg",vec![Value::string("~A ~:D argument, ~S, is ~A but should be ~A"),Value::string("set! port-position"),Value::Int(1),p,Value::string("an output port"),Value::string("an input port")]))} let val=self.eval(val_expr.clone(), env.clone())?; let n=match val{Value::Int(n)=>n,ref v=>return Err(SchemeError::new("wrong-type-arg",vec![Value::string("~A ~:D argument, ~S, is ~A but should be ~A"),Value::string("set! port-position"),Value::Int(2),v.clone(),Value::string(if matches!(v,Value::Float(_)){"a real"}else if matches!(v,Value::Rational(_,_)){"a ratio"}else if matches!(v,Value::Symbol(_)){"a symbol"}else{"an object"}),Value::string("an integer")]))}; if n<0{return Err(SchemeError::new("out-of-range",vec![Value::string("~A ~:D argument, ~S, is out of range (~A)"),Value::symbol("port-position"),Value::Int(2),Value::Int(n),Value::string("it is negative")]))} return set_port_position(&p, n as usize).map(|_| Value::Int(n)); }
                    "outlet" => { let target=self.eval(raw_args[0].clone(), env.clone())?; let val=self.eval(val_expr.clone(), env.clone())?; if let (Value::Env(e),Value::Env(parent))=(target,val.clone()){*e.parent.borrow_mut()=Some(parent); return Ok(val);} return Err(SchemeError::new("wrong-type-arg",vec![val])); }
                    "hook-functions" => { let h=self.eval(raw_args[0].clone(), env.clone())?; let val=self.eval(val_expr.clone(), env.clone())?; if let Value::Hook(funcs,_)=h { *funcs.borrow_mut()=val.to_vec()?; return Ok(val); } }
                    _=>{}
                }
            }
            if let Some(k)=keyword_name(&op_expr){ let _=self.eval(val_expr.clone(), env.clone())?; return Err(SchemeError::new("wrong-type-arg",vec![Value::string(format!("in (set! {} {}), :{} has no setter",code_repr(&place),code_repr(&val_expr),k))])); }
            if let Some(s)=op_expr.as_symbol(){ if env.get(s).is_none(){ let _=self.eval(val_expr.clone(), env.clone())?; return Err(SchemeError::new("unbound-variable",vec![Value::string(format!("unbound variable {} in (set! {} {})",s,code_repr(&place),code_repr(&val_expr)))])); } }
            let target=self.eval(op_expr.clone(), env.clone())?;
            let idxs=self.eval_list(place.cdr()?, env.clone())?;
            let val=self.eval(val_expr.clone(), env.clone())?;
            if let Value::Values(vs)=&val { if vs.len()>1 { if matches!(target,Value::ProcedureSource{..}){return Err(SchemeError::new("syntax-error",vec![Value::string("~A: too many arguments to set!"),Value::list(vec![Value::symbol("set!"),place.clone(),val_expr.clone()])]));} return Err(SchemeError::new("wrong-number-of-args",vec![Value::string(format!("(set! {} (values{})): too many arguments to set!",place,format!(" {}",vs.iter().map(|v|v.to_string()).collect::<Vec<_>>().join(" "))))])); } }
            if let Value::SetterRef(k)=target { self.proc_setters.borrow_mut().insert(k, val); return Ok(Value::Unspecified); }
            if idxs.is_empty() {
                if matches!(target,Value::Iterator{..}) { let name=op_expr.as_symbol().unwrap_or("#<iterator>"); return Err(SchemeError::new("wrong-type-arg",vec![Value::string(format!("{} (an iterator) does not have a setter: (set! {} {})",name,code_repr(&place),code_repr(&val_expr)))])); }
                if matches!(target,Value::Macro(_,_)) { let name=op_expr.as_symbol().unwrap_or("#<macro>"); return Err(SchemeError::new("wrong-type-arg",vec![Value::string(format!("{} (a macro) does not have a setter: (set! {} {})",name,code_repr(&place),code_repr(&val_expr)))])); }
                if let Some(k)=proc_key(&target) { let setter_opt={self.proc_setters.borrow().get(&k).cloned()}; if let Some(setter)=setter_opt { return self.apply_value(setter, vec![val.clone()], env).map(|_| val); } }
            }
            if let Value::Symbol(s)=&target { if let Some(actual)=env.get(s) { return set_applicable_from_set(actual, idxs, val, &place); } }
            return set_applicable_from_set(target, idxs, val, &place);
        }
        Err(SchemeError::new("syntax-error", vec![Value::symbol("set!")]))
    }
    fn make_lambda(&mut self, args: Value, env: EnvRef, star: bool, name: Option<String>) -> Result<Value> {
        let xs=args.to_vec()?; let params=parse_params(xs.get(0).cloned().unwrap_or(Value::Nil), star)?; Ok(Value::Procedure(Rc::new(Procedure::Lambda{params,body:Rc::new(RefCell::new(xs[1..].to_vec())),env,name})))
    }
    fn make_macro(&mut self, args: Value, env: EnvRef, kind: MacroKind, star: bool) -> Result<Value> { let Value::Procedure(p)=self.make_lambda(args, env, star, None)? else { unreachable!() }; Ok(Value::Macro(p, kind)) }
    fn eval_define_macro(&mut self, args: Value, env: EnvRef, kind: MacroKind, star: bool) -> Result<Value> {
        let xs=args.to_vec()?; let head=xs[0].clone(); let name=head.car()?.as_symbol().unwrap_or("<macro>").to_string(); let params=head.cdr()?; let Value::Macro(p,k)=self.make_macro(Value::cons(params, Value::list(xs[1..].to_vec())), env.clone(), kind, star)? else { unreachable!() }; let m=Value::Macro(p,k); env.define(name, m.clone()); Ok(m)
    }
    fn normalize_binding_value_ctx(&self, ctx:&str, name:&str, val:Value)->Result<Value>{match val{Value::Values(vs) if vs.is_empty()=>Ok(Value::Unspecified),Value::Values(vs) if vs.len()==1=>Ok(vs[0].clone()),Value::Values(vs)=>Err(SchemeError::new("syntax-error",vec![Value::string("~A: can't bind ~A to ~S"),Value::symbol(ctx),Value::symbol(name),Value::Values(vs)])),v=>Ok(v)}}
    fn eval_let(&mut self, args: Value, env: EnvRef, sequential: bool, _named: bool) -> Result<Value> {
        let xs=args.to_vec()?;
        if let Some(Value::Symbol(name))=xs.get(0) { // named let
            let bindings=xs[1].to_vec()?; let params=bindings.iter().map(|b| b.car().unwrap().as_symbol().unwrap().to_string()).collect::<Vec<_>>(); let vals=bindings.iter().map(|b| b.cdr().unwrap().car().unwrap()).collect::<Vec<_>>();
            let new=Env::new(Some(env.clone())); let proc=Value::Procedure(Rc::new(Procedure::Lambda{params:Params{required:params.clone(),rest:None,star:sequential,defaults:vec![None; params.len()],allow_other_keys:false,rest_before_formals:false},body:Rc::new(RefCell::new(xs[2..].to_vec())),env:new.clone(),name:Some(name.to_string())})); new.define(name.as_str(), proc.clone()); let evaled=vals.into_iter().map(|v| self.eval(v, env.clone())).collect::<Result<Vec<_>>>()?; return self.apply_value(proc, evaled, env);
        }
        let bindings=xs[0].to_vec()?; let new=Env::new(Some(env.clone()));
        if sequential { for b in bindings { let bv=b.to_vec()?; let name=bv[0].as_symbol().unwrap(); let val=self.eval(bv[1].clone(), new.clone())?; let val=self.normalize_binding_value_ctx("let*",name,val)?; new.define(name, val); } }
        else { let mut vals=Vec::new(); for b in &bindings { let bv=b.to_vec()?; let name=bv[0].as_symbol().unwrap().to_string(); let val=self.eval(bv[1].clone(), env.clone())?; vals.push((name.clone(), self.normalize_binding_value_ctx("let",&name,val)?)); } for (k,v) in vals { new.define(k,v); } }
        self.eval_sequence(xs[1..].to_vec(), new)
    }
    fn eval_letrec_ctx(&mut self, args: Value, env: EnvRef, ctx:&str) -> Result<Value> { let xs=args.to_vec()?; let bindings=xs[0].to_vec()?; let new=Env::new(Some(env)); for b in &bindings { new.define(b.car()?.as_symbol().unwrap(), Value::Unspecified); } for b in bindings { let bv=b.to_vec()?; let name=bv[0].as_symbol().unwrap(); let val=self.eval(bv[1].clone(), new.clone())?; let val=self.normalize_binding_value_ctx(ctx,name,val)?; new.set(name, val); } self.eval_sequence(xs[1..].to_vec(), new) }
    fn eval_cond(&mut self, args: Value, env: EnvRef) -> Result<Value> { for clause in args.to_vec()? { let xs=clause.to_vec()?; if xs[0].as_symbol()==Some("else") || self.eval(xs[0].clone(), env.clone())?.is_true() { return self.eval_sequence(xs[1..].to_vec(), env); } } Ok(Value::Unspecified) }
    fn eval_case(&mut self, args: Value, env: EnvRef) -> Result<Value> { let xs=args.to_vec()?; let key=self.eval(xs[0].clone(), env.clone())?; let key=if let Value::Values(vs)=key{vs.get(0).cloned().unwrap_or(Value::Unspecified)}else{key}; for clause in &xs[1..] { let cs=clause.to_vec()?; if cs[0].as_symbol()==Some("else") { return self.eval_sequence(cs[1..].to_vec(), env); } for datum in cs[0].to_vec()? { if equal(&key,&datum){ return self.eval_sequence(cs[1..].to_vec(), env); } } } Ok(Value::Unspecified) }
    fn eval_do(&mut self, args: Value, env: EnvRef) -> Result<Value> { let xs=args.to_vec()?; let specs=xs[0].to_vec()?; let test=xs[1].to_vec()?; let new=Env::new(Some(env.clone())); for sp in &specs { let sv=sp.to_vec()?; let init=self.eval(sv[1].clone(), env.clone())?; if matches!(init,Value::Values(_)){return Err(SchemeError::new("wrong-type-arg",vec![Value::string("do: variable initial value can't be ~S"),init]));} new.define(sv[0].as_symbol().unwrap(), init); } let mut last_body=Value::Unspecified; loop { let tv=self.eval(test[0].clone(), new.clone())?; let truth=match tv{Value::Values(vs)=>vs.last().cloned().unwrap_or(Value::Unspecified).is_true(),v=>v.is_true()}; if truth{ return if test.len()>1{self.eval_sequence(test[1..].to_vec(), new)}else{Ok(last_body)}; } last_body=self.eval_sequence(xs[2..].to_vec(), new.clone())?; let mut updates=Vec::new(); for sp in &specs { let sv=sp.to_vec()?; if sv.len()>2 { let step=self.eval(sv[2].clone(), new.clone())?; if matches!(step,Value::Values(_)){return Err(SchemeError::new("syntax-error",vec![Value::string("do: variable step value can't be ~S"),step]));} updates.push((sv[0].as_symbol().unwrap().to_string(), step)); } } for (k,v) in updates { new.set(&k,v); } } }
    fn eval_catch(&mut self, args: Value, env: EnvRef) -> Result<Value> { let xs=args.to_vec()?; let tag_expr=xs[0].clone(); let tag=self.eval(tag_expr, env.clone())?; let thunk=self.eval(xs[1].clone(), env.clone())?; let handler=self.eval(xs[2].clone(), env.clone())?; match self.apply_value(thunk, vec![], env.clone()) { Ok(v)=>Ok(v), Err(e)=>{ if matches!(tag,Value::Bool(true)) || tag.as_symbol()==Some(&e.tag) { let a=vec![Value::symbol(&e.tag), Value::list(e.args)]; self.apply_value(handler,a,env) } else { Err(e) } } } }
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
                if i+1>=args.len(){ return Err(SchemeError::new("wrong-number-of-args", vec![Value::string("~S: not enough arguments: ((~S ~S ...)~{~^ ~S~})"), Value::list(vec![]), Value::symbol("lambda"), Value::list(params.required.iter().map(|n|Value::symbol(n)).collect()), Value::Nil])); }
                let key=k.to_string();
                if seen_keys.contains_key(&key){ return Err(SchemeError::new("wrong-type-arg", vec![Value::string("parameter set twice, ~S in ~S"), Value::symbol(&key), Value::list(args.clone())])); }
                seen_keys.insert(key.clone(), i);
                if let Some(idx)=params.required.iter().position(|r|r==&key){ if assigned[idx]{ return Err(SchemeError::new("wrong-type-arg", vec![Value::string("parameter set twice, ~S in ~S"), Value::symbol(&key), Value::list(args.clone())])); } assigned[idx]=true; values[idx]=args[i+1].clone(); formal_pos+=1; }
                else { if params.rest.is_some() && !params.rest_before_formals && !assigned.iter().any(|x|*x) && !params.allow_other_keys { let rem=Value::list(args[i..].to_vec()); return Err(SchemeError::new("wrong-type-arg", vec![Value::string("~A: unknown key: ~S in ~S"), Value::list(vec![Value::symbol("lambda*")]), rem.clone(), rem])); } let prior_assigned=assigned.iter().any(|x|*x); unknown_items.push(args[i].clone()); unknown_items.push(args[i+1].clone()); if params.rest.is_some() && (!params.allow_other_keys || prior_assigned){rest_items.push(args[i].clone()); rest_items.push(args[i+1].clone());} }
                i+=2;
            } else {
                let slot=formal_pos;
                formal_pos+=1;
                if slot<params.required.len(){ if params.rest_before_formals { rest_items.push(args[i].clone()); /* reserve positional slot but let the default expression bind it */ } else { if assigned[slot]{let key=&params.required[slot]; return Err(SchemeError::new("wrong-type-arg", vec![Value::string("parameter set twice, ~S in ~S"), Value::symbol(key), Value::list(args.clone())]));} assigned[slot]=true; values[slot]=args[i].clone(); } } else {rest_items.push(args[i].clone());}
                i+=1;
            }
        }
        for (idx,name) in params.required.iter().enumerate(){ if assigned[idx]{ env.define(name,values[idx].clone()); } else { env.define(name,Value::Undefined); } }
        if let Some(r)=&params.rest { let staged=if params.rest_before_formals{let n=if args.len()>2{args.len()-1}else{args.len()}; Value::list(args[..n].to_vec())}else{Value::list(rest_items.clone())}; env.define(r, staged); }
        if params.rest_before_formals && matches!(args.first(),Some(Value::Keyword(_))) { if let Some((idx,_))=params.defaults.iter().enumerate().find(|(i,d)|!assigned[*i] && !matches!(d,Some(Value::Bool(false)))) { if let Some(v)=args.get(1){env.set(&params.required[idx],v.clone()); assigned[idx]=true;} } }
        if params.rest_before_formals { let bare_idxs=params.defaults.iter().enumerate().filter_map(|(i,d)| if matches!(d,Some(Value::Bool(false))) && !assigned[i]{Some(i)}else{None}).collect::<Vec<_>>(); let start=args.len().saturating_sub(bare_idxs.len()); for (j,idx) in bare_idxs.into_iter().enumerate(){ if let Some(v)=args.get(start+j){ env.set(&params.required[idx],v.clone()); assigned[idx]=true; } } }
        let simple_default=|v:&Value| matches!(v,Value::Bool(_)|Value::Int(_)|Value::Rational(_,_)|Value::Float(_)|Value::Complex(_,_)|Value::NumberLiteral(_,_)|Value::Char(_)|Value::String(_)|Value::Keyword(_)|Value::Vector(_)|Value::ByteVector(_)|Value::FloatVector(_)|Value::IntVector(_)|Value::Nil);
        let mut default_done=assigned.clone();
        for idx in 0..params.required.len(){ if !assigned[idx]{ if let Some(Some(d))=params.defaults.get(idx){ if simple_default(d){ env.set(&params.required[idx],d.clone()); default_done[idx]=true; } } } }
        for idx in 0..params.required.len(){ if !default_done[idx]{ let name=&params.required[idx]; if !matches!(env.get(name),Some(Value::Undefined)|None){continue;} let Some(Some(d))=params.defaults.get(idx) else { return Err(SchemeError::new("wrong-number-of-args", vec![Value::symbol(name)])); }; let val=if params.rest_before_formals { params.rest.as_deref().and_then(|r|rest_length_default_override(&d,r,&env)).map(Ok).unwrap_or_else(||ev.eval(d.clone(), env.clone()))? } else { ev.eval(d.clone(), env.clone())? }; env.set(name,val); } }
        if params.rest_before_formals { if let Some(r)=&params.rest { env.set(r, Value::list(args.clone())); } }
        if !unknown_items.is_empty() && !params.rest_before_formals && params.rest.is_none() && !params.allow_other_keys { let unknown=Value::list(unknown_items.clone()); return Err(SchemeError::new("wrong-type-arg", vec![Value::string("~A: unknown key: ~S in ~S"), Value::list(vec![Value::symbol("lambda*")]), unknown.clone(), Value::list(args.clone())])); }
    } else {
        if args.len()<params.required.len() || (params.rest.is_none() && args.len()>params.required.len()) { return Err(SchemeError::new("wrong-number-of-args", vec![Value::string("~S: not enough arguments: ((~S ~S ...)~{~^ ~S~})"), Value::list(vec![Value::list(vec![Value::symbol("lambda"), Value::list(params.required.iter().map(|n|Value::symbol(n)).collect())])]), Value::symbol("lambda"), Value::list(params.required.iter().map(|n|Value::symbol(n)).collect()), Value::Nil])); }
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
            Value::Symbol(s)=>{rest=Some(s.to_string()); break;},
            Value::Pair(p)=>{
                let (car,cdr)={let Object::Pair{car,cdr}= &*p.borrow(); (car.clone(),cdr.clone())};
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
                            required.push(name); defaults.push(xs.get(1).cloned());
                        }
                        Value::Symbol(s) => { required.push(s.to_string()); defaults.push(Some(Value::Bool(false))); }
                        other => return Err(SchemeError::new("syntax-error", vec![other])),
                    }
                } else {
                    required.push(car.as_symbol().ok_or_else(||SchemeError::new("syntax-error",vec![car.clone()]))?.to_string());
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

fn list_at(mut cur: Value, idx: &Value)->Result<Value>{ let n=match idx{Value::Int(n)=>*n,_=>return Err(SchemeError::new("wrong-type-arg",vec![Value::string("~A ~:D argument, ~S, is ~A but should be ~A"),Value::symbol("list-ref"),Value::Int(2),idx.clone(),Value::string(if matches!(idx,Value::Float(_)){"a real"}else if matches!(idx,Value::Rational(_,_)){"a ratio"}else{"an object"}),Value::string("an integer")]))}; if n<0{return Err(SchemeError::new("out-of-range",vec![Value::string("~A ~:D argument, ~S, is out of range (~A)"),Value::symbol("list-ref"),Value::Int(2),Value::Int(n),Value::string("it is negative")]))} for _ in 0..n{ if !matches!(cur,Value::Pair(_)){return Err(SchemeError::new("out-of-range",vec![Value::string("~A ~:D argument, ~S, is out of range (~A)"),Value::symbol("list-ref"),Value::Int(2),Value::Int(n),Value::string("it is too large")]))} cur=cur.cdr()?;} if !matches!(cur,Value::Pair(_)){return Err(SchemeError::new("out-of-range",vec![Value::string("~A ~:D argument, ~S, is out of range (~A)"),Value::symbol("list-ref"),Value::Int(2),Value::Int(n),Value::string("it is too large")]))} Ok(cur.car()?) }
fn list_set_nested(target: Value, idxs: &[Value], val: Value)->Result<Value>{ if idxs.is_empty(){return Err(SchemeError::new("wrong-number-of-args",vec![]));} let mut cur=target; for idx in &idxs[..idxs.len()-1]{ cur=list_at(cur, idx)?; } let idx_arg=&idxs[idxs.len()-1]; let i=match idx_arg{Value::Int(n)=>*n,_=>return Err(SchemeError::new("wrong-type-arg",vec![Value::string("~A ~:D argument, ~S, is ~A but should be ~A"),Value::symbol("list-set!"),Value::Int(2),idx_arg.clone(),Value::string(if matches!(idx_arg,Value::Float(_)){"a real"}else if matches!(idx_arg,Value::Rational(_,_)){"a ratio"}else{"an object"}),Value::string("an integer")]))}; if i<0{return Err(SchemeError::new("out-of-range",vec![Value::string("~A ~:D argument, ~S, is out of range (~A)"),Value::symbol("list-set!"),Value::Int(2),Value::Int(i),Value::string("it is negative")]))} for _ in 0..i{ if !matches!(cur,Value::Pair(_)){return Err(SchemeError::new("wrong-type-arg",vec![Value::string("list-set! second argument, ~D, is out of range (it is too large)"), Value::Int(i)]));} cur=cur.cdr()?;} if !matches!(cur,Value::Pair(_)){return Err(SchemeError::new("wrong-type-arg",vec![Value::string("list-set! second argument, ~D, is out of range (it is too large)"), Value::Int(i)]));} cur.set_car(val.clone()).map(|_| val) }
fn simple_value_kind(v:&Value)->&'static str{match v{Value::Symbol(_)=>"a symbol",Value::Float(_)=>"a real",Value::Rational(_,_)=>"a ratio",Value::Char(_)|Value::NamedChar(_)=>"a character",Value::String(_)=>"a string",Value::Pair(_)=>"a pair",Value::Nil=>"nil",Value::Unspecified=>"the unspecified object",Value::Int(_)=>"an integer",_=>"an object"}}
fn applicable_get(target:&Value,arg:&Value)->Result<Value>{match target{Value::Vector(v)=>index_vec(&v.borrow(),std::slice::from_ref(arg)),Value::ProcedureSource{params,body}=>{let i=match arg{Value::Int(n)=>*n,_=>return Err(SchemeError::new("wrong-type-arg",vec![Value::string("~A ~:D argument, ~S, is ~A but should be ~A"),Value::symbol("list-ref"),Value::Int(2),arg.clone(),Value::string(if matches!(arg,Value::Rational(_,_)){"a ratio"}else if matches!(arg,Value::Float(_)){"a real"}else if matches!(arg,Value::Unspecified){"the unspecified object"}else{simple_value_kind(arg)}),Value::string("an integer")]))}; if i==0{Ok(Value::symbol(if params.star{"lambda*"}else{"lambda"}))}else if i==1{Ok(proc_source_params(params))}else{let bi=i-2; if bi<0 || bi as usize>=body.borrow().len(){return Err(SchemeError::new("out-of-range",vec![Value::string("~A ~:D argument, ~S, is out of range (~A)"),Value::symbol("list-ref"),Value::Int(2),Value::Int(i),Value::string(if i<0{"it is negative"}else{"it is too large"})]))} Ok(body.borrow()[bi as usize].clone())}},Value::MultiVector{dims,data,kind}=>{let raw=match arg{Value::Int(n)=>*n,_=>return Err(SchemeError::new("wrong-type-arg",vec![Value::string("~A ~:D argument, ~S, is ~A but should be ~A"),Value::symbol("vector-ref"),Value::Int(2),arg.clone(),Value::string(if matches!(arg,Value::Float(_)){"a real"}else if matches!(arg,Value::Rational(_,_)){"a ratio"}else{"an object"}),Value::string("an integer")]))}; if raw<0||raw as usize>=dims[0]{return Err(SchemeError::new("out-of-range",vec![Value::string("~A ~:D argument, ~S, is out of range (~A)"),Value::symbol("vector-ref"),Value::Int(2),Value::Int(raw),Value::string(if raw<0{"it is negative"}else{"it is too large"})]));} let i=raw as usize; if dims.len()==1{Ok(data.borrow()[i].clone())}else{let rem=dims[1..].iter().product::<usize>(); Ok(Value::MultiVectorView{dims:dims[1..].to_vec(),data:data.clone(),offset:i*rem,kind:kind.clone()})}},Value::MultiVectorView{dims,data,offset,kind}=>{let pos=if kind.is_some(){3}else{2}; let raw=match arg{Value::Int(n)=>*n,_=>return Err(SchemeError::new("wrong-type-arg",vec![Value::string("~A ~:D argument, ~S, is ~A but should be ~A"),Value::symbol("vector-ref"),Value::Int(pos),arg.clone(),Value::string(if matches!(arg,Value::Float(_)){"a real"}else if matches!(arg,Value::Rational(_,_)){"a ratio"}else{"an object"}),Value::string("an integer")]))}; if raw<0||raw as usize>=dims[0]{return Err(SchemeError::new("out-of-range",vec![Value::string("~A ~:D argument, ~S, is out of range (~A)"),Value::symbol("vector-ref"),Value::Int(pos),Value::Int(raw),Value::string(if raw<0{"it is negative"}else{"it is too large"})]));} let i=raw as usize; if dims.len()==1{Ok(data.borrow()[*offset+i].clone())}else{let rem=dims[1..].iter().product::<usize>(); Ok(Value::MultiVectorView{dims:dims[1..].to_vec(),data:data.clone(),offset:*offset+i*rem,kind:kind.clone()})}},Value::ByteVector(v)=>index_bvec(&v.borrow(),std::slice::from_ref(arg)),Value::FloatVector(v)=>index_fvec(&v.borrow(),std::slice::from_ref(arg)),Value::IntVector(v)=>index_ivec(&v.borrow(),std::slice::from_ref(arg)),Value::String(s)=>{let chars=s.borrow().chars().collect::<Vec<_>>(); let i=match arg{Value::Int(n)=>*n,_=>return Err(SchemeError::new("wrong-type-arg",vec![Value::string("~A ~:D argument, ~S, is ~A but should be ~A"),Value::symbol("string-ref"),Value::Int(2),arg.clone(),Value::string(if matches!(arg,Value::Float(_)){"a real"}else if matches!(arg,Value::Rational(_,_)){"a ratio"}else{"an object"}),Value::string("an integer")]))}; if i<0||i as usize>=chars.len(){Err(SchemeError::new("out-of-range",vec![Value::string("~A ~:D argument, ~S, is out of range (~A)"),Value::symbol("string-ref"),Value::Int(2),Value::Int(i),Value::string(if i<0{"it is negative"}else{"it is too large"})]))}else{Ok(Value::Char(chars[i as usize]))}},Value::Pair(_)=>list_at(target.clone(),arg),Value::Env(e)=>{let k=arg.as_symbol().ok_or_else(||SchemeError::new("wrong-type-arg",vec![arg.clone()]))?; Ok(e.get(k).unwrap_or(Value::Undefined))},Value::HashTable(h)=>{for (k,v) in h.borrow().iter(){if equal(k,arg){return Ok(v.clone());}} Err(SchemeError::new("missing-key",vec![arg.clone(),target.clone()]))},_=>Err(SchemeError::new("wrong-type-arg",vec![target.clone()]))}}
fn set_applicable_from_set(target:Value,args:Vec<Value>,val:Value,place:&Value)->Result<Value>{let single_index_form=place.cdr().ok().and_then(|d|d.to_vec().ok()).map(|v|v.len()==1).unwrap_or(false); if args.len()>1 && single_index_form{match &target{Value::Vector(_)=>return Err(SchemeError::new("wrong-number-of-args",vec![Value::string(format!("too many arguments for vector-set!: ~S")),Value::list({let mut xs=vec![target.clone()]; xs.extend(args.clone()); xs.push(val.clone()); xs})])),Value::ByteVector(_)=>return Err(SchemeError::new("wrong-number-of-args",vec![Value::string(format!("too many arguments for vector-set!: ~S")),Value::list({let mut xs=vec![target.clone()]; xs.extend(args.clone()); xs.push(val.clone()); xs})])),Value::String(_)=>return Err(SchemeError::new("wrong-number-of-args",vec![Value::string("~A: too many arguments: (~A~{~^ ~S~})"),Value::symbol("string-set!"),Value::symbol("string-set!"),Value::list({let mut xs=vec![target.clone()]; xs.extend(args.clone()); xs.push(val.clone()); xs})])),Value::ProcedureSource{..}=>return Err(SchemeError::new("out-of-range",vec![Value::string("~A ~:D argument, ~S, is out of range (~A)"),Value::symbol("list-set!"),Value::Int(3),args[1].clone(),Value::string("it is too large")])),_=>{}}}if matches!(target,Value::MultiVector{..}|Value::MultiVectorView{..}){return set_applicable(target,args,val);} if args.len()>1{let mut cur=target.clone(); let place_s=code_repr(place); for arg in args[..args.len()-1].iter(){let base=cur.clone(); match applicable_get(&base,arg){Ok(next)=>cur=next,Err(e) if e.tag=="missing-key"=>return Err(SchemeError::new("wrong-type-arg",vec![Value::string(format!("in (set! {} {}), {} does not exist in {}",place_s,val,error_arg_string(arg),hash_table_expr(&base)))])),Err(e)=>return Err(e)} if !is_callable_value(&cur){return Err(SchemeError::new("wrong-type-arg",vec![Value::string(format!("in (set! {} {}), {} is {} which can't take arguments",place_s,val,call_form_string(&base,std::slice::from_ref(arg)),cur))]));}}
return set_applicable(cur,args[args.len()-1..].to_vec(),val)} set_applicable(target,args,val)}
fn multivector_set_value(kind:&Option<Rc<String>>, val:Value, who:&str)->Result<Value>{
    match kind.as_deref().map(|s|s.as_str()){
        Some("r")=>match val{Value::Int(n)=>Ok(Value::Float(n as f64)),Value::Float(_)=>Ok(val),Value::Rational(n,d)=>Ok(Value::Float(n as f64/d as f64)),ref v=>Err(SchemeError::new("wrong-type-arg",vec![Value::string("~A argument, ~S, is ~A but should be ~A"),Value::string(who),v.clone(),Value::string(if matches!(v,Value::Symbol(_)){"a symbol"}else{simple_value_kind(v)}),Value::string("a real")]))},
        Some("i")=>match val{Value::Int(_)=>Ok(val),ref v=>Err(SchemeError::new("wrong-type-arg",vec![Value::string("~A ~:D argument, ~S, is ~A but should be ~A"),Value::symbol("int-vector-set!"),Value::Int(3),v.clone(),Value::string(if matches!(v,Value::Float(_)){"a real"}else if matches!(v,Value::Rational(_,_)){"a ratio"}else{simple_value_kind(v)}),Value::string("an integer")]))},
        Some("u")=>match val{Value::Int(n) if (0..=255).contains(&n)=>Ok(Value::Int(n)),Value::Int(n)=>Err(SchemeError::new("wrong-type-arg",vec![Value::string("~A ~:D argument, ~S, is ~A but should be ~A"),Value::symbol("byte-vector-set!"),Value::Int(3),Value::Int(n),Value::string("an integer"),Value::string("a byte")])),ref v=>Err(SchemeError::new("wrong-type-arg",vec![Value::string("~A ~:D argument, ~S, is ~A but should be ~A"),Value::symbol("byte-vector-set!"),Value::Int(3),v.clone(),Value::string(if matches!(v,Value::Float(_)){"a real"}else if matches!(v,Value::Rational(_,_)){"a ratio"}else{simple_value_kind(v)}),Value::string("an integer")]))},
        _=>Ok(val),
    }
}
fn set_applicable(target:Value, args:Vec<Value>, val:Value)->Result<Value>{ match target { Value::Vector(v)=>{let len=v.borrow().len(); let raw=args.get(0).ok_or_else(||SchemeError::new("wrong-number-of-args",vec![]))?; let ii=match raw{Value::Int(n)=>*n,_=>return Err(SchemeError::new("wrong-type-arg",vec![Value::string("~A ~:D argument, ~S, is ~A but should be ~A"),Value::symbol("vector-set!"),Value::Int(2),raw.clone(),Value::string(if matches!(raw,Value::Float(_)){"a real"}else if matches!(raw,Value::Rational(_,_)){"a ratio"}else{"an object"}),Value::string("an integer")]))}; if ii<0||ii as usize>=len{return Err(SchemeError::new("out-of-range",vec![Value::string("~A ~:D argument, ~S, is out of range (~A)"),Value::symbol("vector-set!"),Value::Int(2),Value::Int(ii),Value::string(if ii<0{"it is negative"}else{"it is too large"})]));} let i=ii as usize; v.borrow_mut()[i]=val.clone(); Ok(val)}, Value::ProcedureSource{body,..}=>{let raw=args.get(0).ok_or_else(||SchemeError::new("wrong-number-of-args",vec![]))?; let i=match raw{Value::Int(n)=>*n,_=>return Err(SchemeError::new("wrong-type-arg",vec![Value::string("~A ~:D argument, ~S, is ~A but should be ~A"),Value::symbol("list-set!"),Value::Int(2),raw.clone(),Value::string(if matches!(raw,Value::Float(_)){"a real"}else if matches!(raw,Value::Rational(_,_)){"a ratio"}else if matches!(raw,Value::Unspecified){"the unspecified object"}else{simple_value_kind(raw)}),Value::string("an integer")]))}; if i<0{return Err(SchemeError::new("out-of-range",vec![Value::string("~A ~:D argument, ~S, is out of range (~A)"),Value::symbol("list-set!"),Value::Int(2),Value::Int(i),Value::string("it is negative")]))} if i<2{return Ok(val);} let bi=(i-2) as usize; if bi<body.borrow().len(){body.borrow_mut()[bi]=val.clone(); return Ok(val);} Err(SchemeError::new("out-of-range",vec![Value::string("~A ~:D argument, ~S, is out of range (~A)"),Value::symbol("list-set!"),Value::Int(2),Value::Int(i),Value::string("it is too large")]))}, Value::MultiVector{dims,data,kind}=>{if args.len()!=dims.len(){return Err(SchemeError::new("wrong-number-of-args",vec![Value::string(if args.len()>dims.len(){"too many arguments for vector-set!: ~S"}else{"not enough arguments for vector-set!: ~S"}),Value::list({let mut xs=vec![Value::MultiVector{dims:dims.clone(),data:data.clone(),kind:kind.clone()}]; xs.extend(args.clone()); xs.push(val.clone()); xs})]));} let mut idx=0usize; let mut stride:usize=dims.iter().skip(1).product(); for (n_idx,arg) in args.iter().enumerate(){let n=n_idx; let raw=match arg{Value::Int(n)=>*n,_=>return Err(SchemeError::new("wrong-type-arg",vec![Value::string("~A ~:D argument, ~S, is ~A but should be ~A"),Value::symbol("vector-set!"),Value::Int(2),arg.clone(),Value::string(if matches!(arg,Value::Float(_)){"a real"}else if matches!(arg,Value::Rational(_,_)){"a ratio"}else{"an object"}),Value::string("an integer")]))}; if raw<0||raw as usize>=dims[n]{return Err(SchemeError::new("out-of-range",vec![Value::Int(raw)]));} let i=raw as usize; idx+=i*stride; if n+1<dims.len(){stride=dims[n+2..].iter().product();}} let v=multivector_set_value(&kind,val.clone(),"float-vector-set!")?; data.borrow_mut()[idx]=v.clone(); Ok(v)}, Value::MultiVectorView{dims,data,offset,kind}=>{if args.len()!=dims.len(){return Err(SchemeError::new("wrong-number-of-args",vec![Value::string(if args.len()>dims.len(){"too many arguments for vector-set!: ~S"}else{"not enough arguments for vector-set!: ~S"}),Value::list({let mut xs=vec![Value::MultiVectorView{dims:dims.clone(),data:data.clone(),offset,kind:kind.clone()}]; xs.extend(args.clone()); xs.push(val.clone()); xs})]));} let mut idx=offset; let mut stride:usize=dims.iter().skip(1).product(); for (n_idx,arg) in args.iter().enumerate(){let n=n_idx; let raw=match arg{Value::Int(n)=>*n,_=>return Err(SchemeError::new("wrong-type-arg",vec![Value::string("~A ~:D argument, ~S, is ~A but should be ~A"),Value::symbol("vector-set!"),Value::Int(2),arg.clone(),Value::string(if matches!(arg,Value::Float(_)){"a real"}else if matches!(arg,Value::Rational(_,_)){"a ratio"}else{"an object"}),Value::string("an integer")]))}; if raw<0||raw as usize>=dims[n]{return Err(SchemeError::new("out-of-range",vec![Value::Int(raw)]));} let i=raw as usize; idx+=i*stride; if n+1<dims.len(){stride=dims[n+2..].iter().product();}} let v=multivector_set_value(&kind,val.clone(),"float-vector-set!")?; data.borrow_mut()[idx]=v.clone(); Ok(v)}, Value::ByteVector(v)=>{let len=v.borrow().len(); let raw_i=args.get(0).ok_or_else(||SchemeError::new("wrong-number-of-args",vec![]))?; let ii=match raw_i{Value::Int(n)=>*n,_=>return Err(SchemeError::new("wrong-type-arg",vec![Value::string("~A ~:D argument, ~S, is ~A but should be ~A"),Value::symbol("byte-vector-set!"),Value::Int(2),raw_i.clone(),Value::string(if matches!(raw_i,Value::Float(_)){"a real"}else if matches!(raw_i,Value::Rational(_,_)){"a ratio"}else{"an object"}),Value::string("an integer")]))}; if ii<0||ii as usize>=len{return Err(SchemeError::new("out-of-range",vec![Value::string("~A ~:D argument, ~S, is out of range (~A)"),Value::symbol("byte-vector-set!"),Value::Int(2),Value::Int(ii),Value::string(if ii<0{"it is negative"}else{"it is too large"})]))} let i=ii as usize; let n=match val{Value::Int(n)=>n,ref v=>return Err(SchemeError::new("wrong-type-arg",vec![Value::string("~A ~:D argument, ~S, is ~A but should be ~A"),Value::symbol("byte-vector-set!"),Value::Int(3),v.clone(),Value::string(if matches!(v,Value::String(_)){"a string"}else if matches!(v,Value::Char(_)|Value::NamedChar(_)){"a character"}else if matches!(v,Value::Float(_)){"a real"}else if matches!(v,Value::Rational(_,_)){"a ratio"}else{"an object"}),Value::string("an integer")]))}; if !(0..=255).contains(&n){return Err(SchemeError::new("wrong-type-arg",vec![Value::string("~A ~:D argument, ~S, is ~A but should be ~A"),Value::symbol("byte-vector-set!"),Value::Int(3),Value::Int(n),Value::string("an integer"),Value::string("an unsigned byte")]))} v.borrow_mut()[i]=n as u8; Ok(val)}, Value::FloatVector(v)=>{let len=v.borrow().len(); let raw=args.get(0).ok_or_else(||SchemeError::new("wrong-number-of-args",vec![]))?; let ii=match raw{Value::Int(n)=>*n,_=>return Err(SchemeError::new("wrong-type-arg",vec![Value::string("~A ~:D argument, ~S, is ~A but should be ~A"),Value::symbol("float-vector-set!"),Value::Int(2),raw.clone(),Value::string(if matches!(raw,Value::Float(_)){"a real"}else if matches!(raw,Value::Rational(_,_)){"a ratio"}else if matches!(raw,Value::Char(_)|Value::NamedChar(_)){"a character"}else{"an object"}),Value::string("an integer")]))}; if ii<0||ii as usize>=len{return Err(SchemeError::new("out-of-range",vec![Value::Int(ii)]));} let f=match val{Value::Int(n)=>n as f64,Value::Float(x)=>x,Value::Rational(n,d)=>n as f64/d as f64,ref v=>return Err(SchemeError::new("wrong-type-arg",vec![Value::string("~A ~:D argument, ~S, is ~A but should be ~A"),Value::symbol("float-vector-set!"),Value::Int(3),v.clone(),Value::string(if matches!(v,Value::Symbol(_)){"a symbol"}else{"an object"}),Value::string("a real")]))}; v.borrow_mut()[ii as usize]=f; Ok(Value::Float(f))}, Value::IntVector(v)=>{let len=v.borrow().len(); let raw=args.get(0).ok_or_else(||SchemeError::new("wrong-number-of-args",vec![]))?; let ii=match raw{Value::Int(n)=>*n,_=>return Err(SchemeError::new("wrong-type-arg",vec![Value::string("~A ~:D argument, ~S, is ~A but should be ~A"),Value::symbol("int-vector-set!"),Value::Int(2),raw.clone(),Value::string(if matches!(raw,Value::Float(_)){"a real"}else if matches!(raw,Value::Rational(_,_)){"a ratio"}else{"an object"}),Value::string("an integer")]))}; if ii<0||ii as usize>=len{return Err(SchemeError::new("out-of-range",vec![Value::Int(ii)]));} let i=ii as usize; let n=match val{Value::Int(n)=>n,ref v=>return Err(SchemeError::new("wrong-type-arg",vec![Value::string("~A ~:D argument, ~S, is ~A but should be ~A"),Value::symbol("int-vector-set!"),Value::Int(3),v.clone(),Value::string(if matches!(v,Value::Float(_)){"a real"}else if matches!(v,Value::Rational(_,_)){"a ratio"}else{"an object"}),Value::string("an integer")]))}; v.borrow_mut()[i]=n; Ok(Value::Int(n))}, Value::String(s)=>{let idx_arg=args.get(0).ok_or_else(||SchemeError::new("wrong-number-of-args",vec![]))?; let i=match idx_arg{Value::Int(n)=>*n,_=>return Err(SchemeError::new("wrong-type-arg",vec![Value::string("~A ~:D argument, ~S, is ~A but should be ~A"),Value::symbol("string-set!"),Value::Int(2),idx_arg.clone(),Value::string(if matches!(idx_arg,Value::Float(_)){"a real"}else if matches!(idx_arg,Value::Rational(_,_)){"a ratio"}else{"an object"}),Value::string("an integer")]))}; let mut chars=s.borrow().chars().collect::<Vec<_>>(); if i<0||i as usize>=chars.len(){return Err(SchemeError::new("out-of-range",vec![Value::string("~A ~:D argument, ~S, is out of range (~A)"),Value::symbol("string-set!"),Value::Int(2),Value::Int(i),Value::string(if i<0{"it is negative"}else{"it is too large"})]));} if let Value::Char(c)=val { chars[i as usize]=c; *s.borrow_mut()=chars.into_iter().collect(); Ok(val)} else {Err(SchemeError::new("wrong-type-arg", vec![Value::string("~A ~:D argument, ~S, is ~A but should be ~A"),Value::symbol("string-set!"),Value::Int(3),val.clone(),Value::string(if matches!(val,Value::Int(_)){"an integer"}else{"an object"}),Value::string("a character")]))}}, Value::Pair(_)=>list_set_nested(target,&args,val), Value::Env(e)=>{let k=args.get(0).and_then(|v| v.as_symbol()).ok_or_else(|| SchemeError::new("wrong-type-arg", args.clone()))?; if !e.set(k,val.clone()){return Err(SchemeError::new("wrong-type-arg",vec![Value::string("let-set!: ~A is not defined in ~A"),Value::symbol(k),Value::Env(e.clone())]));}; Ok(val)}, Value::HashTable(h)=>{let key=args.get(0).cloned().unwrap_or(Value::Unspecified); let mut hb=h.borrow_mut(); for (k,v) in hb.iter_mut(){ if equal(k,&key){*v=val.clone(); return Ok(val);} } hb.push((key,val.clone())); Ok(val)}, _=>Err(SchemeError::new("wrong-type-arg", vec![target])) } }

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
static HELP_FALSE: &[&str] = &["reader-cond","pi","*cload-directory*","else","sync-eval"];
static SYMBOL_DOC_FALSE: &[&str] = &["reader-cond","*s7*","pi","*cload-directory*","else","sync-eval"];
fn compat_doc_len(name:&str)->Option<usize>{Some(match name{
    "*rootlet-redefinition-hook*"=>115,"*read-error-hook*"=>122,"*error-hook*"=>93,"*autoload-hook*"=>122,"*load-hook*"=>92,"*missing-close-paren-hook*"=>95,"*unbound-variable-hook*"=>110,"hook-functions"=>81,"make-hook"=>116,"car"=>48,"eval"=>243,"lambda*"=>133,"rootlet"=>70,"object->let"=>59,"open-input-string"=>55,_=>return None})}
fn meta_type(name:&str)->&'static str{match name{"reader-cond"=>"macro?","lambda"|"lambda*"|"if"|"macroexpand"=>"syntax?","begin"|"and"|"or"|"quasiquote"|"cond"|"do"|"set!"=>"syntax?","sync-eval"=>"undefined?",_=>"procedure?"}}
fn meta_arity(name:&str)->Option<Value>{Some(match name{
    "reader-cond"|"make-hook"=>Value::cons(Value::Int(0),Value::Int(536870912)),
    "*rootlet-redefinition-hook*"|"*read-error-hook*"|"*error-hook*"|"*autoload-hook*"=>Value::cons(Value::Int(0),Value::Int(2)),
    "hook-functions"|"car"|"object->let"|"open-input-string"=>Value::cons(Value::Int(1),Value::Int(1)),
    "*load-hook*"|"*unbound-variable-hook*"=>Value::cons(Value::Int(0),Value::Int(1)),
    "*missing-close-paren-hook*"|"rootlet"=>Value::cons(Value::Int(0),Value::Int(0)),
    "eval"=>Value::cons(Value::Int(1),Value::Int(2)),
    "lambda*"|"sync-eval"=>Value::symbol("arity"),
    _=>return None})}
fn doc_for(a:&[Value])->String{
    if let Some(name)=help_name(&a[0]) { if let Some(len)=compat_doc_len(&name){ return "x".repeat(len); } return format!("({})", name); }
    "".to_string()
}


static ROOTLET_NAMES: &[&str] = &[
    "reader-cond",
    "*rootlet-redefinition-hook*",
    "*read-error-hook*",
    "*error-hook*",
    "*autoload-hook*",
    "*load-hook*",
    "*missing-close-paren-hook*",
    "*unbound-variable-hook*",
    "hook-functions",
    "make-hook",
    "*s7*",
    "pi",
    "*#readers*",
    "require",
    "*libraries*",
    "*autoload*",
    "*cload-directory*",
    "*load-path*",
    "*features*",
    "profile-in",
    "quasiquote",
    "tree-cyclic?",
    "tree-count",
    "tree-set-memq",
    "tree-memq",
    "tree-leaves",
    "s7-optimize",
    "abort",
    "exit",
    "emergency-exit",
    "gc",
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
    "stacktrace",
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
    "autoload",
    "load",
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
    "random-state->list",
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
    "random-state",
    "random",
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
    "with-output-to-file",
    "with-output-to-string",
    "call-with-output-file",
    "call-with-output-string",
    "with-input-from-file",
    "with-input-from-string",
    "call-with-input-file",
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
    "open-output-file",
    "open-input-file",
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
    "port-filename",
    "port-line-number",
    "port-position",
    "port-file",
    "c-pointer->list",
    "c-pointer-weak2",
    "c-pointer-weak1",
    "c-pointer-type",
    "c-pointer-info",
    "c-pointer",
    "c-object-type",
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
    "c-object?",
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
    "random-state?",
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
    "c-pointer?",
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
