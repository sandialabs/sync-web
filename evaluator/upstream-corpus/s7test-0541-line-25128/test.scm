;; Imported from upstream s7test.scm line 25128.
;; Original form:
;; (test (object->string (inlet 'a lambda) :readable) "(inlet :a #_lambda)")

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (object->string (inlet 'a lambda) :readable))))
       (expected (upstream-safe (lambda () "(inlet :a #_lambda)")))
       (ok? (equal? actual expected)))
  (list 'upstream-test 25128 actual expected ok?))
