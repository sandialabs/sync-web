;; Imported from upstream s7test.scm line 35631.
;; Original form:
;; (test ((apply (values values values '((1 2))))) '(1 2))

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () ((apply (values values values '((1 2))))))))
       (expected (upstream-safe (lambda () '(1 2))))
       (ok? (equal? actual expected)))
  (list 'upstream-test 35631 actual expected ok?))
