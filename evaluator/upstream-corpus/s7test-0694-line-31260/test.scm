;; Imported from upstream s7test.scm line 31260.
;; Original form:
;; (test (+ 1 (and (define (hi a) a) (hi 2))) 3)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (+ 1 (and (define (hi a) a) (hi 2))))))
       (expected (upstream-safe (lambda () 3)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 31260 actual expected ok?))
