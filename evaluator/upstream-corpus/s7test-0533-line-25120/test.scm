;; Imported from upstream s7test.scm line 25120.
;; Original form:
;; (test (object->string (inlet 'a 1+i) :readable) "(inlet :a 1.0+1.0i)")

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (object->string (inlet 'a 1+i) :readable))))
       (expected (upstream-safe (lambda () "(inlet :a 1.0+1.0i)")))
       (ok? (equal? actual expected)))
  (list 'upstream-test 25120 actual expected ok?))
