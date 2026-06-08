;; Imported from upstream s7test.scm line 25125.
;; Original form:
;; (test (object->string (inlet 'a #<undefined>) :readable) "(inlet :a #<undefined>)")

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (object->string (inlet 'a #<undefined>) :readable))))
       (expected (upstream-safe (lambda () "(inlet :a #<undefined>)")))
       (ok? (equal? actual expected)))
  (list 'upstream-test 25125 actual expected ok?))
