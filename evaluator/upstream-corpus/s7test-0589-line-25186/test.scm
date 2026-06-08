;; Imported from upstream s7test.scm line 25186.
;; Original form:
;; (test (object->string (inlet 'a *stdin*) :readable) "(inlet :a *stdin*)")

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (object->string (inlet 'a *stdin*) :readable))))
       (expected (upstream-safe (lambda () "(inlet :a *stdin*)")))
       (ok? (equal? actual expected)))
  (list 'upstream-test 25186 actual expected ok?))
