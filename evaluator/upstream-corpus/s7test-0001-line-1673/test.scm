;; Imported from upstream s7test.scm line 1673.
;; Original form:
;; (test (eq? 'a 3) #f)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (eq? 'a 3))))
       (expected (upstream-safe (lambda () #f)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 1673 actual expected ok?))
