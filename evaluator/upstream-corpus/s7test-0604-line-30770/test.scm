;; Imported from upstream s7test.scm line 30770.
;; Original form:
;; (test (let ((a 1)) (set! a a) a) 1)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (let ((a 1)) (set! a a) a))))
       (expected (upstream-safe (lambda () 1)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 30770 actual expected ok?))
