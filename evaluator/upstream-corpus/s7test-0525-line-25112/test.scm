;; Imported from upstream s7test.scm line 25112.
;; Original form:
;; (test (object->string (inlet) :readable) "(inlet)")

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (object->string (inlet) :readable))))
       (expected (upstream-safe (lambda () "(inlet)")))
       (ok? (equal? actual expected)))
  (list 'upstream-test 25112 actual expected ok?))
