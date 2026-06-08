;; Imported from upstream s7test.scm line 35612.
;; Original form:
;; (test (let () (define-macro (hi a) `(+ 1 ,a)) (hi (values 1 2 3))) 7)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (let () (define-macro (hi a) `(+ 1 ,a)) (hi (values 1 2 3))))))
       (expected (upstream-safe (lambda () 7)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 35612 actual expected ok?))
