;; Imported from upstream s7test.scm line 35710.
;; Original form:
;; (test (let () (define (hi) (symbol? (values 1))) (hi)) #f)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (let () (define (hi) (symbol? (values 1))) (hi)))))
       (expected (upstream-safe (lambda () #f)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 35710 actual expected ok?))
