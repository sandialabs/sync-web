;; Imported from upstream s7test.scm line 5008.
;; Original form:
;; (test (let () (define (ho1) (*s7* 'version)) (define (ho2) (ho1)) (string? (ho2))) #t)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (let () (define (ho1) (*s7* 'version)) (define (ho2) (ho1)) (string? (ho2))))))
       (expected (upstream-safe (lambda () #t)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 5008 actual expected ok?))
