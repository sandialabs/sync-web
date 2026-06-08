;; Imported from upstream s7test.scm line 25155.
;; Original form:
;; (test (object->string (inlet 'a (define-macro (_m_ b) `(+ ,b 1))) :readable) "(inlet :a (macro (b) (list-values '+ b 1)))")

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (object->string (inlet 'a (define-macro (_m_ b) `(+ ,b 1))) :readable))))
       (expected (upstream-safe (lambda () "(inlet :a (macro (b) (list-values '+ b 1)))")))
       (ok? (equal? actual expected)))
  (list 'upstream-test 25155 actual expected ok?))
