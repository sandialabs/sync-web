use std::collections::HashSet;
use std::rc::Rc;

use super::*;

pub(crate) fn equal(a:&Value,b:&Value)->bool{ equal_seen(a,b,&mut HashSet::new()) }
pub(crate) fn equal_seen(a:&Value,b:&Value,seen:&mut HashSet<(usize,usize)>)->bool{ match (a,b){(Value::Bool(x),Value::Bool(y))=>x==y,(Value::Nil,Value::Nil)|(Value::Eof,Value::Eof)|(Value::Undefined,Value::Undefined)|(Value::Unspecified,Value::Unspecified)=>true,(Value::Int(x),Value::Int(y))=>x==y,(Value::Rational(an,ad),Value::Rational(bn,bd))=>an==bn&&ad==bd,(Value::Int(x),Value::Rational(n,d))| (Value::Rational(n,d),Value::Int(x))=>*x * *d == *n,(Value::Float(x),Value::Float(y))=>x==y,(Value::Int(x),Value::Float(y))=>*x as f64==*y,(Value::Float(x),Value::Int(y))=>*x==*y as f64,(Value::Rational(n,d),Value::Float(y))|(Value::Float(y),Value::Rational(n,d))=>((*n as f64)/(*d as f64))==*y,(Value::Complex(ar,ai),Value::Complex(br,bi))=>ar==br&&ai==bi,(Value::NumberLiteral(_,x),Value::NumberLiteral(_,y))=>x==y,(Value::Char(x),Value::Char(y))=>x==y,(Value::NamedChar(x),Value::NamedChar(y))=>x==y,(Value::String(x),Value::String(y))=>*x.borrow()==*y.borrow(),(Value::Symbol(x),Value::Symbol(y))=>x==y,(Value::Keyword(x),Value::Keyword(y))=>x==y,(Value::RootMeta(x),Value::RootMeta(y))=>x==y,(Value::Procedure(x),Value::Procedure(y))=>Rc::ptr_eq(x,y),(Value::Macro(x,_),Value::Macro(y,_))=>Rc::ptr_eq(x,y),(Value::Hook(x,_),Value::Hook(y,_))=>Rc::ptr_eq(x,y),(Value::Pair(x),Value::Pair(y))=>{let key=(Rc::as_ptr(x) as usize,Rc::as_ptr(y) as usize); if !seen.insert(key){return true;} let (ac,ad)={let Object::Pair{car,cdr}= &*x.borrow();(car.clone(),cdr.clone())}; let (bc,bd)={let Object::Pair{car,cdr}= &*y.borrow();(car.clone(),cdr.clone())}; equal_seen(&ac,&bc,seen)&&equal_seen(&ad,&bd,seen)},(Value::Vector(x),Value::Vector(y))=>{let key=(Rc::as_ptr(x) as usize,Rc::as_ptr(y) as usize); if !seen.insert(key){return true;} x.borrow().len()==y.borrow().len()&&x.borrow().iter().zip(y.borrow().iter()).all(|(a,b)|equal_seen(a,b,seen))},(Value::ByteVector(x),Value::ByteVector(y))=>*x.borrow()==*y.borrow(),_=>false} }
pub(crate) fn eq(a:&Value,b:&Value)->bool{ match (a,b){(Value::Pair(x),Value::Pair(y))=>Rc::ptr_eq(x,y),(Value::String(x),Value::String(y))=>Rc::ptr_eq(x,y)|| (x.borrow().is_empty() && y.borrow().is_empty()),(Value::Vector(x),Value::Vector(y))=>Rc::ptr_eq(x,y),(Value::Port(x),Value::Port(y))=>Rc::ptr_eq(x,y),(Value::Env(x),Value::Env(y))=>Rc::ptr_eq(x,y),(Value::Procedure(x),Value::Procedure(y))=>Rc::ptr_eq(x,y),(Value::Macro(x,_),Value::Macro(y,_))=>Rc::ptr_eq(x,y),_=>equal(a,b)} }

pub(crate) fn set_first_equal(e:&EnvRef, old:&Value, newv:Value)->bool{
    let keys=e.order.borrow().clone();
    for k in keys { let hit={e.vars.borrow().get(&k).cloned().map(|v|equal(&v,old)).unwrap_or(false)}; if hit { e.set(&k,newv); return true; } }
    e.parent.borrow().as_ref().map(|p|set_first_equal(p,old,newv)).unwrap_or(false)
}
pub(crate) fn env_entries(e:&EnvRef)->Value{
    let keys=e.order.borrow().clone();
    Value::list(keys.into_iter().map(|k|Value::cons(Value::symbol(&k), e.vars.borrow().get(&k).cloned().unwrap_or(Value::Undefined))).collect())
}

pub(crate) fn list_sequence_to_vec(v:&Value)->Vec<Value>{
    let mut out=Vec::new(); let mut cur=v.clone(); let mut seen=HashSet::new();
    while let Value::Pair(p)=cur {
        let id=Rc::as_ptr(&p) as usize;
        if !seen.insert(id){break;}
        let Object::Pair{car,cdr}= &*p.borrow();
        out.push(car.clone());
        cur=cdr.clone();
    }
    out
}
pub(crate) fn list_sequence_to_vec_repeat(v:&Value)->Vec<Value>{
    let mut out=Vec::new(); let mut cur=v.clone(); let mut seen=HashSet::new();
    while let Value::Pair(p)=cur { let id=Rc::as_ptr(&p) as usize; let Object::Pair{car,cdr}= &*p.borrow(); out.push(car.clone()); if !seen.insert(id){break;} cur=cdr.clone(); }
    out
}
pub(crate) fn sequence_to_vec(v:&Value)->Result<Vec<Value>>{
    match v {
        Value::Env(e)=>env_entries(e).to_vec(),
        Value::Pair(_)|Value::Nil=>Ok(list_sequence_to_vec_repeat(v)),
        Value::Vector(xs)=>Ok(xs.borrow().clone()),
        Value::ProcedureSource{params,body}=>{let mut xs=vec![Value::symbol(if params.star{"lambda*"}else{"lambda"}), proc_source_params(params)]; xs.extend(body.borrow().iter().cloned()); Ok(xs)},
        Value::ByteVector(xs)=>Ok(xs.borrow().iter().map(|x|Value::Int(*x as i64)).collect()),
        Value::FloatVector(xs)=>Ok(xs.borrow().iter().map(|x|Value::Float(*x)).collect()),
        Value::IntVector(xs)=>Ok(xs.borrow().iter().map(|x|Value::Int(*x)).collect()),
        Value::String(s)=>Ok(s.borrow().chars().map(Value::Char).collect()),
        _=>v.to_vec()
    }
}

pub(crate) fn proc_source_params(params:&Params)->Value{
    if params.star { let mut xs=Vec::new(); for (i,n) in params.required.iter().enumerate(){ if let Some(Some(d))=params.defaults.get(i){xs.push(Value::list(vec![Value::symbol(n),d.clone()]));}else{xs.push(Value::symbol(n));} } if params.allow_other_keys{xs.push(Value::keyword("allow-other-keys"));} if let Some(r)=&params.rest{xs.push(Value::symbol("."));xs.push(Value::symbol(r));} Value::list(xs) }
    else { let mut xs=params.required.iter().map(|n|Value::symbol(n)).collect::<Vec<_>>(); if let Some(r)=&params.rest{xs.push(Value::symbol("."));xs.push(Value::symbol(r));} Value::list(xs) }
}

pub(crate) fn is_callable_value(v:&Value)->bool{matches!(v,Value::Procedure(_)|Value::ProcedureSource{..}|Value::Macro(_,_)|Value::RootMeta(_)|Value::Dilambda(_)|Value::Vector(_)|Value::MultiVector{..}|Value::MultiVectorView{..}|Value::ByteVector(_)|Value::FloatVector(_)|Value::IntVector(_)|Value::String(_)|Value::Pair(_)|Value::Env(_)|Value::HashTable(_)|Value::Hook(_,_)|Value::Iterator{..}|Value::Values(_))}
pub(crate) fn error_arg_string(v:&Value)->String{match v{Value::Symbol(s)=>format!("'{}",s),_=>v.to_string()}}
pub(crate) fn call_form_string(proc:&Value,args:&[Value])->String{format!("({}{})",hash_table_expr(proc), if args.is_empty(){String::new()}else{format!(" {}",args.iter().map(error_arg_string).collect::<Vec<_>>().join(" "))})}
pub(crate) fn hash_table_expr(v:&Value)->String{if let Value::HashTable(h)=v{let mut parts=Vec::new(); for (k,val) in h.borrow().iter(){parts.push(error_arg_string(k)); parts.push(val.to_string());} if parts.is_empty(){"(hash-table)".to_string()}else{format!("(hash-table {})",parts.join(" "))}}else{v.to_string()}}
pub(crate) fn cant_take_arguments_error(form:&str, first:&Value, rest:&[Value])->SchemeError{let mut becomes=vec![first.clone()]; becomes.extend(rest.iter().cloned()); SchemeError::new("syntax-error",vec![Value::string("~$ becomes ~$, but ~S can't take arguments"),Value::string(form),Value::list(becomes),first.clone()])}
pub(crate) fn cant_take_arguments_error_value(form:Value, first:&Value, rest:&[Value])->SchemeError{let mut becomes=vec![first.clone()]; becomes.extend(rest.iter().cloned()); SchemeError::new("syntax-error",vec![Value::string("~$ becomes ~$, but ~S can't take arguments"),form,Value::list(becomes),first.clone()])}
pub(crate) fn index_vec(v:&[Value], args:&[Value])->Result<Value>{ let raw=args.get(0).ok_or_else(||SchemeError::new("wrong-number-of-args",vec![]))?; let i=match raw{Value::Int(n)=>*n,_=>return Err(SchemeError::new("wrong-type-arg",vec![Value::string("~A ~:D argument, ~S, is ~A but should be ~A"),Value::symbol("vector-ref"),Value::Int(2),raw.clone(),Value::string(match raw{Value::Float(_)=>"a real",Value::Rational(_,_)=>"a ratio",Value::Symbol(_)=>"a symbol",_=>"an object"}),Value::string("an integer")]))}; if i<0||i as usize>=v.len(){return Err(SchemeError::new("out-of-range",vec![Value::string("~A ~:D argument, ~S, is out of range (~A)"),Value::symbol("vector-ref"),Value::Int(2),Value::Int(i),Value::string(if i<0{"it is negative"}else{"it is too large"})]));} Ok(v[i as usize].clone()) }
pub(crate) fn index_bvec(v:&[u8], args:&[Value])->Result<Value>{ if args.len()>1{return Err(SchemeError::new("wrong-number-of-args",vec![Value::string("~S: too many indices: ~S"),Value::symbol("vector-ref"),Value::list(args.to_vec())]));} let raw=args.get(0).ok_or_else(||SchemeError::new("wrong-number-of-args",vec![]))?; let i=match raw{Value::Int(n)=>*n,_=>return Err(SchemeError::new("wrong-type-arg",vec![Value::string("~A ~:D argument, ~S, is ~A but should be ~A"),Value::symbol("byte-vector-ref"),Value::Int(2),raw.clone(),Value::string(match raw{Value::Float(_)=>"a real",Value::Rational(_,_)=>"a ratio",Value::Symbol(_)=>"a symbol",_=>"an object"}),Value::string("an integer")]))}; if i<0||i as usize>=v.len(){return Err(SchemeError::new("out-of-range",vec![Value::string("~A ~:D argument, ~S, is out of range (~A)"),Value::symbol("byte-vector-ref"),Value::Int(2),Value::Int(i),Value::string(if i<0{"it is negative"}else{"it is too large"})]))} Ok(Value::Int(v[i as usize] as i64)) }
pub(crate) fn index_fvec(v:&[f64], args:&[Value])->Result<Value>{ if args.len()>1{return Err(SchemeError::new("wrong-number-of-args",vec![Value::string(format!("vector-ref: too many indices: ({})",args.iter().map(|v|v.to_string()).collect::<Vec<_>>().join(" ")))]));} let raw=args.get(0).ok_or_else(||SchemeError::new("wrong-number-of-args",vec![]))?; let i=match raw{Value::Int(n)=>*n,_=>return Err(SchemeError::new("wrong-type-arg",vec![Value::string("~A ~:D argument, ~S, is ~A but should be ~A"),Value::symbol("float-vector-ref"),Value::Int(2),raw.clone(),Value::string(match raw{Value::Float(_)=>"a real",Value::Rational(_,_)=>"a ratio",Value::Symbol(_)=>"a symbol",_=>"an object"}),Value::string("an integer")]))}; if i<0||i as usize>=v.len(){return Err(SchemeError::new("out-of-range",vec![Value::string("~A ~:D argument, ~S, is out of range (~A)"),Value::symbol("float-vector-ref"),Value::Int(2),Value::Int(i),Value::string(if i<0{"it is negative"}else{"it is too large"})]));} Ok(Value::Float(v[i as usize])) }
pub(crate) fn index_ivec(v:&[i64], args:&[Value])->Result<Value>{ if args.len()>1{return Err(SchemeError::new("wrong-number-of-args",vec![Value::string("~S: too many indices: ~S"),Value::symbol("vector-ref"),Value::list(args.to_vec())]));} let raw=args.get(0).ok_or_else(||SchemeError::new("wrong-number-of-args",vec![]))?; let i=match raw{Value::Int(n)=>*n,_=>return Err(SchemeError::new("wrong-type-arg",vec![Value::string("~A ~:D argument, ~S, is ~A but should be ~A"),Value::symbol("int-vector-ref"),Value::Int(2),raw.clone(),Value::string(match raw{Value::Float(_)=>"a real",Value::Rational(_,_)=>"a ratio",Value::Symbol(_)=>"a symbol",_=>"an object"}),Value::string("an integer")]))}; if i<0||i as usize>=v.len(){return Err(SchemeError::new("out-of-range",vec![Value::string("~A ~:D argument, ~S, is out of range (~A)"),Value::symbol("int-vector-ref"),Value::Int(2),Value::Int(i),Value::string(if i<0{"it is negative"}else{"it is too large"})]));} Ok(Value::Int(v[i as usize])) }
