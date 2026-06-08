;; Imported from upstream s7test.scm line 30902.
;; Original form:
;; (test (let () (define (hi) (let ((x 1000.5)) (set! x (+ x 1)) x)) (hi) (hi)) 1001.5)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (let () (define (hi) (let ((x 1000.5)) (set! x (+ x 1)) x)) (hi) (hi)))))
       (expected (upstream-safe (lambda () 1001.5)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 30902 actual expected ok?))
