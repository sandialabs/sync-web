;; Imported from upstream s7test.scm line 25142.
;; Original form:
;; (test (object->string (inlet 'a (hash-table 'b "hi")) :readable) "(inlet :a (hash-table 'b \"hi\"))")

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (object->string (inlet 'a (hash-table 'b "hi")) :readable))))
       (expected (upstream-safe (lambda () "(inlet :a (hash-table 'b \"hi\"))")))
       (ok? (equal? actual expected)))
  (list 'upstream-test 25142 actual expected ok?))
