;; Imported from upstream s7test.scm line 5003.
;; Original form:
;; (test (let () (define (ho a) (+ a 2)) (define (hi) (+ (ho 1) (ho 2))) (hi)) 7)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (let () (define (ho a) (+ a 2)) (define (hi) (+ (ho 1) (ho 2))) (hi)))))
       (expected (upstream-safe (lambda () 7)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 5003 actual expected ok?))
