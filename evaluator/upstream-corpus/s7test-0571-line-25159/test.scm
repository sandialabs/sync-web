;; Imported from upstream s7test.scm line 25159.
;; Original form:
;; (test (object->string (inlet 'a (lambda* ((b 1)) (+ b 1))) :readable) "(inlet :a (lambda* ((b 1)) (+ b 1)))")

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (object->string (inlet 'a (lambda* ((b 1)) (+ b 1))) :readable))))
       (expected (upstream-safe (lambda () "(inlet :a (lambda* ((b 1)) (+ b 1)))")))
       (ok? (equal? actual expected)))
  (list 'upstream-test 25159 actual expected ok?))
