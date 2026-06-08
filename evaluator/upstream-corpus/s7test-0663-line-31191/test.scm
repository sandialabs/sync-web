;; Imported from upstream s7test.scm line 31191.
;; Original form:
;; (test (or . (1 2)) 1)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (or . (1 2)))))
       (expected (upstream-safe (lambda () 1)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 31191 actual expected ok?))
