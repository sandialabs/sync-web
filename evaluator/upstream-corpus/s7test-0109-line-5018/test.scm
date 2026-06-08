;; Imported from upstream s7test.scm line 5018.
;; Original form:
;; (test (let () (define (hi a b) (- (+ a (abs b)))) (define (ho) (hi 1 -2)) (ho)) -3)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (let () (define (hi a b) (- (+ a (abs b)))) (define (ho) (hi 1 -2)) (ho)))))
       (expected (upstream-safe (lambda () -3)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 5018 actual expected ok?))
