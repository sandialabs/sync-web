;; Imported from upstream s7test.scm line 30903.
;; Original form:
;; (test (let () (define (hi) (let ((x 3/2)) (set! x (+ x 1)) x)) (hi) (hi)) 5/2)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (let () (define (hi) (let ((x 3/2)) (set! x (+ x 1)) x)) (hi) (hi)))))
       (expected (upstream-safe (lambda () 5/2)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 30903 actual expected ok?))
