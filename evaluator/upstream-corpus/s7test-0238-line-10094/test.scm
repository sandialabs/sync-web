;; Imported from upstream s7test.scm line 10094.
;; Original form:
;; (test (let ((L '((1 2 3) (4 5 6)))) (set! (L 1) 32) L) '((1 2 3) 32))

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (let ((L '((1 2 3) (4 5 6)))) (set! (L 1) 32) L))))
       (expected (upstream-safe (lambda () '((1 2 3) 32))))
       (ok? (equal? actual expected)))
  (list 'upstream-test 10094 actual expected ok?))
