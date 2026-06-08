;; Imported from upstream s7test.scm line 31177.
;; Original form:
;; (test (or (and (or (> 3 2) (> 3 4)) (> 2 3)) 4) 4)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (or (and (or (> 3 2) (> 3 4)) (> 2 3)) 4))))
       (expected (upstream-safe (lambda () 4)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 31177 actual expected ok?))
