;; Imported from upstream s7test.scm line 10198.
;; Original form:
;; (test (let ((a (list 1 2))) (list 3 4 'a (car (cons 'b 'c)) (+ 6 -2))) '(3 4 a b 4))

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (let ((a (list 1 2))) (list 3 4 'a (car (cons 'b 'c)) (+ 6 -2))))))
       (expected (upstream-safe (lambda () '(3 4 a b 4))))
       (ok? (equal? actual expected)))
  (list 'upstream-test 10198 actual expected ok?))
