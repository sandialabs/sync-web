;; Imported from upstream s7test.scm line 15003.
;; Original form:
;; (test (let ((v (vector 0))) (equal? (vector v) (vector v))) #t)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (let ((v (vector 0))) (equal? (vector v) (vector v))))))
       (expected (upstream-safe (lambda () #t)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 15003 actual expected ok?))
