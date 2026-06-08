;; Imported from upstream s7test.scm line 35599.
;; Original form:
;; (test (let () + (values 1 2) 4) 4)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (let () + (values 1 2) 4))))
       (expected (upstream-safe (lambda () 4)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 35599 actual expected ok?))
