;; Imported from upstream s7test.scm line 25149.
;; Original form:
;; (test (object->string (inlet 'a #2d((1 2 3) (4 5 6))) :readable) "(inlet :a (subvector (vector 1 2 3 4 5 6) 0 6 '(2 3)))")

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (object->string (inlet 'a #2d((1 2 3) (4 5 6))) :readable))))
       (expected (upstream-safe (lambda () "(inlet :a (subvector (vector 1 2 3 4 5 6) 0 6 '(2 3)))")))
       (ok? (equal? actual expected)))
  (list 'upstream-test 25149 actual expected ok?))
