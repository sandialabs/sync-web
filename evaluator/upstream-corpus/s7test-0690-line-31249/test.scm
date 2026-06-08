;; Imported from upstream s7test.scm line 31249.
;; Original form:
;; (test (and '#t ()) ())

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (and '#t ()))))
       (expected (upstream-safe (lambda () ())))
       (ok? (equal? actual expected)))
  (list 'upstream-test 31249 actual expected ok?))
