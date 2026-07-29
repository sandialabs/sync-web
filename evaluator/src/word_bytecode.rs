//! Fixed-width bytecode island over [`Word`](crate::word::Word).
//!
//! The initial eligibility set is intentionally small and side-effect free
//! apart from loop-slot updates. Unsupported legacy instructions reject during
//! translation, before execution. The production evaluator is not routed here
//! until the operation set includes exact local slow paths for overflow,
//! multiple values, calls, and structured errors.

#![allow(dead_code)]

use crate::bytecode::{AddTerm,BytecodeFunction,Instr,MulTerm,ValueOperand};
use crate::compiled::BuiltinId;
use crate::core::{EnvRef,Params,Procedure,SchemeError,Value};
use std::{cell::Cell,collections::HashSet,rc::Rc};

#[cfg(test)]
thread_local!{static ELIGIBILITY:std::cell::RefCell<std::collections::HashMap<&'static str,usize>>=std::cell::RefCell::new(std::collections::HashMap::new());}
#[cfg(test)]
fn record_eligibility(reason:&'static str){ELIGIBILITY.with(|stats|*stats.borrow_mut().entry(reason).or_default()+=1)}
#[cfg(not(test))]
#[inline(always)]
fn record_eligibility(_reason:&'static str){}
#[cfg(test)]
pub(crate) fn reset_eligibility(){ELIGIBILITY.with(|stats|stats.borrow_mut().clear())}
#[cfg(test)]
pub(crate) fn eligibility_report()->Vec<(&'static str,usize)>{ELIGIBILITY.with(|stats|{let mut rows=stats.borrow().iter().map(|(key,value)|(*key,*value)).collect::<Vec<_>>();rows.sort_unstable();rows})}

#[cfg(test)]
fn instruction_rejection(instruction:&Instr)->&'static str{match instruction{Instr::LoadConst(_)=>"instruction:LoadConst",Instr::LoadDynamic(_)=>"instruction:LoadDynamic",Instr::LoadSlot(_)=>"instruction:LoadSlot",Instr::StoreTemp(_)=>"instruction:StoreTemp",Instr::BindTemp{..}=>"instruction:BindTemp",Instr::LoadTemp(_)=>"instruction:LoadTemp",Instr::Pop=>"instruction:Pop",Instr::Jump(_)=>"instruction:Jump",Instr::JumpIfFalse(_)=>"instruction:JumpIfFalse",Instr::JumpIfFalsePop(_)=>"instruction:JumpIfFalsePop",Instr::JumpIfOrTrue(_)=>"instruction:JumpIfOrTrue",Instr::CaseJump{..}=>"instruction:CaseJump",Instr::BuiltinCall{..}=>"instruction:BuiltinCall",Instr::FastBuiltinCall{..}=>"instruction:FastBuiltinCall",Instr::UnarySlot{..}=>"instruction:UnarySlot",Instr::OneArgSlotCall{..}=>"instruction:OneArgSlotCall",Instr::OneArgCarSlotCall{..}=>"instruction:OneArgCarSlotCall",Instr::BinaryOperands{..}=>"instruction:BinaryOperands",Instr::UnaryCompareSlot{..}=>"instruction:UnaryCompareSlot",Instr::IntAddTerms{..}=>"instruction:IntAddTerms",Instr::IntAddTermsRecur{..}=>"instruction:IntAddTermsRecur",Instr::IntMulTerms{..}=>"instruction:IntMulTerms",Instr::IntBinaryTerms{..}=>"instruction:IntBinaryTerms",Instr::GenericCall{..}=>"instruction:GenericCall",Instr::ApplicableRef=>"instruction:ApplicableRef",Instr::ApplicableRefDynamic{..}=>"instruction:ApplicableRefDynamic",Instr::SetApplicable=>"instruction:SetApplicable",Instr::SetApplicableDynamic{..}=>"instruction:SetApplicableDynamic",Instr::SetApplicableOperands{..}=>"instruction:SetApplicableOperands",Instr::QCons=>"instruction:QCons",Instr::QSpliceCons=>"instruction:QSpliceCons",Instr::QVector(_)=>"instruction:QVector",Instr::Fallback(_)=>"instruction:Fallback",Instr::MakeLambda(_)=>"instruction:MakeLambda",Instr::ApplyLambda(_)=>"instruction:ApplyLambda",Instr::ListRefSlot(_)=>"instruction:ListRefSlot",Instr::ListRefConst(_)=>"instruction:ListRefConst",Instr::MakeValues(_)=>"instruction:MakeValues",Instr::Recur{..}=>"instruction:Recur",Instr::Return=>"instruction:Return"}}
#[cfg(test)]
fn record_instruction_rejection(instruction:&Instr){record_eligibility(instruction_rejection(instruction))}
#[cfg(not(test))]
#[inline(always)]
fn record_instruction_rejection(_instruction:&Instr){}
use crate::word::{Singleton,Word,WordHeap};

const OP_CONST:u8=0;
const OP_SLOT:u8=1;
const OP_STORE:u8=2;
const OP_LOAD_TEMP:u8=3;
const OP_POP:u8=4;
const OP_JUMP:u8=5;
const OP_JUMP_FALSE:u8=6;
const OP_JUMP_FALSE_POP:u8=7;
const OP_ADD:u8=8;
const OP_ADD_RECUR:u8=9;
const OP_MUL:u8=10;
const OP_BINARY:u8=11;
const OP_RECUR:u8=12;
const OP_RETURN:u8=13;
const OP_DYNAMIC:u8=14;
const OP_BUILTIN:u8=15;
const OP_UNARY:u8=16;
const OP_GENERIC:u8=17;
const OP_BINARY_OPERANDS:u8=18;
const OP_LIST_REF:u8=19;
const OP_UNARY_COMPARE:u8=20;
const OP_CASE:u8=21;
const OP_APPLY:u8=22;
const OP_APPLICABLE_REF:u8=23;
const OP_JUMP_OR_TRUE:u8=26;
const OP_ONE_ARG_SLOT:u8=27;
const OP_QCONS:u8=28;
const OP_QVECTOR:u8=29;
const OP_QSPLICE_CONS:u8=30;

#[derive(Clone,Copy,Debug)]
#[repr(C)]
pub(crate) struct WordInstr{op:u8,flags:u8,pad:u16,a:u32,b:u32,c:u32}

#[derive(Clone,Copy,Debug)]
enum Term{Slot(u32),Int(i64)}

#[derive(Clone,Debug)]
struct Binary{kind:BuiltinId,lhs:Term,rhs:Term}

#[derive(Clone,Copy,Debug)]
struct Recur{argc:u32,target:u32,param_start:u32,param_count:u32}
#[derive(Clone,Copy,Debug)]
struct BuiltinCall{id:BuiltinId,argc:u32,slot:Option<u32>}
#[derive(Clone,Debug)]
struct OperandCall{id:BuiltinId,lhs:ValueOperand,rhs:ValueOperand}
#[derive(Clone,Debug)]
struct UnaryCompare{getter:BuiltinId,cmp:BuiltinId,slot:u32,rhs:ValueOperand}
#[derive(Clone,Debug)]
struct CaseBranch{datums:Vec<Value>,target:u32}

pub(crate) type WordProgramRef=Rc<WordProgram>;

#[derive(Debug)]
pub(crate) struct WordProgram{
    code:Vec<WordInstr>,
    constants:Vec<Value>,
    dynamics:Vec<std::rc::Rc<String>>,
    add_terms:Vec<Vec<Term>>,
    mul_terms:Vec<Vec<Term>>,
    binaries:Vec<Binary>,
    recurs:Vec<Recur>,
    builtins:Vec<BuiltinCall>,
    operand_calls:Vec<OperandCall>,
    list_refs:Vec<ValueOperand>,
    unary_compares:Vec<UnaryCompare>,
    cases:Vec<CaseBranch>,
    one_arg_calls:Vec<(u32,ValueOperand)>,
    max_temps:usize,
    preflight_cache:Cell<(usize,u64,i8)>,
    star_cache:Cell<i8>,
}

#[inline(always)]
fn execute_word_builtin(heap:&mut WordHeap,id:BuiltinId,args:&[Word])->Option<Word>{Some(match id{BuiltinId::Add=>{let mut out=0i64;for arg in args{out=out.checked_add(heap.integer_value(*arg)?)?}heap.integer(out)},BuiltinId::Sub if !args.is_empty()=>{let mut out=heap.integer_value(args[0])?;if args.len()==1{out=out.checked_neg()?}else{for arg in &args[1..]{out=out.checked_sub(heap.integer_value(*arg)?)?}}heap.integer(out)},BuiltinId::Mul=>{let mut out=1i64;for arg in args{out=out.checked_mul(heap.integer_value(*arg)?)?}heap.integer(out)},BuiltinId::NumEq|BuiltinId::Less|BuiltinId::LessEq|BuiltinId::Greater|BuiltinId::GreaterEq=>{let mut result=true;for pair in args.windows(2){let lhs=heap.integer_value(pair[0])?;let rhs=heap.integer_value(pair[1])?;result&=match id{BuiltinId::NumEq=>lhs==rhs,BuiltinId::Less=>lhs<rhs,BuiltinId::LessEq=>lhs<=rhs,BuiltinId::Greater=>lhs>rhs,BuiltinId::GreaterEq=>lhs>=rhs,_=>false};if !result{break}}Word::singleton(if result{Singleton::True}else{Singleton::False})},BuiltinId::Cons if args.len()==2=>heap.pair(args[0],args[1]),BuiltinId::Car if args.len()==1=>heap.pair_values(args[0])?.0,BuiltinId::Cdr if args.len()==1=>heap.pair_values(args[0])?.1,BuiltinId::Caar if args.len()==1=>heap.pair_values(heap.pair_values(args[0])?.0)?.0,BuiltinId::Cdar if args.len()==1=>heap.pair_values(heap.pair_values(args[0])?.0)?.1,BuiltinId::Cadr if args.len()==1=>heap.pair_values(heap.pair_values(args[0])?.1)?.0,BuiltinId::Cadar if args.len()==1=>heap.pair_values(heap.pair_values(heap.pair_values(args[0])?.0)?.1)?.0,BuiltinId::NullP if args.len()==1=>Word::singleton(if args[0].as_singleton()==Some(Singleton::Nil){Singleton::True}else{Singleton::False}),BuiltinId::PairP if args.len()==1=>Word::singleton(if heap.is_pair(args[0]){Singleton::True}else{Singleton::False}),BuiltinId::NumberP if args.len()==1=>Word::singleton(if heap.integer_value(args[0]).is_some(){Singleton::True}else{Singleton::False}),BuiltinId::CharP if args.len()==1=>Word::singleton(if args[0].as_character().is_some(){Singleton::True}else{Singleton::False}),BuiltinId::SymbolP if args.len()==1=>Word::singleton(if args[0].as_symbol().is_some(){Singleton::True}else{Singleton::False}),BuiltinId::BooleanP if args.len()==1=>Word::singleton(if matches!(args[0].as_singleton(),Some(Singleton::False|Singleton::True)){Singleton::True}else{Singleton::False}),BuiltinId::Not if args.len()==1=>Word::singleton(if heap.is_true(args[0]){Singleton::False}else{Singleton::True}),BuiltinId::EqP if args.len()==2=>Word::singleton(if args[0]==args[1]{Singleton::True}else{Singleton::False}),BuiltinId::EqualP if args.len()==2=>Word::singleton(if heap.equal(args[0],args[1]){Singleton::True}else{Singleton::False}),BuiltinId::Assoc if args.len()==2=>heap.assoc(args[0],args[1]).unwrap_or(Word::singleton(Singleton::False)),BuiltinId::Memq if args.len()==2=>heap.memq(args[0],args[1]).unwrap_or(Word::singleton(Singleton::False)),BuiltinId::Remainder if args.len()==2=>{let rhs=heap.integer_value(args[1])?;if rhs==0{return None}heap.integer(heap.integer_value(args[0])?%rhs)},BuiltinId::Modulo if args.len()==2=>{let rhs=heap.integer_value(args[1])?;if rhs<=0{return None}let lhs=heap.integer_value(args[0])?;heap.integer(((lhs%rhs)+rhs)%rhs)},BuiltinId::VectorRef if args.len()==2=>heap.vector_ref(args[0],heap.integer_value(args[1])?.try_into().ok()?)?,BuiltinId::ByteVectorRef if args.len()==2=>heap.byte_vector_ref(args[0],heap.integer_value(args[1])?.try_into().ok()?)?,BuiltinId::HashTable if args.len()%2==0=>heap.hash_table(args.chunks_exact(2).map(|pair|(pair[0],pair[1])).collect()),BuiltinId::HashRef if args.len()==2=>heap.hash_get(args[0],args[1]).unwrap_or(Word::singleton(Singleton::False)),BuiltinId::List=>heap.list(args.iter().copied()),BuiltinId::Length if args.len()==1=>{let length=if args[0].as_singleton()==Some(Singleton::Nil){0}else if heap.is_pair(args[0]){heap.list_length(args[0])?}else if let Some(length)=heap.vector_length(args[0]){length}else{heap.string_length(args[0])?};heap.integer(length.try_into().ok()?)},BuiltinId::ListRef if args.len()==2=>{let mut list=args[0];for _ in 0..usize::try_from(heap.integer_value(args[1])?).ok()?{list=heap.pair_values(list)?.1}heap.pair_values(list)?.0},BuiltinId::ObjectToString if args.len()==1=>heap.string(heap.object_string_acyclic(args[0])?),_=>return None})}

#[inline(always)]
fn execute_named_word_builtin(heap:&mut WordHeap,name:&str,args:&[Word])->Option<Word>{match name{"length" if args.len()==1=>{let length=if args[0].as_singleton()==Some(Singleton::Nil){0}else if heap.is_pair(args[0]){heap.list_length(args[0])?}else if let Some(length)=heap.vector_length(args[0]){length}else{heap.string_length(args[0])?};Some(heap.integer(length.try_into().ok()?))},"list"=>Some(heap.list(args.iter().copied())),"values"=>Some(match args{[]=>Word::singleton(Singleton::Unspecified),[value]=>*value,_=>heap.values(args.to_vec())}),"quotient" if args.len()==2=>{let rhs=heap.integer_value(args[1])?;if rhs==0{return None}Some(heap.integer(heap.integer_value(args[0])?/rhs))},_=>execute_word_builtin(heap,BuiltinId::from_name(name)?,args)}}

fn word_constant_too_large(value:&Value,budget:&mut usize)->bool{if *budget==0{return true}*budget-=1;match value{Value::Pair(pair)=>{let pair=pair.borrow();word_constant_too_large(&pair.car,budget)||word_constant_too_large(&pair.cdr,budget)},Value::Vector(values)=>values.values().iter().any(|value|word_constant_too_large(value,budget)),Value::HashTable(table)=>{let table=table.borrow();table.len()>256||table.iter().any(|(key,value)|word_constant_too_large(key,budget)||word_constant_too_large(value,budget))},Value::String(value)=>value.borrow().len()>1024,_=>false}}

fn word_pure_builtin(id:BuiltinId)->bool{matches!(id,BuiltinId::Add|BuiltinId::Sub|BuiltinId::Mul|BuiltinId::NumEq|BuiltinId::Less|BuiltinId::LessEq|BuiltinId::Greater|BuiltinId::GreaterEq|BuiltinId::Cons|BuiltinId::Car|BuiltinId::Cdr|BuiltinId::NullP|BuiltinId::PairP|BuiltinId::NumberP|BuiltinId::CharP|BuiltinId::SymbolP|BuiltinId::BooleanP|BuiltinId::Not|BuiltinId::EqP|BuiltinId::EqualP|BuiltinId::Assoc|BuiltinId::Memq|BuiltinId::Caar|BuiltinId::Cdar|BuiltinId::Cadr|BuiltinId::Cadar|BuiltinId::VectorRef|BuiltinId::ByteVectorRef|BuiltinId::HashTable|BuiltinId::HashRef|BuiltinId::Remainder|BuiltinId::Modulo|BuiltinId::ListRef|BuiltinId::Length|BuiltinId::List|BuiltinId::ObjectToString)}

fn bind_default(heap:&mut WordHeap,default:&Value)->Option<Word>{match default{Value::Bool(_)|Value::Nil|Value::Int(_)|Value::Float(_)|Value::RationalValue(_)|Value::ComplexValue(_)|Value::NumberLiteral(_,_ )|Value::Keyword(_)|Value::Char(_)|Value::NamedChar(_)|Value::String(_)|Value::Undefined|Value::Unspecified|Value::Eof=>Some(heap.from_value(default)),Value::Pair(_)=>{let items=default.to_vec().ok()?;(items.len()==2&&items[0].as_symbol()==Some("quote")).then(||heap.from_value(&items[1]))},_=>None}}

#[inline(always)]
fn bind_closure(heap:&mut WordHeap,params:&Params,keyword_slots:&[(u32,u8)],args:&[Word],captures:&[Word])->Option<([Word;16],usize)>{let total=params.required.len()+usize::from(params.rest.is_some())+captures.len();if total>16{return None}let undefined=Word::singleton(Singleton::Undefined);let mut slots=[undefined;16];let mut length;if params.star{if params.rest_before_formals||params.allow_other_keys{return None}let mut assigned=0u128;let mut positional=0usize;let mut saw_keyword=false;let mut index=0usize;while index<args.len(){if let Some(slot)=args[index].as_symbol().and_then(|id|keyword_slots.iter().find(|(keyword,_)|*keyword==id).map(|(_,slot)|*slot as usize)){saw_keyword=true;let value=*args.get(index+1)?;let bit=1u128<<slot;if assigned&bit!=0{return None}slots[slot]=value;assigned|=bit;index+=2;}else{if saw_keyword||positional>=params.required.len(){return None}slots[positional]=args[index];assigned|=1u128<<positional;positional+=1;index+=1;}}for slot in 0..params.required.len(){if assigned&(1u128<<slot)==0{let default=params.defaults.get(slot)?.as_ref()?;slots[slot]=bind_default(heap,default)?;}}length=params.required.len();}else{if args.len()<params.required.len()||params.rest.is_none()&&args.len()!=params.required.len(){return None}slots[..params.required.len()].copy_from_slice(&args[..params.required.len()]);length=params.required.len();if params.rest.is_some(){slots[length]=heap.list(args[params.required.len()..].iter().copied());length+=1;}}slots[length..length+captures.len()].copy_from_slice(captures);length+=captures.len();Some((slots,length))}

#[inline(always)]
fn call_word<F>(heap:&mut WordHeap,procedure:Word,args:&[Word],builtin:&mut F)->Option<Word> where F:FnMut(BuiltinId,Vec<Value>)->Option<Value>{if args.iter().any(|arg|heap.values_ref(*arg).is_some()){let undefined=Word::singleton(Singleton::Undefined);let mut expanded=[undefined;16];let mut length=0usize;for arg in args{if let Some(values)=heap.values_ref(*arg){if length+values.len()>expanded.len(){return None}expanded[length..length+values.len()].copy_from_slice(values);length+=values.len();}else{if length==expanded.len(){return None}expanded[length]=*arg;length+=1;}}return call_word_flat(heap,procedure,&expanded[..length],builtin)}call_word_flat(heap,procedure,args,builtin)}

#[inline(always)]
fn call_word_flat<F>(heap:&mut WordHeap,procedure:Word,args:&[Word],builtin:&mut F)->Option<Word> where F:FnMut(BuiltinId,Vec<Value>)->Option<Value>{if heap.is_hash_table(procedure)&&args.len()==1{return Some(heap.hash_get(procedure,args[0]).unwrap_or(Word::singleton(Singleton::False)))}if let Some(name)=heap.builtin_name(procedure){if name=="apply"{if args.len()<2||args.len()-2>16{return None}let target=args[0];let undefined=Word::singleton(Singleton::Undefined);let mut applied=[undefined;16];let prefix=args.len()-2;applied[..prefix].copy_from_slice(&args[1..args.len()-1]);let tail=heap.list_into(*args.last()?,&mut applied[prefix..])?;return call_word(heap,target,&applied[..prefix+tail],builtin)}return execute_named_word_builtin(heap,name,args)}let closure_ptr=heap.closure_ptr(procedure)?;// SAFETY: WordHeap allocations are nonmoving and the closure remains rooted by `procedure` for the nested call.
let closure=unsafe{closure_ptr.as_ref()};let (slots,length)=bind_closure(heap,&closure.params,&closure.keyword_slots,args,&closure.captures)?;closure.program.execute_words(heap,&slots[..length],&closure.constants,&closure.dynamics.borrow(),builtin)}

fn operand_value(_program:&WordProgram,_heap:&mut WordHeap,base:&[Word],temps:&[Word],constants:&[Word],operand:&ValueOperand)->Option<Word>{match operand{ValueOperand::Slot(index)=>if *index<base.len(){base.get(*index).copied()}else{temps.get(*index-base.len()).copied()},ValueOperand::Const(index)=>constants.get(*index).copied()}}

fn instruction(op:u8,a:usize,b:usize,c:usize)->WordInstr{WordInstr{op,flags:0,pad:0,a:a as u32,b:b as u32,c:c as u32}}
fn add_term(term:&AddTerm)->Option<Term>{Some(match term{AddTerm::Slot(slot)=>Term::Slot((*slot).try_into().ok()?),AddTerm::Const(value)=>Term::Int(*value)})}
fn mul_term(term:&MulTerm)->Option<Term>{Some(match term{MulTerm::Slot(slot)=>Term::Slot((*slot).try_into().ok()?),MulTerm::Const(value)=>Term::Int(*value)})}

impl WordProgram{
    pub(crate) fn profitable(&self)->bool{true}
    pub(crate) fn has_loop(&self)->bool{let result=self.code.iter().any(|instruction|instruction.op==OP_RECUR);record_eligibility(if result{"route:loop"}else{"route:no-loop"});result}
    pub(crate) fn compile_native_trace(&self)->Option<crate::native_jit::NativeWordTrace>{use crate::native_jit::{NativeWordOp as Op,NativeWordTerm as NwTerm};let convert=|source:&Term|match source{Term::Slot(slot)=>NwTerm::Temp(*slot),Term::Int(value)=>NwTerm::Int(*value)};let mut ops=Vec::with_capacity(self.code.len());for instruction in &self.code{ops.push(match instruction.op{OP_CONST=>Op::Const(instruction.a),OP_DYNAMIC=>Op::Dynamic(instruction.a),OP_SLOT|OP_LOAD_TEMP=>Op::Load(instruction.a),OP_STORE=>Op::Store(instruction.a),OP_POP=>Op::Pop,OP_JUMP=>Op::Jump(instruction.a),OP_JUMP_FALSE_POP=>Op::JumpFalse(instruction.a),OP_BINARY=>{let binary=self.binaries.get(instruction.a as usize)?;Op::Binary{kind:binary.kind,lhs:convert(&binary.lhs),rhs:convert(&binary.rhs)}},OP_ADD_RECUR=>Op::Add(self.add_terms.get(instruction.a as usize)?.iter().map(convert).collect()),OP_BUILTIN|OP_UNARY=>{let call=self.builtins.get(instruction.a as usize)?;if instruction.op==OP_BUILTIN&&call.id==BuiltinId::Add{Op::BuiltinAdd(call.argc)}else{Op::Host}},OP_GENERIC=>Op::Host,OP_RECUR=>{let recur=self.recurs.get(instruction.a as usize)?;if recur.argc!=recur.param_count{return None}Op::Recur{argc:recur.argc,target:recur.target,param_start:recur.param_start}},OP_RETURN=>Op::Return,_=>return None})}crate::native_jit::compile_word_trace(&ops)}
    pub(crate) fn has_star_closure(&self,env:&EnvRef)->bool{let cached=self.star_cache.get();if cached!=0{return cached>0}let result=self.star_seen(env,&mut HashSet::new());self.star_cache.set(if result{1}else{-1});record_eligibility(if result{"route:star-closure"}else{"route:no-star-closure"});result}
    fn star_seen(&self,env:&EnvRef,seen:&mut HashSet<usize>)->bool{for name in &self.dynamics{let Some(Value::Procedure(procedure))=env.get(name.as_str()) else{continue};let id=Rc::as_ptr(&procedure) as usize;if !seen.insert(id){continue}if let Procedure::Lambda{params,env,compiled:Some(compiled),..}=&*procedure{if params.star{return true}if let Some(bytecode)=&compiled.bytecode{if let Some(program)=Self::compile(bytecode){if program.star_seen(env,seen){return true}}}}}false}

    pub(crate) fn preflight(&self,env:&EnvRef)->bool{let cached=self.preflight_cache.get();if cached.2<0{return false}let key=(Rc::as_ptr(env) as usize,env.guard_generation());if (cached.0,cached.1)==key&&cached.2>0{return true}let result=self.preflight_seen(env,&mut HashSet::new());self.preflight_cache.set((key.0,key.1,if result{1}else{-1}));record_eligibility(if result{"preflight:eligible"}else{"preflight:rejected"});result}
    fn preflight_seen(&self,env:&EnvRef,seen:&mut HashSet<usize>)->bool{for name in &self.dynamics{let Some(value)=env.get(name.as_str()) else{continue};match value{Value::Procedure(procedure)=>{let id=Rc::as_ptr(&procedure) as usize;if !seen.insert(id){continue}match &*procedure{Procedure::Builtin{name,..}=>{if !matches!(*name,"length"|"list"|"values"|"quotient"|"apply")&&BuiltinId::from_name(name).map(|id|!word_pure_builtin(id)).unwrap_or(true){record_eligibility("preflight:impure-builtin");return false}},Procedure::Lambda{env,compiled:Some(compiled),..}=>{let Some(bytecode)=&compiled.bytecode else{record_eligibility("preflight:lambda-without-bytecode");return false};let Some(program)=Self::compile(bytecode) else{record_eligibility("preflight:ineligible-lambda");return false};if !program.preflight_seen(env,seen){record_eligibility("preflight:nested-rejection");return false}},_=>{record_eligibility("preflight:unsupported-procedure");return false}}},Value::RootMeta(_)|Value::Macro(_,_)|Value::Dilambda(_)=>{record_eligibility("preflight:observable-callable");return false},value=>{if word_constant_too_large(&value,&mut 256){record_eligibility("preflight:large-dynamic-value");return false}}}}true}

    pub(crate) fn compile(function:&BytecodeFunction)->Option<Self>{
        record_eligibility("attempt");
        if !function.word_calls_safe{record_eligibility("unsafe-call-shape");return None}
        if function.constants.iter().any(|value|word_constant_too_large(value,&mut 256)){record_eligibility("large-constant-graph");return None}
        let mut program=Self{code:Vec::with_capacity(function.code.len()),constants:function.constants.clone(),dynamics:Vec::new(),add_terms:Vec::new(),mul_terms:Vec::new(),binaries:Vec::new(),recurs:Vec::new(),builtins:Vec::new(),operand_calls:Vec::new(),list_refs:Vec::new(),unary_compares:Vec::new(),cases:Vec::new(),one_arg_calls:Vec::new(),max_temps:function.max_temps,preflight_cache:Cell::new((0,0,0)),star_cache:Cell::new(0)};
        for legacy in &function.code{
            let next=match legacy{
                Instr::LoadConst(index)=>instruction(OP_CONST,*index,0,0),
                Instr::LoadDynamic(name)=>{let index=program.dynamics.len();program.dynamics.push(name.clone());instruction(OP_DYNAMIC,index,0,0)},
                Instr::LoadSlot(index)=>instruction(OP_SLOT,*index,0,0),
                Instr::StoreTemp(index)|Instr::BindTemp{index,..}=>instruction(OP_STORE,*index,0,0),
                Instr::LoadTemp(index)=>instruction(OP_LOAD_TEMP,*index,0,0),
                Instr::Pop=>instruction(OP_POP,0,0,0),
                Instr::Jump(target)=>instruction(OP_JUMP,*target,0,0),
                Instr::JumpIfFalse(target)=>instruction(OP_JUMP_FALSE,*target,0,0),
                Instr::JumpIfFalsePop(target)=>instruction(OP_JUMP_FALSE_POP,*target,0,0),
                Instr::JumpIfOrTrue(target)=>instruction(OP_JUMP_OR_TRUE,*target,0,0),
                Instr::CaseJump{datums,target}=>{let index=program.cases.len();program.cases.push(CaseBranch{datums:datums.clone(),target:(*target).try_into().ok()?});instruction(OP_CASE,index,0,0)},
                Instr::BuiltinCall{id:BuiltinId::Apply,argc}=>instruction(OP_APPLY,*argc,0,0),
                Instr::BuiltinCall{id,argc}|Instr::FastBuiltinCall{id,argc} if word_pure_builtin(*id)=>{let index=program.builtins.len();program.builtins.push(BuiltinCall{id:*id,argc:(*argc).try_into().ok()?,slot:None});instruction(OP_BUILTIN,index,0,0)},
                Instr::UnarySlot{id,slot} if word_pure_builtin(*id)=>{let index=program.builtins.len();program.builtins.push(BuiltinCall{id:*id,argc:1,slot:Some((*slot).try_into().ok()?)});instruction(OP_UNARY,index,0,0)},
                Instr::GenericCall{argc}=>instruction(OP_GENERIC,*argc,0,0),
                Instr::OneArgSlotCall{callee,arg}=>{let index=program.one_arg_calls.len();program.one_arg_calls.push(((*callee).try_into().ok()?,arg.clone()));instruction(OP_ONE_ARG_SLOT,index,0,0)},
                Instr::BinaryOperands{id,lhs,rhs} if word_pure_builtin(*id)=>{let index=program.operand_calls.len();program.operand_calls.push(OperandCall{id:*id,lhs:lhs.clone(),rhs:rhs.clone()});instruction(OP_BINARY_OPERANDS,index,0,0)},
                Instr::ListRefSlot(slot)=>{let index=program.list_refs.len();program.list_refs.push(ValueOperand::Slot(*slot));instruction(OP_LIST_REF,index,0,0)},
                Instr::ListRefConst(constant)=>{let index=program.list_refs.len();program.list_refs.push(ValueOperand::Const(*constant));instruction(OP_LIST_REF,index,0,0)},
                Instr::UnaryCompareSlot{getter,cmp,slot,rhs} if word_pure_builtin(*getter)&&word_pure_builtin(*cmp)=>{let index=program.unary_compares.len();program.unary_compares.push(UnaryCompare{getter:*getter,cmp:*cmp,slot:(*slot).try_into().ok()?,rhs:rhs.clone()});instruction(OP_UNARY_COMPARE,index,0,0)},
                Instr::ApplicableRef=>instruction(OP_APPLICABLE_REF,0,0,0),
                Instr::ApplicableRefDynamic{target,index}=>{let dynamic=program.dynamics.len();program.dynamics.push(target.clone());program.code.push(instruction(OP_DYNAMIC,dynamic,0,0));program.code.push(match index{ValueOperand::Slot(slot)=>instruction(OP_SLOT,*slot,0,0),ValueOperand::Const(constant)=>instruction(OP_CONST,*constant,0,0)});instruction(OP_APPLICABLE_REF,0,0,0)},
                Instr::QCons=>instruction(OP_QCONS,0,0,0),
                Instr::QSpliceCons=>instruction(OP_QSPLICE_CONS,0,0,0),
                Instr::QVector(length)=>instruction(OP_QVECTOR,*length,0,0),
                Instr::IntAddTerms{terms}|Instr::IntAddTermsRecur{terms}=>{let terms=terms.iter().map(add_term).collect::<Option<Vec<_>>>()?;let index=program.add_terms.len();program.add_terms.push(terms);instruction(if matches!(legacy,Instr::IntAddTermsRecur{..}){OP_ADD_RECUR}else{OP_ADD},index,0,0)},
                Instr::IntMulTerms{terms}=>{let terms=terms.iter().map(mul_term).collect::<Option<Vec<_>>>()?;let index=program.mul_terms.len();program.mul_terms.push(terms);instruction(OP_MUL,index,0,0)},
                Instr::IntBinaryTerms{id,lhs,rhs}=>{let index=program.binaries.len();program.binaries.push(Binary{kind:*id,lhs:add_term(lhs)?,rhs:add_term(rhs)?});instruction(OP_BINARY,index,0,0)},
                Instr::Recur{argc,target,param_start,param_count,..}=>{let index=program.recurs.len();program.recurs.push(Recur{argc:(*argc).try_into().ok()?,target:(*target).try_into().ok()?,param_start:(*param_start).try_into().ok()?,param_count:(*param_count).try_into().ok()?});instruction(OP_RECUR,index,0,0)},
                Instr::Return=>instruction(OP_RETURN,0,0,0),
                _=>{record_instruction_rejection(legacy);return None},
            };
            program.code.push(next);
        }
        if program.dynamics.iter().any(|name|BuiltinId::from_name(name.as_str()).map(|id|!word_pure_builtin(id)).unwrap_or(false)){record_eligibility("impure-dynamic-builtin");return None}
        record_eligibility("eligible");
        Some(program)
    }

    pub(crate) fn resolve_constants(&self,heap:&mut WordHeap)->Vec<Word>{self.constants.iter().map(|value|heap.from_value(value)).collect()}
    pub(crate) fn resolve_dynamics(&self,heap:&mut WordHeap,env:&EnvRef)->Vec<Word>{self.dynamics.iter().filter_map(|name|env.get(name.as_str()).map(|value|heap.from_value(&value))).collect()}

    pub(crate) fn execute<F>(&self,slots:&[Value],env:Option<&EnvRef>,mut builtin:F)->Option<Value> where F:FnMut(BuiltinId,Vec<Value>)->Option<Value>{
        let mut heap=WordHeap::default();
        let slots=slots.iter().map(|value|heap.from_value(value)).collect::<Vec<_>>();
        let constants=self.resolve_constants(&mut heap);let dynamics=env.map(|env|self.resolve_dynamics(&mut heap,env)).unwrap_or_default();if dynamics.len()!=self.dynamics.len(){return None}
        let result=self.execute_words(&mut heap,&slots,&constants,&dynamics,&mut builtin)?;
        heap.to_value(result)
    }

    fn execute_words<F>(&self,heap:&mut WordHeap,base:&[Word],constants:&[Word],dynamics:&[Word],builtin:&mut F)->Option<Word> where F:FnMut(BuiltinId,Vec<Value>)->Option<Value>{
        let mut temps=vec![Word::singleton(Singleton::Unspecified);self.max_temps];
        let mut stack=Vec::with_capacity(16);
        let mut pc=0usize;
        loop{
            let instruction=*self.code.get(pc)?;pc+=1;
            let load=|index:usize,temps:&[Word]|if index<base.len(){base.get(index).copied()}else{temps.get(index-base.len()).copied()};
            let term=|term:Term,temps:&[Word],heap:&WordHeap|match term{Term::Int(value)=>Some(value),Term::Slot(index)=>heap.integer_value(load(index as usize,temps)?)};
            match instruction.op{
                OP_CONST=>stack.push(*constants.get(instruction.a as usize)?),
                OP_DYNAMIC=>stack.push(*dynamics.get(instruction.a as usize)?),
                OP_SLOT|OP_LOAD_TEMP=>stack.push(load(instruction.a as usize,&temps)?),
                OP_STORE=>{let value=stack.pop()?;*temps.get_mut((instruction.a as usize).checked_sub(base.len())?)?=value;}
                OP_POP=>{stack.pop()?;}
                OP_JUMP=>pc=instruction.a as usize,
                OP_JUMP_OR_TRUE=>{let value=stack.last_mut()?;if let Some(values)=heap.values_ref(*value){if let Some(hit)=values.iter().copied().find(|value|heap.is_true(*value)){*value=hit;pc=instruction.a as usize;}}else if heap.is_true(*value){pc=instruction.a as usize;}}
                OP_JUMP_FALSE|OP_JUMP_FALSE_POP=>{let value=if instruction.op==OP_JUMP_FALSE_POP{stack.pop()?}else{*stack.last()?};if !heap.is_true(value){pc=instruction.a as usize;}}
                OP_APPLICABLE_REF=>{let index=stack.pop()?;let target=stack.pop()?;let value=if heap.is_hash_table(target){heap.hash_get(target,index).unwrap_or(Word::singleton(Singleton::False))}else if let Some(raw)=heap.integer_value(index){heap.vector_ref(target,raw.try_into().ok()?)?}else{return None};stack.push(value);}
                OP_QCONS=>{let cdr=stack.pop()?;let car=stack.pop()?;if heap.values_ref(cdr).is_some(){return None}let value=if let Some(values)=heap.values_ref(car).map(|values|values.to_vec()){values.into_iter().rev().fold(cdr,|cdr,car|heap.pair(car,cdr))}else{heap.pair(car,cdr)};stack.push(value);}
                OP_QSPLICE_CONS=>{let tail=stack.pop()?;let mut splice=stack.pop()?;if let Some(values)=heap.values_ref(splice){if values.len()!=1{return None}splice=values[0]}let values=heap.list_values(splice)?;stack.push(values.into_iter().rev().fold(tail,|cdr,car|heap.pair(car,cdr)));}
                OP_QVECTOR=>{let length=instruction.a as usize;if stack.len()<length{return None}let values=stack.split_off(stack.len()-length);stack.push(heap.vector(values));}
                OP_CASE=>{let branch=self.cases.get(instruction.a as usize)?;let key=*stack.last()?;if branch.datums.iter().any(|datum|{let word=heap.from_value(datum);heap.equal(key,word)}){pc=branch.target as usize;}}
                OP_LIST_REF=>{let index=stack.pop()?;let target=operand_value(self,heap,base,&temps,&constants,self.list_refs.get(instruction.a as usize)?)?;stack.push(execute_word_builtin(heap,BuiltinId::ListRef,&[target,index])?);}
                OP_UNARY_COMPARE=>{let call=self.unary_compares.get(instruction.a as usize)?;let source=load(call.slot as usize,&temps)?;let got=execute_word_builtin(heap,call.getter,&[source])?;let rhs=operand_value(self,heap,base,&temps,&constants,&call.rhs)?;stack.push(execute_word_builtin(heap,call.cmp,&[got,rhs])?);}
                OP_BINARY_OPERANDS=>{let call=self.operand_calls.get(instruction.a as usize)?;let operand=|operand:&ValueOperand|match operand{ValueOperand::Slot(index)=>load(*index,&temps),ValueOperand::Const(index)=>constants.get(*index).copied()};let lhs=operand(&call.lhs)?;let rhs=operand(&call.rhs)?;stack.push(execute_word_builtin(heap,call.id,&[lhs,rhs])?);}
                OP_ONE_ARG_SLOT=>{let (callee,arg)=self.one_arg_calls.get(instruction.a as usize)?;let procedure=load(*callee as usize,&temps)?;let arg=operand_value(self,heap,base,&temps,&constants,arg)?;stack.push(call_word(heap,procedure,&[arg],builtin)?);}
                OP_APPLY=>{let argc=instruction.a as usize;if argc<2||stack.len()<argc||argc-2>16{return None}let start=stack.len()-argc;let procedure=stack[start];let undefined=Word::singleton(Singleton::Undefined);let mut args=[undefined;16];let prefix=argc-2;args[..prefix].copy_from_slice(&stack[start+1..stack.len()-1]);let tail=heap.list_into(*stack.last()?,&mut args[prefix..])?;let result=call_word(heap,procedure,&args[..prefix+tail],builtin)?;stack.truncate(start);stack.push(result);}
                OP_GENERIC=>{let argc=instruction.a as usize;if stack.len()<argc+1{return None}let start=stack.len()-argc;let procedure=stack[start-1];let result=call_word(heap,procedure,&stack[start..],builtin)?;stack.truncate(start-1);stack.push(result);}
                OP_BUILTIN|OP_UNARY=>{let call=*self.builtins.get(instruction.a as usize)?;if instruction.op==OP_UNARY{stack.push(load(call.slot? as usize,&temps)?);}if stack.len()<call.argc as usize{return None}let start=stack.len()-call.argc as usize;let result=if let Some(value)=execute_word_builtin(heap,call.id,&stack[start..]){value}else{let values=stack[start..].iter().map(|word|heap.to_value(*word)).collect::<Option<Vec<_>>>()?;heap.from_value(&builtin(call.id,values)?)};stack.truncate(start);stack.push(result);}
                OP_ADD|OP_ADD_RECUR=>{let terms=self.add_terms.get(instruction.a as usize)?;let mut value=0i64;for item in terms{value=if instruction.op==OP_ADD_RECUR{value.wrapping_add(term(*item,&temps,heap)?)}else{value.checked_add(term(*item,&temps,heap)?)?};}stack.push(heap.integer(value));}
                OP_MUL=>{let terms=self.mul_terms.get(instruction.a as usize)?;let mut value=1i64;for item in terms{value=value.checked_mul(term(*item,&temps,heap)?)?;}stack.push(heap.integer(value));}
                OP_BINARY=>{let binary=self.binaries.get(instruction.a as usize)?;let lhs=term(binary.lhs,&temps,heap)?;let rhs=term(binary.rhs,&temps,heap)?;let value=match binary.kind{BuiltinId::Sub=>heap.integer(lhs.checked_sub(rhs)?),BuiltinId::NumEq=>Word::singleton(if lhs==rhs{Singleton::True}else{Singleton::False}),BuiltinId::Less=>Word::singleton(if lhs<rhs{Singleton::True}else{Singleton::False}),BuiltinId::LessEq=>Word::singleton(if lhs<=rhs{Singleton::True}else{Singleton::False}),BuiltinId::Greater=>Word::singleton(if lhs>rhs{Singleton::True}else{Singleton::False}),BuiltinId::GreaterEq=>Word::singleton(if lhs>=rhs{Singleton::True}else{Singleton::False}),BuiltinId::Remainder if rhs!=0=>heap.integer(lhs%rhs),BuiltinId::Modulo if rhs>0=>heap.integer(((lhs%rhs)+rhs)%rhs),_=>return None};stack.push(value);}
                OP_RECUR=>{let recur=*self.recurs.get(instruction.a as usize)?;if recur.argc!=recur.param_count||stack.len()<recur.argc as usize{return None}let start=(recur.param_start as usize).checked_sub(base.len())?;for index in (0..recur.param_count as usize).rev(){*temps.get_mut(start+index)?=stack.pop()?;}pc=recur.target as usize;}
                OP_RETURN=>return Some(stack.pop().unwrap_or(Word::singleton(Singleton::Unspecified))),
                _=>return None,
            }
        }
    }
}


pub(crate) enum ResumeHostCall{Procedure{procedure:Value,args:Vec<Value>},Builtin{id:BuiltinId,args:Vec<Value>}}
pub(crate) enum ResumeHostOutcome{Value(Value),SchemeError(SchemeError),Suspend}
pub(crate) enum ResumeStatus{Success(Value),SchemeError(SchemeError),Continuation{pc:usize}}
#[derive(Debug,PartialEq,Eq)]
pub(crate) enum NativeStepStatus{Advanced,Continuation{pc:usize}}
pub(crate) enum ResumeHostStep{Advanced,SchemeError(SchemeError),Suspended{pc:usize}}
pub(crate) struct ResumableFrameStack{heap:WordHeap,frames:Vec<(ResumableWordMachine,crate::native_jit::NativeWordTrace)>}

pub(crate) struct ResumableWordMachine{program:WordProgramRef,heap:WordHeap,base:Vec<Word>,constants:Vec<Word>,dynamics:Vec<Word>,temps:Vec<Word>,stack:Vec<Word>,pc:usize,awaiting_result:bool}

impl ResumableWordMachine{
    pub(crate) fn new(program:WordProgramRef,slots:&[Value],env:&EnvRef)->Option<Self>{
        if program.code.iter().any(|instruction|!matches!(instruction.op,OP_CONST|OP_SLOT|OP_STORE|OP_LOAD_TEMP|OP_POP|OP_JUMP|OP_JUMP_FALSE|OP_JUMP_FALSE_POP|OP_ADD_RECUR|OP_BINARY|OP_RECUR|OP_RETURN|OP_DYNAMIC|OP_BUILTIN|OP_UNARY|OP_GENERIC)){return None}if program.code.iter().filter(|instruction|instruction.op==OP_BINARY).any(|instruction|program.binaries.get(instruction.a as usize).map(|binary|!matches!(binary.kind,BuiltinId::NumEq|BuiltinId::Less|BuiltinId::LessEq|BuiltinId::Greater|BuiltinId::GreaterEq|BuiltinId::Remainder|BuiltinId::Modulo)).unwrap_or(true)){return None}
        let mut heap=WordHeap::default();let base=slots.iter().map(|value|heap.from_value(value)).collect();let constants=program.resolve_constants(&mut heap);let dynamics=program.resolve_dynamics(&mut heap,env);if dynamics.len()!=program.dynamics.len(){return None}
        let temp_count=program.max_temps;Some(Self{program,heap,base,constants,dynamics,temps:vec![Word::singleton(Singleton::Unspecified);temp_count],stack:Vec::with_capacity(16),pc:0,awaiting_result:false})
    }
    fn new_shared(program:WordProgramRef,env:&EnvRef,heap:&mut WordHeap)->Option<Self>{let constants=program.resolve_constants(heap);let dynamics=program.resolve_dynamics(heap,env);if dynamics.len()!=program.dynamics.len(){return None}let temp_count=program.max_temps;Some(Self{program,heap:WordHeap::default(),base:Vec::new(),constants,dynamics,temps:vec![Word::singleton(Singleton::Unspecified);temp_count],stack:Vec::with_capacity(16),pc:0,awaiting_result:false})}
    fn take_generic_call(&mut self)->Option<(Word,Vec<Word>)>{let instruction=*self.program.code.get(self.pc)?;if instruction.op!=OP_GENERIC{return None}let argc=instruction.a as usize;if self.stack.len()<argc+1{return None}let start=self.stack.len()-argc;let procedure=self.stack[start-1];let args=self.stack[start..].to_vec();self.stack.truncate(start-1);self.pc+=1;Some((procedure,args))}
    fn take_native_word(&mut self)->Word{self.stack.pop().unwrap_or(Word::singleton(Singleton::Unspecified))}
    fn push_native_word(&mut self,value:Word){self.stack.push(value)}
    pub(crate) fn continuation_pc(&self)->usize{self.pc}
    pub(crate) fn compile_native_trace(&self)->Option<crate::native_jit::NativeWordTrace>{self.program.compile_native_trace()}
    pub(crate) fn run_native_trace(&mut self,trace:&crate::native_jit::NativeWordTrace)->i32{if !self.base.is_empty(){return 0}self.stack.reserve(64usize.saturating_sub(self.stack.len()));let mut frame=crate::native_jit::NativeWordFrame{constants:self.constants.as_ptr().cast::<u64>(),dynamics:self.dynamics.as_ptr().cast::<u64>(),temps:self.temps.as_mut_ptr().cast::<u64>(),stack:self.stack.as_mut_ptr().cast::<u64>(),sp:self.stack.len() as u64,pc:self.pc as u64};let status=trace.run(&mut frame);assert!(frame.sp as usize<=self.stack.capacity());unsafe{self.stack.set_len(frame.sp as usize)}self.pc=frame.pc as usize;status}
    pub(crate) fn finish_native(&mut self)->ResumeStatus{let word=self.stack.pop().unwrap_or(Word::singleton(Singleton::Unspecified));match self.heap.to_value(word){Some(value)=>ResumeStatus::Success(value),None=>ResumeStatus::SchemeError(SchemeError::new("unsupported-compiled-form",vec![]))}}
    pub(crate) fn resume_host_once<F>(&mut self,host:&mut F)->ResumeHostStep where F:FnMut(ResumeHostCall)->ResumeHostOutcome{if self.awaiting_result{return ResumeHostStep::Suspended{pc:self.pc}}let Some(instruction)=self.program.code.get(self.pc).copied() else{return ResumeHostStep::SchemeError(SchemeError::new("unsupported-compiled-form",vec![]))};self.pc+=1;match instruction.op{OP_GENERIC=>{let argc=instruction.a as usize;if self.stack.len()<argc+1{return ResumeHostStep::SchemeError(SchemeError::new("unsupported-compiled-form",vec![]))}let start=self.stack.len()-argc;let procedure_word=self.stack[start-1];if let Some(name)=self.heap.builtin_name(procedure_word){if let Some(value)=execute_named_word_builtin(&mut self.heap,name,&self.stack[start..]){self.stack.truncate(start-1);self.stack.push(value);return ResumeHostStep::Advanced}}let Some(procedure)=self.heap.to_value(procedure_word) else{return ResumeHostStep::SchemeError(SchemeError::new("unsupported-compiled-form",vec![]))};let Some(args)=self.stack[start..].iter().map(|word|self.heap.to_value(*word)).collect::<Option<Vec<_>>>() else{return ResumeHostStep::SchemeError(SchemeError::new("unsupported-compiled-form",vec![]))};self.stack.truncate(start-1);match host(ResumeHostCall::Procedure{procedure,args}){ResumeHostOutcome::Value(value)=>{let word=self.heap.from_value(&value);self.stack.push(word);ResumeHostStep::Advanced},ResumeHostOutcome::SchemeError(error)=>ResumeHostStep::SchemeError(error),ResumeHostOutcome::Suspend=>{self.awaiting_result=true;ResumeHostStep::Suspended{pc:self.pc}}}},OP_BUILTIN|OP_UNARY=>{let Some(call)=self.program.builtins.get(instruction.a as usize).copied() else{return ResumeHostStep::SchemeError(SchemeError::new("unsupported-compiled-form",vec![]))};if instruction.op==OP_UNARY{return ResumeHostStep::SchemeError(SchemeError::new("unsupported-compiled-form",vec![]))}let argc=call.argc as usize;if self.stack.len()<argc{return ResumeHostStep::SchemeError(SchemeError::new("unsupported-compiled-form",vec![]))}let start=self.stack.len()-argc;let Some(args)=self.stack[start..].iter().map(|word|self.heap.to_value(*word)).collect::<Option<Vec<_>>>() else{return ResumeHostStep::SchemeError(SchemeError::new("unsupported-compiled-form",vec![]))};self.stack.truncate(start);match host(ResumeHostCall::Builtin{id:call.id,args}){ResumeHostOutcome::Value(value)=>{let word=self.heap.from_value(&value);self.stack.push(word);ResumeHostStep::Advanced},ResumeHostOutcome::SchemeError(error)=>ResumeHostStep::SchemeError(error),ResumeHostOutcome::Suspend=>{self.awaiting_result=true;ResumeHostStep::Suspended{pc:self.pc}}}},_=>{self.pc-=1;ResumeHostStep::SchemeError(SchemeError::new("unsupported-compiled-form",vec![]))}}}
    pub(crate) fn try_native_add(&mut self,trace:&crate::native_jit::NativeResumeAdd)->NativeStepStatus{let Some(instruction)=self.program.code.get(self.pc) else{return NativeStepStatus::Continuation{pc:self.pc}};if instruction.op!=OP_BUILTIN{return NativeStepStatus::Continuation{pc:self.pc}}let Some(call)=self.program.builtins.get(instruction.a as usize) else{return NativeStepStatus::Continuation{pc:self.pc}};if call.id!=BuiltinId::Add||call.argc!=2||self.stack.len()<2{return NativeStepStatus::Continuation{pc:self.pc}}let start=self.stack.len()-2;let Some(lhs)=self.heap.integer_value(self.stack[start]) else{return NativeStepStatus::Continuation{pc:self.pc}};let Some(rhs)=self.heap.integer_value(self.stack[start+1]) else{return NativeStepStatus::Continuation{pc:self.pc}};let Some(sum)=trace.run(lhs,rhs) else{return NativeStepStatus::Continuation{pc:self.pc}};self.stack.truncate(start);self.stack.push(self.heap.integer(sum));self.pc+=1;NativeStepStatus::Advanced}
    pub(crate) fn collect_roots(&mut self){let mut roots=Vec::with_capacity(self.base.len()+self.constants.len()+self.dynamics.len()+self.temps.len()+self.stack.len());roots.extend_from_slice(&self.base);roots.extend_from_slice(&self.constants);roots.extend_from_slice(&self.dynamics);roots.extend_from_slice(&self.temps);roots.extend_from_slice(&self.stack);self.heap.collect(&roots)}
    pub(crate) fn supply_call_result(&mut self,value:Value)->bool{if !self.awaiting_result{return false}let word=self.heap.from_value(&value);self.stack.push(word);self.awaiting_result=false;true}
    pub(crate) fn resume<F>(&mut self,host:&mut F)->ResumeStatus where F:FnMut(ResumeHostCall)->ResumeHostOutcome{
        if self.awaiting_result{return ResumeStatus::Continuation{pc:self.pc}}
        loop{
            let Some(instruction)=self.program.code.get(self.pc).copied() else{return ResumeStatus::SchemeError(SchemeError::new("unsupported-compiled-form",vec![]))};self.pc+=1;
            let load=|index:usize,temps:&[Word],base:&[Word]|if index<base.len(){base.get(index).copied()}else{temps.get(index-base.len()).copied()};
            let term=|term:Term,temps:&[Word],base:&[Word],heap:&WordHeap|match term{Term::Int(value)=>Some(value),Term::Slot(index)=>heap.integer_value(load(index as usize,temps,base)?)};
            match instruction.op{
                OP_CONST=>self.stack.push(*self.constants.get(instruction.a as usize).unwrap()),
                OP_DYNAMIC=>self.stack.push(*self.dynamics.get(instruction.a as usize).unwrap()),
                OP_SLOT|OP_LOAD_TEMP=>{let Some(value)=load(instruction.a as usize,&self.temps,&self.base) else{return ResumeStatus::SchemeError(SchemeError::new("unsupported-compiled-form",vec![]))};self.stack.push(value)},
                OP_STORE=>{let Some(value)=self.stack.pop() else{return ResumeStatus::SchemeError(SchemeError::new("unsupported-compiled-form",vec![]))};let Some(index)=(instruction.a as usize).checked_sub(self.base.len()) else{return ResumeStatus::SchemeError(SchemeError::new("unsupported-compiled-form",vec![]))};let Some(slot)=self.temps.get_mut(index) else{return ResumeStatus::SchemeError(SchemeError::new("unsupported-compiled-form",vec![]))};*slot=value},
                OP_POP=>{self.stack.pop();},OP_JUMP=>self.pc=instruction.a as usize,
                OP_JUMP_FALSE|OP_JUMP_FALSE_POP=>{let value=if instruction.op==OP_JUMP_FALSE_POP{self.stack.pop()}else{self.stack.last().copied()};let Some(value)=value else{return ResumeStatus::SchemeError(SchemeError::new("unsupported-compiled-form",vec![]))};if !self.heap.is_true(value){self.pc=instruction.a as usize}},
                OP_BINARY=>{let Some(binary)=self.program.binaries.get(instruction.a as usize) else{return ResumeStatus::SchemeError(SchemeError::new("unsupported-compiled-form",vec![]))};let Some(lhs)=term(binary.lhs,&self.temps,&self.base,&self.heap) else{return ResumeStatus::SchemeError(SchemeError::new("wrong-type-arg",vec![]))};let Some(rhs)=term(binary.rhs,&self.temps,&self.base,&self.heap) else{return ResumeStatus::SchemeError(SchemeError::new("wrong-type-arg",vec![]))};let value=match binary.kind{BuiltinId::NumEq=>Word::singleton(if lhs==rhs{Singleton::True}else{Singleton::False}),BuiltinId::Less=>Word::singleton(if lhs<rhs{Singleton::True}else{Singleton::False}),BuiltinId::LessEq=>Word::singleton(if lhs<=rhs{Singleton::True}else{Singleton::False}),BuiltinId::Greater=>Word::singleton(if lhs>rhs{Singleton::True}else{Singleton::False}),BuiltinId::GreaterEq=>Word::singleton(if lhs>=rhs{Singleton::True}else{Singleton::False}),BuiltinId::Sub=>self.heap.integer(lhs.wrapping_sub(rhs)),BuiltinId::Remainder if rhs!=0=>self.heap.integer(lhs%rhs),BuiltinId::Modulo if rhs>0=>self.heap.integer(((lhs%rhs)+rhs)%rhs),_=>return ResumeStatus::SchemeError(SchemeError::new("unsupported-compiled-form",vec![]))};self.stack.push(value)},
                OP_ADD|OP_ADD_RECUR=>{let Some(terms)=self.program.add_terms.get(instruction.a as usize) else{return ResumeStatus::SchemeError(SchemeError::new("unsupported-compiled-form",vec![]))};let mut value=0i64;for item in terms{let Some(next)=term(*item,&self.temps,&self.base,&self.heap) else{return ResumeStatus::SchemeError(SchemeError::new("wrong-type-arg",vec![]))};value=value.wrapping_add(next)}self.stack.push(self.heap.integer(value))},
                OP_GENERIC=>{let argc=instruction.a as usize;if self.stack.len()<argc+1{return ResumeStatus::SchemeError(SchemeError::new("unsupported-compiled-form",vec![]))}let start=self.stack.len()-argc;let procedure_word=self.stack[start-1];if let Some(name)=self.heap.builtin_name(procedure_word){if let Some(value)=execute_named_word_builtin(&mut self.heap,name,&self.stack[start..]){self.stack.truncate(start-1);self.stack.push(value);continue}}let Some(procedure)=self.heap.to_value(procedure_word) else{return ResumeStatus::SchemeError(SchemeError::new("unsupported-compiled-form",vec![]))};let Some(args)=self.stack[start..].iter().map(|word|self.heap.to_value(*word)).collect::<Option<Vec<_>>>() else{return ResumeStatus::SchemeError(SchemeError::new("unsupported-compiled-form",vec![]))};self.stack.truncate(start-1);match host(ResumeHostCall::Procedure{procedure,args}){ResumeHostOutcome::Value(value)=>{let word=self.heap.from_value(&value);self.stack.push(word)},ResumeHostOutcome::SchemeError(error)=>return ResumeStatus::SchemeError(error),ResumeHostOutcome::Suspend=>{self.awaiting_result=true;return ResumeStatus::Continuation{pc:self.pc}}}},
                OP_BUILTIN|OP_UNARY=>{let Some(call)=self.program.builtins.get(instruction.a as usize).copied() else{return ResumeStatus::SchemeError(SchemeError::new("unsupported-compiled-form",vec![]))};if instruction.op==OP_UNARY{let Some(value)=load(call.slot.unwrap() as usize,&self.temps,&self.base) else{return ResumeStatus::SchemeError(SchemeError::new("unsupported-compiled-form",vec![]))};self.stack.push(value)}let argc=call.argc as usize;if self.stack.len()<argc{return ResumeStatus::SchemeError(SchemeError::new("unsupported-compiled-form",vec![]))}let start=self.stack.len()-argc;if let Some(value)=execute_word_builtin(&mut self.heap,call.id,&self.stack[start..]){self.stack.truncate(start);self.stack.push(value);continue}let Some(args)=self.stack[start..].iter().map(|word|self.heap.to_value(*word)).collect::<Option<Vec<_>>>() else{return ResumeStatus::SchemeError(SchemeError::new("unsupported-compiled-form",vec![]))};self.stack.truncate(start);match host(ResumeHostCall::Builtin{id:call.id,args}){ResumeHostOutcome::Value(value)=>{let word=self.heap.from_value(&value);self.stack.push(word)},ResumeHostOutcome::SchemeError(error)=>return ResumeStatus::SchemeError(error),ResumeHostOutcome::Suspend=>{self.awaiting_result=true;return ResumeStatus::Continuation{pc:self.pc}}}},
                OP_RECUR=>{let Some(recur)=self.program.recurs.get(instruction.a as usize).copied() else{return ResumeStatus::SchemeError(SchemeError::new("unsupported-compiled-form",vec![]))};if recur.argc!=recur.param_count||self.stack.len()<recur.argc as usize{return ResumeStatus::SchemeError(SchemeError::new("unsupported-compiled-form",vec![]))}let Some(start)=(recur.param_start as usize).checked_sub(self.base.len()) else{return ResumeStatus::SchemeError(SchemeError::new("unsupported-compiled-form",vec![]))};for index in (0..recur.param_count as usize).rev(){let Some(value)=self.stack.pop() else{return ResumeStatus::SchemeError(SchemeError::new("unsupported-compiled-form",vec![]))};let Some(slot)=self.temps.get_mut(start+index) else{return ResumeStatus::SchemeError(SchemeError::new("unsupported-compiled-form",vec![]))};*slot=value}self.pc=recur.target as usize},
                OP_RETURN=>{let word=self.stack.pop().unwrap_or(Word::singleton(Singleton::Unspecified));return match self.heap.to_value(word){Some(value)=>ResumeStatus::Success(value),None=>ResumeStatus::SchemeError(SchemeError::new("unsupported-compiled-form",vec![]))}},
                _=>return ResumeStatus::SchemeError(SchemeError::new("unsupported-compiled-form",vec![])),
            }
        }
    }
}

impl ResumableFrameStack{
    pub(crate) fn new(mut root:ResumableWordMachine)->Option<Self>{let trace=root.compile_native_trace()?;let heap=std::mem::take(&mut root.heap);Some(Self{heap,frames:vec![(root,trace)]})}
    pub(crate) fn run_native_top(&mut self)->i32{let (frame,trace)=self.frames.last_mut().expect("resumable frame");std::mem::swap(&mut frame.heap,&mut self.heap);let status=frame.run_native_trace(trace);std::mem::swap(&mut frame.heap,&mut self.heap);status}
    pub(crate) fn push_child_for_current_call(&mut self,program:WordProgramRef,env:&EnvRef)->bool{let Some((parent,_))=self.frames.last_mut() else{return false};let Some((_procedure,_args))=parent.take_generic_call() else{return false};let Some(child)=ResumableWordMachine::new_shared(program,env,&mut self.heap) else{return false};let Some(trace)=child.compile_native_trace() else{return false};self.frames.push((child,trace));true}
    fn push_current_closure(&mut self)->bool{let Some((parent,_))=self.frames.last() else{return false};let Some(instruction)=parent.program.code.get(parent.pc) else{return false};if instruction.op!=OP_GENERIC{return false}let argc=instruction.a as usize;if parent.stack.len()<argc+1{return false}let start=parent.stack.len()-argc;let procedure=parent.stack[start-1];let args=parent.stack[start..].to_vec();let Some((params,program,captures,keyword_slots,constants,dynamics))=self.heap.closure_frame_parts(procedure) else{return false};let Some((slots,length))=bind_closure(&mut self.heap,&params,&keyword_slots,&args,&captures) else{return false};let mut temps=vec![Word::singleton(Singleton::Unspecified);length+program.max_temps];temps[..length].copy_from_slice(&slots[..length]);let child=ResumableWordMachine{program,heap:WordHeap::default(),base:Vec::new(),constants,dynamics,temps,stack:Vec::with_capacity(16),pc:0,awaiting_result:false};let Some(trace)=child.compile_native_trace() else{return false};let Some((parent,_))=self.frames.last_mut() else{return false};if parent.take_generic_call().is_none(){return false}self.frames.push((child,trace));true}
    pub(crate) fn run<F>(&mut self,host:&mut F)->ResumeStatus where F:FnMut(ResumeHostCall)->ResumeHostOutcome{loop{if self.run_native_top()==1{let mut completed=self.frames.pop().expect("completed frame").0;let word=completed.take_native_word();if let Some((parent,_))=self.frames.last_mut(){parent.push_native_word(word);continue}return match self.heap.to_value(word){Some(value)=>ResumeStatus::Success(value),None=>ResumeStatus::SchemeError(SchemeError::new("unsupported-compiled-form",vec![]))}}if self.push_current_closure(){continue}let (frame,_)=self.frames.last_mut().expect("resumable frame");std::mem::swap(&mut frame.heap,&mut self.heap);let step=frame.resume_host_once(host);std::mem::swap(&mut frame.heap,&mut self.heap);match step{ResumeHostStep::Advanced=>{},ResumeHostStep::SchemeError(error)=>return ResumeStatus::SchemeError(error),ResumeHostStep::Suspended{pc}=>return ResumeStatus::Continuation{pc}}}}
}

const _:[();16]=[();std::mem::size_of::<WordInstr>()];
