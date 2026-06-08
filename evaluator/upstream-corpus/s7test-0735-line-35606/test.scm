;; Imported from upstream s7test.scm line 35606.
;; Original form:
;; (test (apply + (list (values 1 2))) 3)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (apply + (list (values 1 2))))))
       (expected (upstream-safe (lambda () 3)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 35606 actual expected ok?))
