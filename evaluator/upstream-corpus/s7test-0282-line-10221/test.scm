;; Imported from upstream s7test.scm line 10221.
;; Original form:
;; (test (list-tail (list 1 2) 2) ())

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (list-tail (list 1 2) 2))))
       (expected (upstream-safe (lambda () ())))
       (ok? (equal? actual expected)))
  (list 'upstream-test 10221 actual expected ok?))
