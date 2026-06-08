;; Imported from upstream s7test.scm line 35620.
;; Original form:
;; (test (list-ref '(1 (2 3)) (values 1 1)) 3)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (list-ref '(1 (2 3)) (values 1 1)))))
       (expected (upstream-safe (lambda () 3)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 35620 actual expected ok?))
