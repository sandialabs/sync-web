;; Imported from upstream s7test.scm line 30874.
;; Original form:
;; (test (let ((x 0)) (apply set! 'x '(3)) x) 3)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (let ((x 0)) (apply set! 'x '(3)) x))))
       (expected (upstream-safe (lambda () 3)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 30874 actual expected ok?))
