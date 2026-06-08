;; Imported from upstream s7test.scm line 1728.
;; Original form:
;; (test `(+ 1 ,#||#(+ 3 4)) '(+ 1 7))

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () `(+ 1 ,#||#(+ 3 4)))))
       (expected (upstream-safe (lambda () '(+ 1 7))))
       (ok? (equal? actual expected)))
  (list 'upstream-test 1728 actual expected ok?))
