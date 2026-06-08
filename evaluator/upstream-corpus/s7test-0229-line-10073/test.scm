;; Imported from upstream s7test.scm line 10073.
;; Original form:
;; (test (set! ('(1 2 . 3) 1) 23) 23)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (set! ('(1 2 . 3) 1) 23))))
       (expected (upstream-safe (lambda () 23)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 10073 actual expected ok?))
