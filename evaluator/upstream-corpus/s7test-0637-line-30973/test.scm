;; Imported from upstream s7test.scm line 30973.
;; Original form:
;; (test (let ((x (list 1 2))) (set! x (define x 31)) x) 31)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (let ((x (list 1 2))) (set! x (define x 31)) x))))
       (expected (upstream-safe (lambda () 31)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 30973 actual expected ok?))
