;; Imported from upstream s7test.scm line 1706.
;; Original form:
;; (test (eq? (cons 'a 'b) (cons 'a 'b)) #f)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (eq? (cons 'a 'b) (cons 'a 'b)))))
       (expected (upstream-safe (lambda () #f)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 1706 actual expected ok?))
