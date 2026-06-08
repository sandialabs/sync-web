;; Imported from upstream s7test.scm line 10111.
;; Original form:
;; (test (let ((L '(((1 2 3))))) (set! ((L 0) 0 1) 32) L) '(((1 32 3))))

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (let ((L '(((1 2 3))))) (set! ((L 0) 0 1) 32) L))))
       (expected (upstream-safe (lambda () '(((1 32 3))))))
       (ok? (equal? actual expected)))
  (list 'upstream-test 10111 actual expected ok?))
