;; Imported from upstream s7test.scm line 30834.
;; Original form:
;; (test (let ((x 1)) (set! x x) x) 1)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (let ((x 1)) (set! x x) x))))
       (expected (upstream-safe (lambda () 1)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 30834 actual expected ok?))
