;; Imported from upstream s7test.scm line 30844.
;; Original form:
;; (test (let ((x '(1)) (y '(2))) (set! ((if #t x y) 0) 32) x) '(32))

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (let ((x '(1)) (y '(2))) (set! ((if #t x y) 0) 32) x))))
       (expected (upstream-safe (lambda () '(32))))
       (ok? (equal? actual expected)))
  (list 'upstream-test 30844 actual expected ok?))
