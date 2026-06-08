;; Imported from upstream s7test.scm line 1702.
;; Original form:
;; (test (eq? ':a (symbol->keyword (symbol "a"))) #t)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (eq? ':a (symbol->keyword (symbol "a"))))))
       (expected (upstream-safe (lambda () #t)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 1702 actual expected ok?))
