;; Imported from upstream s7test.scm line 31197.
;; Original form:
;; (test (let () (and (define (hi a) a) (define (hi a) (+ a 1))) (hi 1)) 2)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (let () (and (define (hi a) a) (define (hi a) (+ a 1))) (hi 1)))))
       (expected (upstream-safe (lambda () 2)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 31197 actual expected ok?))
