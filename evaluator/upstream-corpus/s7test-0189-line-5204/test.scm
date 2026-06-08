;; Imported from upstream s7test.scm line 5204.
;; Original form:
;; (test (procedure? #(1 2)) #f)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (procedure? #(1 2)))))
       (expected (upstream-safe (lambda () #f)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 5204 actual expected ok?))
