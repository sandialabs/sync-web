;; Imported from upstream s7test.scm line 5007.
;; Original form:
;; (test (let () (define (ho a) (values a 1)) (define (hi) (- (ho 2))) (hi)) 1)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (let () (define (ho a) (values a 1)) (define (hi) (- (ho 2))) (hi)))))
       (expected (upstream-safe (lambda () 1)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 5007 actual expected ok?))
