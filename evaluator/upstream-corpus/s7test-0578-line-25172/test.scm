;; Imported from upstream s7test.scm line 25172.
;; Original form:
;; (test (object->string (inlet 'a quasiquote) :readable) "(inlet :a #_quasiquote)")

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (object->string (inlet 'a quasiquote) :readable))))
       (expected (upstream-safe (lambda () "(inlet :a #_quasiquote)")))
       (ok? (equal? actual expected)))
  (list 'upstream-test 25172 actual expected ok?))
