;; Imported from upstream s7test.scm line 35601.
;; Original form:
;; (test (vector-ref (values (vector 1 2 3)) 1) 2)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (vector-ref (values (vector 1 2 3)) 1))))
       (expected (upstream-safe (lambda () 2)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 35601 actual expected ok?))
