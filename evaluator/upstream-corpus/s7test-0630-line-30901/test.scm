;; Imported from upstream s7test.scm line 30901.
;; Original form:
;; (test (let () (define (hi) (let ((x 1000)) (set! x (+ x 1)) x)) (hi) (hi)) 1001)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (let () (define (hi) (let ((x 1000)) (set! x (+ x 1)) x)) (hi) (hi)))))
       (expected (upstream-safe (lambda () 1001)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 30901 actual expected ok?))
