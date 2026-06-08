;; Imported from upstream s7test.scm line 35602.
;; Original form:
;; (test (+ (values (+ 1 (values 2 3)) 4) 5 (values 6) (values 7 8 (+ (values 9 10) 11))) 66)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (+ (values (+ 1 (values 2 3)) 4) 5 (values 6) (values 7 8 (+ (values 9 10) 11))))))
       (expected (upstream-safe (lambda () 66)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 35602 actual expected ok?))
