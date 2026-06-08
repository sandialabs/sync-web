;; Imported from upstream s7test.scm line 31223.
;; Original form:
;; (test (let ((x '(1))) (eq? (and x) x)) #t)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (let ((x '(1))) (eq? (and x) x)))))
       (expected (upstream-safe (lambda () #t)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 31223 actual expected ok?))
