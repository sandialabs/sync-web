;; Imported from upstream s7test.scm line 1722.
;; Original form:
;; (test (eq? () '(#|@%$&|#)) #t)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (eq? () '(#|@%$&|#)))))
       (expected (upstream-safe (lambda () #t)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 1722 actual expected ok?))
