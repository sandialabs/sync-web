;; Imported from upstream s7test.scm line 25130.
;; Original form:
;; (test (object->string (inlet 'a (symbol "( a b c )")) :readable) "(inlet :a (symbol \"( a b c )\"))")

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (object->string (inlet 'a (symbol "( a b c )")) :readable))))
       (expected (upstream-safe (lambda () "(inlet :a (symbol \"( a b c )\"))")))
       (ok? (equal? actual expected)))
  (list 'upstream-test 25130 actual expected ok?))
