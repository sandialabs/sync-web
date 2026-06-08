;; Imported from upstream s7test.scm line 35596.
;; Original form:
;; (test (let ((f1 (lambda (x) (values x (+ x 1)))) (f2 (lambda () (values 2)))) (+ (f1 3) (* 2 (f2)))) 11)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (let ((f1 (lambda (x) (values x (+ x 1)))) (f2 (lambda () (values 2)))) (+ (f1 3) (* 2 (f2)))))))
       (expected (upstream-safe (lambda () 11)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 35596 actual expected ok?))
