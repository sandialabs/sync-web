;; Imported from upstream s7test.scm line 10098.
;; Original form:
;; (test (let ((L '(((1 2 3) (4 5 6)) ((7 8 9) (10 11 12))))) (set! (L 1) 32) L) '(((1 2 3) (4 5 6)) 32))

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (let ((L '(((1 2 3) (4 5 6)) ((7 8 9) (10 11 12))))) (set! (L 1) 32) L))))
       (expected (upstream-safe (lambda () '(((1 2 3) (4 5 6)) 32))))
       (ok? (equal? actual expected)))
  (list 'upstream-test 10098 actual expected ok?))
