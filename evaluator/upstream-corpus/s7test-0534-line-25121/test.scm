;; Imported from upstream s7test.scm line 25121.
;; Original form:
;; (test (object->string (inlet 'a (log 0)) :readable) "(inlet :a -inf.0+3.141592653589793i)")

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (object->string (inlet 'a (log 0)) :readable))))
       (expected (upstream-safe (lambda () "(inlet :a -inf.0+3.141592653589793i)")))
       (ok? (equal? actual expected)))
  (list 'upstream-test 25121 actual expected ok?))
