;; Imported from upstream s7test.scm line 10020.
;; Original form:
;; (test ('(1 2 3) 1) 2)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () ('(1 2 3) 1))))
       (expected (upstream-safe (lambda () 2)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 10020 actual expected ok?))
