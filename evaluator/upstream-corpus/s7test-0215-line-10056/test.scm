;; Imported from upstream s7test.scm line 10056.
;; Original form:
;; (test (let ((x (cons 1 2))) (list-set! x 0 3) x) '(3 . 2))

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (let ((x (cons 1 2))) (list-set! x 0 3) x))))
       (expected (upstream-safe (lambda () '(3 . 2))))
       (ok? (equal? actual expected)))
  (list 'upstream-test 10056 actual expected ok?))
