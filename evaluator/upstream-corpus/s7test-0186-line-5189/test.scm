;; Imported from upstream s7test.scm line 5189.
;; Original form:
;; (test (procedure? set!) #f)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (procedure? set!))))
       (expected (upstream-safe (lambda () #f)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 5189 actual expected ok?))
