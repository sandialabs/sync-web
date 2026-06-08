;; Imported from upstream s7test.scm line 35642.
;; Original form:
;; (test (let loop ((a 2) (b 0)) (if (zero? a) b (loop (values (- a 1) (+ b 1))))) 2)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (let loop ((a 2) (b 0)) (if (zero? a) b (loop (values (- a 1) (+ b 1))))))))
       (expected (upstream-safe (lambda () 2)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 35642 actual expected ok?))
