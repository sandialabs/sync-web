;; Imported from upstream s7test.scm line 21598.
;; Original form:
;; (test (format #f "hiho") "hiho")

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (format #f "hiho"))))
       (expected (upstream-safe (lambda () "hiho")))
       (ok? (equal? actual expected)))
  (list 'upstream-test 21598 actual expected ok?))
