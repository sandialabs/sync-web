;; Imported from upstream s7test.scm line 35589.
;; Original form:
;; (test (apply + (values ())) 0)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (apply + (values ())))))
       (expected (upstream-safe (lambda () 0)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 35589 actual expected ok?))
