;; Imported from upstream s7test.scm line 5005.
;; Original form:
;; (test (let () (define (ho a) (+ a 2)) (define (hi) (+ (values 3 4) (ho 1))) (hi)) 10)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (let () (define (ho a) (+ a 2)) (define (hi) (+ (values 3 4) (ho 1))) (hi)))))
       (expected (upstream-safe (lambda () 10)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 5005 actual expected ok?))
