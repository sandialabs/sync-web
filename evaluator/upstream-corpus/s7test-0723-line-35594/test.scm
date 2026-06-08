;; Imported from upstream s7test.scm line 35594.
;; Original form:
;; (test (* (values (+ (values 1 2)) (- (values 3 4)))) -3)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (* (values (+ (values 1 2)) (- (values 3 4)))))))
       (expected (upstream-safe (lambda () -3)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 35594 actual expected ok?))
