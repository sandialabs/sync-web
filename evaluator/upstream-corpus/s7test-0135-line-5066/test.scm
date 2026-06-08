;; Imported from upstream s7test.scm line 5066.
;; Original form:
;; (test (symbol? (vector-ref #(1 a 34) 1)) #t)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (symbol? (vector-ref #(1 a 34) 1)))))
       (expected (upstream-safe (lambda () #t)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 5066 actual expected ok?))
