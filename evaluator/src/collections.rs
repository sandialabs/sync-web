use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use super::*;

type RawHashTable = Rc<RefCell<Vec<(Value, Value)>>>;
thread_local! { static INT_HASH_INDEX: RefCell<HashMap<usize, HashMap<i64, usize>>> = RefCell::new(HashMap::new()); }

fn hash_id(h:&RawHashTable)->usize{Rc::as_ptr(h) as usize}

fn hash_key_equal(a:&Value,b:&Value)->bool{
    match (a,b){
        (Value::Int(x),Value::Int(y))=>x==y,
        (Value::RationalValue(a),Value::RationalValue(b))=>a.num==b.num&&a.den==b.den,
        (Value::Float(x),Value::Float(y))=>x==y,
        (Value::ComplexValue(a),Value::ComplexValue(b))=>a.real==b.real&&a.imag==b.imag,
        (Value::NumberLiteral(ax,_),Value::NumberLiteral(bx,_))=>ax.repr==bx.repr&&ax.value==bx.value,
        (Value::Symbol(a),Value::Symbol(b))|(Value::Keyword(a),Value::Keyword(b))=>a==b,
        (Value::Bool(a),Value::Bool(b))=>a==b,
        (Value::Char(a),Value::Char(b))=>a==b,
        (Value::Nil,Value::Nil)=>true,
        (Value::String(a),Value::String(b))=>*a.borrow()==*b.borrow(),
        (Value::Host(a),Value::Host(b))=>crate::host::host_values_equal(a,b),
        (Value::Int(_),Value::RationalValue(_)|Value::Float(_)|Value::ComplexValue(_)|Value::NumberLiteral(_,_))|
        (Value::RationalValue(_),Value::Int(_)|Value::Float(_)|Value::ComplexValue(_)|Value::NumberLiteral(_,_))|
        (Value::Float(_),Value::Int(_)|Value::RationalValue(_)|Value::ComplexValue(_)|Value::NumberLiteral(_,_))|
        (Value::ComplexValue(_),Value::Int(_)|Value::RationalValue(_)|Value::Float(_)|Value::NumberLiteral(_,_))|
        (Value::NumberLiteral(_,_),Value::Int(_)|Value::RationalValue(_)|Value::Float(_)|Value::ComplexValue(_))=>false,
        _=>equal(a,b),
    }
}

pub(crate) fn hash_lookup(h:&RawHashTable,key:&Value)->Option<Value>{
    if let Value::Int(n)=key{
        let id=hash_id(h);
        if let Some(pos)=INT_HASH_INDEX.with(|idx|idx.borrow().get(&id).and_then(|m|m.get(n).copied())){
            if let Some((k,v))=h.borrow().get(pos){if hash_key_equal(k,key){return Some(v.clone());}}
        }
        let hb=h.borrow();
        for (i,(k,v)) in hb.iter().enumerate(){if hash_key_equal(k,key){INT_HASH_INDEX.with(|idx|{idx.borrow_mut().entry(id).or_default().insert(*n,i);}); return Some(v.clone());}}
        return None;
    }
    for (k,v) in h.borrow().iter(){if hash_key_equal(k,key){return Some(v.clone());}}
    None
}

pub(crate) fn hash_invalidate(h:&RawHashTable){let id=hash_id(h); INT_HASH_INDEX.with(|idx|{idx.borrow_mut().remove(&id);});}

pub(crate) fn hash_set_entry(h:&RawHashTable,key:Value,val:Value)->Value{
    let id=hash_id(h);
    let int_key=if let Value::Int(n)=key{Some(n)}else{None};
    let mut hb=h.borrow_mut();
    if let Some(n)=int_key{
        let indexed=INT_HASH_INDEX.with(|idx|idx.borrow().contains_key(&id));
        if let Some(pos)=INT_HASH_INDEX.with(|idx|idx.borrow().get(&id).and_then(|m|m.get(&n).copied())){
            if let Some((k,v))=hb.get_mut(pos){if hash_key_equal(k,&Value::Int(n)){*v=val.clone(); return val;}}
            hash_invalidate(h);
        } else if indexed {
            let pos=hb.len();
            hb.push((Value::Int(n),val.clone()));
            INT_HASH_INDEX.with(|idx|{idx.borrow_mut().entry(id).or_default().insert(n,pos);});
            return val;
        } else {
            INT_HASH_INDEX.with(|idx|{idx.borrow_mut().entry(id).or_default();});
        }
    }
    for (i,(k,v)) in hb.iter_mut().enumerate(){if hash_key_equal(k,&key){*v=val.clone(); if let Some(n)=int_key{INT_HASH_INDEX.with(|idx|{idx.borrow_mut().entry(id).or_default().insert(n,i);});} return val;}}
    let pos=hb.len();
    hb.push((key,val.clone()));
    if let Some(n)=int_key{INT_HASH_INDEX.with(|idx|{idx.borrow_mut().entry(id).or_default().insert(n,pos);});}
    val
}

pub(crate) fn hash_set_entry_mutating(h:&RawHashTable,key:Value,val:Value,op:&str,target:&Value)->Result<Value>{
    if is_marked_immutable(target){return Err(immutable_error(op,target));}
    Ok(hash_set_entry(h,key,val))
}

fn simple_list_atom_equal(left:&Value,right:&Value)->Option<bool>{match (left,right){(Value::Bool(left),Value::Bool(right))=>Some(left==right),(Value::Nil,Value::Nil)=>Some(true),(Value::Int(left),Value::Int(right))=>Some(left==right),(Value::Float(left),Value::Float(right))=>Some(left==right),(Value::Char(left),Value::Char(right))=>Some(left==right),(Value::Symbol(left),Value::Symbol(right))=>Some(left==right),(Value::Keyword(left),Value::Keyword(right))=>Some(left==right),_=>None}}
pub(crate) fn equal_two_simple_lists(left:&Value,right:&Value)->Option<bool>{let (Value::Pair(left),Value::Pair(right))=(left,right) else{return None};let left=left.borrow();let right=right.borrow();let first=simple_list_atom_equal(&left.car,&right.car)?;let (Value::Pair(left_tail),Value::Pair(right_tail))=(&left.cdr,&right.cdr) else{return None};let left_tail=left_tail.borrow();let right_tail=right_tail.borrow();if !matches!(left_tail.cdr,Value::Nil)||!matches!(right_tail.cdr,Value::Nil){return None}Some(first&&simple_list_atom_equal(&left_tail.car,&right_tail.car)?)}

#[inline(never)]pub(crate) fn equal_fast_acyclic(a:&Value,b:&Value,depth:usize)->Option<bool>{
    if depth>16{return None;}
    match (a,b){
        (Value::Bool(x),Value::Bool(y))=>Some(x==y),(Value::Nil,Value::Nil)|(Value::Eof,Value::Eof)|(Value::Undefined,Value::Undefined)|(Value::Unspecified,Value::Unspecified)=>Some(true),
        (Value::Int(x),Value::Int(y))=>Some(x==y),(Value::Float(x),Value::Float(y))=>Some(x==y),(Value::Char(x),Value::Char(y))=>Some(x==y),(Value::Symbol(x),Value::Symbol(y))=>Some(x==y),(Value::Keyword(x),Value::Keyword(y))=>Some(x==y),
        (Value::String(x),Value::String(y))=>Some(*x.borrow()==*y.borrow()),
        (Value::Pair(x),Value::Pair(y))=>{let xb=x.borrow(); let yb=y.borrow(); Some(equal_fast_acyclic(&xb.car,&yb.car,depth+1)? && equal_fast_acyclic(&xb.cdr,&yb.cdr,depth+1)?)},
        (Value::Vector(x),Value::Vector(y))=>{let mut result=Some(true);if x.len()!=y.len(){return Some(false)}x.pairwise_all(y,|left,right|{match equal_fast_acyclic(left,right,depth+1){Some(true)=>true,Some(false)=>{result=Some(false);false},None=>{result=None;false}}});result},
        (Value::HashTable(x),Value::HashTable(y))=>{let x=x.borrow();let y=y.borrow();if x.len()!=y.len(){return Some(false)}for (x_key,x_value) in x.iter(){let mut found=false;let mut unknown=false;for (y_key,y_value) in y.iter(){match equal_fast_acyclic(x_key,y_key,depth+1){Some(true)=>match equal_fast_acyclic(x_value,y_value,depth+1){Some(true)=>{found=true;break},Some(false)=>{},None=>unknown=true},Some(false)=>{},None=>unknown=true}}if !found{if unknown{return None}return Some(false)}}Some(true)},
        _=>None,
    }
}
pub(crate) fn equal(a:&Value,b:&Value)->bool{ equal_fast_acyclic(a,b,0).unwrap_or_else(||equal_seen(a,b,&mut HashSet::new())) }
pub(crate) fn equal_seen(a:&Value,b:&Value,seen:&mut HashSet<(usize,usize)>)->bool{ match (a,b){(Value::Bool(x),Value::Bool(y))=>x==y,(Value::Nil,Value::Nil)|(Value::Eof,Value::Eof)|(Value::Undefined,Value::Undefined)|(Value::Unspecified,Value::Unspecified)=>true,(Value::Int(x),Value::Int(y))=>x==y,(Value::RationalValue(a),Value::RationalValue(b))=>a.num==b.num&&a.den==b.den,(Value::Int(x),Value::RationalValue(r))|(Value::RationalValue(r),Value::Int(x))=>*x*r.den==r.num,(Value::Float(x),Value::Float(y))=>x==y,(Value::Int(x),Value::Float(y))=>*x as f64==*y,(Value::Float(x),Value::Int(y))=>*x==*y as f64,(Value::RationalValue(r),Value::Float(y))|(Value::Float(y),Value::RationalValue(r))=>(r.num as f64/r.den as f64)==*y,(Value::ComplexValue(a),Value::ComplexValue(b))=>a.real==b.real&&a.imag==b.imag,(Value::NumberLiteral(x,_),Value::NumberLiteral(y,_))=>x.value==y.value,(Value::Char(x),Value::Char(y))=>x==y,(Value::NamedChar(x),Value::NamedChar(y))=>x==y,(Value::String(x),Value::String(y))=>*x.borrow()==*y.borrow(),(Value::Symbol(x),Value::Symbol(y))=>x==y,(Value::Keyword(x),Value::Keyword(y))=>x==y,(Value::RootMeta(x),Value::RootMeta(y))=>x==y,(Value::Host(x),Value::Host(y))=>crate::host::host_values_equal(x,y),(Value::Procedure(x),Value::Procedure(y))=>Rc::ptr_eq(x,y),(Value::Macro(x,_),Value::Macro(y,_))=>Rc::ptr_eq(x,y),(Value::Hook(x,_),Value::Hook(y,_))=>Rc::ptr_eq(x,y),(Value::Pair(x),Value::Pair(y))=>{let key=(x.as_ptr() as usize,y.as_ptr() as usize); if !seen.insert(key){return true;} let (ac,ad)={let PairData{car,cdr}= &*x.borrow();(car.clone(),cdr.clone())}; let (bc,bd)={let PairData{car,cdr}= &*y.borrow();(car.clone(),cdr.clone())}; equal_seen(&ac,&bc,seen)&&equal_seen(&ad,&bd,seen)},(Value::Vector(x),Value::Vector(y))=>{let key=(Rc::as_ptr(x) as usize,Rc::as_ptr(y) as usize); if !seen.insert(key){return true;} x.pairwise_all(y,|a,b|equal_seen(a,b,seen))},(Value::ByteVector(x),Value::ByteVector(y))=>*x.borrow()==*y.borrow(),(Value::HashTable(x),Value::HashTable(y))=>{let xb=x.borrow(); let yb=y.borrow(); xb.len()==yb.len()&&xb.iter().all(|(kx,vx)|yb.iter().any(|(ky,vy)|equal_seen(kx,ky,seen)&&equal_seen(vx,vy,seen)))},_=>false} }
pub(crate) fn eq(a:&Value,b:&Value)->bool{ match (a,b){(Value::Pair(x),Value::Pair(y))=>PairRef::ptr_eq(x,y),(Value::String(x),Value::String(y))=>Rc::ptr_eq(x,y)|| (x.borrow().is_empty() && y.borrow().is_empty()),(Value::Vector(x),Value::Vector(y))=>Rc::ptr_eq(x,y),(Value::Port(x),Value::Port(y))=>Rc::ptr_eq(x,y),(Value::Env(x),Value::Env(y))=>Rc::ptr_eq(x,y),(Value::HashTable(x),Value::HashTable(y))=>Rc::ptr_eq(x,y),(Value::Procedure(x),Value::Procedure(y))=>Rc::ptr_eq(x,y),(Value::Macro(x,_),Value::Macro(y,_))=>Rc::ptr_eq(x,y),_=>equal(a,b)} }

pub(crate) fn set_first_equal(e:&EnvRef, old:&Value, newv:Value)->bool{
    let keys=e.order.borrow().clone();
    for k in keys { let hit={e.vars.borrow().get(&k).cloned().map(|v|equal(&v,old)).unwrap_or(false)}; if hit { e.set(&k,newv); return true; } }
    e.parent.borrow().as_ref().map(|p|set_first_equal(p,old,newv)).unwrap_or(false)
}
pub(crate) fn env_entries(e:&EnvRef)->Value{
    let keys=e.order.borrow().clone();
    Value::list(keys.into_iter().map(|k|Value::cons(Value::symbol(&k), e.vars.borrow().get(&k).cloned().unwrap_or(Value::Undefined))).collect::<Vec<_>>())
}

pub(crate) enum ListLength { Proper(usize), Dotted, Cyclic }

pub(crate) fn list_length_shape(v:&Value)->ListLength{
    let mut n=0usize;let mut cur=v.clone();let mut slow=v.clone();
    loop{match cur{Value::Nil=>return ListLength::Proper(n),Value::Pair(p)=>{n+=1;cur=p.borrow().cdr.clone();if n%2==0{slow=if let Value::Pair(p)=slow{p.borrow().cdr.clone()}else{Value::Nil};if matches!((&cur,&slow),(Value::Pair(a),Value::Pair(b))if PairRef::ptr_eq(a,b)){return ListLength::Cyclic;}}},_=>return ListLength::Dotted}}
}

pub(crate) fn list_sequence_to_vec(v:&Value)->Vec<Value>{
    let mut out=Vec::new(); let mut cur=v.clone(); let mut seen=HashSet::new();
    while let Value::Pair(p)=cur {
        let id=p.as_ptr() as usize;
        if !seen.insert(id){break;}
        let PairData{car,cdr}= &*p.borrow();
        out.push(car.clone());
        cur=cdr.clone();
    }
    out
}
pub(crate) fn list_sequence_to_vec_repeat(v:&Value)->Vec<Value>{
    let mut out=Vec::new(); let mut cur=v.clone(); let mut seen=HashSet::new();
    while let Value::Pair(p)=cur { let id=p.as_ptr() as usize; let PairData{car,cdr}= &*p.borrow(); out.push(car.clone()); if !seen.insert(id){break;} cur=cdr.clone(); }
    out
}

pub(crate) fn reverse_list_s7(v:&Value)->Result<Value>{
    let mut cars=Vec::new(); let mut cur=v.clone(); let mut seen=HashSet::new();
    loop { match cur {
        Value::Nil=>{cars.reverse(); return Ok(Value::list(cars));},
        Value::Pair(p)=>{let id=p.as_ptr() as usize; if !seen.insert(id){cars.reverse(); return Ok(Value::list(cars));} let PairData{car,cdr}= &*p.borrow(); cars.push(car.clone()); cur=cdr.clone();},
        tail=>{if cars.len()==1{return Ok(Value::cons(tail,cars.remove(0)));} let mut out=Vec::with_capacity(cars.len()+1); out.push(tail); out.extend(cars.into_iter().rev()); return Ok(Value::list(out));}
    }}
}
pub(crate) fn sequence_to_vec(v:&Value)->Result<Vec<Value>>{
    match v {
        Value::Env(e)=>env_entries(e).to_vec(),
        Value::HashTable(h)=>Ok(h.borrow().iter().map(|(k,v)|Value::cons(k.clone(),v.clone())).collect()),
        Value::Pair(_)|Value::Nil=>Ok(list_sequence_to_vec_repeat(v)),
        Value::Vector(xs)=>Ok(xs.values()),
        Value::ProcedureSource(ps)=>{let params=&ps.params; let body=&ps.body; let macro_kind=ps.macro_kind; let head=match (macro_kind,params.star){(Some(MacroKind::Macro),true)=>"macro*",(Some(MacroKind::Macro),false)=>"macro",(Some(MacroKind::BMacro),true)=>"bacro*",(Some(MacroKind::BMacro),false)=>"bacro",(None,true)=>"lambda*",(None,false)=>"lambda"}; let mut xs=vec![Value::symbol(head), proc_source_params(params)]; xs.extend(body.borrow().iter().cloned()); Ok(xs)},
        Value::ByteVector(xs)=>Ok(xs.borrow().iter().map(|x|Value::Int(*x as i64)).collect()),
        Value::FloatVector(xs)=>Ok(xs.borrow().iter().map(|x|Value::Float(*x)).collect()),
        Value::IntVector(xs)=>Ok(xs.borrow().iter().map(|x|Value::Int(*x)).collect()),
        Value::String(s)=>Ok(s.borrow().chars().map(Value::Char).collect()),
        _=>v.to_vec()
    }
}

pub(crate) fn proc_source_params(params:&Params)->Value{
    if params.star { let mut xs=Vec::new(); if params.rest_before_formals{if let Some(r)=&params.rest{xs.push(Value::keyword("rest")); xs.push(Value::symbol(r));}} for (i,n) in params.required.iter().enumerate(){ if let Some(Some(d))=params.defaults.get(i){xs.push(Value::list(vec![Value::symbol(n),d.clone()]));}else{xs.push(Value::symbol(n));} } if params.allow_other_keys{xs.push(Value::keyword("allow-other-keys"));} if !params.rest_before_formals{if let Some(r)=&params.rest{xs.push(Value::symbol("."));xs.push(Value::symbol(r));}} Value::list(xs) }
    else { let mut xs=params.required.iter().map(|n|Value::symbol(n)).collect::<Vec<_>>(); if let Some(r)=&params.rest{xs.push(Value::symbol("."));xs.push(Value::symbol(r));} Value::list(xs) }
}

pub(crate) fn is_callable_value(v:&Value)->bool{matches!(v,Value::Procedure(_)|Value::ProcedureSource(_)|Value::Macro(_,_)|Value::RootMeta(_)|Value::Dilambda(_)|Value::Vector(_)|Value::MultiVector(_)|Value::MultiVectorView(_)|Value::ByteVector(_)|Value::FloatVector(_)|Value::IntVector(_)|Value::String(_)|Value::Pair(_)|Value::Env(_)|Value::HashTable(_)|Value::Hook(_,_)|Value::Iterator(_)|Value::ValuesData(_))}
pub(crate) fn error_arg_string(v:&Value)->String{match v{Value::Symbol(s)=>format!("'{}",s),_=>v.to_string()}}
pub(crate) fn call_form_string(proc:&Value,args:&[Value])->String{format!("({}{})",hash_table_expr(proc), if args.is_empty(){String::new()}else{format!(" {}",args.iter().map(error_arg_string).collect::<Vec<_>>().join(" "))})}
pub(crate) fn hash_table_expr(v:&Value)->String{if let Value::HashTable(h)=v{let mut parts=Vec::new(); for (k,val) in h.borrow().iter(){parts.push(error_arg_string(k)); parts.push(val.to_string());} if parts.is_empty(){"(hash-table)".to_string()}else{format!("(hash-table {})",parts.join(" "))}}else{v.to_string()}}
#[allow(dead_code)]
pub(crate) fn cant_take_arguments_error(form:&str, first:&Value, rest:&[Value])->SchemeError{let mut becomes=vec![first.clone()]; becomes.extend(rest.iter().cloned()); SchemeError::new("syntax-error",vec![Value::string("~$ becomes ~$, but ~S can't take arguments"),Value::string(form),Value::list(becomes),first.clone()])}
pub(crate) fn cant_take_arguments_error_value(form:Value, first:&Value, rest:&[Value])->SchemeError{let mut becomes=vec![first.clone()]; becomes.extend(rest.iter().cloned()); SchemeError::new("syntax-error",vec![Value::string("~$ becomes ~$, but ~S can't take arguments"),form,Value::list(becomes),first.clone()])}
#[allow(dead_code)]
pub(crate) fn index_vec(v:&[Value], args:&[Value])->Result<Value>{ let raw=args.get(0).ok_or_else(||SchemeError::new("wrong-number-of-args",vec![]))?; let i=match raw{Value::Int(n)=>*n,_=>return Err(SchemeError::new("wrong-type-arg",vec![Value::string("~A ~:D argument, ~S, is ~A but should be ~A"),Value::symbol("vector-ref"),Value::Int(2),raw.clone(),Value::string(match raw{Value::Float(_)=>"a real",Value::RationalValue(_)=>"a ratio",Value::Symbol(_)=>"a symbol",_=>"an object"}),Value::string("an integer")]))}; if i<0||i as usize>=v.len(){return Err(SchemeError::new("out-of-range",vec![Value::string("~A ~:D argument, ~S, is out of range (~A)"),Value::symbol("vector-ref"),Value::Int(2),Value::Int(i),Value::string(if i<0{"it is negative"}else{"it is too large"})]));} Ok(v[i as usize].clone()) }
pub(crate) fn index_bvec(v:&[u8], args:&[Value])->Result<Value>{ if args.len()>1{return Err(SchemeError::new("wrong-number-of-args",vec![Value::string("~S: too many indices: ~S"),Value::symbol("vector-ref"),Value::list(args.to_vec())]));} let raw=args.get(0).ok_or_else(||SchemeError::new("wrong-number-of-args",vec![]))?; let i=match raw{Value::Int(n)=>*n,_=>return Err(SchemeError::new("wrong-type-arg",vec![Value::string("~A ~:D argument, ~S, is ~A but should be ~A"),Value::symbol("byte-vector-ref"),Value::Int(2),raw.clone(),Value::string(match raw{Value::Float(_)=>"a real",Value::RationalValue(_)=>"a ratio",Value::Symbol(_)=>"a symbol",_=>"an object"}),Value::string("an integer")]))}; if i<0||i as usize>=v.len(){return Err(SchemeError::new("out-of-range",vec![Value::string("~A ~:D argument, ~S, is out of range (~A)"),Value::symbol("byte-vector-ref"),Value::Int(2),Value::Int(i),Value::string(if i<0{"it is negative"}else{"it is too large"})]))} Ok(Value::Int(v[i as usize] as i64)) }
pub(crate) fn index_fvec(v:&[f64], args:&[Value])->Result<Value>{ if args.len()>1{return Err(SchemeError::new("wrong-number-of-args",vec![Value::string("~S: too many indices: ~S"),Value::symbol("vector-ref"),Value::list(args.to_vec())]));} let raw=args.get(0).ok_or_else(||SchemeError::new("wrong-number-of-args",vec![]))?; let i=match raw{Value::Int(n)=>*n,_=>return Err(SchemeError::new("wrong-type-arg",vec![Value::string("~A ~:D argument, ~S, is ~A but should be ~A"),Value::symbol("float-vector-ref"),Value::Int(2),raw.clone(),Value::string(match raw{Value::Float(_)=>"a real",Value::RationalValue(_)=>"a ratio",Value::Symbol(_)=>"a symbol",_=>"an object"}),Value::string("an integer")]))}; if i<0||i as usize>=v.len(){return Err(SchemeError::new("out-of-range",vec![Value::string("~A ~:D argument, ~S, is out of range (~A)"),Value::symbol("float-vector-ref"),Value::Int(2),Value::Int(i),Value::string(if i<0{"it is negative"}else{"it is too large"})]));} Ok(Value::Float(v[i as usize])) }
pub(crate) fn index_ivec(v:&[i64], args:&[Value])->Result<Value>{ if args.len()>1{return Err(SchemeError::new("wrong-number-of-args",vec![Value::string("~S: too many indices: ~S"),Value::symbol("vector-ref"),Value::list(args.to_vec())]));} let raw=args.get(0).ok_or_else(||SchemeError::new("wrong-number-of-args",vec![]))?; let i=match raw{Value::Int(n)=>*n,_=>return Err(SchemeError::new("wrong-type-arg",vec![Value::string("~A ~:D argument, ~S, is ~A but should be ~A"),Value::symbol("int-vector-ref"),Value::Int(2),raw.clone(),Value::string(match raw{Value::Float(_)=>"a real",Value::RationalValue(_)=>"a ratio",Value::Symbol(_)=>"a symbol",_=>"an object"}),Value::string("an integer")]))}; if i<0||i as usize>=v.len(){return Err(SchemeError::new("out-of-range",vec![Value::string("~A ~:D argument, ~S, is out of range (~A)"),Value::symbol("int-vector-ref"),Value::Int(2),Value::Int(i),Value::string(if i<0{"it is negative"}else{"it is too large"})]));} Ok(Value::Int(v[i as usize])) }
