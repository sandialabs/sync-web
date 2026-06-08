;; Imported from upstream s7test.scm line 10059.
;; Original form:
;; (test (let ((x '(1 2))) (list-set! x 1 (list 3 4)) x) '(1 (3 4)))

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (let ((x '(1 2))) (list-set! x 1 (list 3 4)) x))))
       (expected (upstream-safe (lambda () '(1 (3 4)))))
       (ok? (equal? actual expected)))
  (list 'upstream-test 10059 actual expected ok?))
