;; Imported from upstream s7test.scm line 1697.
;; Original form:
;; (test (eq? ':a 'a:) #f)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (eq? ':a 'a:))))
       (expected (upstream-safe (lambda () #f)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 1697 actual expected ok?))
