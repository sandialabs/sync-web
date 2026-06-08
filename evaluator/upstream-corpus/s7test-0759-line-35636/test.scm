;; Imported from upstream s7test.scm line 35636.
;; Original form:
;; (test (list? (values 1)) #f)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (list? (values 1)))))
       (expected (upstream-safe (lambda () #f)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 35636 actual expected ok?))
