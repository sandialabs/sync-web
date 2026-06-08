;; Imported from upstream s7test.scm line 25119.
;; Original form:
;; (test (object->string (inlet 'a 1/2) :readable) "(inlet :a 1/2)")

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (object->string (inlet 'a 1/2) :readable))))
       (expected (upstream-safe (lambda () "(inlet :a 1/2)")))
       (ok? (equal? actual expected)))
  (list 'upstream-test 25119 actual expected ok?))
