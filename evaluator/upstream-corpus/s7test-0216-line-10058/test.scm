;; Imported from upstream s7test.scm line 10058.
;; Original form:
;; (test (let ((x '((1) 2))) (list-set! x 0 1) x) '(1 2))

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (let ((x '((1) 2))) (list-set! x 0 1) x))))
       (expected (upstream-safe (lambda () '(1 2))))
       (ok? (equal? actual expected)))
  (list 'upstream-test 10058 actual expected ok?))
