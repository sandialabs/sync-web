;; Imported from upstream s7test.scm line 25118.
;; Original form:
;; (test (object->string (inlet 'a 3.0) :readable) "(inlet :a 3.0)")

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (object->string (inlet 'a 3.0) :readable))))
       (expected (upstream-safe (lambda () "(inlet :a 3.0)")))
       (ok? (equal? actual expected)))
  (list 'upstream-test 25118 actual expected ok?))
