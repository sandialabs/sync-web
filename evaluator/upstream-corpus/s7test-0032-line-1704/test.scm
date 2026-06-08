;; Imported from upstream s7test.scm line 1704.
;; Original form:
;; (test (let ((x '(a . b))) (eq? x x)) #t)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (let ((x '(a . b))) (eq? x x)))))
       (expected (upstream-safe (lambda () #t)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 1704 actual expected ok?))
