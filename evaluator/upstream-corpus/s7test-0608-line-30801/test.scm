;; Imported from upstream s7test.scm line 30801.
;; Original form:
;; (test (let ((x '((1 2 3)))) (set! ((car x) 0) 3) x) '((3 2 3)))

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (let ((x '((1 2 3)))) (set! ((car x) 0) 3) x))))
       (expected (upstream-safe (lambda () '((3 2 3)))))
       (ok? (equal? actual expected)))
  (list 'upstream-test 30801 actual expected ok?))
