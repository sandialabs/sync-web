//! Private bytecode/VM scaffold for the next s7-rust execution tier.
//!
//! This module is intentionally behavior-free for now: it defines a compact
//! instruction stream and a conservative translator from the existing compiled
//! AST (`CExpr`), but no production evaluator is wired to it yet.  The goal is
//! to make the next optimization step a real VM/frame redesign rather than more
//! source-shaped fast paths.

#![allow(dead_code)]

use std::cell::Cell;
use std::rc::Rc;

use crate::compiled::{BuiltinId, CExpr, CompiledLambda, QTemplate, VarRef};
use crate::core::Value;
use crate::word_bytecode::WordProgram;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum BytecodeLayout {
    DynamicEnv,
    SlotFrame { names: Vec<Rc<String>> },
}

#[derive(Clone, Debug)]
pub(crate) enum AddTerm { Slot(usize), Const(i64) }
#[derive(Clone, Debug)]
pub(crate) enum ValueOperand { Slot(usize), Const(usize) }

#[derive(Clone, Debug)]
pub(crate) enum MulTerm { Slot(usize), Const(i64) }

#[derive(Clone, Debug)]
pub(crate) enum Instr {
    LoadConst(usize),
    LoadDynamic(Rc<String>),
    LoadSlot(usize),
    StoreTemp(usize),
    BindTemp { index: usize, name: Rc<String>, sequential: bool },
    LoadTemp(usize),
    Pop,
    Jump(usize),
    JumpIfFalse(usize),
    JumpIfFalsePop(usize),
    JumpIfOrTrue(usize),
    CaseJump { datums: Vec<Value>, target: usize },
    BuiltinCall { id: BuiltinId, argc: usize },
    FastBuiltinCall { id: BuiltinId, argc: usize },
    UnarySlot { id: BuiltinId, slot: usize },
    OneArgSlotCall { callee: usize, arg: ValueOperand },
    OneArgCarSlotCall { list: usize, arg: ValueOperand },
    BinaryOperands { id: BuiltinId, lhs: ValueOperand, rhs: ValueOperand },
    UnaryCompareSlot { getter: BuiltinId, cmp: BuiltinId, slot: usize, rhs: ValueOperand },
    IntAddTerms { terms: Vec<AddTerm> },
    IntAddTermsRecur { terms: Vec<AddTerm> },
    IntMulTerms { terms: Vec<MulTerm> },
    IntBinaryTerms { id: BuiltinId, lhs: AddTerm, rhs: AddTerm },
    GenericCall { argc: usize },
    ApplicableRef,
    ApplicableRefDynamic { target: Rc<String>, index: ValueOperand },
    SetApplicable,
    SetApplicableDynamic { target: Rc<String>, index: ValueOperand },
    SetApplicableOperands { target: usize, index: ValueOperand },
    QCons,
    QSpliceCons,
    QVector(usize),
    Fallback(usize),
    MakeLambda(usize),
    ApplyLambda(usize),
    ListRefSlot(usize),
    ListRefConst(usize),
    MakeValues(usize),
    Recur { argc: usize, target: usize, param_start: usize, param_count: usize, name: Rc<String> },
    Return,
}

#[derive(Clone, Debug)]
pub(crate) struct BytecodeFunction {
    pub(crate) layout: BytecodeLayout,
    pub(crate) constants: Vec<Value>,
    pub(crate) code: Vec<Instr>,
    pub(crate) max_temps: usize,
    pub(crate) cache_stable_builtins: bool,
    pub(crate) required_builtins: Vec<&'static str>,
    pub(crate) lambdas: Vec<CompiledLambda>,
    pub(crate) materialization_bindings: Vec<(usize,Vec<(Rc<String>,usize)>)>,
    pub(crate) validated_env: Cell<(usize,u64)>,
    pub(crate) word_program:Option<Rc<WordProgram>>,
    pub(crate) word_calls_safe:bool,
}

#[derive(Default)]
struct Emitter {
    constants: Vec<Value>,
    code: Vec<Instr>,
    max_temps: usize,
    next_temp: usize,
    base_slots: usize,
    lambdas: Vec<CompiledLambda>,
    loop_stack: Vec<(usize, usize, usize, Rc<String>)>,
    active_bindings: Vec<(Rc<String>,usize)>,
    materialization_bindings: Vec<(usize,Vec<(Rc<String>,usize)>)>,
    word_calls_safe:bool,
}

fn offset_template_slots(template:&mut QTemplate,offset:usize)->bool{match template{QTemplate::Literal(_)=>true,QTemplate::Pair(car,cdr)=>offset_template_slots(car,offset)&&offset_template_slots(cdr,offset),QTemplate::Vector(values)=>values.iter_mut().all(|value|offset_template_slots(value,offset)),QTemplate::Unquote(expr)|QTemplate::Splice(expr)=>offset_expr_slots(expr,offset)}}
fn offset_expr_slots(expr:&mut CExpr,offset:usize)->bool{match expr{CExpr::Const(_)|CExpr::Quasiquote(_)|CExpr::Fallback(_)|CExpr::Lambda(_)=>true,CExpr::Var(VarRef::Lexical{slot,..})=>{*slot+=offset;true},CExpr::Var(VarRef::Dynamic{..})=>true,CExpr::SetVar{target,expr}=>{if let VarRef::Lexical{slot,..}=target{*slot+=offset}offset_expr_slots(expr,offset)},CExpr::If{test,conseq,alt}=>offset_expr_slots(test,offset)&&offset_expr_slots(conseq,offset)&&offset_expr_slots(alt,offset),CExpr::Begin(values)|CExpr::And(values)|CExpr::Or(values)|CExpr::Recur{args:values}=>values.iter_mut().all(|value|offset_expr_slots(value,offset)),CExpr::Let{bindings,body,..}=>bindings.iter_mut().all(|(_,value)|offset_expr_slots(value,offset))&&body.iter_mut().all(|value|offset_expr_slots(value,offset)),CExpr::Cond{clauses,else_body}=>clauses.iter_mut().all(|(test,body)|offset_expr_slots(test,offset)&&body.iter_mut().all(|value|offset_expr_slots(value,offset)))&&else_body.as_mut().map(|body|body.iter_mut().all(|value|offset_expr_slots(value,offset))).unwrap_or(true),CExpr::Case{key,clauses,else_body}=>offset_expr_slots(key,offset)&&clauses.iter_mut().all(|(_,body)|body.iter_mut().all(|value|offset_expr_slots(value,offset)))&&else_body.as_mut().map(|body|body.iter_mut().all(|value|offset_expr_slots(value,offset))).unwrap_or(true),CExpr::QuasiquoteTemplate(template)=>offset_template_slots(template,offset),CExpr::Call{op,args,..}=>offset_expr_slots(op,offset)&&args.iter_mut().all(|value|offset_expr_slots(value,offset)),CExpr::BuiltinCall{args,..}=>args.iter_mut().all(|value|offset_expr_slots(value,offset)),CExpr::ApplicableRef{target,index}=>offset_expr_slots(target,offset)&&offset_expr_slots(index,offset),CExpr::SetApplicable{target,index,value}=>offset_expr_slots(target,offset)&&offset_expr_slots(index,offset)&&offset_expr_slots(value,offset),CExpr::Loop{..}=>false}}

impl Emitter {
    fn new(layout: &BytecodeLayout) -> Self {
        let base_slots = match layout { BytecodeLayout::DynamicEnv => 0, BytecodeLayout::SlotFrame { names } => names.len() };
        Self { constants: Vec::new(), code: Vec::new(), max_temps: 0, next_temp: 0, base_slots, lambdas:Vec::new(),loop_stack: Vec::new(),active_bindings:Vec::new(),materialization_bindings:Vec::new(),word_calls_safe:true }
    }

    fn alloc_temp(&mut self) -> usize {
        let idx = self.base_slots + self.next_temp;
        self.next_temp += 1;
        self.max_temps = self.max_temps.max(self.next_temp);
        idx
    }

    fn const_index(&mut self, value: &Value) -> usize {
        self.constants.push(value.clone());
        self.constants.len() - 1
    }

    fn emit_exprs(&mut self, exprs: &[CExpr]) -> Option<()> {
        if exprs.is_empty() {
            let idx = self.const_index(&Value::Unspecified);
            self.code.push(Instr::LoadConst(idx));
            return Some(());
        }
        for expr in &exprs[..exprs.len() - 1] {
            self.emit_expr(expr)?;
            self.code.push(Instr::Pop);
        }
        self.emit_expr(&exprs[exprs.len() - 1])?;
        Some(())
    }

    fn emit_qtemplate(&mut self,t:&QTemplate)->Option<()>{match t{QTemplate::Literal(v)=>{let i=self.const_index(v);self.code.push(Instr::LoadConst(i));},QTemplate::Unquote(e)|QTemplate::Splice(e)=>self.emit_expr(e)?,QTemplate::Pair(car,cdr)=>{match &**car{QTemplate::Splice(e)=>{self.emit_expr(e)?;self.emit_qtemplate(cdr)?;self.code.push(Instr::QSpliceCons);},_=>{self.emit_qtemplate(car)?;self.emit_qtemplate(cdr)?;self.code.push(Instr::QCons);}}},QTemplate::Vector(xs)=>{for x in xs{self.emit_qtemplate(x)?;}self.code.push(Instr::QVector(xs.len()));}}Some(())}

    fn emit_expr(&mut self, expr: &CExpr) -> Option<()> {
        match expr {
            CExpr::Const(v) => {
                let idx = self.const_index(v);
                self.code.push(Instr::LoadConst(idx));
            }
            CExpr::Var(VarRef::Dynamic { name }) => self.code.push(Instr::LoadDynamic(name.clone())),
            CExpr::Var(VarRef::Lexical { slot, .. }) => self.code.push(Instr::LoadSlot(*slot)),
            CExpr::If { test, conseq, alt } => {
                self.emit_expr(test)?;
                if matches!(&**conseq,CExpr::Loop{..}){
                    let jf=self.code.len();self.code.push(Instr::JumpIfFalsePop(usize::MAX));let jt=self.code.len();self.code.push(Instr::Jump(usize::MAX));let alt_pc=self.code.len();self.emit_expr(alt)?;let end_alt=self.code.len();self.code.push(Instr::Jump(usize::MAX));let conseq_pc=self.code.len();self.emit_expr(conseq)?;let end=self.code.len();self.code[jf]=Instr::JumpIfFalsePop(alt_pc);self.code[jt]=Instr::Jump(conseq_pc);self.code[end_alt]=Instr::Jump(end);
                }else{
                    let jf = self.code.len();
                    self.code.push(Instr::JumpIfFalsePop(usize::MAX));
                    self.emit_expr(conseq)?;
                    let j = self.code.len();
                    self.code.push(Instr::Jump(usize::MAX));
                    let alt_pc = self.code.len();
                    self.emit_expr(alt)?;
                    let end_pc = self.code.len();
                    self.code[jf] = Instr::JumpIfFalsePop(alt_pc);
                    self.code[j] = Instr::Jump(end_pc);
                }
            }
            CExpr::Begin(xs) => self.emit_exprs(xs)?,
            CExpr::BuiltinCall { id, args, .. } => {
                if args.len()==2&&matches!(id,BuiltinId::EqP|BuiltinId::EqualP){if let CExpr::BuiltinCall{id:getter,args:getter_args,..}=&args[0]{if getter_args.len()==1&&matches!(getter,BuiltinId::Car|BuiltinId::Cdr|BuiltinId::Caar|BuiltinId::Cdar){if let CExpr::Var(VarRef::Lexical{slot,..})=&getter_args[0]{let rhs=match &args[1]{CExpr::Var(VarRef::Lexical{slot,..})=>Some(ValueOperand::Slot(*slot)),CExpr::Const(v)=>{let i=self.const_index(v);Some(ValueOperand::Const(i))},_=>None};if let Some(rhs)=rhs{self.code.push(Instr::UnaryCompareSlot{getter:*getter,cmp:*id,slot:*slot,rhs});return Some(());}}}}}
                if args.len()==2&&matches!(id,BuiltinId::Cons|BuiltinId::EqP|BuiltinId::EqualP){let lhs=match &args[0]{CExpr::Var(VarRef::Lexical{slot,..})=>Some(ValueOperand::Slot(*slot)),CExpr::Const(v)=>{let i=self.const_index(v);Some(ValueOperand::Const(i))},_=>None};let rhs=match &args[1]{CExpr::Var(VarRef::Lexical{slot,..})=>Some(ValueOperand::Slot(*slot)),CExpr::Const(v)=>{let i=self.const_index(v);Some(ValueOperand::Const(i))},_=>None};if let (Some(lhs),Some(rhs))=(lhs,rhs){self.code.push(Instr::BinaryOperands{id:*id,lhs,rhs});return Some(());}}
                if args.len()==1&&matches!(id,BuiltinId::Car|BuiltinId::Cdr|BuiltinId::Caar|BuiltinId::Cdar|BuiltinId::NullP|BuiltinId::PairP|BuiltinId::NumberP|BuiltinId::CharP|BuiltinId::SymbolP|BuiltinId::BooleanP|BuiltinId::Not){if let CExpr::Var(VarRef::Lexical{slot,..})=&args[0]{self.code.push(Instr::UnarySlot{id:*id,slot:*slot});return Some(());}}
                if *id==BuiltinId::Apply&&args.len()==2{if let CExpr::Lambda(l)=&args[0]{self.emit_expr(&args[1])?;let i=self.lambdas.len();self.lambdas.push(l.clone());self.materialization_bindings.push((self.code.len(),self.active_bindings.clone()));self.code.push(Instr::ApplyLambda(i));return Some(());}}
                if *id==BuiltinId::ListRef&&args.len()==2&&matches!(&args[1],CExpr::Const(Value::Int(_))|CExpr::Var(VarRef::Lexical{..})|CExpr::BuiltinCall{id:BuiltinId::Add|BuiltinId::Sub|BuiltinId::Mul|BuiltinId::Remainder|BuiltinId::Modulo,..}){match &args[0]{CExpr::Var(VarRef::Lexical{slot,..})=>{self.emit_expr(&args[1])?;self.code.push(Instr::ListRefSlot(*slot));return Some(())},CExpr::Const(v)=>{self.emit_expr(&args[1])?;let i=self.const_index(v);self.code.push(Instr::ListRefConst(i));return Some(())},_=>{}}}
                if *id==BuiltinId::Add {
                    let mut terms=Vec::with_capacity(args.len());
                    let mut ok=true;
                    for arg in args{
                        match arg{
                            CExpr::Const(Value::Int(n))=>terms.push(AddTerm::Const(*n)),
                            CExpr::Var(VarRef::Lexical{slot,..})=>terms.push(AddTerm::Slot(*slot)),
                            _=>{ok=false; break;}
                        }
                    }
                    if ok{self.code.push(Instr::IntAddTerms{terms}); return Some(());}
                }
                if *id==BuiltinId::Mul {
                    let mut terms=Vec::with_capacity(args.len());
                    let mut ok=true;
                    for arg in args{
                        match arg{
                            CExpr::Const(Value::Int(n))=>terms.push(MulTerm::Const(*n)),
                            CExpr::Var(VarRef::Lexical{slot,..})=>terms.push(MulTerm::Slot(*slot)),
                            _=>{ok=false; break;}
                        }
                    }
                    if ok{self.code.push(Instr::IntMulTerms{terms}); return Some(());}
                }
                let as_add_term=|arg:&CExpr| match arg{CExpr::Const(Value::Int(n))=>Some(AddTerm::Const(*n)),CExpr::Var(VarRef::Lexical{slot,..})=>Some(AddTerm::Slot(*slot)),_=>None};
                if matches!(id,BuiltinId::Sub|BuiltinId::NumEq|BuiltinId::Less|BuiltinId::LessEq|BuiltinId::Greater|BuiltinId::GreaterEq|BuiltinId::Remainder|BuiltinId::Modulo) && args.len()==2{
                    if let (Some(lhs),Some(rhs))=(as_add_term(&args[0]),as_add_term(&args[1])){self.code.push(Instr::IntBinaryTerms{id:*id,lhs,rhs}); return Some(());}
                }
                for arg in args { self.emit_expr(arg)?; }
                let instr = match id {
                    BuiltinId::Add | BuiltinId::Sub | BuiltinId::Mul |
                    BuiltinId::NumEq | BuiltinId::Less | BuiltinId::LessEq | BuiltinId::Greater | BuiltinId::GreaterEq |
                    BuiltinId::Remainder | BuiltinId::Modulo |
                    BuiltinId::Length | BuiltinId::List | BuiltinId::Cons | BuiltinId::Car | BuiltinId::Cdr | BuiltinId::NullP | BuiltinId::PairP | BuiltinId::NumberP | BuiltinId::CharP | BuiltinId::SymbolP | BuiltinId::BooleanP | BuiltinId::Not | BuiltinId::EqP | BuiltinId::EqualP | BuiltinId::Caar | BuiltinId::Cdar |
                    BuiltinId::VectorRef | BuiltinId::VectorSet | BuiltinId::HashRef | BuiltinId::HashSet => Instr::FastBuiltinCall { id: *id, argc: args.len() },
                    _ => Instr::BuiltinCall { id: *id, argc: args.len() },
                };
                self.code.push(instr);
            }
            CExpr::Call { op, args, .. } => {
                if args.len()==1{let arg=match &args[0]{CExpr::Var(VarRef::Lexical{slot,..})=>Some(ValueOperand::Slot(*slot)),CExpr::Const(v)=>{let i=self.const_index(v);Some(ValueOperand::Const(i))},_=>None};if let Some(arg)=arg{if let CExpr::Var(VarRef::Lexical{slot:callee,..})=&**op{self.code.push(Instr::OneArgSlotCall{callee:*callee,arg});return Some(())}if let CExpr::BuiltinCall{id:BuiltinId::Car,args:getter_args,..}=&**op{if let [CExpr::Var(VarRef::Lexical{slot:list,..})]=getter_args.as_slice(){self.code.push(Instr::OneArgCarSlotCall{list:*list,arg});return Some(())}}}}
                if !matches!(&**op,CExpr::Var(VarRef::Dynamic{..})){self.word_calls_safe=false;}
                self.emit_expr(op)?;
                for arg in args { self.emit_expr(arg)?; }
                self.code.push(Instr::GenericCall { argc: args.len() });
            }
            CExpr::ApplicableRef { target, index } => {
                if let CExpr::Var(VarRef::Dynamic{name})=&**target{let index=match &**index{CExpr::Var(VarRef::Lexical{slot,..})=>Some(ValueOperand::Slot(*slot)),CExpr::Const(value)=>Some(ValueOperand::Const(self.const_index(value))),_=>None};if let Some(index)=index{self.code.push(Instr::ApplicableRefDynamic{target:name.clone(),index});return Some(())}}
                self.emit_expr(target)?;
                self.emit_expr(index)?;
                self.code.push(Instr::ApplicableRef);
            }
            CExpr::SetApplicable { target, index, value } => {
                if let CExpr::Var(VarRef::Dynamic{name})=&**target{let index=match &**index{CExpr::Var(VarRef::Lexical{slot,..})=>Some(ValueOperand::Slot(*slot)),CExpr::Const(value)=>Some(ValueOperand::Const(self.const_index(value))),_=>None};if let Some(index)=index{self.emit_expr(value)?;self.code.push(Instr::SetApplicableDynamic{target:name.clone(),index});return Some(())}}
                if let CExpr::Var(VarRef::Lexical{slot:target,..})=&**target{let index=match &**index{CExpr::Var(VarRef::Lexical{slot,..})=>Some(ValueOperand::Slot(*slot)),CExpr::Const(v)=>{let i=self.const_index(v);Some(ValueOperand::Const(i))},_=>None};if let Some(index)=index{self.emit_expr(value)?;self.code.push(Instr::SetApplicableOperands{target:*target,index});return Some(());}}
                self.emit_expr(target)?;
                self.emit_expr(index)?;
                self.emit_expr(value)?;
                self.code.push(Instr::SetApplicable);
            }
            CExpr::Recur { args } => {
                let (target, param_start, param_count, name) = self.loop_stack.last()?.clone();
                for arg in args {
                    if let CExpr::BuiltinCall{id:BuiltinId::Add,args,..}=arg{let terms=args.iter().map(|x|match x{CExpr::Const(Value::Int(n))=>Some(AddTerm::Const(*n)),CExpr::Var(VarRef::Lexical{slot,..})=>Some(AddTerm::Slot(*slot)),_=>None}).collect::<Option<Vec<_>>>();if let Some(terms)=terms{self.code.push(Instr::IntAddTermsRecur{terms});continue;}}
                    self.emit_expr(arg)?;
                }
                self.code.push(Instr::Recur { argc: args.len(), target, param_start, param_count, name });
            }
            CExpr::And(xs) => self.emit_short_circuit(xs, true)?,
            CExpr::Or(xs) => self.emit_short_circuit(xs, false)?,
            CExpr::Let { sequential, bindings, body } => self.emit_let(*sequential, bindings, body)?,
            CExpr::Cond { clauses, else_body } => self.emit_cond(clauses, else_body.as_deref())?,
            CExpr::Case { key, clauses, else_body } => self.emit_case(key, clauses, else_body.as_deref())?,
            CExpr::Loop { name, params, inits, body, relative_slots } => self.emit_loop(name, params, inits, body, *relative_slots)?,
            CExpr::QuasiquoteTemplate(t)=>self.emit_qtemplate(t)?,
            CExpr::Fallback(v)=>{let i=self.const_index(v);self.materialization_bindings.push((self.code.len(),self.active_bindings.clone()));self.code.push(Instr::Fallback(i));}
            CExpr::Lambda(l)=>{let i=self.lambdas.len();self.lambdas.push(l.clone());self.materialization_bindings.push((self.code.len(),self.active_bindings.clone()));self.code.push(Instr::MakeLambda(i));}
            CExpr::SetVar { .. } | CExpr::Quasiquote(_) => return None,
        }
        Some(())
    }

    fn emit_loop(&mut self, name: &Rc<String>, params: &[Rc<String>], inits: &[CExpr], body: &CExpr, relative_slots:bool) -> Option<()> {
        if params.is_empty() || params.len() != inits.len() { return None; }
        let temp_start=self.next_temp;
        let temps = (0..params.len()).map(|_| self.alloc_temp()).collect::<Vec<_>>();
        for init in inits { self.emit_expr(init)?; }
        for (param, temp) in params.iter().zip(temps.iter().copied()).rev() {
            self.code.push(Instr::BindTemp { index: temp, name: param.clone(), sequential: false });
        }
        let loop_pc = self.code.len();
        let param_start = temps[0];
        self.loop_stack.push((loop_pc, param_start, params.len(), name.clone()));
        let binding_start=self.active_bindings.len();
        self.active_bindings.extend(params.iter().cloned().zip(temps.iter().copied()));
        let mut body=body.clone();if relative_slots&&param_start>0&&!offset_expr_slots(&mut body,param_start){return None}self.emit_expr(&body)?;
        self.active_bindings.truncate(binding_start);
        self.next_temp=temp_start;
        self.loop_stack.pop();
        Some(())
    }

    fn emit_case(&mut self, key: &CExpr, clauses: &[(Vec<Value>, Vec<CExpr>)], else_body: Option<&[CExpr]>) -> Option<()> {
        self.emit_expr(key)?;
        let mut case_jumps=Vec::new();
        for (datums,_) in clauses{
            let at=self.code.len();
            self.code.push(Instr::CaseJump{datums:datums.clone(),target:usize::MAX});
            case_jumps.push(at);
        }
        let mut exits=Vec::new();
        if let Some(body)=else_body{
            if !body.is_empty(){self.code.push(Instr::Pop); self.emit_exprs(body)?;}
        }else{
            self.code.push(Instr::Pop);
            let idx=self.const_index(&Value::Unspecified);
            self.code.push(Instr::LoadConst(idx));
        }
        let j=self.code.len(); self.code.push(Instr::Jump(usize::MAX)); exits.push(j);
        for (idx,(_,body)) in clauses.iter().enumerate(){
            let target=self.code.len();
            if let Instr::CaseJump{target:t,..}= &mut self.code[case_jumps[idx]]{*t=target;}
            if !body.is_empty(){self.code.push(Instr::Pop); self.emit_exprs(body)?;}
            let j=self.code.len(); self.code.push(Instr::Jump(usize::MAX)); exits.push(j);
        }
        let end=self.code.len();
        for j in exits{self.code[j]=Instr::Jump(end);}
        Some(())
    }

    fn emit_cond(&mut self, clauses: &[(CExpr, Vec<CExpr>)], else_body: Option<&[CExpr]>) -> Option<()> {
        let mut exits = Vec::new();
        for (test, body) in clauses {
            self.emit_expr(test)?;
            let jf = self.code.len();
            self.code.push(Instr::JumpIfFalsePop(usize::MAX));
            self.emit_exprs(body)?;
            let j = self.code.len();
            self.code.push(Instr::Jump(usize::MAX));
            let next_pc = self.code.len();
            self.code[jf] = Instr::JumpIfFalsePop(next_pc);
            exits.push(j);
        }
        if let Some(body) = else_body { self.emit_exprs(body)?; }
        else { let idx = self.const_index(&Value::Unspecified); self.code.push(Instr::LoadConst(idx)); }
        let end = self.code.len();
        for j in exits { self.code[j] = Instr::Jump(end); }
        Some(())
    }

    fn emit_let(&mut self, sequential: bool, bindings: &[(Rc<String>, CExpr)], body: &[CExpr]) -> Option<()> {
        let temp_start=self.next_temp;
        let binding_start=self.active_bindings.len();
        if sequential {
            for (name, init) in bindings {
                self.emit_expr(init)?;
                let temp=self.alloc_temp();
                self.code.push(Instr::BindTemp { index: temp, name: name.clone(), sequential: true });
                self.active_bindings.push((name.clone(),temp));
            }
        } else {
            let temps = (0..bindings.len()).map(|_| self.alloc_temp()).collect::<Vec<_>>();
            for (_, init) in bindings { self.emit_expr(init)?; }
            for ((name, _), temp) in bindings.iter().zip(temps.iter().copied()).rev() { self.code.push(Instr::BindTemp { index: temp, name: name.clone(), sequential: false }); }
            self.active_bindings.extend(bindings.iter().map(|(name,_)|name.clone()).zip(temps.iter().copied()));
        }
        self.emit_exprs(body)?;
        self.active_bindings.truncate(binding_start);
        self.next_temp=temp_start;
        Some(())
    }

    fn emit_short_circuit(&mut self, xs: &[CExpr], is_and: bool) -> Option<()> {
        if xs.is_empty() {
            let idx = self.const_index(&Value::Bool(is_and));
            self.code.push(Instr::LoadConst(idx));
            return Some(());
        }
        let mut exits = Vec::new();
        for (i, expr) in xs.iter().enumerate() {
            self.emit_expr(expr)?;
            if i + 1 != xs.len() {
                let exit = self.code.len();
                if is_and { self.code.push(Instr::JumpIfFalse(usize::MAX)); exits.push(exit); }
                else { self.code.push(Instr::JumpIfOrTrue(usize::MAX)); exits.push(exit); }
                self.code.push(Instr::Pop);
            }
        }
        let end = self.code.len();
        for exit in exits {
            match self.code[exit] {
                Instr::JumpIfFalse(_) if is_and => self.code[exit] = Instr::JumpIfFalse(end),
                Instr::JumpIfOrTrue(_) if !is_and => self.code[exit] = Instr::JumpIfOrTrue(end),
                _ => {}
            }
        }
        Some(())
    }
}

fn verify_definite_init(code:&[Instr],base_slots:usize,max_temps:usize)->bool{
    let count=base_slots+max_temps;
    let mut entry=vec![true;base_slots];entry.resize(count,false);
    let mut states=vec![None::<Vec<bool>>;code.len()];
    if code.is_empty(){return false}states[0]=Some(entry);
    let mut work=vec![0usize];
    let operand_slot=|operand:&ValueOperand|match operand{ValueOperand::Slot(slot)=>Some(*slot),ValueOperand::Const(_)=>None};
    while let Some(pc)=work.pop(){
        let mut state=states[pc].clone().unwrap();
        let initialized=|slot:usize|slot<count&&state[slot];
        let terms_ok=|terms:&[AddTerm]|terms.iter().all(|term|match term{AddTerm::Slot(slot)=>initialized(*slot),AddTerm::Const(_)=>true});
        let mul_ok=|terms:&[MulTerm]|terms.iter().all(|term|match term{MulTerm::Slot(slot)=>initialized(*slot),MulTerm::Const(_)=>true});
        let add_ok=|term:&AddTerm|match term{AddTerm::Slot(slot)=>initialized(*slot),AddTerm::Const(_)=>true};
        let reads_ok=match &code[pc]{
            Instr::LoadSlot(slot)|Instr::LoadTemp(slot)|Instr::UnarySlot{slot,..}|Instr::ListRefSlot(slot)=>initialized(*slot),
            Instr::OneArgSlotCall{callee,arg}=>initialized(*callee)&&operand_slot(arg).map(initialized).unwrap_or(true),
            Instr::OneArgCarSlotCall{list,arg}=>initialized(*list)&&operand_slot(arg).map(initialized).unwrap_or(true),
            Instr::BinaryOperands{lhs,rhs,..}=>operand_slot(lhs).map(initialized).unwrap_or(true)&&operand_slot(rhs).map(initialized).unwrap_or(true),
            Instr::UnaryCompareSlot{slot,rhs,..}=>initialized(*slot)&&operand_slot(rhs).map(initialized).unwrap_or(true),
            Instr::IntAddTerms{terms}|Instr::IntAddTermsRecur{terms}=>terms_ok(terms),
            Instr::IntMulTerms{terms}=>mul_ok(terms),
            Instr::IntBinaryTerms{lhs,rhs,..}=>add_ok(lhs)&&add_ok(rhs),
            Instr::ApplicableRefDynamic{index,..}|Instr::SetApplicableDynamic{index,..}=>operand_slot(index).map(initialized).unwrap_or(true),
            Instr::SetApplicableOperands{target,index}=>initialized(*target)&&operand_slot(index).map(initialized).unwrap_or(true),
            _=>true,
        };
        if !reads_ok{return false}
        match &code[pc]{Instr::StoreTemp(slot)|Instr::BindTemp{index:slot,..}=>{if *slot>=count{return false}state[*slot]=true},_=>{}}
        let mut successors=Vec::with_capacity(2);
        match &code[pc]{
            Instr::Return=>{},
            Instr::Jump(target)=>successors.push((*target,state)),
            Instr::JumpIfFalse(target)|Instr::JumpIfFalsePop(target)|Instr::JumpIfOrTrue(target)=>{successors.push((*target,state.clone()));successors.push((pc+1,state));},
            Instr::CaseJump{target,..}=>{successors.push((*target,state.clone()));successors.push((pc+1,state));},
            Instr::Recur{target,param_start,param_count,..}=>{for slot in *param_start..param_start+param_count{if slot>=count{return false}state[slot]=true}successors.push((*target,state));},
            _=>successors.push((pc+1,state)),
        }
        for (next,incoming) in successors{if next>=code.len(){return false}match &mut states[next]{None=>{states[next]=Some(incoming);work.push(next)},Some(existing)=>{let mut changed=false;for (old,new) in existing.iter_mut().zip(incoming){let merged=*old&&new;if merged!=*old{*old=merged;changed=true}}if changed{work.push(next)}}}}
    }
    true
}

impl BytecodeFunction {
    pub(crate) fn from_exprs(layout: BytecodeLayout, exprs: &[CExpr]) -> Option<Self> {
        let mut emitter = Emitter::new(&layout);
        emitter.emit_exprs(exprs)?;
        emitter.code.push(Instr::Return);
        if !verify_definite_init(&emitter.code,emitter.base_slots,emitter.max_temps){return None}
        let cache_stable_builtins=!emitter.code.iter().any(|instr|matches!(instr,Instr::GenericCall{..}|Instr::SetApplicable|Instr::SetApplicableDynamic{..}));
        let mut required_builtins=Vec::new();
        for instr in &emitter.code{
            let name=match instr{Instr::BuiltinCall{id,..}|Instr::FastBuiltinCall{id,..}|Instr::UnarySlot{id,..}|Instr::BinaryOperands{id,..}|Instr::IntBinaryTerms{id,..}=>Some(id.name()),Instr::UnaryCompareSlot{cmp,..}=>Some(cmp.name()),Instr::IntAddTerms{..}|Instr::IntAddTermsRecur{..}=>Some("+"),Instr::ListRefSlot(..)|Instr::ListRefConst(..)=>Some("list-ref"),Instr::IntMulTerms{..}=>Some("*"),_=>None};
            if let Some(name)=name{if !required_builtins.contains(&name){required_builtins.push(name);}}
            if let Instr::UnaryCompareSlot{getter,..}=instr{let name=getter.name();if !required_builtins.contains(&name){required_builtins.push(name);}}
        }
        let mut function=Self {
            layout,
            constants: emitter.constants,
            code: emitter.code,
            max_temps: emitter.max_temps,
            cache_stable_builtins,
            required_builtins,
            lambdas:emitter.lambdas,
            materialization_bindings:emitter.materialization_bindings,
            validated_env: Cell::new((0,0)),
            word_program:None,
            word_calls_safe:emitter.word_calls_safe,
        };
        function.word_program=WordProgram::compile(&function).map(Rc::new);
        Some(function)
    }
}
