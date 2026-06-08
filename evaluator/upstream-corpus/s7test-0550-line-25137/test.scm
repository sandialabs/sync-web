;; Imported from upstream s7test.scm line 25137.
;; Original form:
;; (test (object->string (inlet 'a #t) :readable) "(inlet :a #t)")

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (object->string (inlet 'a #t) :readable))))
       (expected (upstream-safe (lambda () "(inlet :a #t)")))
       (ok? (equal? actual expected)))
  (list 'upstream-test 25137 actual expected ok?))
