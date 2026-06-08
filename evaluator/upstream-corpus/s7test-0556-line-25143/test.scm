;; Imported from upstream s7test.scm line 25143.
;; Original form:
;; (test (object->string (inlet 'a (hash-table 'b "h\"i")) :readable) "(inlet :a (hash-table 'b \"h\\\"i\"))")

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (object->string (inlet 'a (hash-table 'b "h\"i")) :readable))))
       (expected (upstream-safe (lambda () "(inlet :a (hash-table 'b \"h\\\"i\"))")))
       (ok? (equal? actual expected)))
  (list 'upstream-test 25143 actual expected ok?))
