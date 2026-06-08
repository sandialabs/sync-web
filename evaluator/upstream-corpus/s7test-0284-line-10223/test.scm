;; Imported from upstream s7test.scm line 10223.
;; Original form:
;; (test (list-tail (cons 1 2) 1) 2)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (list-tail (cons 1 2) 1))))
       (expected (upstream-safe (lambda () 2)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 10223 actual expected ok?))
