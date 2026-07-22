#[cfg(test)]
mod tests{
    use std::{mem::size_of,rc::Rc};
    use crate::{core::{PairRef,Value},run_source_output};

    #[test]
    fn value_stays_two_words(){assert_eq!(size_of::<Value>(),16);}

    #[test]
    fn pair_identity_survives_mutation(){
        let pair=Value::cons(Value::Int(1),Value::Int(2));
        let Value::Pair(before)=pair else{panic!("expected pair")};
        pair.set_car(Value::Int(3)).unwrap();
        let Value::Pair(after)=pair else{panic!("expected pair")};
        assert!(PairRef::ptr_eq(&before,&after));
        assert!(matches!(pair.car().unwrap(),Value::Int(3)));
    }

    #[test]
    fn cloned_cold_payload_preserves_identity(){
        let value=Value::Rational(3,7);
        let clone=value.clone();
        let (Value::RationalValue(a),Value::RationalValue(b))=(value,clone) else{panic!("expected ratio")};
        assert!(Rc::ptr_eq(&a,&b));
    }

    #[test]
    fn vector_sort_keeps_movable_slot_mutation(){
        let source="(let ((x (vector 4 3 2 1))) (sort! x (lambda (a b) (vector-set! x 0 9) (< a b))) x)";
        assert_eq!(run_source_output(source).unwrap(),"#(1 2 9 9)");
    }

    #[test]
    fn trivial_compiled_closures_preserve_slots_and_dynamic_updates(){let source="(let ((x 1)) (define (identity value) value) (define (get) x) (let ((before (get))) (set! x 2) (list (identity before) (get))))";assert_eq!(run_source_output(source).unwrap(),"(1 2)");}

    #[test]
    fn compiled_structural_list_filter_preserves_entry_identity(){let source="(begin (define (drop-key al k) (cond ((null? al) '()) ((equal? (caar al) k) (drop-key (cdr al) k)) (else (cons (car al) (drop-key (cdr al) k))))) (let* ((a (cons (list 'path 1) 10)) (b (cons (list 'path 2) 20)) (out (drop-key (list a b a) (list 'path 1)))) (list (length out) (eq? (car out) b))))";assert_eq!(run_source_output(source).unwrap(),"(1 #t)");let rebound="(begin (define count 0) (define (drop-key al k) (cond ((null? al) '()) ((equal? (caar al) k) (drop-key (cdr al) k)) (else (cons (car al) (drop-key (cdr al) k))))) (define (invoke xs k) (drop-key xs k)) (set! equal? (lambda (a b) (set! count (+ count 1)) #f)) (list (length (invoke (list (cons (list 'p 1) 1) (cons (list 'p 2) 2)) (list 'p 1))) count))";assert_eq!(run_source_output(rebound).unwrap(),"(2 2)");}

    #[test]
    fn word_byte_vector_reads_authoritative_legacy_storage(){let source="(let ((b (byte-vector 1 2 3))) (byte-vector-set! b 0 9) (let loop ((i 0) (sum 0)) (if (= i 3) sum (loop (+ i 1) (+ sum (byte-vector-ref b i))))))";assert_eq!(run_source_output(source).unwrap(),"14");}

    #[test]
    fn word_object_string_preserves_origin_cycles(){let source="(let ((p (cons 1 '()))) (set-cdr! p p) (let loop ((i 0)) (if (= i 1) (object->string `(,p)) (loop (+ i 1)))))";assert_eq!(run_source_output(source).unwrap(),"\"(#1=(1 . #1#))\"");}

    #[test]
    fn nested_named_loops_preserve_branch_slots_and_outer_fallback(){let source="(list (let loop ((i 0) (acc '())) (if (= i 5) (let sum ((xs acc) (n 0)) (if (null? xs) n (sum (cdr xs) (+ n (car xs))))) (let ((next (+ i 1))) (loop next (cons i acc))))) (let loop ((i 0) (x 3)) (if (= i 1) (let inner ((n 0)) (if (= n 1) x (inner (+ n 1)))) (loop (+ i 1) x))))";assert_eq!(run_source_output(source).unwrap(),"(10 3)");}

    #[test]
    fn word_quasiquote_preserves_unquoted_identity(){let source="(let ((p (cons 1 2))) (let ((out (let loop ((i 0)) (if (= i 1) `(,p . ,p) (loop (+ i 1)))))) (set-car! p 9) (list (eq? (car out) (cdr out)) (caar out))))";assert_eq!(run_source_output(source).unwrap(),"(#t 9)");}

    #[test]
    fn named_loop_hot_cache_does_not_capture_caller_scalars(){let source="(define (f x) (let loop ((i 0)) (if (= i 2) x (loop (+ i 1))))) (list (f 1) (f 2) (f 3))";assert_eq!(run_source_output(source).unwrap(),"(1 2 3)");}

    #[test]
    fn compiled_car_call_preserves_procedure_and_macro_behavior(){let source="(list (let ((fs (list (lambda (x) (+ x 1))))) ((car fs) 2)) (let ((fs (list (macro (x) `(+ ,x 2))))) ((car fs) 3)))";assert_eq!(run_source_output(source).unwrap(),"(3 5)");}

    #[test]
    fn compiled_rest_forwarder_preserves_procedures_and_macro_fallback(){let source="(let ((methods (hash-table))) (set! (methods 'add) +) (set! (methods 'm) (macro (x) `(+ ,x 1))) (let ((dispatch (lambda (msg . args) (apply (methods msg) args)))) (list (dispatch 'add 2 3) (dispatch 'm 2))))";assert_eq!(run_source_output(source).unwrap(),"(5 3)");}

    #[test]
    fn compiled_hash_getter_observes_later_mutation(){let source="(let ((state (hash-table 'value 1))) (define (get) (state 'value)) (let ((before (get))) (set! (state 'value) 2) (list before (get))))";assert_eq!(run_source_output(source).unwrap(),"(1 2)");}

    #[test]
    fn borrowed_dynamic_setter_falls_back_for_non_hash_targets(){let source="(let ((x (vector 0))) (define (set-x value) (set! (x 0) value)) (set-x 7) x)";assert_eq!(run_source_output(source).unwrap(),"#(7)");}

    #[test]
    fn compiled_setter_preserves_multiple_values_error(){
        let source="(let ((x (vector 0))) (catch #t (lambda () (set! (x 0) (values 1 2))) (lambda args 'caught)))";
        assert_eq!(run_source_output(source).unwrap(),"caught");
    }

    #[test]
    fn unsupported_compilation_does_not_replay_effects(){
        let source="(let ((count 0)) (define (f form) (set! count (+ count 1)) (eval form)) (catch #t (lambda () (f '(error 'boom \"x\"))) (lambda args count)))";
        assert_eq!(run_source_output(source).unwrap(),"1");
    }
}
