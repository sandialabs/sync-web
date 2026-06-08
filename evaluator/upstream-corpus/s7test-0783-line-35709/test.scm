;; Imported from upstream s7test.scm line 35709.
;; Original form:
;; (test (let () (define (hi) (symbol? (values 'a))) (hi)) #t)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (let () (define (hi) (symbol? (values 'a))) (hi)))))
       (expected (upstream-safe (lambda () #t)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 35709 actual expected ok?))
