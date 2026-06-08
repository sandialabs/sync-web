;; Imported from upstream s7test.scm line 1726.
;; Original form:
;; (test (cadr '#| a comment |#(+ 1 2)) 1)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (cadr '#| a comment |#(+ 1 2)))))
       (expected (upstream-safe (lambda () 1)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 1726 actual expected ok?))
