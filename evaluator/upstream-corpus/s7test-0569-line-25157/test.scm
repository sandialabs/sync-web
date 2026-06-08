;; Imported from upstream s7test.scm line 25157.
;; Original form:
;; (test (object->string (inlet 'a (macro () `(+ ,b 1))) :readable) "(inlet :a (macro () (list-values '+ b 1)))")

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (object->string (inlet 'a (macro () `(+ ,b 1))) :readable))))
       (expected (upstream-safe (lambda () "(inlet :a (macro () (list-values '+ b 1)))")))
       (ok? (equal? actual expected)))
  (list 'upstream-test 25157 actual expected ok?))
