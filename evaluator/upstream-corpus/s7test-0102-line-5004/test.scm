;; Imported from upstream s7test.scm line 5004.
;; Original form:
;; (test (let () (define (ho a) (+ a 2)) (define (hi) (+ (ho 1) (values 3 4))) (hi)) 10)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (let () (define (ho a) (+ a 2)) (define (hi) (+ (ho 1) (values 3 4))) (hi)))))
       (expected (upstream-safe (lambda () 10)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 5004 actual expected ok?))
