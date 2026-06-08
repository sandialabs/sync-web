;; Imported from upstream s7test.scm line 25148.
;; Original form:
;; (test (object->string (inlet 'a (int-vector 1 2 3)) :readable) "(inlet :a #i(1 2 3))")

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (object->string (inlet 'a (int-vector 1 2 3)) :readable))))
       (expected (upstream-safe (lambda () "(inlet :a #i(1 2 3))")))
       (ok? (equal? actual expected)))
  (list 'upstream-test 25148 actual expected ok?))
