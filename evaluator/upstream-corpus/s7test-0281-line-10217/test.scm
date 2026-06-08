;; Imported from upstream s7test.scm line 10217.
;; Original form:
;; (test (list-tail () 0) ())

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (list-tail () 0))))
       (expected (upstream-safe (lambda () ())))
       (ok? (equal? actual expected)))
  (list 'upstream-test 10217 actual expected ok?))
