;; Imported from upstream s7test.scm line 35699.
;; Original form:
;; (test (let ((x 1)) (set! x (values 2)) x) 2)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (let ((x 1)) (set! x (values 2)) x))))
       (expected (upstream-safe (lambda () 2)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 35699 actual expected ok?))
