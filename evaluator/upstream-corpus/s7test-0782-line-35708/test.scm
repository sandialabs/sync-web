;; Imported from upstream s7test.scm line 35708.
;; Original form:
;; (test (let () (define (hi) (symbol? (values))) (hi)) #f)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (let () (define (hi) (symbol? (values))) (hi)))))
       (expected (upstream-safe (lambda () #f)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 35708 actual expected ok?))
