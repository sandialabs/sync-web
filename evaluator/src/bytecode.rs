//! Private bytecode/VM scaffold for the next s7-rust execution tier.
//!
//! This module is intentionally behavior-free for now: it defines a compact
//! instruction stream and a conservative translator from the existing compiled
//! AST (`CExpr`), but no production evaluator is wired to it yet.  The goal is
//! to make the next optimization step a real VM/frame redesign rather than more
//! source-shaped fast paths.

#![allow(dead_code)]

use std::rc::Rc;

use crate::compiled::{BuiltinId, CExpr, VarRef};
use crate::core::Value;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum BytecodeLayout {
    DynamicEnv,
    SlotFrame { names: Vec<Rc<String>> },
}

#[derive(Clone, Debug)]
pub(crate) enum AddTerm { Slot(usize), Const(i64) }

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
    IntAddTerms { terms: Vec<AddTerm> },
    IntMulTerms { terms: Vec<MulTerm> },
    IntBinaryTerms { id: BuiltinId, lhs: AddTerm, rhs: AddTerm },
    GenericCall { argc: usize },
    ApplicableRef,
    SetApplicable,
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
}

#[derive(Default)]
struct Emitter {
    constants: Vec<Value>,
    code: Vec<Instr>,
    max_temps: usize,
    base_slots: usize,
    loop_stack: Vec<(usize, usize, usize, Rc<String>)>,
}

impl Emitter {
    fn new(layout: &BytecodeLayout) -> Self {
        let base_slots = match layout { BytecodeLayout::DynamicEnv => 0, BytecodeLayout::SlotFrame { names } => names.len() };
        Self { constants: Vec::new(), code: Vec::new(), max_temps: 0, base_slots, loop_stack: Vec::new() }
    }

    fn alloc_temp(&mut self) -> usize {
        let idx = self.base_slots + self.max_temps;
        self.max_temps += 1;
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
            CExpr::Begin(xs) => self.emit_exprs(xs)?,
            CExpr::BuiltinCall { id, args, .. } => {
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
                    BuiltinId::Cons | BuiltinId::Car | BuiltinId::Cdr | BuiltinId::NullP | BuiltinId::PairP | BuiltinId::NumberP | BuiltinId::CharP | BuiltinId::SymbolP | BuiltinId::BooleanP | BuiltinId::Not | BuiltinId::EqP | BuiltinId::EqualP | BuiltinId::Caar | BuiltinId::Cdar |
                    BuiltinId::VectorRef | BuiltinId::VectorSet | BuiltinId::HashRef | BuiltinId::HashSet => Instr::FastBuiltinCall { id: *id, argc: args.len() },
                    _ => Instr::BuiltinCall { id: *id, argc: args.len() },
                };
                self.code.push(instr);
            }
            CExpr::Call { op, args, .. } => {
                self.emit_expr(op)?;
                for arg in args { self.emit_expr(arg)?; }
                self.code.push(Instr::GenericCall { argc: args.len() });
            }
            CExpr::ApplicableRef { target, index } => {
                self.emit_expr(target)?;
                self.emit_expr(index)?;
                self.code.push(Instr::ApplicableRef);
            }
            CExpr::SetApplicable { target, index, value } => {
                self.emit_expr(target)?;
                self.emit_expr(index)?;
                self.emit_expr(value)?;
                self.code.push(Instr::SetApplicable);
            }
            CExpr::Recur { args } => {
                let (target, param_start, param_count, name) = self.loop_stack.last()?.clone();
                for arg in args { self.emit_expr(arg)?; }
                self.code.push(Instr::Recur { argc: args.len(), target, param_start, param_count, name });
            }
            CExpr::And(xs) => self.emit_short_circuit(xs, true)?,
            CExpr::Or(xs) => self.emit_short_circuit(xs, false)?,
            CExpr::Let { sequential, bindings, body } => self.emit_let(*sequential, bindings, body)?,
            CExpr::Cond { clauses, else_body } => self.emit_cond(clauses, else_body.as_deref())?,
            CExpr::Case { key, clauses, else_body } => self.emit_case(key, clauses, else_body.as_deref())?,
            CExpr::Loop { name, params, inits, body } => self.emit_loop(name, params, inits, body)?,
            CExpr::SetVar { .. } | CExpr::Lambda(_) | CExpr::Quasiquote(_) | CExpr::QuasiquoteTemplate(_) | CExpr::Fallback(_) => return None,
        }
        Some(())
    }

    fn emit_loop(&mut self, name: &Rc<String>, params: &[Rc<String>], inits: &[CExpr], body: &CExpr) -> Option<()> {
        if params.is_empty() || params.len() != inits.len() { return None; }
        let temps = (0..params.len()).map(|_| self.alloc_temp()).collect::<Vec<_>>();
        for init in inits { self.emit_expr(init)?; }
        for (param, temp) in params.iter().zip(temps.iter().copied()).rev() {
            self.code.push(Instr::BindTemp { index: temp, name: param.clone(), sequential: false });
        }
        let loop_pc = self.code.len();
        let param_start = temps[0];
        self.loop_stack.push((loop_pc, param_start, params.len(), name.clone()));
        self.emit_expr(body)?;
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
        let temps = (0..bindings.len()).map(|_| self.alloc_temp()).collect::<Vec<_>>();
        if sequential {
            for ((name, init), temp) in bindings.iter().zip(temps.iter().copied()) {
                self.emit_expr(init)?;
                self.code.push(Instr::BindTemp { index: temp, name: name.clone(), sequential: true });
            }
        } else {
            for (_, init) in bindings { self.emit_expr(init)?; }
            for ((name, _), temp) in bindings.iter().zip(temps.iter().copied()).rev() { self.code.push(Instr::BindTemp { index: temp, name: name.clone(), sequential: false }); }
        }
        self.emit_exprs(body)
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

impl BytecodeFunction {
    pub(crate) fn from_exprs(layout: BytecodeLayout, exprs: &[CExpr]) -> Option<Self> {
        let mut emitter = Emitter::new(&layout);
        emitter.emit_exprs(exprs)?;
        emitter.code.push(Instr::Return);
        let cache_stable_builtins=!emitter.code.iter().any(|instr|matches!(instr,Instr::GenericCall{..}|Instr::SetApplicable));
        Some(Self {
            layout,
            constants: emitter.constants,
            code: emitter.code,
            max_temps: emitter.max_temps,
            cache_stable_builtins,
        })
    }
}
