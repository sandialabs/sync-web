;; Imported from upstream s7test.scm line 25188.
;; Original form:
;; (test (object->string (inlet 'a *stderr*) :readable) "(inlet :a *stderr*)")

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (object->string (inlet 'a *stderr*) :readable))))
       (expected (upstream-safe (lambda () "(inlet :a *stderr*)")))
       (ok? (equal? actual expected)))
  (list 'upstream-test 25188 actual expected ok?))
