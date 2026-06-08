;; Imported from upstream s7test.scm line 31209.
;; Original form:
;; (test (and (= 2 2) (> 2 1)) #t)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (and (= 2 2) (> 2 1)))))
       (expected (upstream-safe (lambda () #t)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 31209 actual expected ok?))
