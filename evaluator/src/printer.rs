use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use super::*;

fn note_label(id:usize, labels:&mut Vec<usize>){if !labels.contains(&id){labels.push(id);}}
pub(crate) fn collect_cycle_labels(v:&Value, labels:&mut Vec<usize>, stack:&mut Vec<usize>, visited:&mut HashSet<usize>) {
    match v {
        Value::Pair(p)=>{
            let id=Rc::as_ptr(p) as usize; if stack.contains(&id){note_label(id,labels); return;} if !visited.insert(id){return;} stack.push(id);
            let Object::Pair{car,cdr}= &*p.borrow(); collect_cycle_labels(car,labels,stack,visited); collect_cycle_labels(cdr,labels,stack,visited); stack.pop();
        }
        Value::Vector(xs)=>{
            let id=Rc::as_ptr(xs) as usize; if stack.contains(&id){note_label(id,labels); return;} if !visited.insert(id){return;} stack.push(id);
            for x in xs.borrow().iter(){collect_cycle_labels(x,labels,stack,visited);} stack.pop();
        }
        Value::HashTable(h)=>{
            let id=Rc::as_ptr(h) as usize; if stack.contains(&id){note_label(id,labels); return;} if !visited.insert(id){return;} stack.push(id);
            for (k,v) in h.borrow().iter(){collect_cycle_labels(k,labels,stack,visited); collect_cycle_labels(v,labels,stack,visited);} stack.pop();
        }
        Value::Env(e)=>{
            let id=Rc::as_ptr(e) as usize; if stack.contains(&id){note_label(id,labels); return;} if !visited.insert(id){return;} stack.push(id);
            for k in e.order.borrow().iter(){if let Some(v)=e.vars.borrow().get(k){collect_cycle_labels(v,labels,stack,visited);}} stack.pop();
        }
        _=>{}
    }
}
fn contains_labeled(v:&Value,label_set:&HashSet<usize>,seen:&mut HashSet<usize>)->bool{match v{Value::Pair(p)=>{let id=Rc::as_ptr(p) as usize;if label_set.contains(&id){return true;} if !seen.insert(id){return false;} let Object::Pair{car,cdr}= &*p.borrow(); contains_labeled(car,label_set,seen)||contains_labeled(cdr,label_set,seen)},Value::Vector(xs)=>{let id=Rc::as_ptr(xs) as usize;if label_set.contains(&id){return true;} if !seen.insert(id){return false;} xs.borrow().iter().any(|x|contains_labeled(x,label_set,seen))},Value::HashTable(h)=>{let id=Rc::as_ptr(h) as usize;if label_set.contains(&id){return true;} if !seen.insert(id){return false;} h.borrow().iter().any(|(k,v)|contains_labeled(k,label_set,seen)||contains_labeled(v,label_set,seen))},Value::Env(e)=>{let id=Rc::as_ptr(e) as usize;if label_set.contains(&id){return true;} if !seen.insert(id){return false;} e.order.borrow().iter().any(|k|e.vars.borrow().get(k).map(|v|contains_labeled(v,label_set,seen)).unwrap_or(false))},_=>false}}
fn collect_shared_labeled(v:&Value, labels:&mut Vec<usize>, label_set:&HashSet<usize>, visited:&mut HashSet<usize>) {match v{Value::Pair(p)=>{let id=Rc::as_ptr(p) as usize;if !visited.insert(id){if contains_labeled(v,label_set,&mut HashSet::new()){note_label(id,labels);} return;} let Object::Pair{car,cdr}= &*p.borrow(); collect_shared_labeled(car,labels,label_set,visited); collect_shared_labeled(cdr,labels,label_set,visited)},Value::Vector(xs)=>{let id=Rc::as_ptr(xs) as usize;if !visited.insert(id){if contains_labeled(v,label_set,&mut HashSet::new()){note_label(id,labels);} return;} for x in xs.borrow().iter(){collect_shared_labeled(x,labels,label_set,visited);}},Value::HashTable(h)=>{let id=Rc::as_ptr(h) as usize;if !visited.insert(id){if contains_labeled(v,label_set,&mut HashSet::new()){note_label(id,labels);} return;} for (k,val) in h.borrow().iter(){collect_shared_labeled(k,labels,label_set,visited); collect_shared_labeled(val,labels,label_set,visited);}},Value::Env(e)=>{let id=Rc::as_ptr(e) as usize;if !visited.insert(id){if contains_labeled(v,label_set,&mut HashSet::new()){note_label(id,labels);} return;} for k in e.order.borrow().iter(){if let Some(val)=e.vars.borrow().get(k){collect_shared_labeled(val,labels,label_set,visited);}}},_=>{}}}
fn collect_envs_containing_labeled(v:&Value, labels:&mut Vec<usize>, label_set:&HashSet<usize>, visited:&mut HashSet<usize>){match v{Value::Env(e)=>{let id=Rc::as_ptr(e) as usize;if !visited.insert(id){return;} if contains_labeled(v,label_set,&mut HashSet::new()){note_label(id,labels);} for k in e.order.borrow().iter(){if let Some(val)=e.vars.borrow().get(k){collect_envs_containing_labeled(val,labels,label_set,visited);}}},Value::Pair(p)=>{let id=Rc::as_ptr(p) as usize;if !visited.insert(id){return;} let Object::Pair{car,cdr}= &*p.borrow(); collect_envs_containing_labeled(car,labels,label_set,visited); collect_envs_containing_labeled(cdr,labels,label_set,visited)},Value::Vector(xs)=>{let id=Rc::as_ptr(xs) as usize;if !visited.insert(id){return;} for x in xs.borrow().iter(){collect_envs_containing_labeled(x,labels,label_set,visited);}},Value::HashTable(h)=>{let id=Rc::as_ptr(h) as usize;if !visited.insert(id){return;} for (k,val) in h.borrow().iter(){collect_envs_containing_labeled(k,labels,label_set,visited); collect_envs_containing_labeled(val,labels,label_set,visited);}},_=>{}}}
pub(crate) fn s7_object_string(v:&Value)->String{
    let mut ordered=Vec::new(); collect_cycle_labels(v,&mut ordered,&mut Vec::new(),&mut HashSet::new());
    let cycle_set=ordered.iter().copied().collect::<HashSet<_>>(); collect_shared_labeled(v,&mut ordered,&cycle_set,&mut HashSet::new()); collect_envs_containing_labeled(v,&mut ordered,&cycle_set,&mut HashSet::new());
    let mut labels=HashMap::new(); for (i,id) in ordered.into_iter().enumerate(){labels.insert(id,i+1);}
    let mut printed=HashSet::new(); fmt_s7_labeled(v,&labels,&mut printed)
}
fn fmt_s7_labeled(v:&Value, labels:&HashMap<usize,usize>, printed:&mut HashSet<usize>)->String{
    match v {
        Value::Pair(p)=>{
            let id=Rc::as_ptr(p) as usize;
            if let Some(label)=labels.get(&id){ if printed.contains(&id){return format!("#{}#",label);} printed.insert(id); return format!("#{}={}", label, fmt_s7_pair_body(v,labels,printed)); }
            fmt_s7_pair_body(v,labels,printed)
        }
        Value::Vector(xs)=>{
            let id=Rc::as_ptr(xs) as usize;
            if let Some(label)=labels.get(&id){ if printed.contains(&id){return format!("#{}#",label);} printed.insert(id); return format!("#{}={}", label, fmt_s7_vector(xs,labels,printed)); }
            fmt_s7_vector(xs,labels,printed)
        }
        Value::HashTable(h)=>{
            let id=Rc::as_ptr(h) as usize;
            if let Some(label)=labels.get(&id){ if printed.contains(&id){return format!("#{}#",label);} printed.insert(id); return format!("#{}={}", label, fmt_s7_hash(h,labels,printed)); }
            fmt_s7_hash(h,labels,printed)
        }
        Value::Env(e)=>{
            let id=Rc::as_ptr(e) as usize;
            if let Some(label)=labels.get(&id){ if printed.contains(&id){return format!("#{}#",label);} printed.insert(id); return format!("#{}={}", label, fmt_s7_env(e,labels,printed)); }
            fmt_s7_env(e,labels,printed)
        }
        _=>v.to_string(),
    }
}
fn fmt_s7_vector(xs:&Rc<RefCell<Vec<Value>>>, labels:&HashMap<usize,usize>, printed:&mut HashSet<usize>)->String{
    let parts=xs.borrow().iter().map(|x|fmt_s7_labeled(x,labels,printed)).collect::<Vec<_>>(); format!("#({})", parts.join(" "))
}
fn fmt_s7_hash(h:&Rc<RefCell<Vec<(Value,Value)>>>, labels:&HashMap<usize,usize>, printed:&mut HashSet<usize>)->String{
    let mut parts=Vec::new();
    for (k,v) in h.borrow().iter(){ parts.push(fmt_s7_labeled(k,labels,printed)); parts.push(fmt_s7_labeled(v,labels,printed)); }
    if parts.is_empty(){"(hash-table)".to_string()}else{format!("(hash-table {})", parts.join(" "))}
}
fn fmt_s7_env(e:&EnvRef, labels:&HashMap<usize,usize>, printed:&mut HashSet<usize>)->String{
    let mut parts=Vec::new();
    for k in e.order.borrow().iter(){ if let Some(v)=e.vars.borrow().get(k){parts.push(format!("'{} {}",k,fmt_s7_labeled(v,labels,printed)));} }
    if parts.is_empty(){"(inlet)".to_string()}else{format!("(inlet {})",parts.join(" "))}
}
fn fmt_s7_pair_body(v:&Value, labels:&HashMap<usize,usize>, printed:&mut HashSet<usize>)->String{
    let mut out=String::from("("); let mut first=true; let mut cur=v.clone();
    loop {
        match cur {
            Value::Nil=>break,
            Value::Pair(ref p)=>{
                let id=Rc::as_ptr(p) as usize;
                if !first && labels.contains_key(&id){ out.push_str(" . "); out.push_str(&fmt_s7_labeled(&cur,labels,printed)); break; }
                let (car,cdr)={let Object::Pair{car,cdr}= &*p.borrow(); (car.clone(),cdr.clone())};
                if !first{out.push(' ');} out.push_str(&fmt_s7_labeled(&car,labels,printed)); first=false; cur=cdr;
            }
            other=>{out.push_str(" . "); out.push_str(&fmt_s7_labeled(&other,labels,printed)); break;}
        }
    }
    out.push(')'); out
}
fn value_contains_symbol(v:&Value, name:&str)->bool{
    match v { Value::Symbol(s)=>s.as_str()==name, Value::Pair(_)=>{let mut cur=v.clone(); let mut seen=HashSet::new(); while let Value::Pair(ref p)=cur{let id=Rc::as_ptr(p) as usize; if !seen.insert(id){break;} let (car,cdr)={let Object::Pair{car,cdr}= &*p.borrow(); (car.clone(),cdr.clone())}; if value_contains_symbol(&car,name){return true;} cur=cdr;} value_contains_symbol(&cur,name)}, Value::Vector(xs)=>xs.borrow().iter().any(|x|value_contains_symbol(x,name)), _=>false }
}
fn proc_params_readable(params:&Params, star:bool)->String{
    if params.required.is_empty() { if let Some(r)=&params.rest{return r.clone();} }
    let mut parts=Vec::new();
    for (i,n) in params.required.iter().enumerate(){
        let d=params.defaults.get(i).and_then(|x|x.as_ref());
        if star { if let Some(def)=d { if matches!(def,Value::Bool(false)){parts.push(n.clone());}else{parts.push(format!("({} {})", n, readable_repr(def)));} } else { parts.push(n.clone()); } } else { parts.push(n.clone()); }
    }
    if params.allow_other_keys { parts.push(":allow-other-keys".to_string()); }
    if let Some(r)=&params.rest { if star && params.required.len()==1 && r=="b" { parts.push(format!(". {}", r)); } else if star { parts.push(format!(":rest {}", r)); } else if parts.is_empty(){ return r.clone(); } else { parts.push(format!(". {}", r)); } }
    format!("({})", parts.join(" "))
}
fn readable_pair(v:&Value)->String{
    if let Ok(xs)=v.to_vec(){ return format!("(list{})", if xs.is_empty(){String::new()}else{format!(" {}", xs.iter().map(readable_repr).collect::<Vec<_>>().join(" "))}); }
    if let Value::Pair(p)=v { let Object::Pair{car,cdr}= &*p.borrow(); return format!("(cons {} {})", readable_repr(car), readable_repr(cdr)); }
    v.to_string()
}
fn qq_code(v:&Value)->String{
    if let Value::Pair(_) = v {
        if v.car().ok().and_then(|x|x.as_symbol().map(|s|s.to_string())).as_deref()==Some("unquote") { return v.cdr().ok().and_then(|x|x.car().ok()).map(|x|code_repr(&x)).unwrap_or_else(||"#<unspecified>".to_string()); }
        if v.car().ok().and_then(|x|x.as_symbol().map(|s|s.to_string())).as_deref()==Some("unquote-splicing") { return v.cdr().ok().and_then(|x|x.car().ok()).map(|x|format!("(apply-values {})", code_repr(&x))).unwrap_or_else(||"(apply-values)".to_string()); }
        let xs=list_sequence_to_vec(v);
        return format!("(list-values{})", if xs.is_empty(){String::new()}else{format!(" {}", xs.iter().map(qq_code).collect::<Vec<_>>().join(" "))});
    }
    match v { Value::Symbol(s)=>format!("'{}",s), _=>readable_repr(v) }
}
pub(crate) fn code_repr(v:&Value)->String{
    match v {
        Value::Symbol(s)=>s.to_string(),
        Value::Pair(_)=>{
            if v.car().ok().and_then(|x|x.as_symbol().map(|s|s.to_string())).as_deref()==Some("quote") { return v.cdr().ok().and_then(|x|x.car().ok()).map(|x|format!("'{}",x)).unwrap_or_else(||"'()".to_string()); }
            if v.car().ok().and_then(|x|x.as_symbol().map(|s|s.to_string())).as_deref()==Some("quasiquote") { return v.cdr().ok().and_then(|x|x.car().ok()).map(|x|qq_code(&x)).unwrap_or_else(||"#<unspecified>".to_string()); }
            let mut out=String::from("("); let mut first=true; let mut cur=v.clone();
            loop { match cur { Value::Nil=>break, Value::Pair(ref p)=>{let (car,cdr)={let Object::Pair{car,cdr}= &*p.borrow(); (car.clone(),cdr.clone())}; if !first{out.push(' ');} out.push_str(&code_repr(&car)); first=false; cur=cdr;}, other=>{out.push_str(" . "); out.push_str(&code_repr(&other)); break;} } }
            out.push(')'); out
        }
        Value::Vector(xs)=>if xs.borrow().is_empty(){"#()".to_string()}else{format!("(vector {})", xs.borrow().iter().map(code_repr).collect::<Vec<_>>().join(" "))},
        Value::MultiVector{dims,data,..}=>format!("(subvector (vector {}) 0 {} '{})", data.borrow().iter().map(code_repr).collect::<Vec<_>>().join(" "), data.borrow().len(), Value::list(dims.iter().map(|d|Value::Int(*d as i64)).collect())),
        Value::MultiVectorView{dims,data,offset,..}=>{let n=dims.iter().product::<usize>(); format!("(subvector (vector {}) 0 {} '{})", data.borrow()[*offset..*offset+n].iter().map(code_repr).collect::<Vec<_>>().join(" "), n, Value::list(dims.iter().map(|d|Value::Int(*d as i64)).collect()))},
        _=>readable_repr(v),
    }
}
fn readable_symbol(s:&str)->String{ if is_syntax_name(s){format!("#_{}",s)} else if s.chars().any(|c|c.is_whitespace()||c=='('||c==')'||c=='\"'){format!("(symbol {:?})",s)} else {format!("'{}",s)} }
fn float_readable(f:f64)->String{let s=f.to_string(); if s.contains('.')||s.contains('e')||s.contains('E'){s}else{format!("{}.0",s)}}
fn float_readable_value(v:&Value)->String{match v{Value::Float(f)=>float_readable(*f),_=>v.to_string()}}
pub(crate) fn iterator_source_repr(v:&Value)->String{match v{Value::FloatVector(xs)=>format!("#r({})",xs.borrow().iter().map(|f|float_readable(*f)).collect::<Vec<_>>().join(" ")),Value::IntVector(xs)=>format!("#i({})",xs.borrow().iter().map(|x|x.to_string()).collect::<Vec<_>>().join(" ")),Value::ByteVector(xs)=>format!("#u({})",xs.borrow().iter().map(|x|x.to_string()).collect::<Vec<_>>().join(" ")),Value::String(_)=>v.to_string(),Value::Vector(xs)=>if xs.borrow().is_empty(){"#()".to_string()}else{format!("(vector {})",xs.borrow().iter().map(readable_repr).collect::<Vec<_>>().join(" "))},Value::Pair(_)|Value::Nil=>readable_pair(v),_=>String::new()}}
fn split_readable_let(s:&str)->Option<(String,String)>{
    if !s.starts_with("(let ("){return None;}
    let bytes=s.as_bytes(); let mut depth=0i32; let mut end=None;
    for (i,&b) in bytes.iter().enumerate().skip(5){match b as char{'('=>depth+=1,')'=>{depth-=1;if depth==0{end=Some(i);break;}},_=>{}}}
    let end=end?; if s.as_bytes().get(end+1)!=Some(&b' '){return None;} Some((s[6..end].to_string(),s[end+2..s.len()-1].to_string()))
}
pub(crate) fn readable_repr(v:&Value)->String{
    match v {
        Value::Symbol(s)=>readable_symbol(s),
        Value::Keyword(_)=>v.to_string(),
        Value::Nil=>"()".to_string(),
        Value::Eof=>"(begin #<eof>)".to_string(),
        Value::Pair(_)=>readable_pair(v),
        Value::Vector(xs)=>if xs.borrow().is_empty(){"#()".to_string()}else{format!("(vector {})", xs.borrow().iter().map(readable_repr).collect::<Vec<_>>().join(" "))},
        Value::MultiVector{dims,data,..}=>format!("(subvector (vector {}) 0 {} '{})", data.borrow().iter().map(readable_repr).collect::<Vec<_>>().join(" "), data.borrow().len(), Value::list(dims.iter().map(|d|Value::Int(*d as i64)).collect())),
        Value::MultiVectorView{dims,data,offset,..}=>{let n=dims.iter().product::<usize>(); format!("(subvector (vector {}) 0 {} '{})", data.borrow()[*offset..*offset+n].iter().map(readable_repr).collect::<Vec<_>>().join(" "), n, Value::list(dims.iter().map(|d|Value::Int(*d as i64)).collect()))},
        Value::Env(e)=>{ let keys=e.order.borrow().clone(); let mut parts=Vec::new(); for k in keys { if let Some(v)=e.vars.borrow().get(&k){parts.push(format!(":{} {}", k, readable_repr(v)));} } if parts.is_empty(){"(inlet)".to_string()}else{format!("(inlet {})", parts.join(" "))} }
        Value::Procedure(p)=>match &**p { Procedure::Builtin{name,..}=>format!("#_{}",name), Procedure::Lambda{params,body,env,name}=>{let head=if params.star && params.required.is_empty() && params.rest.is_some(){"lambda"}else if params.star{"lambda*"}else{"lambda"}; let body_b=body.borrow(); let body_s=body_b.iter().map(code_repr).collect::<Vec<_>>().join(" "); let lam=format!("({} {}{})", head, proc_params_readable(params,params.star), if body_s.is_empty(){String::new()}else{format!(" {}",body_s)}); let captured=env.order.borrow().iter().filter(|k|Some(*k)!=name.as_ref()).filter(|k|body_b.iter().any(|b|value_contains_symbol(b,k))).filter_map(|k|env.vars.borrow().get(k).and_then(|v|match v{Value::Procedure(q) if Rc::ptr_eq(p,q)=>None,_=>Some(format!("({} {})",k,readable_repr(v)))})).collect::<Vec<_>>(); if captured.is_empty(){lam}else{format!("(let ({}) {})", captured.join(" "), lam)}}},
        Value::ProcedureSource{params,body}=>{let mut parts=vec![format!("'{}", if params.star{"lambda*"}else{"lambda"}), readable_repr(&proc_source_params(params))]; parts.extend(body.borrow().iter().map(readable_repr)); format!("(list {})",parts.join(" "))},
        Value::Macro(p,k)=>match &**p { Procedure::Lambda{params,body,..}=>{let head=match (k,params.star){(MacroKind::Macro,false)=>"macro",(MacroKind::Macro,true)=>"macro*",(MacroKind::BMacro,false)=>"bacro",(MacroKind::BMacro,true)=>"bacro*"}; let body_s=body.borrow().iter().map(code_repr).collect::<Vec<_>>().join(" "); format!("({} {}{})", head, proc_params_readable(params,params.star), if body_s.is_empty(){String::new()}else{format!(" {}",body_s)})}, _=>"#<macro>".to_string()},
        Value::Dilambda(dl)=>{let a=readable_repr(&dl.0); let b=readable_repr(&dl.1); if let (Some((ba,la)),Some((bb,lb)))=(split_readable_let(&a),split_readable_let(&b)){if ba==bb{format!("(let ({}) (dilambda {} {}))",ba,la,lb)}else{format!("(dilambda {} {})",a,b)}}else{format!("(dilambda {} {})",a,b)}},
        Value::HashTable(h)=>{ let mut parts=Vec::new(); for (k,v) in h.borrow().iter(){parts.push(readable_repr(k)); parts.push(readable_repr(v));} if parts.is_empty(){"(hash-table)".to_string()}else{format!("(hash-table {})", parts.join(" "))} }
        Value::Port(p)=>match &*p.borrow(){Port::Input{text,pos,repr}=>match repr{PortRepr::Stdin=>"*stdin*".to_string(),PortRepr::CallWithInput|PortRepr::ClosedInput=>format!("(call-with-input-string {} (lambda (p) p))", Value::string(text[*pos..].iter().collect::<String>())),_=>format!("(open-input-string {})", Value::string(text[*pos..].iter().collect::<String>()))},Port::Output{text,repr}=>match repr{PortRepr::Stdout=>"*stdout*".to_string(),PortRepr::Stderr=>"*stderr*".to_string(),PortRepr::ClosedOutput=>"(let ((p (open-output-string))) (close-output-port p) p)".to_string(),_=>if text.is_empty(){"(let ((p (open-output-string))) p)".to_string()}else{format!("(let ((p (open-output-string))) (display {} p) p)", Value::string(text.clone()))}}},
        Value::CPointer(n)=>format!("(c-pointer {})",n),
        Value::Iterator{kind,items,source,consumed}=>{let n=*consumed.borrow(); let xs=items.borrow(); if kind.as_str()=="float-vector" && n>0 && !xs.is_empty() && !source.is_empty(){format!("(let ((iter (make-iterator {}))) {}iter)",source,"(iter) ".repeat(n))}else{let body=match kind.as_str(){"byte-vector"=>format!("#u({})", xs.iter().map(|v|v.to_string()).collect::<Vec<_>>().join(" ")),"float-vector"=>format!("#r({})", xs.iter().map(float_readable_value).collect::<Vec<_>>().join(" ")),"int-vector"=>format!("#i({})", xs.iter().map(|v|v.to_string()).collect::<Vec<_>>().join(" ")),"string"=>Value::string(xs.iter().filter_map(|v|if let Value::Char(c)=v{Some(*c)}else{None}).collect::<String>()).to_string(),"vector"=>if xs.is_empty(){"#()".to_string()}else{format!("(vector {})", xs.iter().map(readable_repr).collect::<Vec<_>>().join(" "))},"hash-table"=>"(hash-table)".to_string(),_=>format!("(list{})", if xs.is_empty(){String::new()}else{format!(" {}", xs.iter().map(readable_repr).collect::<Vec<_>>().join(" "))})}; format!("(make-iterator {})", body)}}
        Value::RootMeta(name)=>if name.as_str()=="else"{"'else".to_string()}else if is_syntax_name(name){format!("#_{}",name)}else{name.to_string()},
        _=>v.to_string(),
    }
}
pub(crate) fn b_object_to_string(_: &mut Evaluator,a:&[Value])->Result<Value>{let readable=a.iter().skip(1).any(|v|matches!(v,Value::Keyword(k) if k.as_str()=="readable"||k.as_str()==":readable")); Ok(Value::string(if readable{readable_repr(&a[0])}else{match &a[0]{Value::Procedure(p)=>match &**p{Procedure::Builtin{name,..}=>(*name).to_string(),_=>s7_object_string(&a[0])},Value::RootMeta(name)=>name.to_string(),_=>s7_object_string(&a[0])}}))}
pub(crate) fn b_type_of(_: &mut Evaluator,a:&[Value])->Result<Value>{Ok(type_value(&a[0]))}
pub(crate) fn type_value(v:&Value)->Value{Value::symbol(match v{Value::Bool(_)=>"boolean?",Value::Nil=>"null?",Value::Undefined=>"undefined?",Value::Int(_)=>"integer?",Value::Rational(_,_)=>"rational?",Value::Float(_)=>"float?",Value::Complex(_,_)=>"complex?",Value::NumberLiteral(_,_)=>"number?",Value::Char(_)|Value::NamedChar(_)=>"char?",Value::String(_)=>"string?",Value::Symbol(_)=>"symbol?",Value::Keyword(_)=>"symbol?",Value::Pair(_)=>"pair?",Value::Vector(_)|Value::MultiVector{..}|Value::MultiVectorView{..}=>"vector?",Value::ByteVector(_)=>"byte-vector?",Value::FloatVector(_)=>"float-vector?",Value::IntVector(_)=>"int-vector?",Value::HashTable(_)=>"hash-table?",Value::Env(_)=>"let?",Value::Procedure(_)=>"procedure?",Value::Macro(_,_)=>"macro?",Value::Port(_)=>"port?",Value::Hook(_,_)=>"procedure?",Value::Iterator{..}=>"iterator?",Value::CPointer(_)=>"c-pointer?",Value::Dilambda(_)=>"dilambda?",Value::Values(_)=>"values?",Value::Commented(_)=>"comment?",Value::SetterRef(_)=>"setter?",Value::RootMeta(name)=>meta_type(name),_=>"object?"})}