;; Imported from upstream s7test.scm line 10055.
;; Original form:
;; (test (let ((x (list 1))) (list-set! x 0 2) x) (list 2))

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (let ((x (list 1))) (list-set! x 0 2) x))))
       (expected (upstream-safe (lambda () (list 2))))
       (ok? (equal? actual expected)))
  (list 'upstream-test 10055 actual expected ok?))
