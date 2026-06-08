;; Imported from upstream s7test.scm line 10066.
;; Original form:
;; (test (list-set! '(1 2 3) 1 4) 4)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (list-set! '(1 2 3) 1 4))))
       (expected (upstream-safe (lambda () 4)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 10066 actual expected ok?))
