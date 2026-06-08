;; Imported from upstream s7test.scm line 35715.
;; Original form:
;; (test (let () (define (func) (let ((i 0)) ((lambda (a) (sort! a >)) (list-values (values 1 2 3) (+ i 1))))) (func)) '(3 2 1 1))

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (let () (define (func) (let ((i 0)) ((lambda (a) (sort! a >)) (list-values (values 1 2 3) (+ i 1))))) (func)))))
       (expected (upstream-safe (lambda () '(3 2 1 1))))
       (ok? (equal? actual expected)))
  (list 'upstream-test 35715 actual expected ok?))
