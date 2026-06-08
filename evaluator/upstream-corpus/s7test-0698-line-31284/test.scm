;; Imported from upstream s7test.scm line 31284.
;; Original form:
;; (test (and #t (+ 1 2 3)) 6)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (and #t (+ 1 2 3)))))
       (expected (upstream-safe (lambda () 6)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 31284 actual expected ok?))
