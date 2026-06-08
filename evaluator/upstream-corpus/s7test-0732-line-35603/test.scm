;; Imported from upstream s7test.scm line 35603.
;; Original form:
;; (test (+ (if (values) (values 1 2) (values 3 4)) (if (null? (values)) (values 5 6) (values 7 8))) 18)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (+ (if (values) (values 1 2) (values 3 4)) (if (null? (values)) (values 5 6) (values 7 8))))))
       (expected (upstream-safe (lambda () 18)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 35603 actual expected ok?))
