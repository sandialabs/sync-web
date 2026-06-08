;; Imported from upstream s7test.scm line 30750.
;; Original form:
;; (test (let ((a (lambda (b) (+ b 1)))) (set! a (lambda (b) (+ b 2))) (a 3)) 5)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (let ((a (lambda (b) (+ b 1)))) (set! a (lambda (b) (+ b 2))) (a 3)))))
       (expected (upstream-safe (lambda () 5)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 30750 actual expected ok?))
