;; Imported from upstream s7test.scm line 30743.
;; Original form:
;; (test (let ((a 1)) (set! a 2) a) 2)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (let ((a 1)) (set! a 2) a))))
       (expected (upstream-safe (lambda () 2)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 30743 actual expected ok?))
