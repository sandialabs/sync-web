;; Imported from upstream s7test.scm line 31244.
;; Original form:
;; (test (and (or (and (> 3 2) (> 3 4)) (> 2 3)) 4) #f)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (and (or (and (> 3 2) (> 3 4)) (> 2 3)) 4))))
       (expected (upstream-safe (lambda () #f)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 31244 actual expected ok?))
