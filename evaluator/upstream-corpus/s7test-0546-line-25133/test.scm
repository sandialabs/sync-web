;; Imported from upstream s7test.scm line 25133.
;; Original form:
;; (test (object->string (inlet 'a (list 1 2)) :readable) "(inlet :a (list 1 2))")

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (object->string (inlet 'a (list 1 2)) :readable))))
       (expected (upstream-safe (lambda () "(inlet :a (list 1 2))")))
       (ok? (equal? actual expected)))
  (list 'upstream-test 25133 actual expected ok?))
