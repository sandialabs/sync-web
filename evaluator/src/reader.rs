use std::cell::RefCell;
use std::rc::Rc;

use super::*;

pub(crate) struct Reader { pub(crate) chars: Vec<char>, pub(crate) pos: usize }
pub(crate) fn parse_all(s:&str)->Result<Vec<Value>>{ let mut r=Reader{chars:s.chars().collect(),pos:0}; let mut out=Vec::new(); while {r.skip_ws(); r.pos<r.chars.len()} { match r.read_expr(){ Ok(v)=>out.push(v), Err(e)=>{ if e.tag=="read-error" && matches!(e.args.get(0), Some(Value::String(msg)) if msg.borrow().as_str()=="unexpected )") { continue; } else { return Err(e); } } } } Ok(out) }
impl Reader {
    fn peek(&self)->Option<char>{self.chars.get(self.pos).copied()}
    fn next(&mut self)->Option<char>{let c=self.peek()?; self.pos+=1; Some(c)}
    fn skip_ws(&mut self){ loop { while self.peek().map(|c| c.is_whitespace()).unwrap_or(false){self.pos+=1;} if self.peek()==Some(';'){ while self.peek().is_some() && self.peek()!=Some('\n'){self.pos+=1;} } else if self.peek()==Some('#') && self.chars.get(self.pos+1)==Some(&'|') { self.pos+=2; while self.pos+1<self.chars.len() && !(self.chars[self.pos]=='|' && self.chars[self.pos+1]=='#'){self.pos+=1;} if self.pos+1<self.chars.len(){self.pos+=2;} } else {break;} } }
    pub(crate) fn read_expr(&mut self)->Result<Value>{ self.skip_ws(); match self.next().ok_or_else(||SchemeError::new("read-error",vec![]))? { '('=>self.read_list(), ')'=>Err(SchemeError::new("read-error",vec![Value::string("unexpected )")])), '\''=>Ok(Value::list(vec![Value::symbol("quote"), self.read_expr()?])), '`'=>Ok(Value::list(vec![Value::symbol("quasiquote"), self.read_expr()?])), ','=>{ if self.peek()==Some('@'){ self.pos+=1; Ok(Value::list(vec![Value::symbol("unquote-splicing"), self.read_expr()?])) } else { Ok(Value::list(vec![Value::symbol("unquote"), self.read_expr()?])) } }, '"'=>self.read_string(), '|'=>self.read_bar_symbol(), '#'=>self.read_sharp(), c=>self.read_atom(c) } }
    fn read_list(&mut self)->Result<Value>{ let mut xs=Vec::new(); loop { self.skip_ws(); if self.peek()==Some(')'){self.pos+=1; return Ok(Value::list(xs));} if self.peek()==Some('.') { self.pos+=1; let tail=self.read_expr()?; self.skip_ws(); if self.next()!=Some(')'){return Err(SchemeError::new("read-error",vec![]));} return Ok(xs.into_iter().rev().fold(tail, |cdr,car| Value::cons(car,cdr))); } xs.push(self.read_expr()?); } }
    fn read_string(&mut self)->Result<Value>{ let mut s=String::new(); loop { match self.next().ok_or_else(||SchemeError::new("string-read-error",vec![]))? { '"'=>break, '\\'=>{ let c=self.next().unwrap_or('\\'); if c=='x' { let mut hex=String::new(); while let Some(h)=self.next(){ if h==';'{break;} hex.push(h); } if let Ok(n)=u32::from_str_radix(&hex,16){ if let Some(ch)=char::from_u32(n){s.push(ch);} } } else { s.push(match c{'n'=>'\n','t'=>'\t','r'=>'\r','b'=>'\u{8}','"'=>'"','\\'=>'\\',c=>c});}}, c=>s.push(c) } } Ok(Value::string(s)) }
    fn read_sharp(&mut self)->Result<Value>{ match self.next().ok_or_else(||SchemeError::new("read-error",vec![]))? { ';'=>{let v=self.read_expr()?; Ok(Value::Commented(Box::new(v)))}, ':'=>Ok(Value::symbol("#:")), '_'=>Ok(Value::symbol(&self.read_token())), '<'=>{let mut name=String::new(); while let Some(c)=self.next(){ if c=='>'{break;} name.push(c); } match name.as_str(){"eof"=>Ok(Value::Eof),"undefined"=>Ok(Value::Undefined),"unspecified"=>Ok(Value::Unspecified),_=>Ok(Value::symbol(&format!("#<{}>",name))) }}, c if c.is_ascii_digit()=>self.read_multivector(c), 't'=>Ok(Value::Bool(true)), 'f'=>Ok(Value::Bool(false)), '\\'=>self.read_char(), '('=>{let lst=self.read_list()?; let v=lst.to_vec().map_err(|_|SchemeError::new("read-error",vec![Value::string("vector contents list is not a proper list")]))?; Ok(Value::Vector(Rc::new(RefCell::new(v))))},
 'u'=>{ if self.peek().map(|c|c.is_ascii_digit()).unwrap_or(false){let d=self.next().unwrap(); return self.read_multivector_kind(d,Some("u"));} if self.next()!=Some('('){return Err(SchemeError::new("read-error",vec![]));} let mut out=Vec::new(); for (i,x) in self.read_list()?.to_vec()?.into_iter().enumerate(){let n=match x{Value::Int(n)=>n,ref v=>return Err(SchemeError::new("wrong-type-arg",vec![Value::string("~A ~:D argument, ~S, is ~A but should be ~A"),Value::symbol("byte-vector"),Value::Int((i+1) as i64),v.clone(),Value::string(match v{Value::Symbol(_)=>"a symbol",Value::Float(_)=>"a real",Value::Rational(_,_)=>"a ratio",_=>"an object"}),Value::string("an integer")]))}; if !(0..=255).contains(&n){return Err(SchemeError::new("wrong-type-arg",vec![Value::string("~A ~:D argument, ~S, is ~A but should be ~A"),Value::symbol("byte-vector"),Value::Int((i+1) as i64),Value::Int(n),Value::string("an integer"),Value::string("an unsigned byte")]))} out.push(n as u8);} Ok(Value::ByteVector(Rc::new(RefCell::new(out))))}, 'r'=>{ if self.peek().map(|c|c.is_ascii_digit()).unwrap_or(false){let d=self.next().unwrap(); return self.read_multivector_kind(d,Some("r"));} if self.next()!=Some('('){return Err(SchemeError::new("read-error",vec![]));} let mut out=Vec::new(); for (i,x) in self.read_list()?.to_vec()?.into_iter().enumerate(){match x{Value::Int(_)|Value::Float(_)|Value::Rational(_,_)=>out.push(as_f64(&x)?),ref v=>return Err(SchemeError::new("wrong-type-arg",vec![Value::string("~A ~:D argument, ~S, is ~A but should be ~A"),Value::symbol("float-vector"),Value::Int((i+1) as i64),v.clone(),Value::string(match v{Value::Symbol(_)=>"a symbol",_=>"an object"}),Value::string("a real")]))}} Ok(Value::FloatVector(Rc::new(RefCell::new(out))))}, 'i'=>{ if self.peek().map(|c|c.is_ascii_digit()).unwrap_or(false) && self.chars.get(self.pos+1)==Some(&'d'){let d=self.next().unwrap(); return self.read_multivector_kind(d,Some("i"));} if self.peek()==Some('('){ self.pos+=1; let mut out=Vec::new(); for (i,x) in self.read_list()?.to_vec()?.into_iter().enumerate(){match x{Value::Int(n)=>out.push(n),ref v=>return Err(SchemeError::new("wrong-type-arg",vec![Value::string("~A ~:D argument, ~S, is ~A but should be ~A"),Value::symbol("int-vector"),Value::Int((i+1) as i64),v.clone(),Value::string(match v{Value::Float(_)=>"a real",Value::Rational(_,_)=>"a ratio",Value::Symbol(_)=>"a symbol",_=>"an object"}),Value::string("an integer")]))}} Ok(Value::IntVector(Rc::new(RefCell::new(out)))) } else { self.read_exact_prefix(false) }}, 'b'=>self.read_radix(2), 'o'=>self.read_radix(8), 'd'=>self.read_decimal_prefixed(), 'x'=>self.read_radix(16), 'e'=>self.read_exact_prefix(true), c=>Err(SchemeError::new("read-error",vec![Value::Char(c)])) } }
    fn read_bar_symbol(&mut self)->Result<Value>{
        let mut raw=String::from("|");
        while let Some(c)=self.next(){
            raw.push(c);
            if c=='|' { break; }
            if c=='\\' { if let Some(n)=self.next(){ raw.push(n); } }
        }
        let inner=&raw[1..raw.len().saturating_sub(1)];
        if inner.is_empty(){ return Ok(Value::symbol("||")); }
        if inner.contains("\\|") { return Ok(Value::list(vec![Value::symbol("symbol"), Value::string(raw)])); }
        if let Some(first)=inner.split_whitespace().next(){ return Ok(Value::symbol(&format!("|{}", first))); }
        Ok(Value::symbol(&raw))
    }
    fn read_char(&mut self)->Result<Value>{ if self.peek().map(|c|c.is_whitespace()).unwrap_or(false){return Ok(Value::Char(' '));} let tok=self.read_token(); let c=match tok.as_str(){"space"=>' ',"newline"=>'\n',"tab"=>'\t',"null"=>'\0',s if s.chars().count()==1=>s.chars().next().unwrap(), _=>return Ok(Value::NamedChar(Rc::new(tok)))}; Ok(Value::Char(c)) }
    fn read_radix(&mut self, radix:u32)->Result<Value>{ let tok=self.read_token(); i64::from_str_radix(&tok,radix).map(Value::Int).map_err(|_|SchemeError::new("read-error",vec![Value::string(tok)])) }
    fn read_decimal_prefixed(&mut self)->Result<Value>{ let tok=self.read_token(); let n=tok.parse::<i64>().map_err(|_|SchemeError::new("read-error",vec![Value::string(tok.clone())]))?; Ok(Value::NumberLiteral(Rc::new(format!("#d{}",tok)), n as f64)) }
    fn read_atom(&mut self, first:char)->Result<Value>{
        let mut tok=String::new(); tok.push(first); tok.push_str(&self.read_token());
        parse_atom_token(&tok)
    }
    fn read_exact_prefix(&mut self, exact: bool)->Result<Value>{
        let raw = if self.peek()==Some('#') {
            self.pos += 1;
            let c=self.next().ok_or_else(||SchemeError::new("read-error",vec![]))?;
            let tok=self.read_token();
            format!("#{}{}", c, tok)
        } else { self.read_token() };
        let mut r=Reader{chars:raw.chars().collect(),pos:0};
        let v=if raw.starts_with('#'){ r.read_expr()? } else { parse_atom_token(&raw)? };
        let prefix=if exact{"#e"}else{"#i"};
        Ok(Value::NumberLiteral(Rc::new(format!("{}{}",prefix,raw)), to_f64(&v).unwrap_or(0.0)))
    }
    fn read_multivector(&mut self, first:char)->Result<Value>{ self.read_multivector_kind(first,None) }
    fn read_multivector_kind(&mut self, first:char, kind:Option<&str>)->Result<Value>{
        let rank=first.to_digit(10).unwrap() as usize;
        if rank==0{return Err(SchemeError::new("out-of-range",vec![Value::string("#nD(...) dimensions, ~A, should be 1 or more"),Value::Int(0)]));}
        if self.next()!=Some('d'){return Err(SchemeError::new("read-error",vec![Value::Char(first)]));}
        let v=self.read_expr()?;
        fn err(msg:&str, v:&Value)->SchemeError{SchemeError::new("read-error",vec![Value::string("reading constant vector, ~A: ~A"),Value::string(msg),v.clone()])}
        fn walk(v:&Value, root:&Value, rank:usize, depth:usize, dims:&mut Vec<usize>, data:&mut Vec<Value>)->Result<()> {
            if depth<rank {
                if !matches!(v,Value::Pair(_)|Value::Nil){return Err(err("we need a list that fully specifies the vector's elements",root));}
                let xs=v.to_vec().map_err(|_|err("found too many elements",root))?;
                if dims.len()<=depth{dims.push(xs.len());} else if dims[depth]!=xs.len(){return Err(err(if xs.len()<dims[depth]{"not enough elements found"}else{"found too many elements"},root));}
                if depth+1==rank { if xs.is_empty(){return Err(err("we need a list that fully specifies the vector's elements",root));} for x in xs { if rank>1 && matches!(x,Value::Pair(_)|Value::Nil){return Err(err("found too many elements",root));} data.push(x); } }
                else { for x in xs { walk(&x,root,rank,depth+1,dims,data)?; } }
                Ok(())
            } else { data.push(v.clone()); Ok(()) }
        }
        let mut dims=Vec::new(); let mut data=Vec::new(); if matches!(v,Value::Nil){dims=vec![0;rank];}else{walk(&v,&v,rank,0,&mut dims,&mut data)?;}
        match kind{
            Some("r")=>{let vals=data.into_iter().enumerate().map(|(i,x)|match x{Value::Int(n)=>Ok(Value::Float(n as f64)),Value::Float(_)=>Ok(x),Value::Rational(n,d)=>Ok(Value::Float(n as f64/d as f64)),ref v=>Err(SchemeError::new("wrong-type-arg",vec![Value::string("~A ~:D argument, ~S, is ~A but should be ~A"),if rank==1{Value::symbol("float-vector")}else{Value::string("#r(...)")},Value::Int((i+1) as i64),v.clone(),Value::string(if matches!(v,Value::Symbol(_)){"a symbol"}else if matches!(v,Value::Nil){"nil"}else{"an object"}),Value::string("a real")]))}).collect::<Result<Vec<_>>>()?; if rank==1{return Ok(Value::FloatVector(Rc::new(RefCell::new(vals.into_iter().map(|v|as_f64(&v).unwrap()).collect()))));} Ok(Value::MultiVector{dims,data:Rc::new(RefCell::new(vals)),kind:Some(Rc::new("r".to_string()))})}
            Some("i")=>{let mut out=Vec::new(); for x in data{match x{Value::Int(n)=>out.push(n),ref v=>return Err(SchemeError::new("wrong-type-arg",vec![Value::string("~A ~:D argument, ~S, is ~A but should be ~A"),if rank==1{Value::symbol("int-vector")}else{Value::string("#i(...)")},Value::Int(if rank==1{1}else{(out.len()+1) as i64}),v.clone(),Value::string(if matches!(v,Value::Float(_)){"a real"}else if matches!(v,Value::Rational(_,_)){"a ratio"}else if matches!(v,Value::Nil){"nil"}else{"an object"}),Value::string("an integer")]))}} if rank==1{return Ok(Value::IntVector(Rc::new(RefCell::new(out))));} Ok(Value::MultiVector{dims,data:Rc::new(RefCell::new(out.into_iter().map(Value::Int).collect())),kind:Some(Rc::new("i".to_string()))})}
            Some("u")=>{let mut out=Vec::new(); for (i,x) in data.into_iter().enumerate(){match x{Value::Int(n) if (0..=255).contains(&n)=>out.push(n),Value::Int(n)=>return Err(SchemeError::new("wrong-type-arg",vec![Value::string("~A ~:D argument, ~S, is ~A but should be ~A"),if rank==1{Value::symbol("byte-vector")}else{Value::string("#u(...)")},Value::Int((i+1) as i64),Value::Int(n),Value::string("an integer"),Value::string(if rank==1{"an unsigned byte"}else if matches!(v,Value::Float(_)|Value::Rational(_,_)){"a byte"}else{"a byte"})])),ref v=>return Err(SchemeError::new("wrong-type-arg",vec![Value::string("~A ~:D argument, ~S, is ~A but should be ~A"),if rank==1{Value::symbol("byte-vector")}else{Value::string("#u(...)")},Value::Int((i+1) as i64),v.clone(),Value::string(if matches!(v,Value::Float(_)){"a real"}else if matches!(v,Value::Rational(_,_)){"a ratio"}else if matches!(v,Value::Nil){"nil"}else{"an object"}),Value::string(if rank==1{"an integer"}else{"a byte"})]))}} if rank==1{return Ok(Value::ByteVector(Rc::new(RefCell::new(out.into_iter().map(|n|n as u8).collect()))));} Ok(Value::MultiVector{dims,data:Rc::new(RefCell::new(out.into_iter().map(Value::Int).collect())),kind:Some(Rc::new("u".to_string()))})}
            _=>{if rank==1{return Ok(Value::Vector(Rc::new(RefCell::new(data))));} Ok(Value::MultiVector{dims,data:Rc::new(RefCell::new(data)),kind:None})}
        }
    }
    fn read_token(&mut self)->String{ let mut s=String::new(); while let Some(c)=self.peek(){ if c.is_whitespace()||c=='('||c==')'||c=='"'||c==';'{break;} s.push(c); self.pos+=1;} s }
}



fn parse_atom_token(tok:&str)->Result<Value>{
    match tok { "+nan.0"|"nan.0"=>return Ok(Value::Float(f64::NAN)), "+inf.0"|"inf.0"=>return Ok(Value::Float(f64::INFINITY)), "-inf.0"=>return Ok(Value::Float(f64::NEG_INFINITY)), _=>{} }
    if tok.starts_with(':') && tok.ends_with(':') && tok.len()>2 { return Ok(Value::keyword(tok)); }
    if tok.starts_with(':') && tok.len()>1 { return Ok(Value::keyword(&tok[1..])); }
    if tok.ends_with(':') && tok.len()>1 { return Ok(Value::keyword(tok)); }
    if let Some(c)=parse_complex_token(tok) { return Ok(c); }
    if let Some(r)=parse_rational_token(tok) { return Ok(r); }
    if let Ok(i)=tok.parse::<i64>(){return Ok(Value::Int(i));}
    if let Ok(f)=tok.parse::<f64>(){return Ok(Value::Float(f));}
    Ok(Value::symbol(tok))
}
fn parse_rational_token(tok:&str)->Option<Value>{
    let ps=tok.split('/').collect::<Vec<_>>();
    if ps.len()==2 { if let (Ok(a),Ok(b))=(ps[0].parse::<i64>(),ps[1].parse::<i64>()) { return Some(normalize_rat(a,b)); } }
    None
}
fn parse_complex_token(tok:&str)->Option<Value>{
    if !tok.ends_with('i') { return None; }
    let body=&tok[..tok.len()-1];
    if body.is_empty() { return None; }
    if body=="+" { return Some(Value::Complex(0.0,1.0)); }
    if body=="-" { return Some(Value::Complex(0.0,-1.0)); }
    let split = body.char_indices().skip(1).filter(|(_,c)|*c=='+'||*c=='-').last().map(|(i,_)|i);
    if let Some(i)=split { let (r, im)=body.split_at(i); let imv = if im=="+" { Some(1.0) } else if im=="-" { Some(-1.0) } else { im.parse::<f64>().ok() }; if let (Ok(re), Some(imv))=(r.parse::<f64>(), imv) { return Some(Value::Complex(re, imv)); } }
    if let Ok(im)=body.parse::<f64>() { return Some(Value::Complex(0.0,im)); }
    None
}