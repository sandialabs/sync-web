;; Imported from upstream s7test.scm line 10061.
;; Original form:
;; (test (let ((x (list 1 2))) (list-set! x 0 x) (list? x)) #t)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (let ((x (list 1 2))) (list-set! x 0 x) (list? x)))))
       (expected (upstream-safe (lambda () #t)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 10061 actual expected ok?))
