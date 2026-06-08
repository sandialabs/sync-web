;; Imported from upstream s7test.scm line 5174.
;; Original form:
;; (test (let () (define (hi) 1) (procedure? hi)) #t)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (let () (define (hi) 1) (procedure? hi)))))
       (expected (upstream-safe (lambda () #t)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 5174 actual expected ok?))
