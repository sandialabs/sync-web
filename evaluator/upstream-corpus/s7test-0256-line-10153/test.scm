;; Imported from upstream s7test.scm line 10153.
;; Original form:
;; (test (let ((x '(1)) (y '(2))) (set! ((if #t x y) 0) 32) (list x y)) '((32) (2)))

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (let ((x '(1)) (y '(2))) (set! ((if #t x y) 0) 32) (list x y)))))
       (expected (upstream-safe (lambda () '((32) (2)))))
       (ok? (equal? actual expected)))
  (list 'upstream-test 10153 actual expected ok?))
