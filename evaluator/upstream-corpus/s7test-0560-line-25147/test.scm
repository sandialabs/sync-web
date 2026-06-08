;; Imported from upstream s7test.scm line 25147.
;; Original form:
;; (test (object->string (inlet 'a (float-vector 1 2 3)) :readable) "(inlet :a #r(1.0 2.0 3.0))")

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (object->string (inlet 'a (float-vector 1 2 3)) :readable))))
       (expected (upstream-safe (lambda () "(inlet :a #r(1.0 2.0 3.0))")))
       (ok? (equal? actual expected)))
  (list 'upstream-test 25147 actual expected ok?))
