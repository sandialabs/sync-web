;; Imported from upstream s7test.scm line 15028.
;; Original form:
;; (test (vector? (let ((v (vector 0))) (set! (v 0) v) (v 0 0 0 0))) #t)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (vector? (let ((v (vector 0))) (set! (v 0) v) (v 0 0 0 0))))))
       (expected (upstream-safe (lambda () #t)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 15028 actual expected ok?))
