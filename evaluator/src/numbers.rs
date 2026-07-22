use super::*;

fn promoted_float(x: f64) -> Value {
    if !x.is_finite() { return Value::Float(x); }
    let mut text = x.to_string();
    if x.fract() == 0.0 && !text.contains(['.', 'e', 'E']) { text.push_str(".0"); }
    Value::number_literal(Rc::new(text), x)
}
fn promoted_integer(n: i128) -> Value { promoted_float(n as f64) }
fn promoted_ratio(n: i128, d: i128) -> Value {
    let x = n as f64 / d as f64;
    if x.abs() < 1.0e16 { Value::Float(x) } else { promoted_float(x) }
}
fn normalize_wide(n: i128, d: i128) -> Value {
    if let (Ok(nn), Ok(dd)) = (i64::try_from(n), i64::try_from(d)) {
        if nn != i64::MIN && dd != i64::MIN { return normalize_rat(nn, dd); }
    }
    promoted_ratio(n, d)
}
pub(crate) fn most_negative_negation_error() -> SchemeError {
    SchemeError::new("out-of-range", vec![Value::string("~A argument, ~S, is out of range (~A)"), Value::symbol("-"), Value::Int(i64::MIN), Value::string("most-negative-fixnum can't be negated")])
}

pub(crate) fn normalize_rat(n: i64, d: i64) -> Value {
    if d == 0 { return Value::Float(f64::NAN); }
    let (mut n, mut d) = (n, d);
    if d < 0 { n = -n; d = -d; }
    let g = gcd_i(n, d).max(1);
    let (n, d) = (n / g, d / g);
    if d == 1 { Value::Int(n) } else { Value::Rational(n, d) }
}
pub(crate) fn rat_parts(v: &Value) -> Option<(i64, i64)> {
    match v { Value::Int(n) => Some((*n, 1)), Value::RationalValue(r) => Some((r.num, r.den)), _ => None }
}
pub(crate) fn to_f64(v: &Value) -> Result<f64> {
    match v {
        Value::Int(n) => Ok(*n as f64),
        Value::RationalValue(r) => Ok(r.num as f64 / r.den as f64),
        Value::Float(x) => Ok(*x),
        Value::NumberLiteral(f, _) => Ok(f.value),
        _ => Err(SchemeError::new("wrong-type-arg", vec![v.clone()])),
    }
}
pub(crate) fn to_complex(v: &Value) -> Result<(f64, f64)> {
    match v { Value::ComplexValue(c) => Ok((c.real, c.imag)), _ => Ok((to_f64(v)?, 0.0)) }
}

pub(crate) fn add2(a: Value, b: Value) -> Result<Value> {
    match (a, b) {
        (Value::NumberLiteral(x, _), b) => Ok(promoted_float(x.value + to_f64(&b)?)),
        (a, Value::NumberLiteral(y, _)) => Ok(promoted_float(to_f64(&a)? + y.value)),
        (Value::Int(x), Value::Int(y)) => Ok(x.checked_add(y).map(Value::Int).unwrap_or_else(|| promoted_integer(x as i128 + y as i128))),
        (Value::ComplexValue(a), b) => { let (br, bi) = to_complex(&b)?; Ok(Value::Complex(a.real + br, a.imag + bi)) }
        (a, Value::ComplexValue(b)) => { let (ar, ai) = to_complex(&a)?; Ok(Value::Complex(ar + b.real, ai + b.imag)) }
        (Value::Float(x), b) => Ok(Value::Float(x + to_f64(&b)?)),
        (a, Value::Float(y)) => Ok(Value::Float(to_f64(&a)? + y)),
        (a, b) => {
            let (an, ad) = rat_parts(&a).ok_or_else(|| SchemeError::new("wrong-type-arg", vec![Value::string("~A ~:D argument, ~S, is ~A but should be ~A"), Value::symbol("+"), Value::Int(if matches!(a, Value::Undefined) { 1 } else if matches!(b, Value::Int(0)) { 1 } else { 2 }), a.clone(), Value::string(if matches!(a, Value::Undefined) { "an undefined object" } else if matches!(a, Value::Bool(_)) { "boolean" } else { "a symbol" }), Value::string("a number")]))?;
            let (bn, bd) = rat_parts(&b).ok_or_else(|| SchemeError::new("wrong-type-arg", vec![Value::string("~A ~:D argument, ~S, is ~A but should be ~A"), Value::symbol("+"), Value::Int(if matches!(b, Value::Undefined) { 1 } else { 2 }), b.clone(), Value::string(if matches!(b, Value::Undefined) { "an undefined object" } else if matches!(b, Value::Bool(_)) { "boolean" } else { "a symbol" }), Value::string("a number")]))?;
            Ok(normalize_wide(an as i128 * bd as i128 + bn as i128 * ad as i128, ad as i128 * bd as i128))
        }
    }
}
pub(crate) fn sub2(a: Value, b: Value) -> Result<Value> {
    match (a, b) {
        (Value::NumberLiteral(x, _), b) => Ok(promoted_float(x.value - to_f64(&b)?)),
        (a, Value::NumberLiteral(y, _)) => Ok(promoted_float(to_f64(&a)? - y.value)),
        (Value::Int(x), Value::Int(y)) => Ok(x.checked_sub(y).map(Value::Int).unwrap_or_else(|| promoted_integer(x as i128 - y as i128))),
        (Value::ComplexValue(a), b) => { let (br, bi) = to_complex(&b)?; Ok(Value::Complex(a.real - br, a.imag - bi)) }
        (a, Value::ComplexValue(b)) => { let (ar, ai) = to_complex(&a)?; Ok(Value::Complex(ar - b.real, ai - b.imag)) }
        (Value::Float(x), b) => Ok(Value::Float(x - to_f64(&b)?)),
        (a, Value::Float(y)) => Ok(Value::Float(to_f64(&a)? - y)),
        (a, b) => { let (an, ad) = rat_parts(&a).ok_or_else(|| SchemeError::new("wrong-type-arg", vec![a.clone()]))?; let (bn, bd) = rat_parts(&b).ok_or_else(|| SchemeError::new("wrong-type-arg", vec![b.clone()]))?; Ok(normalize_wide(an as i128 * bd as i128 - bn as i128 * ad as i128, ad as i128 * bd as i128)) }
    }
}
pub(crate) fn mul2(a: Value, b: Value) -> Result<Value> {
    match (a, b) {
        (Value::NumberLiteral(x, _), b) => Ok(promoted_float(x.value * to_f64(&b)?)),
        (a, Value::NumberLiteral(y, _)) => Ok(promoted_float(to_f64(&a)? * y.value)),
        (Value::Int(x), Value::Int(y)) => Ok(x.checked_mul(y).map(Value::Int).unwrap_or_else(|| promoted_integer(x as i128 * y as i128))),
        (Value::ComplexValue(a), b) => { let (br, bi) = to_complex(&b)?; Ok(Value::Complex(a.real * br - a.imag * bi, a.real * bi + a.imag * br)) }
        (a, Value::ComplexValue(b)) => { let (ar, ai) = to_complex(&a)?; Ok(Value::Complex(ar * b.real - ai * b.imag, ar * b.imag + ai * b.real)) }
        (Value::Float(x), b) => Ok(Value::Float(x * to_f64(&b)?)),
        (a, Value::Float(y)) => Ok(Value::Float(to_f64(&a)? * y)),
        (a, b) => { let (an, ad) = rat_parts(&a).ok_or_else(|| SchemeError::new("wrong-type-arg", vec![a.clone()]))?; let (bn, bd) = rat_parts(&b).ok_or_else(|| SchemeError::new("wrong-type-arg", vec![b.clone()]))?; Ok(normalize_wide(an as i128 * bn as i128, ad as i128 * bd as i128)) }
    }
}
pub(crate) fn div2(a: Value, b: Value) -> Result<Value> {
    match (a, b) {
        (Value::NumberLiteral(x, _), b) => { let y = to_f64(&b)?; if y == 0.0 { return Err(SchemeError::new("division-by-zero", vec![])); } Ok(promoted_float(x.value / y)) }
        (a, Value::NumberLiteral(y, _)) => { if y.value == 0.0 { return Err(SchemeError::new("division-by-zero", vec![])); } Ok(promoted_float(to_f64(&a)? / y.value)) }
        (a, Value::ComplexValue(b)) => { let (ar, ai) = to_complex(&a)?; let den = b.real * b.real + b.imag * b.imag; if den == 0.0 { return Err(SchemeError::new("division-by-zero", vec![])); } Ok(Value::Complex((ar * b.real + ai * b.imag) / den, (ai * b.real - ar * b.imag) / den)) }
        (Value::ComplexValue(a), b) => { let (br, bi) = to_complex(&b)?; let den = br * br + bi * bi; if den == 0.0 { return Err(SchemeError::new("division-by-zero", vec![])); } Ok(Value::Complex((a.real * br + a.imag * bi) / den, (a.imag * br - a.real * bi) / den)) }
        (Value::Float(x), b) => { let y = to_f64(&b)?; if y == 0.0 { return Err(SchemeError::new("division-by-zero", vec![])); } Ok(Value::Float(x / y)) }
        (a, Value::Float(y)) => { if y == 0.0 { return Err(SchemeError::new("division-by-zero", vec![])); } Ok(Value::Float(to_f64(&a)? / y)) }
        (a, b) => {
            let (an, ad) = rat_parts(&a).ok_or_else(|| SchemeError::new("wrong-type-arg", vec![a.clone()]))?;
            let (bn, bd) = rat_parts(&b).ok_or_else(|| SchemeError::new("wrong-type-arg", vec![b.clone()]))?;
            if bn == 0 { return Err(SchemeError::new("division-by-zero", vec![])); }
            if an == bn && ad == bd { return Ok(Value::Int(1)); }
            let (mut an, mut ad, mut bn, mut bd) = (an as i128, ad as i128, bn as i128, bd as i128);
            let gcd = |mut x: i128, mut y: i128| { x = x.abs(); y = y.abs(); while y != 0 { let r = x % y; x = y; y = r; } x.max(1) };
            let g1 = gcd(an, bn); an /= g1; bn /= g1;
            let g2 = gcd(bd, ad); bd /= g2; ad /= g2;
            Ok(normalize_wide(an * bd, ad * bn))
        }
    }
}
pub(crate) fn fold_num(args: &[Value], init: Value, op: fn(Value, Value) -> Result<Value>) -> Result<Value> { let mut acc = init; for a in args { acc = op(acc, a.clone())?; } Ok(acc) }
pub(crate) fn as_i64(v: &Value) -> Result<i64> { match v { Value::Int(n) => Ok(*n), Value::RationalValue(r) if r.num % r.den == 0 => Ok(r.num / r.den), Value::Float(f) => Ok(*f as i64), Value::NumberLiteral(f, _) => Ok(f.value as i64), _ => Err(SchemeError::new("wrong-type-arg", vec![v.clone()])) } }
pub(crate) fn as_f64(v: &Value) -> Result<f64> { to_f64(v) }
pub(crate) fn as_usize(v: Option<&Value>) -> Result<usize> { Ok(as_i64(v.ok_or_else(|| SchemeError::new("wrong-number-of-args", vec![]))?)? as usize) }
pub(crate) fn cmp(args: &[Value], f: fn(f64, f64) -> bool) -> Result<Value> { for w in args.windows(2) { if !f(as_f64(&w[0])?, as_f64(&w[1])?) { return Ok(Value::Bool(false)); } } Ok(Value::Bool(true)) }
