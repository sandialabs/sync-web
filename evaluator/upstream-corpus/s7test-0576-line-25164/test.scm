;; Imported from upstream s7test.scm line 25164.
;; Original form:
;; (test (object->string (inlet 'a (macro (x . y) `(+ ,x ,@y))) :readable) "(inlet :a (macro (x . y) (list-values '+ x (apply-values y))))")

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (object->string (inlet 'a (macro (x . y) `(+ ,x ,@y))) :readable))))
       (expected (upstream-safe (lambda () "(inlet :a (macro (x . y) (list-values '+ x (apply-values y))))")))
       (ok? (equal? actual expected)))
  (list 'upstream-test 25164 actual expected ok?))
