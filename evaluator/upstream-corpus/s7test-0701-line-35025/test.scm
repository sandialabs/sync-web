;; Imported from upstream s7test.scm line 35025.
;; Original form:
;; (test (let () (define (f1 x) (abs x)) (define (f2 x) (f1 x)) (f2 -1)) 1)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (let () (define (f1 x) (abs x)) (define (f2 x) (f1 x)) (f2 -1)))))
       (expected (upstream-safe (lambda () 1)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 35025 actual expected ok?))
