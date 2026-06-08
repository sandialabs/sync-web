;; Imported from upstream s7test.scm line 5074.
;; Original form:
;; (test (symbol? '(AB\c () xyz)) #f)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (symbol? '(AB\c () xyz)))))
       (expected (upstream-safe (lambda () #f)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 5074 actual expected ok?))
