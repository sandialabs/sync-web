;; Imported from upstream s7test.scm line 1742.
;; Original form:
;; (test (let ((lst (list 1 2 3))) (eq? lst (apply list lst))) #f)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (let ((lst (list 1 2 3))) (eq? lst (apply list lst))))))
       (expected (upstream-safe (lambda () #f)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 1742 actual expected ok?))
