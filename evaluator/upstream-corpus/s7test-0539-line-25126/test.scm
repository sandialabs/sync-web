;; Imported from upstream s7test.scm line 25126.
;; Original form:
;; (test (object->string (inlet 'a #<unspecified>) :readable) "(inlet :a #<unspecified>)")

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (object->string (inlet 'a #<unspecified>) :readable))))
       (expected (upstream-safe (lambda () "(inlet :a #<unspecified>)")))
       (ok? (equal? actual expected)))
  (list 'upstream-test 25126 actual expected ok?))
