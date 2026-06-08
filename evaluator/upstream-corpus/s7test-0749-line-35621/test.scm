;; Imported from upstream s7test.scm line 35621.
;; Original form:
;; (test (list-ref (values '(1 (2 3)) 1 1)) 3)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (list-ref (values '(1 (2 3)) 1 1)))))
       (expected (upstream-safe (lambda () 3)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 35621 actual expected ok?))
