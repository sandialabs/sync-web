;; Imported from upstream s7test.scm line 5014.
;; Original form:
;; (test (let () (define (hi a b c d) (+ a (* (- b c) d))) (define (ho) (hi 1 2 3 4)) (ho)) -3)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (let () (define (hi a b c d) (+ a (* (- b c) d))) (define (ho) (hi 1 2 3 4)) (ho)))))
       (expected (upstream-safe (lambda () -3)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 5014 actual expected ok?))
