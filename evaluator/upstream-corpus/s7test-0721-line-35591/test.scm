;; Imported from upstream s7test.scm line 35591.
;; Original form:
;; (test (let ((f (lambda () (values 1 2 3)))) (+ (f))) 6)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (let ((f (lambda () (values 1 2 3)))) (+ (f))))))
       (expected (upstream-safe (lambda () 6)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 35591 actual expected ok?))
