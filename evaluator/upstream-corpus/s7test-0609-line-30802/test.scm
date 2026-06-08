;; Imported from upstream s7test.scm line 30802.
;; Original form:
;; (test (let ((x '((1 2 3)))) (set! ('(1 2 3) 0) 32) x) '((1 2 3)))

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (let ((x '((1 2 3)))) (set! ('(1 2 3) 0) 32) x))))
       (expected (upstream-safe (lambda () '((1 2 3)))))
       (ok? (equal? actual expected)))
  (list 'upstream-test 30802 actual expected ok?))
