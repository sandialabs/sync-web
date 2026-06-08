;; Imported from upstream s7test.scm line 35624.
;; Original form:
;; (test (+ (values (begin (values 1 2)) (let ((x 1)) (values x (+ x 1))))) 6)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (+ (values (begin (values 1 2)) (let ((x 1)) (values x (+ x 1))))))))
       (expected (upstream-safe (lambda () 6)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 35624 actual expected ok?))
