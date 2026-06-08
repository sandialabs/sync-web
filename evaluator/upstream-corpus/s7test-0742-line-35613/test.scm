;; Imported from upstream s7test.scm line 35613.
;; Original form:
;; (test (+ 1 (eval-string "(values 2 3 4)")) 10)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (+ 1 (eval-string "(values 2 3 4)")))))
       (expected (upstream-safe (lambda () 10)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 35613 actual expected ok?))
