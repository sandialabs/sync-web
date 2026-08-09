use super::*;

pub fn obj2str(sc: *mut s7_scheme, obj: *mut s7_cell) -> String {
    unsafe {
        let expr = s7_string(s7_object_to_string(sc, obj, false));
        let cstr = CStr::from_ptr(expr);
        let result = match cstr.to_str() {
            Ok(rust_str) => match s7_is_string(obj) {
                true => format!("\"{}\"", rust_str),
                false => format!("{}", rust_str.to_owned()),
            },
            Err(_) => format!("(error 'encoding-error \"Failed to encode string\")"),
        };
        result
    }
}

pub fn lisp2json(expression: &str) -> Result<Value, String> {
    // <TYPE>: <JSON>
    // ------------------------------------------------------
    // symbol: "string"
    // number: 5.5
    // boolean: true/false
    // list: []
    // symbol assoc lists: { }
    // special types
    // - @pair: {"*type/pair*": ["first", "second"]}
    // - @string: { "*type/string*": "this is my string" }
    // - @rational: { "*type/rational*": "5/5" }
    // - @complex: { "*type/complex*": "5/5" }
    // - @vector: {"*type/vector*: ["blah", "blah"]}
    // - @byte-vector: {"*type/byte-vector*: "deadbeef0000" }
    // - @float-vector: {"*type/float-vector*": [3.2, 8.6, 0.1]}
    // - @hash-table: {"*type/hash-table*": [["a", 6], [53, 199]]}
    // - @quoted: {"*type/quoted*": [["a", 6], [53, 199]]}

    let mut owned_expr = None;
    let expr = {
        let trimmed = expression.trim_start();
        if let Some(rest) = trimmed.strip_prefix('\'') {
            let rest = rest.trim_start();
            if rest.is_empty() {
                return Err("Empty quoted expression".to_string());
            }
            let mut wrapped = String::from("(quote ");
            wrapped.push_str(rest);
            wrapped.push(')');
            owned_expr = Some(wrapped);
            owned_expr.as_deref().expect("quote wrapper missing")
        } else {
            expression
        }
    };

    unsafe {
        let sc: *mut s7_scheme = s7_init();
        suppress_error_output(sc);

        // Parse the expression without evaluating it
        let c_expr = CString::new(expr).unwrap_or_else(|_| CString::new("()").unwrap());
        let input_port = s7_open_input_string(sc, c_expr.as_ptr());
        let s7_obj = s7_read(sc, input_port);
        s7_close_input_port(sc, input_port);

        let result = s7_obj_to_json(sc, s7_obj);
        s7_free(sc);
        result
    }
}

pub fn json2lisp(expression: &Value) -> Result<String, String> {
    unsafe {
        let sc: *mut s7_scheme = s7_init();
        suppress_error_output(sc);
        match json_to_s7_obj(sc, &expression) {
            Ok(s7_obj) => {
                let result = obj2str(sc, s7_obj);
                s7_free(sc);
                Ok(result)
            }
            Err(err) => {
                s7_free(sc);
                Err(err)
            }
        }
    }
}

unsafe fn s7_obj_to_json(sc: *mut s7_scheme, obj: s7_pointer) -> Result<Value, String> {
    unsafe {
        if s7_is_null(sc, obj) {
            Ok(Value::Null)
        } else if s7_is_boolean(obj) {
            Ok(Value::Bool(s7_boolean(sc, obj)))
        } else if s7_is_integer(obj) {
            Ok(Value::Number(serde_json::Number::from(s7_integer(obj))))
        } else if s7_is_real(obj) {
            if let Some(num) = serde_json::Number::from_f64(s7_real(obj)) {
                Ok(Value::Number(num))
            } else {
                Err("Invalid floating point number - cannot convert to JSON".to_string())
            }
        } else if s7_is_string(obj) {
            // let c_str = s7_string(obj);
            // let rust_str = CStr::from_ptr(c_str).to_string_lossy();
            let rust_str = obj2str(sc, obj);

            // Check if it's a special type marker
            let mut special_type = Map::new();
            special_type.insert(
                "*type/string*".to_string(),
                Value::String(String::from(&rust_str[1..(rust_str.len() - 1)])),
            );
            Ok(Value::Object(special_type))
        } else if s7_is_symbol(obj) {
            let c_str = s7_symbol_name(obj);
            let rust_str = CStr::from_ptr(c_str).to_string_lossy();
            Ok(Value::String(rust_str.to_string()))
        } else if s7_is_pair(obj) {
            // Check if it's a quote form first
            let car = s7_car(obj);
            if s7_is_syntax(car) {
                let car_str = obj2str(sc, car);
                if car_str == "#_quote" {
                    let cdr = s7_cdr(obj);
                    if s7_is_pair(cdr) && s7_is_null(sc, s7_cdr(cdr)) {
                        let quoted_expr = s7_car(cdr);
                        let mut special_type = Map::new();
                        special_type.insert(
                            "*type/quoted*".to_string(),
                            s7_obj_to_json(sc, quoted_expr)?,
                        );
                        return Ok(Value::Object(special_type));
                    }
                }
            }
            if s7_is_symbol(car) {
                let symbol_name_ptr = s7_symbol_name(car);
                if !symbol_name_ptr.is_null() {
                    let symbol_name = CStr::from_ptr(symbol_name_ptr).to_string_lossy();

                    if symbol_name == "quote" {
                        // Handle quote form: convert (quote expr) to {"*type/quoted*": <expr>}
                        let cdr = s7_cdr(obj);
                        if s7_is_pair(cdr) && s7_is_null(sc, s7_cdr(cdr)) {
                            let quoted_expr = s7_car(cdr);
                            let mut special_type = Map::new();
                            special_type.insert(
                                "*type/quoted*".to_string(),
                                s7_obj_to_json(sc, quoted_expr)?,
                            );
                            return Ok(Value::Object(special_type));
                        }
                    }
                }
            }

            // Check if it's an association list with proper list format (for JSON objects)
            if is_proper_assoc_list(sc, obj) {
                let mut map = Map::new();
                let mut current = obj;

                while !s7_is_null(sc, current) {
                    let pair = s7_car(current);
                    if s7_is_pair(pair) {
                        let key_obj = s7_car(pair);
                        let cdr_pair = s7_cdr(pair);

                        // Only handle list format (key value)
                        if s7_is_pair(cdr_pair) && s7_is_null(sc, s7_cdr(cdr_pair)) {
                            let value_obj = s7_car(cdr_pair);

                            if s7_is_symbol(key_obj) {
                                let key_c_str = s7_symbol_name(key_obj);
                                let key = CStr::from_ptr(key_c_str).to_string_lossy().to_string();
                                let value = s7_obj_to_json(sc, value_obj)?;
                                map.insert(key, value);
                            }
                        }
                    }
                    current = s7_cdr(current);
                }
                Ok(Value::Object(map))
            } else if s7_is_pair(obj) && !s7_is_pair(s7_cdr(obj)) && !s7_is_null(sc, s7_cdr(obj)) {
                // Handle pairs as special type
                let mut special_type = Map::new();
                let mut pair_array = Vec::new();
                pair_array.push(s7_obj_to_json(sc, s7_car(obj))?);
                pair_array.push(s7_obj_to_json(sc, s7_cdr(obj))?);
                special_type.insert("*type/pair*".to_string(), Value::Array(pair_array));
                Ok(Value::Object(special_type))
            } else {
                // Regular list - convert to JSON array
                let mut array = Vec::new();
                let mut current = obj;

                while !s7_is_null(sc, current) {
                    array.push(s7_obj_to_json(sc, s7_car(current))?);
                    current = s7_cdr(current);
                }
                Ok(Value::Array(array))
            }
        } else if s7_is_byte_vector(obj) {
            let mut hex_string = String::new();
            let len = s7_vector_length(obj);

            for i in 0..len {
                let byte = s7_byte_vector_ref(obj, i);
                write!(&mut hex_string, "{:02x}", byte).unwrap();
            }

            let mut special_type = Map::new();
            special_type.insert("*type/byte-vector*".to_string(), Value::String(hex_string));
            Ok(Value::Object(special_type))
        } else if s7_is_vector(obj) {
            let mut special_type = Map::new();
            let mut array = Vec::new();
            let len = s7_vector_length(obj);

            for i in 0..len {
                array.push(s7_obj_to_json(sc, s7_vector_ref(sc, obj, i))?);
            }

            special_type.insert("*type/vector*".to_string(), Value::Array(array));
            Ok(Value::Object(special_type))
        } else if s7_is_rational(obj) {
            // Handle rational numbers as special type
            let rational_str = obj2str(sc, obj);
            let mut special_type = Map::new();
            special_type.insert("*type/rational*".to_string(), Value::String(rational_str));
            Ok(Value::Object(special_type))
        } else if s7_is_complex(obj) {
            // Handle complex numbers as special type
            let complex_str = obj2str(sc, obj);
            let mut special_type = Map::new();
            special_type.insert("*type/complex*".to_string(), Value::String(complex_str));
            Ok(Value::Object(special_type))
        } else if s7_is_hash_table(obj) {
            // Handle hash tables as special type
            let mut special_type = Map::new();
            let mut pairs = Vec::new();

            // Convert hash table to array of [key, value] pairs
            // This is a simplified approach - we'd need to iterate through the hash table
            special_type.insert("*type/hash-table*".to_string(), Value::Array(pairs));
            Ok(Value::Object(special_type))
        } else if s7_is_syntax(obj) {
            // Fallback for syntax objects (e.g., nested quote shorthand).
            let expr = obj2str(sc, obj);
            let trimmed = expr.trim_start();
            let quoted_inner = if let Some(rest) = trimmed.strip_prefix('\'') {
                let rest = rest.trim_start();
                if rest.is_empty() {
                    return Err("Empty quoted expression".to_string());
                }
                rest
            } else if let Some(rest) = trimmed.strip_prefix("(quote ") {
                let rest = rest.trim_end();
                if let Some(rest) = rest.strip_suffix(')') {
                    rest.trim()
                } else {
                    return Err("Malformed quote syntax".to_string());
                }
            } else {
                return Err(format!("Unsupported syntax object: {}", expr));
            };

            let quoted_json = lisp2json(quoted_inner)?;
            let mut special_type = Map::new();
            special_type.insert("*type/quoted*".to_string(), quoted_json);
            Ok(Value::Object(special_type))
        } else {
            // For debugging: let's see what type this actually is
            let type_info = if s7_is_procedure(obj) {
                "procedure"
            } else if s7_is_macro(sc, obj) {
                "macro"
            } else {
                "unknown"
            };

            Err(format!(
                "Unknown Scheme type '{}' - cannot convert to JSON",
                type_info
            ))
        }
    }
}

unsafe fn json_to_s7_obj(sc: *mut s7_scheme, json: &Value) -> Result<s7_pointer, String> {
    unsafe {
        match json {
            Value::Null => Ok(s7_nil(sc)),
            Value::Bool(b) => Ok(s7_make_boolean(sc, *b)),
            Value::Number(n) => {
                if let Some(i) = n.as_i64() {
                    Ok(s7_make_integer(sc, i))
                } else if let Some(f) = n.as_f64() {
                    Ok(s7_make_real(sc, f))
                } else {
                    Err("Invalid number format in JSON".to_string())
                }
            }
            Value::String(s) => match CString::new(s.as_str()) {
                Ok(c_str) => Ok(s7_make_symbol(sc, c_str.as_ptr())),
                Err(_) => Err("Invalid string format in JSON - contains null bytes".to_string()),
            },
            Value::Array(arr) => {
                if arr.is_empty() {
                    Ok(s7_nil(sc))
                } else {
                    // Create (list ...) expression
                    let mut result = s7_nil(sc);

                    // Build arguments in reverse order
                    for item in arr.iter().rev() {
                        let s7_item = json_to_s7_obj(sc, item)?;
                        result = s7_cons(sc, s7_item, result);
                    }
                    Ok(result)
                }
            }
            Value::Object(obj) => {
                // Check for special type markers
                if obj.len() == 1 {
                    if let Some(Value::String(s)) = obj.get("*type/string*") {
                        match CString::new(s.as_str()) {
                            Ok(c_str) => return Ok(s7_make_string(sc, c_str.as_ptr())),
                            Err(_) => {
                                return Err(
                                    "Invalid string in *type/string* - contains null bytes"
                                        .to_string(),
                                );
                            }
                        }
                    }
                    if let Some(Value::Array(arr)) = obj.get("*type/vector*") {
                        let len = arr.len() as i64;
                        let vector = s7_make_vector(sc, len);
                        for (i, item) in arr.iter().enumerate() {
                            let s7_item = json_to_s7_obj(sc, item)?;
                            s7_vector_set(sc, vector, i as i64, s7_item);
                        }
                        return Ok(vector);
                    }
                    if let Some(value) = obj.get("*type/quoted*") {
                        let quoted = json_to_s7_obj(sc, value)?;
                        let quote_sym = s7_make_symbol(sc, c"quote".as_ptr());
                        let quoted_list = s7_cons(sc, quoted, s7_nil(sc));
                        return Ok(s7_cons(sc, quote_sym, quoted_list));
                    }
                    if let Some(Value::String(hex)) = obj.get("*type/byte-vector*") {
                        let bytes: Result<Vec<u8>, ParseIntError> = (0..hex.len())
                            .step_by(2)
                            .map(|i| {
                                if i + 2 <= hex.len() {
                                    u8::from_str_radix(&hex[i..i + 2], 16)
                                } else if i + 1 <= hex.len() {
                                    // Handle odd-length hex string
                                    u8::from_str_radix(&hex[i..i + 1], 16)
                                } else {
                                    Ok(0)
                                }
                            })
                            .collect();

                        match bytes {
                            Ok(bytes) => {
                                let len = bytes.len() as i64;
                                let bv = s7_make_byte_vector(sc, len, 1, std::ptr::null_mut());

                                for (i, &byte) in bytes.iter().enumerate() {
                                    s7_byte_vector_set(bv, i as i64, byte);
                                }

                                return Ok(bv);
                            }
                            Err(_) => {
                                return Err("Invalid hex string in *type/byte-vector*".to_string());
                            }
                        }
                    }
                    if let Some(Value::Array(arr)) = obj.get("*type/pair*") {
                        if arr.len() == 2 {
                            let car = json_to_s7_obj(sc, &arr[0])?;
                            let cdr = json_to_s7_obj(sc, &arr[1])?;
                            return Ok(s7_cons(sc, car, cdr));
                        } else {
                            return Err("*type/pair* must contain exactly 2 elements".to_string());
                        }
                    }
                }

                if obj.is_empty() {
                    Ok(s7_nil(sc))
                } else {
                    // Regular object - convert to association list
                    let mut result = s7_nil(sc);

                    for (key, value) in obj.iter().rev() {
                        let key_symbol = match CString::new(key.as_str()) {
                            Ok(c_key) => s7_make_symbol(sc, c_key.as_ptr()),
                            Err(_) => {
                                return Err(format!("Invalid key '{}' - contains null bytes", key));
                            }
                        };
                        let value_obj = json_to_s7_obj(sc, value)?;
                        // Create a list (key value) instead of a pair (key . value)
                        let value_list = s7_cons(sc, value_obj, s7_nil(sc));
                        let pair = s7_cons(sc, key_symbol, value_list);
                        result = s7_cons(sc, pair, result);
                    }
                    Ok(result)
                }
            }
        }
    }
}

unsafe fn is_proper_assoc_list(sc: *mut s7_scheme, obj: s7_pointer) -> bool {
    unsafe {
        if s7_is_null(sc, obj) {
            return true;
        }

        let mut current = obj;
        while !s7_is_null(sc, current) {
            if !s7_is_pair(current) {
                return false;
            }

            let car = s7_car(current);
            if !s7_is_pair(car) {
                return false;
            }

            let key = s7_car(car);
            if !s7_is_symbol(key) {
                return false;
            }

            let cdr_part = s7_cdr(car);
            // Only accept proper list format (key value)
            if !s7_is_pair(cdr_part) || !s7_is_null(sc, s7_cdr(cdr_part)) {
                return false;
            }

            current = s7_cdr(current);
        }
        true
    }
}
