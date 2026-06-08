;; Imported from upstream s7test.scm line 25122.
;; Original form:
;; (test (object->string (inlet 'a 1/0) :readable) "(inlet :a +nan.0)")

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (object->string (inlet 'a 1/0) :readable))))
       (expected (upstream-safe (lambda () "(inlet :a +nan.0)")))
       (ok? (equal? actual expected)))
  (list 'upstream-test 25122 actual expected ok?))
