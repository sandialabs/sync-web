;; Imported from upstream s7test.scm line 25139.
;; Original form:
;; (test (object->string (inlet 'a :b) :readable) "(inlet :a :b)")

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (object->string (inlet 'a :b) :readable))))
       (expected (upstream-safe (lambda () "(inlet :a :b)")))
       (ok? (equal? actual expected)))
  (list 'upstream-test 25139 actual expected ok?))
