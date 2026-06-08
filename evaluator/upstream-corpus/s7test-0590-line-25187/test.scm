;; Imported from upstream s7test.scm line 25187.
;; Original form:
;; (test (object->string (inlet 'a *stdout*) :readable) "(inlet :a *stdout*)")

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (object->string (inlet 'a *stdout*) :readable))))
       (expected (upstream-safe (lambda () "(inlet :a *stdout*)")))
       (ok? (equal? actual expected)))
  (list 'upstream-test 25187 actual expected ok?))
