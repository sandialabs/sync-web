;; Imported from upstream s7test.scm line 5166.
;; Original form:
;; (test (procedure? procedure?) #t)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (procedure? procedure?))))
       (expected (upstream-safe (lambda () #t)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 5166 actual expected ok?))
