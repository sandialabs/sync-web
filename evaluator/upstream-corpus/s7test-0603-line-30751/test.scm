;; Imported from upstream s7test.scm line 30751.
;; Original form:
;; (test (let ((a (lambda (x) (set! x 3) x))) (a 1)) 3)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (let ((a (lambda (x) (set! x 3) x))) (a 1)))))
       (expected (upstream-safe (lambda () 3)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 30751 actual expected ok?))
