;; Imported from upstream s7test.scm line 1748.
;; Original form:
;; (test (eq? ''2 '2) #f)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (eq? ''2 '2))))
       (expected (upstream-safe (lambda () #f)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 1748 actual expected ok?))
