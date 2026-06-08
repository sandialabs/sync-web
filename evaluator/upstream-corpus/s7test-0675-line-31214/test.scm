;; Imported from upstream s7test.scm line 31214.
;; Original form:
;; (test (and 3) 3)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (and 3))))
       (expected (upstream-safe (lambda () 3)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 31214 actual expected ok?))
