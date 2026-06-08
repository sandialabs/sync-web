;; Imported from upstream s7test.scm line 31210.
;; Original form:
;; (test (and (= 2 2) (< 2 1)) #f)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (and (= 2 2) (< 2 1)))))
       (expected (upstream-safe (lambda () #f)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 31210 actual expected ok?))
