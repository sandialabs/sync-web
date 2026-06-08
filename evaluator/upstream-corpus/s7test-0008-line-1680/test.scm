;; Imported from upstream s7test.scm line 1680.
;; Original form:
;; (test (eq? '(#f) '(#f)) #f)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (eq? '(#f) '(#f)))))
       (expected (upstream-safe (lambda () #f)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 1680 actual expected ok?))
