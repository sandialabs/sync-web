//! Experimental 8-byte evaluator word and nonmoving heap.
//!
//! This module is deliberately not connected to production evaluation yet.
//! A production switch happens only after a complete bytecode island can keep
//! values as `Word`s across loads, control flow, calls, and allocation.

#![allow(dead_code)]

use std::cell::{Cell,RefCell};
use std::collections::HashMap;
use std::ptr::NonNull;
use std::rc::Rc;
use std::hash::BuildHasherDefault;

use crate::collections::equal as legacy_equal;
use crate::compiled::BuiltinId;
use crate::core::{EnvRef,FnvHasher,Params,Procedure,Value,VectorData};
use crate::word_bytecode::{WordProgram,WordProgramRef};


const TAG_MASK:u64=0b111;
const TAG_PTR:u64=0b000;
const TAG_FIXNUM:u64=0b001;
const TAG_SINGLETON:u64=0b010;
const TAG_CHAR:u64=0b011;
const TAG_SYMBOL:u64=0b100;
const TAG_PAIR:u64=0b101;

pub(crate) const FIXNUM_MIN:i64=i64::MIN>>3;
pub(crate) const FIXNUM_MAX:i64=i64::MAX>>3;

#[derive(Clone,Copy,Debug,PartialEq,Eq,Hash)]
#[repr(transparent)]
pub(crate) struct Word(u64);

#[derive(Clone,Copy,Debug,PartialEq,Eq)]
#[repr(u8)]
pub(crate) enum Singleton{False,True,Nil,Unspecified,Undefined,Eof}

#[repr(C,align(8))]
pub(crate) struct HeapObject{active:Cell<bool>,marked:Cell<bool>,origin:Cell<bool>,visit:Cell<u32>,payload:HeapPayload}

pub(crate) enum HeapPayload{
    Integer(i64),
    String(RefCell<String>),
    Vector(RefCell<Vec<Word>>),
    HashTable(RefCell<Vec<(Word,Word)>>),
    Environment(RefCell<WordEnvironment>),
    Builtin(BuiltinId),
    NamedBuiltin(&'static str),
    Closure(WordClosure),
    Values(Vec<Word>),
    Values2([Word;2]),
    Legacy(Value),
}

#[repr(C,align(8))]
pub(crate) struct PairObject{active:Cell<bool>,marked:Cell<bool>,origin:Cell<bool>,visit:Cell<u32>,pair:RefCell<(Word,Word)>}

pub(crate) struct WordClosure{pub(crate) params:Params,pub(crate) program:WordProgramRef,pub(crate) captures:Vec<Word>,pub(crate) keyword_slots:Vec<(u32,u8)>,pub(crate) constants:Vec<Word>,pub(crate) dynamics:RefCell<Vec<Word>>,pub(crate) env:EnvRef,pub(crate) legacy:Value}
pub(crate) struct WordEnvironment{parent:Option<Word>,vars:Vec<(u32,Word)>}
pub(crate) struct SymbolMeta{name:String,keyword:bool,gensym:bool}

type WordMap<K,V>=HashMap<K,V,BuildHasherDefault<FnvHasher>>;
type WordSet<K>=std::collections::HashSet<K,BuildHasherDefault<FnvHasher>>;

#[derive(Default)]
struct ExportMemo{small:Vec<(Word,Value)>,large:Option<WordMap<Word,Value>>}
impl ExportMemo{
    fn get(&self,word:Word)->Option<&Value>{if let Some(large)=&self.large{return large.get(&word)}self.small.iter().find(|(candidate,_)|*candidate==word).map(|(_,value)|value)}
    fn insert(&mut self,word:Word,value:Value){if let Some(large)=&mut self.large{large.insert(word,value);return}if self.small.len()<16{self.small.push((word,value));return}let mut large=WordMap::with_capacity_and_hasher(self.small.len()*2,BuildHasherDefault::default());large.extend(self.small.drain(..));large.insert(word,value);self.large=Some(large);}
}

#[derive(Default)]
pub(crate) struct WordHeap{chunks:Vec<Box<[HeapObject]>>,free:Vec<NonNull<HeapObject>>,pair_chunks:Vec<Box<[PairObject]>>,pair_free:Vec<NonNull<PairObject>>,object_count:usize,export_epoch:Cell<u32>,symbols:Vec<SymbolMeta>,symbol_ids:WordMap<String,u32>,keyword_ids:WordMap<String,u32>,legacy_objects:WordMap<(u8,usize),Word>,legacy_origins:WordMap<Word,Value>}

impl Word{
    pub(crate) fn raw(self)->u64{self.0}
    pub(crate) fn from_raw(raw:u64)->Self{Self(raw)}
    pub(crate) fn fixnum(value:i64)->Option<Self>{
        (FIXNUM_MIN..=FIXNUM_MAX).contains(&value).then(||Self(((value as u64)<<3)|TAG_FIXNUM))
    }

    pub(crate) fn as_fixnum(self)->Option<i64>{
        (self.0&TAG_MASK==TAG_FIXNUM).then_some((self.0 as i64)>>3)
    }

    pub(crate) fn singleton(value:Singleton)->Self{Self(((value as u64)<<3)|TAG_SINGLETON)}

    pub(crate) fn as_singleton(self)->Option<Singleton>{
        if self.0&TAG_MASK!=TAG_SINGLETON{return None}
        match self.0>>3{0=>Some(Singleton::False),1=>Some(Singleton::True),2=>Some(Singleton::Nil),3=>Some(Singleton::Unspecified),4=>Some(Singleton::Undefined),5=>Some(Singleton::Eof),_=>None}
    }

    pub(crate) fn character(value:char)->Self{Self(((value as u32 as u64)<<3)|TAG_CHAR)}
    pub(crate) fn as_character(self)->Option<char>{if self.0&TAG_MASK!=TAG_CHAR{return None}char::from_u32((self.0>>3) as u32)}

    pub(crate) fn symbol(id:u32)->Self{Self(((id as u64)<<3)|TAG_SYMBOL)}
    pub(crate) fn as_symbol(self)->Option<u32>{(self.0&TAG_MASK==TAG_SYMBOL).then_some((self.0>>3) as u32)}

    fn pointer(ptr:NonNull<HeapObject>)->Self{let raw=ptr.as_ptr() as u64;debug_assert_ne!(raw,0);debug_assert_eq!(raw&TAG_MASK,TAG_PTR);Self(raw)}
    fn as_ptr(self)->Option<NonNull<HeapObject>>{if self.0!=0&&self.0&TAG_MASK==TAG_PTR{NonNull::new(self.0 as *mut HeapObject)}else{None}}
    fn pair_pointer(ptr:NonNull<PairObject>)->Self{let raw=ptr.as_ptr() as u64;debug_assert_eq!(raw&TAG_MASK,0);Self(raw|TAG_PAIR)}
    fn as_pair_ptr(self)->Option<NonNull<PairObject>>{if self.0&TAG_MASK==TAG_PAIR{NonNull::new((self.0&!TAG_MASK) as *mut PairObject)}else{None}}
}

impl WordHeap{
    fn allocate(&mut self,payload:HeapPayload)->Word{
        if self.free.is_empty(){let mut chunk=(0..4096).map(|_|HeapObject{active:Cell::new(false),marked:Cell::new(false),origin:Cell::new(false),visit:Cell::new(0),payload:HeapPayload::Integer(0)}).collect::<Vec<_>>().into_boxed_slice();for object in chunk.iter_mut(){self.free.push(NonNull::from(object));}self.chunks.push(chunk);}
        let ptr=self.free.pop().expect("word heap free slot");let object=unsafe{&mut*ptr.as_ptr()};object.payload=payload;object.marked.set(false);object.origin.set(false);object.active.set(true);self.object_count+=1;Word::pointer(ptr)
    }

    pub(crate) fn object_count(&self)->usize{self.object_count}
    fn mark_origin(word:Word){if let Some(ptr)=word.as_pair_ptr(){unsafe{ptr.as_ref()}.origin.set(true)}else if let Some(ptr)=word.as_ptr(){unsafe{ptr.as_ref()}.origin.set(true)}}
    fn has_origin(word:Word)->bool{if let Some(ptr)=word.as_pair_ptr(){unsafe{ptr.as_ref()}.origin.get()}else{word.as_ptr().map(|ptr|unsafe{ptr.as_ref()}.origin.get()).unwrap_or(false)}}

    unsafe fn mark_word(word:Word){if let Some(ptr)=word.as_pair_ptr(){let pair=unsafe{ptr.as_ref()};if !pair.active.get()||pair.marked.replace(true){return}let (car,cdr)=*pair.pair.borrow();unsafe{Self::mark_word(car);Self::mark_word(cdr);}return}let Some(ptr)=word.as_ptr() else{return};let object=unsafe{ptr.as_ref()};if !object.active.get()||object.marked.replace(true){return}match &object.payload{HeapPayload::Vector(values)=>for value in values.borrow().iter().copied(){unsafe{Self::mark_word(value);}},HeapPayload::HashTable(entries)=>for (key,value) in entries.borrow().iter().copied(){unsafe{Self::mark_word(key);Self::mark_word(value);}},HeapPayload::Environment(environment)=>{let environment=environment.borrow();if let Some(parent)=environment.parent{unsafe{Self::mark_word(parent)}}for (_,value) in &environment.vars{unsafe{Self::mark_word(*value);}}},HeapPayload::Integer(_)|HeapPayload::String(_)|HeapPayload::Builtin(_)|HeapPayload::NamedBuiltin(_)|HeapPayload::Legacy(_)=>{},HeapPayload::Closure(closure)=>{for value in &closure.captures{unsafe{Self::mark_word(*value);}}for value in &closure.constants{unsafe{Self::mark_word(*value);}}for value in closure.dynamics.borrow().iter().copied(){unsafe{Self::mark_word(value);}}},HeapPayload::Values(values)=>for value in values{unsafe{Self::mark_word(*value);}},HeapPayload::Values2(values)=>for value in values{unsafe{Self::mark_word(*value);}}}}

    fn marked(word:Word)->bool{if let Some(ptr)=word.as_pair_ptr(){return unsafe{ptr.as_ref()}.marked.get()}word.as_ptr().map(|ptr|unsafe{ptr.as_ref()}.marked.get()).unwrap_or(true)}

    pub(crate) fn collect(&mut self,roots:&[Word]){for chunk in &self.chunks{for object in chunk.iter(){if object.active.get(){object.marked.set(false)}}}for chunk in &self.pair_chunks{for pair in chunk.iter(){if pair.active.get(){pair.marked.set(false)}}}for root in roots{unsafe{Self::mark_word(*root)}}self.legacy_objects.retain(|_,word|Self::marked(*word));self.legacy_origins.retain(|word,_|Self::marked(*word));for chunk in self.chunks.iter_mut(){for object in chunk.iter_mut(){if object.active.get()&&!object.marked.get(){object.payload=HeapPayload::Integer(0);object.active.set(false);self.object_count-=1;self.free.push(NonNull::from(object));}}}for chunk in self.pair_chunks.iter_mut(){for pair in chunk.iter_mut(){if pair.active.get()&&!pair.marked.get(){pair.pair.replace((Word::singleton(Singleton::Nil),Word::singleton(Singleton::Nil)));pair.active.set(false);self.object_count-=1;self.pair_free.push(NonNull::from(pair));}}}}

    pub(crate) fn integer(&mut self,value:i64)->Word{Word::fixnum(value).unwrap_or_else(||self.allocate(HeapPayload::Integer(value)))}

    pub(crate) fn integer_value(&self,word:Word)->Option<i64>{
        if let Some(value)=word.as_fixnum(){return Some(value)}
        let ptr=word.as_ptr()?;
        // SAFETY: WordHeap owns every pointer it creates, objects never move,
        // and this method only accepts Words whose heap outlives the borrow.
        match &unsafe{ptr.as_ref()}.payload{HeapPayload::Integer(value)=>Some(*value),_=>None}
    }

    pub(crate) fn from_value(&mut self,value:&Value)->Word{match value{Value::Bool(false)=>Word::singleton(Singleton::False),Value::Bool(true)=>Word::singleton(Singleton::True),Value::Nil=>Word::singleton(Singleton::Nil),Value::Unspecified=>Word::singleton(Singleton::Unspecified),Value::Undefined=>Word::singleton(Singleton::Undefined),Value::Eof=>Word::singleton(Singleton::Eof),Value::Int(value)=>self.integer(*value),Value::Char(value)=>Word::character(*value),Value::Symbol(value)=>self.intern_symbol(value.as_str(),false),Value::Keyword(value)=>self.intern_symbol(value.as_str(),true),Value::ValuesData(values)=>{let values=values.iter().map(|value|self.from_value(value)).collect();self.values(values)},Value::Procedure(procedure)=>{if let Procedure::Builtin{name,..}=&**procedure{return BuiltinId::from_name(name).map(|id|self.builtin(id)).unwrap_or_else(||self.named_builtin(name))}let key=(5,Rc::as_ptr(procedure) as usize);if let Some(word)=self.legacy_objects.get(&key){return *word}if let Procedure::Lambda{params,env,compiled:Some(compiled),..}=&**procedure{if let Some(bytecode)=&compiled.bytecode{if let Some(program)=WordProgram::compile(bytecode){let captures=compiled.capture_values.iter().map(|value|self.from_value(value)).collect::<Vec<_>>();let program=Rc::new(program);let constants=program.resolve_constants(self);let keyword_slots=params.required.iter().enumerate().filter_map(|(slot,name)|self.intern_symbol(name,true).as_symbol().map(|id|(id,slot as u8))).collect();let word=self.allocate(HeapPayload::Closure(WordClosure{params:params.clone(),program:program.clone(),captures,keyword_slots,constants,dynamics:RefCell::new(Vec::new()),env:env.clone(),legacy:value.clone()}));self.legacy_objects.insert(key,word);let dynamics=program.resolve_dynamics(self,env);self.set_closure_dynamics(word,dynamics);return word}}}let word=self.allocate(HeapPayload::Legacy(value.clone()));self.legacy_objects.insert(key,word);word},Value::String(value)=>{let key=(1,Rc::as_ptr(value) as usize);if let Some(word)=self.legacy_objects.get(&key){return *word}let word=self.string(value.borrow().clone());self.legacy_objects.insert(key,word);self.legacy_origins.insert(word,Value::String(value.clone()));Self::mark_origin(word);word},Value::Pair(pair)=>{let key=(2,pair.as_ptr() as usize);if let Some(word)=self.legacy_objects.get(&key){return *word}let nil=Word::singleton(Singleton::Nil);let word=self.pair(nil,nil);self.legacy_objects.insert(key,word);self.legacy_origins.insert(word,value.clone());Self::mark_origin(word);let pair=pair.borrow();let car=self.from_value(&pair.car);let cdr=self.from_value(&pair.cdr);self.set_pair(word,car,cdr);word},Value::Vector(vector)=>{let key=(3,Rc::as_ptr(vector) as usize);if let Some(word)=self.legacy_objects.get(&key){return *word}let word=self.vector(Vec::new());self.legacy_objects.insert(key,word);self.legacy_origins.insert(word,Value::Vector(vector.clone()));Self::mark_origin(word);let values=vector.values().iter().map(|value|self.from_value(value)).collect::<Vec<_>>();self.replace_vector(word,values);word},Value::HashTable(table)=>{let key=(4,Rc::as_ptr(table) as usize);if let Some(word)=self.legacy_objects.get(&key){return *word}let word=self.hash_table(Vec::new());self.legacy_objects.insert(key,word);self.legacy_origins.insert(word,Value::HashTable(table.clone()));Self::mark_origin(word);let entries=table.borrow().iter().map(|(key,value)|(self.from_value(key),self.from_value(value))).collect();self.replace_hash_table(word,entries);word},_=>self.allocate(HeapPayload::Legacy(value.clone()))}}

    fn graph_repeats(&self,word:Word,epoch:u32)->bool{if Self::has_origin(word){return false}if let Some(ptr)=word.as_pair_ptr(){let pair=unsafe{ptr.as_ref()};if pair.visit.replace(epoch)==epoch{return true}let (car,cdr)=*pair.pair.borrow();return self.graph_repeats(car,epoch)||self.graph_repeats(cdr,epoch)}let Some(ptr)=word.as_ptr() else{return false};let object=unsafe{ptr.as_ref()};let traversed=matches!(object.payload,HeapPayload::String(_)|HeapPayload::Vector(_)|HeapPayload::HashTable(_)|HeapPayload::Values(_)|HeapPayload::Values2(_));if !traversed{return false}if object.visit.replace(epoch)==epoch{return true}match &object.payload{HeapPayload::Vector(values)=>values.borrow().iter().copied().any(|value|self.graph_repeats(value,epoch)),HeapPayload::HashTable(entries)=>entries.borrow().iter().copied().any(|(key,value)|self.graph_repeats(key,epoch)||self.graph_repeats(value,epoch)),HeapPayload::Values(values)=>values.iter().copied().any(|value|self.graph_repeats(value,epoch)),HeapPayload::Values2(values)=>values.iter().copied().any(|value|self.graph_repeats(value,epoch)),_=>false}}

    pub(crate) fn to_value(&self,word:Word)->Option<Value>{let epoch=self.export_epoch.get().wrapping_add(1).max(1);self.export_epoch.set(epoch);if self.graph_repeats(word,epoch){self.export_value(word,&mut ExportMemo::default())}else{self.export_acyclic(word)}}

    pub(crate) fn object_string_acyclic(&self,word:Word)->Option<String>{fn append(heap:&WordHeap,word:Word,output:&mut String,origins:&mut WordSet<Word>)->Option<()>{if WordHeap::has_origin(word){if !origins.insert(word){return None}output.push_str(&crate::s7_object_string(heap.legacy_origins.get(&word)?));return Some(())}if let Some(value)=word.as_fixnum(){use std::fmt::Write;write!(output,"{}",value).ok()?;return Some(())}if let Some(value)=word.as_singleton(){output.push_str(match value{Singleton::False=>"#f",Singleton::True=>"#t",Singleton::Nil=>"()",Singleton::Unspecified=>"#<unspecified>",Singleton::Undefined=>"#<undefined>",Singleton::Eof=>"#<eof>"});return Some(())}if let Some(value)=word.as_character(){match value{' '=>output.push_str("#\\space"),'\n'=>output.push_str("#\\newline"),'\0'=>output.push_str("#\\null"),value=>{output.push_str("#\\");output.push(value)}}return Some(())}if let Some((name,keyword,_))=heap.symbol_meta(word){if keyword&&!name.starts_with(':')&&!name.ends_with(':'){output.push(':')}output.push_str(name);return Some(())}if heap.is_pair(word){output.push('(');let mut cursor=word;let mut first=true;loop{let (car,cdr)=heap.pair_values(cursor)?;if !first{output.push(' ')}append(heap,car,output,origins)?;if cdr.as_singleton()==Some(Singleton::Nil){output.push(')');return Some(())}if heap.is_pair(cdr){if WordHeap::has_origin(cdr)&&!origins.insert(cdr){return None}cursor=cdr;first=false;continue}output.push_str(" . ");append(heap,cdr,output,origins)?;output.push(')');return Some(())}}let value=heap.export_acyclic(word)?;output.push_str(&crate::s7_object_string(&value));Some(())}let epoch=self.export_epoch.get().wrapping_add(1).max(1);self.export_epoch.set(epoch);if self.graph_repeats(word,epoch){return None}let mut output=String::new();append(self,word,&mut output,&mut WordSet::default())?;Some(output)}

    fn export_acyclic(&self,word:Word)->Option<Value>{if let Some(value)=word.as_fixnum(){return Some(Value::Int(value))}if let Some(value)=word.as_singleton(){return Some(match value{Singleton::False=>Value::Bool(false),Singleton::True=>Value::Bool(true),Singleton::Nil=>Value::Nil,Singleton::Unspecified=>Value::Unspecified,Singleton::Undefined=>Value::Undefined,Singleton::Eof=>Value::Eof})}if let Some(value)=word.as_character(){return Some(Value::Char(value))}if let Some((name,keyword,_))=self.symbol_meta(word){return Some(if keyword{Value::keyword(name)}else{Value::symbol(name)})}if Self::has_origin(word){if let Some(value)=self.legacy_origins.get(&word){return Some(value.clone())}}if self.is_pair(word){let mut cars=Vec::new();let mut tail=word;while !Self::has_origin(tail){let Some((car,cdr))=self.pair_values(tail) else{break};cars.push(car);tail=cdr}let mut output=self.export_acyclic(tail)?;for car in cars.into_iter().rev(){output=Value::cons(self.export_acyclic(car)?,output)}return Some(output)}let ptr=word.as_ptr()?;match &unsafe{ptr.as_ref()}.payload{HeapPayload::Integer(value)=>Some(Value::Int(*value)),HeapPayload::Legacy(value)=>Some(value.clone()),HeapPayload::String(value)=>Some(Value::string(value.borrow().as_str())),HeapPayload::Vector(values)=>Some(Value::Vector(Rc::new(VectorData::new(values.borrow().iter().map(|word|self.export_acyclic(*word)).collect::<Option<Vec<_>>>()?)))),HeapPayload::HashTable(entries)=>Some(Value::HashTable(Rc::new(RefCell::new(entries.borrow().iter().map(|(key,value)|Some((self.export_acyclic(*key)?,self.export_acyclic(*value)?))).collect::<Option<Vec<_>>>()?)))),HeapPayload::Environment(_)=>None,HeapPayload::Builtin(id)=>Some(Value::symbol(id.name())),HeapPayload::NamedBuiltin(name)=>Some(Value::symbol(name)),HeapPayload::Closure(closure)=>Some(closure.legacy.clone()),HeapPayload::Values(values)=>Some(Value::Values(values.iter().map(|word|self.export_acyclic(*word)).collect::<Option<Vec<_>>>()?)),HeapPayload::Values2(values)=>Some(Value::Values(values.iter().map(|word|self.export_acyclic(*word)).collect::<Option<Vec<_>>>()?))}}


    #[cold]
    #[inline(never)]
    fn export_value(&self,word:Word,memo:&mut ExportMemo)->Option<Value>{
        if let Some(value)=word.as_fixnum(){return Some(Value::Int(value))}
        if let Some(value)=word.as_singleton(){return Some(match value{Singleton::False=>Value::Bool(false),Singleton::True=>Value::Bool(true),Singleton::Nil=>Value::Nil,Singleton::Unspecified=>Value::Unspecified,Singleton::Undefined=>Value::Undefined,Singleton::Eof=>Value::Eof})}
        if let Some(value)=word.as_character(){return Some(Value::Char(value))}
        if let Some((name,keyword,_))=self.symbol_meta(word){return Some(if keyword{Value::keyword(name)}else{Value::symbol(name)})}
        if Self::has_origin(word){if let Some(value)=self.legacy_origins.get(&word){return Some(value.clone())}}if let Some(value)=memo.get(word){return Some(value.clone())}
        if let Some((car,cdr))=self.pair_values(word){let output=Value::cons(Value::Undefined,Value::Nil);memo.insert(word,output.clone());output.set_car(self.export_value(car,memo)?).ok()?;output.set_cdr(self.export_value(cdr,memo)?).ok()?;return Some(output)}
        let ptr=word.as_ptr()?;
        match &unsafe{ptr.as_ref()}.payload{
            HeapPayload::Integer(value)=>Some(Value::Int(*value)),HeapPayload::Legacy(value)=>Some(value.clone()),HeapPayload::String(value)=>{let output=Value::string(value.borrow().as_str());memo.insert(word,output.clone());Some(output)},
            HeapPayload::Vector(values)=>{let words=values.borrow().clone();let output=Value::Vector(Rc::new(VectorData::new(vec![Value::Undefined;words.len()])));memo.insert(word,output.clone());let Value::Vector(vector)=&output else{unreachable!()};for (index,value) in words.into_iter().enumerate(){vector.set(index,self.export_value(value,memo)?)}Some(output)},
            HeapPayload::HashTable(entries)=>{let words=entries.borrow().clone();let output=Value::HashTable(Rc::new(RefCell::new(Vec::new())));memo.insert(word,output.clone());let Value::HashTable(table)=&output else{unreachable!()};*table.borrow_mut()=words.into_iter().map(|(key,value)|Some((self.export_value(key,memo)?,self.export_value(value,memo)?))).collect::<Option<Vec<_>>>()?;Some(output)},
            HeapPayload::Environment(_)=>None,HeapPayload::Builtin(id)=>Some(Value::symbol(id.name())),HeapPayload::NamedBuiltin(name)=>Some(Value::symbol(name)),HeapPayload::Closure(closure)=>Some(closure.legacy.clone()),
            HeapPayload::Values(values)=>Some(Value::Values(values.iter().map(|word|self.export_value(*word,memo)).collect::<Option<Vec<_>>>()?)),HeapPayload::Values2(values)=>Some(Value::Values(values.iter().map(|word|self.export_value(*word,memo)).collect::<Option<Vec<_>>>()?)),
        }
    }

    pub(crate) fn is_true(&self,word:Word)->bool{if let Some(values)=self.values_ref(word){return values.first().map(|value|self.is_true(*value)).unwrap_or(false)}word.as_singleton()!=Some(Singleton::False)}

    pub(crate) fn values(&mut self,values:Vec<Word>)->Word{if let [a,b]=values.as_slice(){self.allocate(HeapPayload::Values2([*a,*b]))}else{self.allocate(HeapPayload::Values(values))}}
    pub(crate) fn values_ref(&self,word:Word)->Option<&[Word]>{let ptr=word.as_ptr()?;match &unsafe{ptr.as_ref()}.payload{HeapPayload::Values(values)=>Some(values),HeapPayload::Values2(values)=>Some(values),_=>None}}

    pub(crate) fn builtin(&mut self,id:BuiltinId)->Word{self.allocate(HeapPayload::Builtin(id))}
    pub(crate) fn named_builtin(&mut self,name:&'static str)->Word{self.allocate(HeapPayload::NamedBuiltin(name))}
    pub(crate) fn builtin_name(&self,word:Word)->Option<&'static str>{let ptr=word.as_ptr()?;match &unsafe{ptr.as_ref()}.payload{HeapPayload::Builtin(id)=>Some(id.name()),HeapPayload::NamedBuiltin(name)=>Some(name),_=>None}}
    fn set_closure_dynamics(&self,word:Word,dynamics:Vec<Word>)->bool{let Some(ptr)=self.closure_ptr(word) else{return false};*unsafe{ptr.as_ref()}.dynamics.borrow_mut()=dynamics;true}
    pub(crate) fn closure_frame_parts(&self,word:Word)->Option<(Params,WordProgramRef,Vec<Word>,Vec<(u32,u8)>,Vec<Word>,Vec<Word>)>{let ptr=self.closure_ptr(word)?;let closure=unsafe{ptr.as_ref()};Some((closure.params.clone(),closure.program.clone(),closure.captures.clone(),closure.keyword_slots.clone(),closure.constants.clone(),closure.dynamics.borrow().clone()))}
    pub(crate) fn closure_ptr(&self,word:Word)->Option<NonNull<WordClosure>>{let ptr=word.as_ptr()?;match &unsafe{ptr.as_ref()}.payload{HeapPayload::Closure(closure)=>Some(NonNull::from(closure)),_=>None}}
    pub(crate) fn builtin_id(&self,word:Word)->Option<BuiltinId>{let ptr=word.as_ptr()?;match &unsafe{ptr.as_ref()}.payload{HeapPayload::Builtin(id)=>Some(*id),_=>None}}

    pub(crate) fn intern_symbol(&mut self,name:&str,keyword:bool)->Word{let ids=if keyword{&mut self.keyword_ids}else{&mut self.symbol_ids};if let Some(id)=ids.get(name){return Word::symbol(*id)}let id=self.symbols.len() as u32;let owned=name.to_string();self.symbols.push(SymbolMeta{name:owned.clone(),keyword,gensym:false});ids.insert(owned,id);Word::symbol(id)}

    pub(crate) fn gensym(&mut self,name:&str)->Word{let id=self.symbols.len() as u32;self.symbols.push(SymbolMeta{name:name.to_string(),keyword:false,gensym:true});Word::symbol(id)}

    pub(crate) fn symbol_meta(&self,word:Word)->Option<(&str,bool,bool)>{let meta=self.symbols.get(word.as_symbol()? as usize)?;Some((&meta.name,meta.keyword,meta.gensym))}

    pub(crate) fn string(&mut self,value:String)->Word{self.allocate(HeapPayload::String(RefCell::new(value)))}
    pub(crate) fn string_value(&self,word:Word)->Option<String>{let ptr=word.as_ptr()?;match &unsafe{ptr.as_ref()}.payload{HeapPayload::String(value)=>Some(value.borrow().clone()),_=>None}}

    pub(crate) fn string_length(&self,word:Word)->Option<usize>{let ptr=word.as_ptr()?;match &unsafe{ptr.as_ref()}.payload{HeapPayload::String(value)=>Some(value.borrow().chars().count()),_=>None}}

    pub(crate) fn vector(&mut self,values:Vec<Word>)->Word{self.allocate(HeapPayload::Vector(RefCell::new(values)))}
    pub(crate) fn vector_length(&self,word:Word)->Option<usize>{let ptr=word.as_ptr()?;match &unsafe{ptr.as_ref()}.payload{HeapPayload::Vector(values)=>Some(values.borrow().len()),_=>None}}
    pub(crate) fn vector_ref(&self,word:Word,index:usize)->Option<Word>{let ptr=word.as_ptr()?;match &unsafe{ptr.as_ref()}.payload{HeapPayload::Vector(values)=>values.borrow().get(index).copied(),_=>None}}
    pub(crate) fn byte_vector_ref(&self,word:Word,index:usize)->Option<Word>{let ptr=word.as_ptr()?;match &unsafe{ptr.as_ref()}.payload{HeapPayload::Legacy(Value::ByteVector(values))=>values.borrow().get(index).copied().and_then(|value|Word::fixnum(value as i64)),_=>None}}
    pub(crate) fn vector_set(&self,word:Word,index:usize,value:Word)->bool{let Some(ptr)=word.as_ptr() else{return false};match &unsafe{ptr.as_ref()}.payload{HeapPayload::Vector(values)=>{let mut values=values.borrow_mut();let Some(slot)=values.get_mut(index) else{return false};*slot=value;true},_=>false}}
    fn replace_vector(&self,word:Word,new_values:Vec<Word>)->bool{let Some(ptr)=word.as_ptr() else{return false};match &unsafe{ptr.as_ref()}.payload{HeapPayload::Vector(values)=>{*values.borrow_mut()=new_values;true},_=>false}}

    pub(crate) fn hash_table(&mut self,entries:Vec<(Word,Word)>)->Word{self.allocate(HeapPayload::HashTable(RefCell::new(entries)))}
    fn replace_hash_table(&self,word:Word,entries:Vec<(Word,Word)>)->bool{let Some(ptr)=word.as_ptr() else{return false};match &unsafe{ptr.as_ref()}.payload{HeapPayload::HashTable(table)=>{*table.borrow_mut()=entries;true},_=>false}}
    pub(crate) fn is_hash_table(&self,word:Word)->bool{word.as_ptr().map(|ptr|matches!(&unsafe{ptr.as_ref()}.payload,HeapPayload::HashTable(_))).unwrap_or(false)}
    pub(crate) fn hash_get(&self,word:Word,key:Word)->Option<Word>{let ptr=word.as_ptr()?;match &unsafe{ptr.as_ref()}.payload{HeapPayload::HashTable(table)=>table.borrow().iter().find(|(candidate,_)|self.equal(*candidate,key)).map(|(_,value)|*value),_=>None}}
    pub(crate) fn hash_set(&self,word:Word,key:Word,value:Word)->bool{let Some(ptr)=word.as_ptr() else{return false};match &unsafe{ptr.as_ref()}.payload{HeapPayload::HashTable(table)=>{let mut table=table.borrow_mut();if let Some((_,slot))=table.iter_mut().find(|(candidate,_)|self.equal(*candidate,key)){*slot=value}else{table.push((key,value))}true},_=>false}}

    pub(crate) fn environment(&mut self,parent:Option<Word>)->Word{self.allocate(HeapPayload::Environment(RefCell::new(WordEnvironment{parent,vars:Vec::new()})))}
    pub(crate) fn env_define(&self,env:Word,symbol:Word,value:Word)->bool{let Some(id)=symbol.as_symbol() else{return false};let Some(ptr)=env.as_ptr() else{return false};match &unsafe{ptr.as_ref()}.payload{HeapPayload::Environment(environment)=>{let mut environment=environment.borrow_mut();if let Some((_,slot))=environment.vars.iter_mut().find(|(key,_)|*key==id){*slot=value}else{environment.vars.push((id,value))}true},_=>false}}
    pub(crate) fn env_get(&self,mut env:Word,symbol:Word)->Option<Word>{let id=symbol.as_symbol()?;loop{let ptr=env.as_ptr()?;let (found,parent)=match &unsafe{ptr.as_ref()}.payload{HeapPayload::Environment(environment)=>{let environment=environment.borrow();(environment.vars.iter().find(|(key,_)|*key==id).map(|(_,value)|*value),environment.parent)},_=>return None};if found.is_some(){return found}env=parent?}}
    pub(crate) fn env_set(&self,mut env:Word,symbol:Word,value:Word)->bool{let Some(id)=symbol.as_symbol() else{return false};loop{let Some(ptr)=env.as_ptr() else{return false};let parent=match &unsafe{ptr.as_ref()}.payload{HeapPayload::Environment(environment)=>{let mut environment=environment.borrow_mut();if let Some((_,slot))=environment.vars.iter_mut().find(|(key,_)|*key==id){*slot=value;return true}environment.parent},_=>return false};let Some(next)=parent else{return false};env=next}}

    pub(crate) fn pair(&mut self,car:Word,cdr:Word)->Word{if self.pair_free.is_empty(){let nil=Word::singleton(Singleton::Nil);let mut chunk=(0..4096).map(|_|PairObject{active:Cell::new(false),marked:Cell::new(false),origin:Cell::new(false),visit:Cell::new(0),pair:RefCell::new((nil,nil))}).collect::<Vec<_>>().into_boxed_slice();for pair in chunk.iter_mut(){self.pair_free.push(NonNull::from(pair));}self.pair_chunks.push(chunk);}let ptr=self.pair_free.pop().expect("word pair free slot");let pair=unsafe{&mut*ptr.as_ptr()};*pair.pair.get_mut()=(car,cdr);pair.marked.set(false);pair.origin.set(false);pair.active.set(true);self.object_count+=1;Word::pair_pointer(ptr)}

    pub(crate) fn pair_values(&self,word:Word)->Option<(Word,Word)>{let ptr=word.as_pair_ptr()?;let pair=unsafe{ptr.as_ref()};pair.active.get().then(||*pair.pair.borrow())}

    fn set_pair(&self,word:Word,car:Word,cdr:Word)->bool{let Some(ptr)=word.as_pair_ptr() else{return false};let pair=unsafe{ptr.as_ref()};if !pair.active.get(){return false}*pair.pair.borrow_mut()=(car,cdr);true}

    pub(crate) fn set_pair_car(&self,word:Word,value:Word)->bool{let Some(ptr)=word.as_pair_ptr() else{return false};let pair=unsafe{ptr.as_ref()};if !pair.active.get(){return false}pair.pair.borrow_mut().0=value;true}

    pub(crate) fn set_pair_cdr(&self,word:Word,value:Word)->bool{let Some(ptr)=word.as_pair_ptr() else{return false};let pair=unsafe{ptr.as_ref()};if !pair.active.get(){return false}pair.pair.borrow_mut().1=value;true}
    pub(crate) fn is_pair(&self,word:Word)->bool{word.as_pair_ptr().map(|ptr|unsafe{ptr.as_ref()}.active.get()).unwrap_or(false)}
    pub(crate) fn list(&mut self,values:impl DoubleEndedIterator<Item=Word>)->Word{values.rev().fold(Word::singleton(Singleton::Nil),|cdr,car|self.pair(car,cdr))}
    pub(crate) fn list_into(&self,mut word:Word,out:&mut [Word])->Option<usize>{let mut length=0usize;let mut slow=word;while word.as_singleton()!=Some(Singleton::Nil){if length==out.len(){return None}let (car,cdr)=self.pair_values(word)?;out[length]=car;length+=1;word=cdr;if length%2==0{slow=self.pair_values(slow)?.1;if word==slow{return None}}}Some(length)}
    pub(crate) fn list_values(&self,mut word:Word)->Option<Vec<Word>>{let mut values=Vec::new();let mut slow=word;while word.as_singleton()!=Some(Singleton::Nil){let (car,cdr)=self.pair_values(word)?;values.push(car);word=cdr;if values.len()%2==0{slow=self.pair_values(slow)?.1;if word==slow{return None}}}Some(values)}
    pub(crate) fn list_length(&self,mut word:Word)->Option<usize>{let mut length=0usize;let mut slow=word;loop{if word.as_singleton()==Some(Singleton::Nil){return Some(length)}let (_,cdr)=self.pair_values(word)?;word=cdr;length+=1;if length%2==0{slow=self.pair_values(slow)?.1;if word==slow{return None}}}}

    #[inline(always)]fn simple_atom_equal(&self,a:Word,b:Word)->Option<bool>{if a==b{return Some(true)}if let Some((left,right))=self.integer_value(a).zip(self.integer_value(b)){return Some(left==right)}if let Some((left,right))=self.string_value(a).zip(self.string_value(b)){return Some(left==right)}if a.as_ptr().is_none()&&b.as_ptr().is_none(){return Some(false)}None}
    #[inline(always)]fn equal_two_simple_lists(&self,a:Word,b:Word)->Option<bool>{let (a_first,a_tail)=self.pair_values(a)?;let (b_first,b_tail)=self.pair_values(b)?;let (a_second,a_end)=self.pair_values(a_tail)?;let (b_second,b_end)=self.pair_values(b_tail)?;if a_end.as_singleton()!=Some(Singleton::Nil)||b_end.as_singleton()!=Some(Singleton::Nil){return None}Some(self.simple_atom_equal(a_first,b_first)?&&self.simple_atom_equal(a_second,b_second)?)}
    pub(crate) fn equal(&self,a:Word,b:Word)->bool{self.equal_two_simple_lists(a,b).unwrap_or_else(||self.equal_seen(a,b,&mut WordSet::default()))}
    fn equal_seen(&self,a:Word,b:Word,seen:&mut WordSet<(Word,Word)>)->bool{if a==b{return true}if self.integer_value(a).zip(self.integer_value(b)).map(|(a,b)|a==b).unwrap_or(false){return true}if let (Some((ac,ad)),Some((bc,bd)))=(self.pair_values(a),self.pair_values(b)){if !seen.insert((a,b)){return true}return self.equal_seen(ac,bc,seen)&&self.equal_seen(ad,bd,seen)}if let (Some(a),Some(b))=(self.string_value(a),self.string_value(b)){return a==b}let aggregate=match (a.as_ptr(),b.as_ptr()){(Some(a_ptr),Some(b_ptr))=>match (&unsafe{a_ptr.as_ref()}.payload,&unsafe{b_ptr.as_ref()}.payload){(HeapPayload::Vector(a_values),HeapPayload::Vector(b_values))=>{if !seen.insert((a,b)){return true}let a_values=a_values.borrow();let b_values=b_values.borrow();Some(a_values.len()==b_values.len()&&a_values.iter().zip(b_values.iter()).all(|(a,b)|self.equal_seen(*a,*b,seen)))},(HeapPayload::HashTable(a_entries),HeapPayload::HashTable(b_entries))=>{if !seen.insert((a,b)){return true}let a_entries=a_entries.borrow();let b_entries=b_entries.borrow();Some(a_entries.len()==b_entries.len()&&a_entries.iter().all(|(ak,av)|b_entries.iter().any(|(bk,bv)|self.equal_seen(*ak,*bk,seen)&&self.equal_seen(*av,*bv,seen))))},(HeapPayload::Legacy(a),HeapPayload::Legacy(b))=>Some(legacy_equal(a,b)),_=>None},_=>None};aggregate.unwrap_or(false)}

    pub(crate) fn memq(&self,key:Word,mut list:Word)->Option<Word>{let mut slow=list;let mut steps=0usize;while let Some((value,cdr))=self.pair_values(list){if value==key{return Some(list)}list=cdr;steps+=1;if steps%2==0{slow=self.pair_values(slow)?.1;if list==slow{return None}}}None}

    pub(crate) fn assoc(&self,key:Word,mut list:Word)->Option<Word>{let mut slow=list;let mut steps=0usize;while let Some((entry,cdr))=self.pair_values(list){if let Some((entry_key,_))=self.pair_values(entry){if self.equal(key,entry_key){return Some(entry)}}list=cdr;steps+=1;if steps%2==0{slow=self.pair_values(slow)?.1;if list==slow{return None}}}None}
}

const _:[();8]=[();std::mem::size_of::<Word>()];
