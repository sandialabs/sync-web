;; Imported from upstream s7test.scm line 25131.
;; Original form:
;; (test (object->string (inlet 'a else) :readable) "(inlet :a 'else)")

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (object->string (inlet 'a else) :readable))))
       (expected (upstream-safe (lambda () "(inlet :a 'else)")))
       (ok? (equal? actual expected)))
  (list 'upstream-test 25131 actual expected ok?))
