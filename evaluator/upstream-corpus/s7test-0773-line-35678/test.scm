;; Imported from upstream s7test.scm line 35678.
;; Original form:
;; (test ((lambda () (format #f "~S" (car (list (list-values ((lambda (a) (values a (+ a 1))) 2) :rest) (make-vector 3 '(1) pair?)))))) "(2 3 :rest)")

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () ((lambda () (format #f "~S" (car (list (list-values ((lambda (a) (values a (+ a 1))) 2) :rest) (make-vector 3 '(1) pair?)))))))))
       (expected (upstream-safe (lambda () "(2 3 :rest)")))
       (ok? (equal? actual expected)))
  (list 'upstream-test 35678 actual expected ok?))
