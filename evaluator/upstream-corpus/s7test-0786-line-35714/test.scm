;; Imported from upstream s7test.scm line 35714.
;; Original form:
;; (test (let () (define (hi) (+ (values 1 2) (values 3 4))) (hi)) 10)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (let () (define (hi) (+ (values 1 2) (values 3 4))) (hi)))))
       (expected (upstream-safe (lambda () 10)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 35714 actual expected ok?))
