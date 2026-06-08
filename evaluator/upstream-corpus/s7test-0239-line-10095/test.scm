;; Imported from upstream s7test.scm line 10095.
;; Original form:
;; (test (let ((L '((1 2 3) (4 5 6)))) (set! (L 1 0) 32) L) '((1 2 3) (32 5 6)))

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (let ((L '((1 2 3) (4 5 6)))) (set! (L 1 0) 32) L))))
       (expected (upstream-safe (lambda () '((1 2 3) (32 5 6)))))
       (ok? (equal? actual expected)))
  (list 'upstream-test 10095 actual expected ok?))
