;; Imported from upstream s7test.scm line 1727.
;; Original form:
;; (test `(+ 1 ,@#||#(list 2 3)) '(+ 1 2 3))

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () `(+ 1 ,@#||#(list 2 3)))))
       (expected (upstream-safe (lambda () '(+ 1 2 3))))
       (ok? (equal? actual expected)))
  (list 'upstream-test 1727 actual expected ok?))
