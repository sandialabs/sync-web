;; Imported from upstream s7test.scm line 25135.
;; Original form:
;; (test (object->string (inlet 'a ()) :readable) "(inlet :a ())")

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (object->string (inlet 'a ()) :readable))))
       (expected (upstream-safe (lambda () "(inlet :a ())")))
       (ok? (equal? actual expected)))
  (list 'upstream-test 25135 actual expected ok?))
