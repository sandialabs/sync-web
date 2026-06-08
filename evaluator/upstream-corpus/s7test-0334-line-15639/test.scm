;; Imported from upstream s7test.scm line 15639.
;; Original form:
;; (test (map (lambda (a) a) '(0 1 2 . 3)) '(0 1 2))

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (map (lambda (a) a) '(0 1 2 . 3)))))
       (expected (upstream-safe (lambda () '(0 1 2))))
       (ok? (equal? actual expected)))
  (list 'upstream-test 15639 actual expected ok?))
