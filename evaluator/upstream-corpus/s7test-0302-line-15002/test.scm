;; Imported from upstream s7test.scm line 15002.
;; Original form:
;; (test (equal? (vector 0 #\a "hi" (list 1 2 3)) (vector 0 #\a "hi" (list 1 2 3))) #t)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (equal? (vector 0 #\a "hi" (list 1 2 3)) (vector 0 #\a "hi" (list 1 2 3))))))
       (expected (upstream-safe (lambda () #t)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 15002 actual expected ok?))
