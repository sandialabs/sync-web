//! Private scaffolding for a future structural compiled execution tier.
//!
//! This module is intentionally conservative: compiled expressions keep enough
//! fallback syntax to defer to the tree evaluator if runtime guards fail.

#![allow(dead_code)]

use std::rc::Rc;

use crate::bytecode::{BytecodeFunction, BytecodeLayout};
use crate::core::{EnvRef, Procedure, Value};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct SymId(pub(crate) u32);

#[repr(usize)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum BuiltinId {
    Add,
    Sub,
    Mul,
    NumEq,
    Less,
    LessEq,
    Greater,
    GreaterEq,
    Cons,
    Car,
    Cdr,
    NullP,
    PairP,
    NumberP,
    CharP,
    SymbolP,
    BooleanP,
    Not,
    EqP,
    EqualP,
    Caar,
    Cdar,
    Assoc,
    Memq,
    VectorRef,
    VectorSet,
    HashRef,
    HashSet,
    Remainder,
    Modulo,
    Inlet,
    HashTable,
}

impl BuiltinId {
    pub(crate) fn from_name(name:&str)->Option<Self>{
        Some(match name{
            "+"=>Self::Add,"-"=>Self::Sub,"*"=>Self::Mul,"="=>Self::NumEq,"<"=>Self::Less,"<="=>Self::LessEq,">"=>Self::Greater,">="=>Self::GreaterEq,
            "cons"=>Self::Cons,"car"=>Self::Car,"cdr"=>Self::Cdr,"null?"=>Self::NullP,"pair?"=>Self::PairP,"number?"=>Self::NumberP,"char?"=>Self::CharP,"symbol?"=>Self::SymbolP,"boolean?"=>Self::BooleanP,"not"=>Self::Not,"eq?"=>Self::EqP,"equal?"=>Self::EqualP,"caar"=>Self::Caar,"cdar"=>Self::Cdar,"assoc"=>Self::Assoc,"memq"=>Self::Memq,
            "vector-ref"=>Self::VectorRef,"vector-set!"=>Self::VectorSet,"hash-table-ref"=>Self::HashRef,"hash-table-set!"=>Self::HashSet,"remainder"=>Self::Remainder,"modulo"=>Self::Modulo,"inlet"=>Self::Inlet,"hash-table"=>Self::HashTable,
            _=>return None,
        })
    }
    pub(crate) fn name(self)->&'static str{
        match self{Self::Add=>"+",Self::Sub=>"-",Self::Mul=>"*",Self::NumEq=>"=",Self::Less=>"<",Self::LessEq=>"<=",Self::Greater=>">",Self::GreaterEq=>">=",Self::Cons=>"cons",Self::Car=>"car",Self::Cdr=>"cdr",Self::NullP=>"null?",Self::PairP=>"pair?",Self::NumberP=>"number?",Self::CharP=>"char?",Self::SymbolP=>"symbol?",Self::BooleanP=>"boolean?",Self::Not=>"not",Self::EqP=>"eq?",Self::EqualP=>"equal?",Self::Caar=>"caar",Self::Cdar=>"cdar",Self::Assoc=>"assoc",Self::Memq=>"memq",Self::VectorRef=>"vector-ref",Self::VectorSet=>"vector-set!",Self::HashRef=>"hash-table-ref",Self::HashSet=>"hash-table-set!",Self::Remainder=>"remainder",Self::Modulo=>"modulo",Self::Inlet=>"inlet",Self::HashTable=>"hash-table"}
    }
}

#[derive(Clone, Debug)]
pub(crate) struct BindingGuard {
    pub(crate) name: &'static str,
    pub(crate) generation: u64,
}

#[derive(Clone, Debug)]
pub(crate) struct ShapeGuard {
    pub(crate) generation: u64,
}

#[derive(Clone, Debug)]
pub(crate) enum VarRef {
    Dynamic { name: Rc<String> },
    Lexical { depth: usize, slot: usize, guard: ShapeGuard },
}

#[derive(Clone, Debug)]
pub(crate) struct CompiledLambda {
    pub(crate) name: Option<Rc<String>>,
    pub(crate) params: Vec<Rc<String>>,
    pub(crate) rest: Option<Rc<String>>,
    pub(crate) body: Vec<CExpr>,
}

#[derive(Clone, Debug)]
pub(crate) enum QTemplate {
    Literal(Value),
    Pair(Box<QTemplate>, Box<QTemplate>),
    Vector(Vec<QTemplate>),
    Unquote(Box<CExpr>),
    Splice(Box<CExpr>),
}

#[derive(Clone, Debug)]
pub(crate) enum CExpr {
    Const(Value),
    Var(VarRef),
    SetVar { target: VarRef, expr: Box<CExpr> },
    If { test: Box<CExpr>, conseq: Box<CExpr>, alt: Box<CExpr> },
    Begin(Vec<CExpr>),
    Let { sequential: bool, bindings: Vec<(Rc<String>, CExpr)>, body: Vec<CExpr> },
    Cond { clauses: Vec<(CExpr, Vec<CExpr>)>, else_body: Option<Vec<CExpr>> },
    Case { key: Box<CExpr>, clauses: Vec<(Vec<Value>, Vec<CExpr>)>, else_body: Option<Vec<CExpr>> },
    And(Vec<CExpr>),
    Or(Vec<CExpr>),
    Quasiquote(Box<Value>),
    QuasiquoteTemplate(Box<QTemplate>),
    Loop { name: Rc<String>, params: Vec<Rc<String>>, inits: Vec<CExpr>, body: Box<CExpr> },
    Recur { args: Vec<CExpr> },
    Lambda(CompiledLambda),
    Call { op: Box<CExpr>, args: Vec<CExpr>, tail: bool, fallback: Box<Value> },
    BuiltinCall { id: BuiltinId, name: &'static str, args: Vec<CExpr>, tail: bool, fallback: Box<Value> },
    ApplicableRef { target: Box<CExpr>, index: Box<CExpr> },
    SetApplicable { target: Box<CExpr>, index: Box<CExpr>, value: Box<CExpr> },
    Fallback(Value),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum CompiledLayout {
    DynamicEnv,
    SlotFrame { params: Vec<Rc<String>> },
}

#[derive(Clone)]
pub struct CompiledBody {
    pub(crate) env: EnvRef,
    pub(crate) exprs: Vec<CExpr>,
    pub(crate) layout: CompiledLayout,
    pub(crate) bytecode: Option<Rc<BytecodeFunction>>,
    pub(crate) capture_values: Vec<Value>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CompileDecision {
    Compiled,
    Fallback,
}

pub(crate) fn analyze_body(env: EnvRef, exprs: &[Value]) -> Option<CompiledBody> {
    let mut out=Vec::with_capacity(exprs.len());
    for expr in exprs { out.push(analyze_expr(expr)?); }
    let bytecode=BytecodeFunction::from_exprs(BytecodeLayout::DynamicEnv,&out).map(Rc::new);
    Some(CompiledBody{env,exprs:out,layout:CompiledLayout::DynamicEnv,bytecode,capture_values:Vec::new()})
}

pub(crate) fn analyze_body_with_params(env:EnvRef, exprs:&[Value], params:&[String])->Option<CompiledBody>{
    let param_rc=params.iter().map(|p|Rc::new(p.clone())).collect::<Vec<_>>();
    let mut raw=Vec::with_capacity(exprs.len());
    for expr in exprs { raw.push(analyze_expr_inner(expr,None,false,Some(&env))?); }
    let captures=collect_stable_captures(&raw,&env,&param_rc);
    let capture_values=captures.iter().map(|n|env.get(n.as_str())).collect::<Option<Vec<_>>>()?;
    let mut slot_names=param_rc.clone();
    slot_names.extend(captures);
    let out=raw.into_iter().map(|expr|slot_loop_vars(expr,&slot_names)).collect::<Option<Vec<_>>>()?;
    let bytecode=BytecodeFunction::from_exprs(BytecodeLayout::SlotFrame{names:slot_names.clone()},&out).map(Rc::new);
    Some(CompiledBody{env,exprs:out,layout:CompiledLayout::SlotFrame{params:slot_names},bytecode,capture_values})
}

pub(crate) fn analyze_named_let(env:EnvRef, name:&str, params:Vec<Rc<String>>, inits:&[Value], body:&[Value])->Option<CompiledBody>{
    if params.is_empty() || inits.len()!=params.len() || body.is_empty(){return None;}
    let mut init_exprs=Vec::with_capacity(inits.len());
    for init in inits{init_exprs.push(analyze_expr_no_loop_env(init,name,&env)?);}
    let raw_body=analyze_body_tail_env(body,name,&env)?;
    let capture_names=if capture_loop_safe(&raw_body){collect_scalar_captures(&raw_body,&env,&params)}else{Vec::new()};
    let exprs=if capture_names.is_empty(){
        let body_expr=slot_loop_vars(raw_body,&params)?;
        vec![CExpr::Loop{name:Rc::new(name.to_string()),params,inits:init_exprs,body:Box::new(body_expr)}]
    }else{
        let mut scope=capture_names.clone();
        scope.extend(params.iter().cloned());
        let body_expr=slot_loop_vars(raw_body,&scope)?;
        let bindings=capture_names.iter().map(|n|env.get(n.as_str()).and_then(scalar_capture_value).map(|v|(n.clone(),CExpr::Const(v)))).collect::<Option<Vec<_>>>()?;
        vec![CExpr::Let{sequential:false,bindings,body:vec![CExpr::Loop{name:Rc::new(name.to_string()),params,inits:init_exprs,body:Box::new(body_expr)}]}]
    };
    let bytecode=BytecodeFunction::from_exprs(BytecodeLayout::DynamicEnv,&exprs).map(Rc::new);
    Some(CompiledBody{env,exprs,layout:CompiledLayout::DynamicEnv,bytecode,capture_values:Vec::new()})
}

fn scalar_capture_value(v:Value)->Option<Value>{
    match v{Value::Bool(_)|Value::Nil|Value::Int(_)|Value::Rational(_,_)|Value::Float(_)|Value::Complex(_,_)|Value::NumberLiteral(_,_)|Value::Char(_)|Value::NamedChar(_)|Value::Keyword(_)=>Some(v),_=>None}
}
fn capture_loop_safe(expr:&CExpr)->bool{
    match expr{
        CExpr::Const(_)|CExpr::Var(_)|CExpr::Quasiquote(_)|CExpr::QuasiquoteTemplate(_)=>true,
        CExpr::If{test,conseq,alt}=>capture_loop_safe(test)&&capture_loop_safe(conseq)&&capture_loop_safe(alt),
        CExpr::Begin(xs)|CExpr::And(xs)|CExpr::Or(xs)=>xs.iter().all(capture_loop_safe),
        CExpr::Recur{args}|CExpr::BuiltinCall{args,..}=>args.iter().all(capture_loop_safe),
        CExpr::Let{bindings,body,..}=>bindings.iter().all(|(_,x)|capture_loop_safe(x))&&body.iter().all(capture_loop_safe),
        CExpr::Cond{clauses,else_body}=>clauses.iter().all(|(t,b)|capture_loop_safe(t)&&b.iter().all(capture_loop_safe))&&else_body.as_ref().map(|b|b.iter().all(capture_loop_safe)).unwrap_or(true),
        CExpr::Case{key,clauses,else_body}=>capture_loop_safe(key)&&clauses.iter().all(|(_,b)|b.iter().all(capture_loop_safe))&&else_body.as_ref().map(|b|b.iter().all(capture_loop_safe)).unwrap_or(true),
        CExpr::Loop{..}|CExpr::SetVar{..}|CExpr::Lambda(_)|CExpr::Call{..}|CExpr::ApplicableRef{..}|CExpr::SetApplicable{..}|CExpr::Fallback(_)=>false,
    }
}
fn collect_dynamic_names(expr:&CExpr,out:&mut Vec<Rc<String>>){
    match expr{
        CExpr::Var(VarRef::Dynamic{name})=>{if !out.iter().any(|n|n.as_str()==name.as_str()){out.push(name.clone());}}
        CExpr::If{test,conseq,alt}=>{collect_dynamic_names(test,out); collect_dynamic_names(conseq,out); collect_dynamic_names(alt,out);}
        CExpr::Begin(xs)|CExpr::And(xs)|CExpr::Or(xs)=>for x in xs{collect_dynamic_names(x,out)},
        CExpr::Quasiquote(_)=>{},
        CExpr::QuasiquoteTemplate(t)=>collect_dynamic_names_template(t,out),
        CExpr::Recur{args}|CExpr::BuiltinCall{args,..}=>for x in args{collect_dynamic_names(x,out)},
        CExpr::Let{bindings,body,..}=>{for (_,x) in bindings{collect_dynamic_names(x,out);} for x in body{collect_dynamic_names(x,out);}}
        CExpr::Cond{clauses,else_body}=>{for (t,b) in clauses{collect_dynamic_names(t,out); for x in b{collect_dynamic_names(x,out);}} if let Some(b)=else_body{for x in b{collect_dynamic_names(x,out);}}}
        CExpr::Case{key,clauses,else_body}=>{collect_dynamic_names(key,out); for (_,b) in clauses{for x in b{collect_dynamic_names(x,out);}} if let Some(b)=else_body{for x in b{collect_dynamic_names(x,out);}}}
        _=>{}
    }
}
fn collect_dynamic_names_template(t:&QTemplate,out:&mut Vec<Rc<String>>){match t{QTemplate::Literal(_)=>{},QTemplate::Unquote(e)|QTemplate::Splice(e)=>collect_dynamic_names(e,out),QTemplate::Pair(a,b)=>{collect_dynamic_names_template(a,out); collect_dynamic_names_template(b,out);},QTemplate::Vector(xs)=>for x in xs{collect_dynamic_names_template(x,out)}}}
fn template_env_observing(t:&QTemplate)->bool{match t{QTemplate::Literal(_)=>false,QTemplate::Unquote(e)|QTemplate::Splice(e)=>capture_env_observing(e),QTemplate::Pair(a,b)=>template_env_observing(a)||template_env_observing(b),QTemplate::Vector(xs)=>xs.iter().any(template_env_observing)}}
fn dynamic_set_names_template(t:&QTemplate,out:&mut Vec<Rc<String>>){match t{QTemplate::Literal(_)=>{},QTemplate::Unquote(e)|QTemplate::Splice(e)=>dynamic_set_names(e,out),QTemplate::Pair(a,b)=>{dynamic_set_names_template(a,out); dynamic_set_names_template(b,out);},QTemplate::Vector(xs)=>for x in xs{dynamic_set_names_template(x,out)}}}
fn slot_template(t:QTemplate, params:&[Rc<String>])->Option<QTemplate>{Some(match t{QTemplate::Literal(v)=>QTemplate::Literal(v),QTemplate::Unquote(e)=>QTemplate::Unquote(Box::new(slot_loop_vars(*e,params)?)),QTemplate::Splice(e)=>QTemplate::Splice(Box::new(slot_loop_vars(*e,params)?)),QTemplate::Pair(a,b)=>QTemplate::Pair(Box::new(slot_template(*a,params)?),Box::new(slot_template(*b,params)?)),QTemplate::Vector(xs)=>QTemplate::Vector(xs.into_iter().map(|x|slot_template(x,params)).collect::<Option<Vec<_>>>()?)})}

fn collect_scalar_captures(expr:&CExpr, env:&EnvRef, params:&[Rc<String>])->Vec<Rc<String>>{
    let mut names=Vec::new();
    collect_dynamic_names(expr,&mut names);
    names.into_iter().filter(|n|!params.iter().any(|p|p.as_str()==n.as_str()) && env.get(n.as_str()).and_then(scalar_capture_value).is_some()).collect()
}
fn stable_capture_value(v:&Value)->bool{
    matches!(v,Value::String(_)|Value::Vector(_)|Value::ByteVector(_)|Value::FloatVector(_)|Value::IntVector(_)|Value::HashTable(_)|Value::Pair(_)|Value::Env(_))
}
fn capture_env_observing(expr:&CExpr)->bool{
    match expr{
        CExpr::Var(VarRef::Dynamic{name})=>matches!(name.as_str(),"curlet"|"funclet"|"eval"|"eval-string"|"with-let"|"let-ref"|"let-set!"|"varlet"|"sublet"|"outlet"|"rootlet"),
        CExpr::If{test,conseq,alt}=>capture_env_observing(test)||capture_env_observing(conseq)||capture_env_observing(alt),
        CExpr::Begin(xs)|CExpr::And(xs)|CExpr::Or(xs)=>xs.iter().any(capture_env_observing),
        CExpr::Recur{args}|CExpr::BuiltinCall{args,..}=>args.iter().any(capture_env_observing),
        CExpr::Let{bindings,body,..}=>bindings.iter().any(|(_,x)|capture_env_observing(x))||body.iter().any(capture_env_observing),
        CExpr::Cond{clauses,else_body}=>clauses.iter().any(|(t,b)|capture_env_observing(t)||b.iter().any(capture_env_observing))||else_body.as_ref().map(|b|b.iter().any(capture_env_observing)).unwrap_or(false),
        CExpr::Case{key,clauses,else_body}=>capture_env_observing(key)||clauses.iter().any(|(_,b)|b.iter().any(capture_env_observing))||else_body.as_ref().map(|b|b.iter().any(capture_env_observing)).unwrap_or(false),
        CExpr::Loop{inits,body,..}=>inits.iter().any(capture_env_observing)||capture_env_observing(body),
        CExpr::Call{op,args,..}=>capture_env_observing(op)||args.iter().any(capture_env_observing),
        CExpr::ApplicableRef{target,index}=>capture_env_observing(target)||capture_env_observing(index),
        CExpr::SetApplicable{target,index,value}=>capture_env_observing(target)||capture_env_observing(index)||capture_env_observing(value),
        CExpr::SetVar{expr,..}=>capture_env_observing(expr),
        CExpr::Const(_)|CExpr::Var(_)|CExpr::Quasiquote(_)|CExpr::Lambda(_)|CExpr::Fallback(_)=>false,
        CExpr::QuasiquoteTemplate(t)=>template_env_observing(t),
    }
}
fn dynamic_set_names(expr:&CExpr,out:&mut Vec<Rc<String>>){
    match expr{
        CExpr::SetVar{target:VarRef::Dynamic{name},expr}=>{if !out.iter().any(|n|n.as_str()==name.as_str()){out.push(name.clone());} dynamic_set_names(expr,out);}
        CExpr::SetVar{expr,..}=>dynamic_set_names(expr,out),
        CExpr::If{test,conseq,alt}=>{dynamic_set_names(test,out); dynamic_set_names(conseq,out); dynamic_set_names(alt,out);}
        CExpr::Begin(xs)|CExpr::And(xs)|CExpr::Or(xs)=>for x in xs{dynamic_set_names(x,out)},
        CExpr::Recur{args}|CExpr::BuiltinCall{args,..}=>for x in args{dynamic_set_names(x,out)},
        CExpr::Let{bindings,body,..}=>{for (_,x) in bindings{dynamic_set_names(x,out);} for x in body{dynamic_set_names(x,out);}}
        CExpr::Cond{clauses,else_body}=>{for (t,b) in clauses{dynamic_set_names(t,out); for x in b{dynamic_set_names(x,out);}} if let Some(b)=else_body{for x in b{dynamic_set_names(x,out);}}}
        CExpr::Case{key,clauses,else_body}=>{dynamic_set_names(key,out); for (_,b) in clauses{for x in b{dynamic_set_names(x,out);}} if let Some(b)=else_body{for x in b{dynamic_set_names(x,out);}}}
        CExpr::Loop{inits,body,..}=>{for x in inits{dynamic_set_names(x,out);} dynamic_set_names(body,out);}
        CExpr::Call{op,args,..}=>{dynamic_set_names(op,out); for x in args{dynamic_set_names(x,out);}}
        CExpr::ApplicableRef{target,index}=>{dynamic_set_names(target,out); dynamic_set_names(index,out);}
        CExpr::SetApplicable{target,index,value}=>{dynamic_set_names(target,out); dynamic_set_names(index,out); dynamic_set_names(value,out);}
        CExpr::Const(_)|CExpr::Var(_)|CExpr::Quasiquote(_)|CExpr::Lambda(_)|CExpr::Fallback(_)=>{}
        CExpr::QuasiquoteTemplate(t)=>dynamic_set_names_template(t,out),
    }
}
fn collect_stable_captures(exprs:&[CExpr], env:&EnvRef, params:&[Rc<String>])->Vec<Rc<String>>{
    if exprs.iter().any(capture_env_observing){return Vec::new();}
    let mut names=Vec::new(); let mut sets=Vec::new();
    for expr in exprs{collect_dynamic_names(expr,&mut names); dynamic_set_names(expr,&mut sets);}
    names.into_iter().filter(|n|{
        if params.iter().any(|p|p.as_str()==n.as_str()) || sets.iter().any(|s|s.as_str()==n.as_str()){return false;}
        let uses=exprs.iter().filter(|expr|{let mut tmp=Vec::new(); collect_dynamic_names(expr,&mut tmp); tmp.iter().any(|x|x.as_str()==n.as_str())}).count();
        uses>1 && env.get(n.as_str()).map(|v|stable_capture_value(&v)).unwrap_or(false)
    }).collect()
}

fn slot_loop_vars(expr:CExpr, params:&[Rc<String>])->Option<CExpr>{
    match expr{
        CExpr::Var(VarRef::Dynamic{name})=>{
            if let Some(slot)=params.iter().rposition(|p|p.as_str()==name.as_str()){
                Some(CExpr::Var(VarRef::Lexical{depth:0,slot,guard:ShapeGuard{generation:0}}))
            }else{Some(CExpr::Var(VarRef::Dynamic{name}))}
        }
        CExpr::If{test,conseq,alt}=>Some(CExpr::If{test:Box::new(slot_loop_vars(*test,params)?),conseq:Box::new(slot_loop_vars(*conseq,params)?),alt:Box::new(slot_loop_vars(*alt,params)?)}),
        CExpr::Begin(xs)=>Some(CExpr::Begin(xs.into_iter().map(|x|slot_loop_vars(x,params)).collect::<Option<Vec<_>>>()?)),
        CExpr::Quasiquote(v)=>Some(CExpr::Quasiquote(v)),
        CExpr::QuasiquoteTemplate(t)=>Some(CExpr::QuasiquoteTemplate(Box::new(slot_template(*t,params)?))),
        CExpr::Recur{args}=>Some(CExpr::Recur{args:args.into_iter().map(|x|slot_loop_vars(x,params)).collect::<Option<Vec<_>>>()?}),
        CExpr::BuiltinCall{id,name,args,tail,fallback}=>Some(CExpr::BuiltinCall{id,name,args:args.into_iter().map(|x|slot_loop_vars(x,params)).collect::<Option<Vec<_>>>()?,tail,fallback}),
        CExpr::Call{op,args,tail,fallback}=>Some(CExpr::Call{op:Box::new(slot_loop_vars(*op,params)?),args:args.into_iter().map(|x|slot_loop_vars(x,params)).collect::<Option<Vec<_>>>()?,tail,fallback}),
        CExpr::ApplicableRef{target,index}=>Some(CExpr::ApplicableRef{target:Box::new(slot_loop_vars(*target,params)?),index:Box::new(slot_loop_vars(*index,params)?)}),
        CExpr::SetApplicable{target,index,value}=>Some(CExpr::SetApplicable{target:Box::new(slot_loop_vars(*target,params)?),index:Box::new(slot_loop_vars(*index,params)?),value:Box::new(slot_loop_vars(*value,params)?)}),
        CExpr::Let{sequential,bindings,body}=>{
            let names=bindings.iter().map(|(n,_)|n.clone()).collect::<Vec<_>>();
            let mut init_scope=params.to_vec();
            let mut out_bindings=Vec::with_capacity(bindings.len());
            for (name,init) in bindings{
                out_bindings.push((name.clone(),slot_loop_vars(init,&init_scope)?));
                if sequential{init_scope.push(name);}
            }
            let mut body_scope=params.to_vec();
            body_scope.extend(names);
            Some(CExpr::Let{sequential,bindings:out_bindings,body:body.into_iter().map(|x|slot_loop_vars(x,&body_scope)).collect::<Option<Vec<_>>>()?})
        },
        CExpr::Cond{clauses,else_body}=>Some(CExpr::Cond{
            clauses:clauses.into_iter().map(|(t,b)|Some((slot_loop_vars(t,params)?,b.into_iter().map(|x|slot_loop_vars(x,params)).collect::<Option<Vec<_>>>()?))).collect::<Option<Vec<_>>>()?,
            else_body:match else_body{Some(b)=>Some(b.into_iter().map(|x|slot_loop_vars(x,params)).collect::<Option<Vec<_>>>()?),None=>None},
        }),
        CExpr::Case{key,clauses,else_body}=>Some(CExpr::Case{
            key:Box::new(slot_loop_vars(*key,params)?),
            clauses:clauses.into_iter().map(|(d,b)|Some((d,b.into_iter().map(|x|slot_loop_vars(x,params)).collect::<Option<Vec<_>>>()?))).collect::<Option<Vec<_>>>()?,
            else_body:match else_body{Some(b)=>Some(b.into_iter().map(|x|slot_loop_vars(x,params)).collect::<Option<Vec<_>>>()?),None=>None},
        }),
        CExpr::And(xs)=>Some(CExpr::And(xs.into_iter().map(|x|slot_loop_vars(x,params)).collect::<Option<Vec<_>>>()?)),
        CExpr::Or(xs)=>Some(CExpr::Or(xs.into_iter().map(|x|slot_loop_vars(x,params)).collect::<Option<Vec<_>>>()?)),
        other=>Some(other),
    }
}

fn analyze_expr(expr:&Value)->Option<CExpr>{ analyze_expr_inner(expr,None,false,None) }
fn analyze_expr_no_loop(expr:&Value, loop_name:&str)->Option<CExpr>{ analyze_expr_inner(expr,Some(loop_name),false,None) }
fn analyze_expr_tail(expr:&Value, loop_name:&str)->Option<CExpr>{ analyze_expr_inner(expr,Some(loop_name),true,None) }
fn analyze_expr_no_loop_env(expr:&Value, loop_name:&str, env:&EnvRef)->Option<CExpr>{ analyze_expr_inner(expr,Some(loop_name),false,Some(env)) }
fn analyze_expr_tail_env(expr:&Value, loop_name:&str, env:&EnvRef)->Option<CExpr>{ analyze_expr_inner(expr,Some(loop_name),true,Some(env)) }

fn analyze_body_tail_env(body:&[Value], loop_name:&str, env:&EnvRef)->Option<CExpr>{
    if body.is_empty(){return None;}
    if body.len()==1{return analyze_expr_tail_env(&body[0],loop_name,env);}
    let mut xs=Vec::with_capacity(body.len());
    for expr in &body[..body.len()-1]{xs.push(analyze_expr_no_loop_env(expr,loop_name,env)?);}
    xs.push(analyze_expr_tail_env(&body[body.len()-1],loop_name,env)?);
    Some(CExpr::Begin(xs))
}

fn const_expr(expr:&Value)->Option<CExpr>{
    match expr {
        Value::Bool(_)|Value::Nil|Value::Unspecified|Value::Undefined|Value::Eof|
        Value::Int(_)|Value::Rational(_,_)|Value::Float(_)|Value::Complex(_,_)|Value::NumberLiteral(_,_)|
        Value::Char(_)|Value::NamedChar(_)|Value::String(_)|Value::Keyword(_)|Value::Vector(_)|
        Value::ByteVector(_)|Value::FloatVector(_)|Value::IntVector(_)|Value::MultiVector{..}|
        Value::MultiVectorView{..}|Value::HashTable(_)|Value::Env(_)|Value::Procedure(_)|
        Value::Macro(_,_)|Value::Port(_)|Value::Hook(_,_)|Value::Iterator{..}|Value::CPointer(_) |
        Value::Dilambda(_)|Value::Values(_)|Value::Commented(_)|Value::SetterRef(_)|Value::RootMeta(_) |
        Value::RawDisplay(_)|Value::ProcedureSource(_)=>Some(CExpr::Const(expr.clone())),
        _=>None,
    }
}

fn analyze_expr_inner(expr:&Value, loop_name:Option<&str>, tail:bool, known_env:Option<&EnvRef>)->Option<CExpr>{
    if let Some(c)=const_expr(expr){return Some(c);}
    match expr {
        Value::Symbol(s)=>{
            if loop_name==Some(s.as_str()){return None;}
            if s.as_str()=="values"{return None;}
            Some(CExpr::Var(VarRef::Dynamic{name:s.clone()}))
        }
        Value::Pair(_)=>analyze_pair(expr,loop_name,tail,known_env),
        _=>None,
    }
}

fn is_known_applicable(env:&EnvRef, name:&str)->bool{
    matches!(env.get(name),Some(Value::Vector(_)|Value::ByteVector(_)|Value::FloatVector(_)|Value::IntVector(_)|Value::String(_)|Value::HashTable(_)|Value::Env(_)|Value::Pair(_)|Value::Nil))
}

fn observes_current_env(name:&str)->bool{
    matches!(name,
        "curlet"|"funclet"|"eval"|"eval-string"|"with-let"|"varlet"|"let-set!"|"let-ref"|"outlet"|"rootlet"|"sublet"|"inlet"|"openlet"|"coverlet"|"immutable!"
    )
}

fn safe_known_apply_target(v:&Value, known_env:Option<&EnvRef>)->bool{
    let Some(name)=v.as_symbol() else{return false;};
    if crate::core::is_syntax_name(name) || observes_current_env(name){return false;}
    known_env.and_then(|env|env.get(name)).map(|v|matches!(v,Value::Procedure(_))).unwrap_or(false)
}
fn compile_quasiquote_template(v:&Value, loop_name:Option<&str>, known_env:Option<&EnvRef>)->Option<QTemplate>{
    match v{
        Value::Pair(_)=>{
            let car=v.car().ok()?;
            let cdr=v.cdr().ok()?;
            if car.as_symbol()==Some("unquote"){
                let xs=cdr.to_vec().ok()?;
                if xs.len()!=1{return None;}
                let e=if let Some(name)=loop_name{if let Some(env)=known_env{analyze_expr_no_loop_env(&xs[0],name,env)?}else{analyze_expr_no_loop(&xs[0],name)?}}else{analyze_expr(&xs[0])?};
                return Some(QTemplate::Unquote(Box::new(e)));
            }
            if car.as_symbol()==Some("unquote-splicing"){return None;}
            Some(QTemplate::Pair(Box::new(compile_quasiquote_template(&car,loop_name,known_env)?),Box::new(compile_quasiquote_template(&cdr,loop_name,known_env)?)))
        }
        Value::Vector(vec)=>{
            let mut out=Vec::new();
            for x in vec.values(){
                if matches!(x,Value::Pair(_)) && x.car().ok().and_then(|h|h.as_symbol().map(|s|s=="unquote-splicing"||(loop_name.is_some()&&s=="unquote"))).unwrap_or(false){return None;}
                out.push(compile_quasiquote_template(&x,loop_name,known_env)?);
            }
            Some(QTemplate::Vector(out))
        }
        _=>Some(QTemplate::Literal(v.clone())),
    }
}

fn env_builtin(env:&EnvRef,name:&str)->bool{
    matches!(env.get(name),Some(Value::Procedure(p)) if matches!(&*p,Procedure::Builtin{name:n,..} if *n==name))
}

fn analyze_pair(expr:&Value, loop_name:Option<&str>, tail:bool, known_env:Option<&EnvRef>)->Option<CExpr>{
    let op=expr.car().ok()?;
    let args=expr.cdr().ok()?;
    if let Some(sym)=op.as_symbol(){
        if sym=="eval"{
            let xs=args.to_vec().ok()?;
            if xs.len()==1{
                if let (Some(env),Value::Pair(_))=(known_env,&xs[0]){
                    let list_xs=xs[0].to_vec().ok()?;
                    if list_xs.len()>=2 && list_xs[0].as_symbol()==Some("list") && env_builtin(env,"eval") && env_builtin(env,"list"){
                        let quoted=&list_xs[1];
                        if let Value::Pair(_)=quoted{let q=quoted.to_vec().ok()?; if q.len()==2 && q[0].as_symbol()==Some("quote"){ if let Some(target)=q[1].as_symbol(){ if env_builtin(env,target){ if let Some(id)=BuiltinId::from_name(target){ let mut out=Vec::with_capacity(list_xs.len()-2); for x in &list_xs[2..]{out.push(if let Some(name)=loop_name{analyze_expr_no_loop_env(x,name,env)?}else{analyze_expr_inner(x,None,false,Some(env))?});} return Some(CExpr::BuiltinCall{id,name:id.name(),args:out,tail:false,fallback:Box::new(expr.clone())}); } } }}}
                    }
                }
            }
        }
        if sym=="apply"{
            let xs=args.to_vec().ok()?;
            if xs.is_empty() || !safe_known_apply_target(&xs[0],known_env){return None;}
            if xs.len()==2{
                if let (Some(env),Value::Pair(_))=(known_env,&xs[1]){
                    if env_builtin(env,"apply") && env_builtin(env,"list") && xs[1].car().ok().and_then(|v|v.as_symbol().map(|s|s=="list")).unwrap_or(false){
                        let raw=xs[1].cdr().ok()?.to_vec().ok()?;
                        let op=if let Some(name)=loop_name{analyze_expr_no_loop_env(&xs[0],name,env)?}else{analyze_expr(&xs[0])?};
                        let mut out=Vec::with_capacity(raw.len());
                        for x in raw{out.push(if let Some(name)=loop_name{analyze_expr_no_loop_env(&x,name,env)?}else{analyze_expr(&x)?});}
                        return Some(CExpr::Call{op:Box::new(op),args:out,tail:false,fallback:Box::new(expr.clone())});
                    }
                }
            }
        }
        if Some(sym)==loop_name{
            if !tail{return None;}
            let xs=args.to_vec().ok()?;
            let mut out=Vec::with_capacity(xs.len());
            for x in xs{out.push(if let Some(env)=known_env{analyze_expr_no_loop_env(&x,sym,env)?}else{analyze_expr_no_loop(&x,sym)?});}
            return Some(CExpr::Recur{args:out});
        }
        match sym {
            "quote"=>{
                let xs=args.to_vec().ok()?;
                return if xs.len()==1{Some(CExpr::Const(xs[0].clone()))}else{None};
            }
            "quasiquote"=>{
                let xs=args.to_vec().ok()?;
                if xs.len()!=1{return None;}
                if let Some(t)=compile_quasiquote_template(&xs[0],loop_name,known_env){return Some(CExpr::QuasiquoteTemplate(Box::new(t)));}
                return Some(CExpr::Quasiquote(Box::new(xs[0].clone())));
            }
            "if"=>{
                let test_expr=args.car().ok()?;
                let rest=args.cdr().ok()?;
                let then_expr=rest.car().ok()?;
                let alt_expr=match rest.cdr().ok()?{Value::Pair(p)=>{let crate::core::PairData{car,..}= &*p.borrow(); car.clone()},Value::Nil=>Value::Unspecified,_=>return None};
                let test=if let Some(name)=loop_name{if let Some(env)=known_env{analyze_expr_no_loop_env(&test_expr,name,env)?}else{analyze_expr_no_loop(&test_expr,name)?}}else{analyze_expr(&test_expr)?};
                let conseq=if let Some(name)=loop_name{if let Some(env)=known_env{if tail{analyze_expr_tail_env(&then_expr,name,env)?}else{analyze_expr_no_loop_env(&then_expr,name,env)?}}else if tail{analyze_expr_tail(&then_expr,name)?}else{analyze_expr_no_loop(&then_expr,name)?}}else{analyze_expr(&then_expr)?};
                let alt=if let Some(name)=loop_name{if let Some(env)=known_env{if tail{analyze_expr_tail_env(&alt_expr,name,env)?}else{analyze_expr_no_loop_env(&alt_expr,name,env)?}}else if tail{analyze_expr_tail(&alt_expr,name)?}else{analyze_expr_no_loop(&alt_expr,name)?}}else{analyze_expr(&alt_expr)?};
                return Some(CExpr::If{test:Box::new(test),conseq:Box::new(conseq),alt:Box::new(alt)});
            }
            "begin"=>{
                let xs=args.to_vec().ok()?;
                if xs.is_empty(){return None;}
                let mut out=Vec::with_capacity(xs.len());
                for (i,x) in xs.iter().enumerate(){
                    let tail_pos=tail && i+1==xs.len();
                    out.push(if let Some(name)=loop_name{if let Some(env)=known_env{if tail_pos{analyze_expr_tail_env(x,name,env)?}else{analyze_expr_no_loop_env(x,name,env)?}}else if tail_pos{analyze_expr_tail(x,name)?}else{analyze_expr_no_loop(x,name)?}}else{analyze_expr(x)?});
                }
                return Some(CExpr::Begin(out));
            }
            "and"|"or"=>{
                let xs=args.to_vec().ok()?;
                let mut out=Vec::with_capacity(xs.len());
                for (i,x) in xs.iter().enumerate(){let tail_pos=tail && i+1==xs.len(); out.push(if let Some(name)=loop_name{if let Some(env)=known_env{if tail_pos{analyze_expr_tail_env(x,name,env)?}else{analyze_expr_no_loop_env(x,name,env)?}}else if tail_pos{analyze_expr_tail(x,name)?}else{analyze_expr_no_loop(x,name)?}}else{analyze_expr(x)?});}
                return if sym=="and"{Some(CExpr::And(out))}else{Some(CExpr::Or(out))};
            }
            "cond"=>{
                let raw_clauses=args.to_vec().ok()?;
                if raw_clauses.is_empty(){return None;}
                let mut clauses=Vec::new();
                let mut else_body=None;
                for (ci,clause) in raw_clauses.iter().enumerate(){
                    let xs=clause.to_vec().ok()?;
                    if xs.len()<2 || xs.iter().any(|x|x.as_symbol()==Some("=>")){return None;}
                    let is_last=ci+1==raw_clauses.len();
                    if xs[0].as_symbol()==Some("else"){
                        if !is_last{return None;}
                        let mut body=Vec::with_capacity(xs.len()-1);
                        for (i,x) in xs[1..].iter().enumerate(){let tail_pos=tail && i+1==xs.len()-1; body.push(if let Some(name)=loop_name{if let Some(env)=known_env{if tail_pos{analyze_expr_tail_env(x,name,env)?}else{analyze_expr_no_loop_env(x,name,env)?}}else if tail_pos{analyze_expr_tail(x,name)?}else{analyze_expr_no_loop(x,name)?}}else{analyze_expr(x)?});}
                        else_body=Some(body);
                    }else{
                        let test=if let Some(name)=loop_name{if let Some(env)=known_env{analyze_expr_no_loop_env(&xs[0],name,env)?}else{analyze_expr_no_loop(&xs[0],name)?}}else{analyze_expr(&xs[0])?};
                        let mut body=Vec::with_capacity(xs.len()-1);
                        for (i,x) in xs[1..].iter().enumerate(){let tail_pos=tail && i+1==xs.len()-1; body.push(if let Some(name)=loop_name{if let Some(env)=known_env{if tail_pos{analyze_expr_tail_env(x,name,env)?}else{analyze_expr_no_loop_env(x,name,env)?}}else if tail_pos{analyze_expr_tail(x,name)?}else{analyze_expr_no_loop(x,name)?}}else{analyze_expr(x)?});}
                        clauses.push((test,body));
                    }
                }
                return Some(CExpr::Cond{clauses,else_body});
            }
            "case"=>{
                let xs=args.to_vec().ok()?;
                if xs.len()<2{return None;}
                let key=if let Some(name)=loop_name{if let Some(env)=known_env{analyze_expr_no_loop_env(&xs[0],name,env)?}else{analyze_expr_no_loop(&xs[0],name)?}}else{analyze_expr(&xs[0])?};
                let mut clauses=Vec::new();
                let mut else_body=None;
                for (ci,clause) in xs[1..].iter().enumerate(){
                    let cs=clause.to_vec().ok()?;
                    if cs.is_empty(){return None;}
                    let is_last=ci+2==xs.len();
                    if cs[0].as_symbol()==Some("else"){
                        if !is_last{return None;}
                        let mut body=Vec::with_capacity(cs.len()-1);
                        for (i,x) in cs[1..].iter().enumerate(){let tail_pos=tail && i+1==cs.len()-1; body.push(if let Some(name)=loop_name{if let Some(env)=known_env{if tail_pos{analyze_expr_tail_env(x,name,env)?}else{analyze_expr_no_loop_env(x,name,env)?}}else if tail_pos{analyze_expr_tail(x,name)?}else{analyze_expr_no_loop(x,name)?}}else{analyze_expr(x)?});}
                        else_body=Some(body);
                    }else{
                        let datums=cs[0].to_vec().ok()?;
                        let mut body=Vec::with_capacity(cs.len()-1);
                        for (i,x) in cs[1..].iter().enumerate(){let tail_pos=tail && i+1==cs.len()-1; body.push(if let Some(name)=loop_name{if let Some(env)=known_env{if tail_pos{analyze_expr_tail_env(x,name,env)?}else{analyze_expr_no_loop_env(x,name,env)?}}else if tail_pos{analyze_expr_tail(x,name)?}else{analyze_expr_no_loop(x,name)?}}else{analyze_expr(x)?});}
                        clauses.push((datums,body));
                    }
                }
                return Some(CExpr::Case{key:Box::new(key),clauses,else_body});
            }
            "let"|"let*"=>{
                let xs=args.to_vec().ok()?;
                if xs.len()<2{return None;}
                if matches!(xs[0],Value::Symbol(_)){return None;}
                let raw_bindings=xs[0].to_vec().ok()?;
                let mut bindings=Vec::with_capacity(raw_bindings.len());
                let mut seen=Vec::new();
                for b in raw_bindings{
                    let bv=b.to_vec().ok()?;
                    if bv.len()!=2{return None;}
                    let Value::Symbol(name)=&bv[0] else {return None;};
                    if BuiltinId::from_name(name.as_str()).is_some(){return None;}
                    if bv[1].as_symbol().map(crate::core::is_syntax_name).unwrap_or(false){return None;}
                    if sym!="let*" && seen.iter().any(|n: &Rc<String>| n.as_str()==name.as_str()){return None;}
                    seen.push(name.clone());
                    let init=if let Some(loop_name)=loop_name{if let Some(env)=known_env{analyze_expr_no_loop_env(&bv[1],loop_name,env)?}else{analyze_expr_no_loop(&bv[1],loop_name)?}}else{analyze_expr(&bv[1])?};
                    bindings.push((name.clone(), init));
                }
                let mut body=Vec::with_capacity(xs.len()-1);
                for (i,x) in xs[1..].iter().enumerate(){
                    let tail_pos=tail && i+1==xs.len()-1;
                    body.push(if let Some(name)=loop_name{if let Some(env)=known_env{if tail_pos{analyze_expr_tail_env(x,name,env)?}else{analyze_expr_no_loop_env(x,name,env)?}}else if tail_pos{analyze_expr_tail(x,name)?}else{analyze_expr_no_loop(x,name)?}}else{analyze_expr(x)?});
                }
                return Some(CExpr::Let{sequential:sym=="let*",bindings,body});
            }
            "set!"=>{
                if let Some(env)=known_env{
                    let xs=args.to_vec().ok()?;
                    if xs.len()!=2{return None;}
                    if let Value::Pair(_)=&xs[0]{
                        let target=xs[0].car().ok()?;
                        let idxs=xs[0].cdr().ok()?.to_vec().ok()?;
                        if idxs.len()==1 && loop_name.map(|n|target.as_symbol()!=Some(n)).unwrap_or_else(||target.as_symbol().map(|s|is_known_applicable(env,s)).unwrap_or(false)){
                            let target_expr=if let Some(n)=loop_name{analyze_expr_no_loop_env(&target,n,env)?}else{CExpr::Var(VarRef::Dynamic{name:Rc::new(target.as_symbol()?.to_string())})};
                            let index_expr=if let Some(n)=loop_name{analyze_expr_no_loop_env(&idxs[0],n,env)?}else{analyze_expr_inner(&idxs[0],None,false,Some(env))?};
                            let value_expr=if let Some(n)=loop_name{analyze_expr_no_loop_env(&xs[1],n,env)?}else{analyze_expr_inner(&xs[1],None,false,Some(env))?};
                            return Some(CExpr::SetApplicable{target:Box::new(target_expr),index:Box::new(index_expr),value:Box::new(value_expr)});
                        }
                    }
                }
                return None;
            }
            _=>{
                if let Some(env)=known_env{
                    let xs=args.to_vec().ok()?;
                    if xs.len()==1 && is_known_applicable(env,sym){
                        let index_expr=if let Some(n)=loop_name{analyze_expr_no_loop_env(&xs[0],n,env)?}else{analyze_expr_inner(&xs[0],None,false,Some(env))?};
                        return Some(CExpr::ApplicableRef{target:Box::new(CExpr::Var(VarRef::Dynamic{name:Rc::new(sym.to_string())})),index:Box::new(index_expr)});
                    }
                }
                if let Some(id)=BuiltinId::from_name(sym){
                    if let Some(env)=known_env{env.builtin_func(sym)?;}
                    let xs=args.to_vec().ok()?;
                    let mut out=Vec::with_capacity(xs.len());
                    for x in xs { out.push(if let Some(name)=loop_name{if let Some(env)=known_env{analyze_expr_no_loop_env(&x,name,env)?}else{analyze_expr_no_loop(&x,name)?}}else{analyze_expr(&x)?}); }
                    return Some(CExpr::BuiltinCall{id,name:id.name(),args:out,tail:false,fallback:Box::new(expr.clone())});
                }
                if !crate::core::is_syntax_name(sym){
                    if observes_current_env(sym){return None;}
                    let xs=args.to_vec().ok()?;
                    let mut out=Vec::with_capacity(xs.len());
                    for x in xs { out.push(if let Some(name)=loop_name{if let Some(env)=known_env{analyze_expr_no_loop_env(&x,name,env)?}else{analyze_expr_no_loop(&x,name)?}}else{analyze_expr(&x)?}); }
                    return Some(CExpr::Call{op:Box::new(CExpr::Var(VarRef::Dynamic{name:Rc::new(sym.to_string())})),args:out,tail:false,fallback:Box::new(expr.clone())});
                }
            }
        }
    }else{
        let Some(loop_name)=loop_name else{return None;};
        let Some(env)=known_env else{return None;};
        let op_expr=analyze_expr_no_loop_env(&op,loop_name,env)?;
        let xs=args.to_vec().ok()?;
        let mut out=Vec::with_capacity(xs.len());
        for x in xs{out.push(analyze_expr_no_loop_env(&x,loop_name,env)?);}
        return Some(CExpr::Call{op:Box::new(op_expr),args:out,tail:false,fallback:Box::new(expr.clone())});
    }
    None
}
