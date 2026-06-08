;; Imported from upstream s7test.scm line 25160.
;; Original form:
;; (test (object->string (inlet 'a (lambda* a (list a))) :readable) "(inlet :a (lambda a (list a)))")

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (object->string (inlet 'a (lambda* a (list a))) :readable))))
       (expected (upstream-safe (lambda () "(inlet :a (lambda a (list a)))")))
       (ok? (equal? actual expected)))
  (list 'upstream-test 25160 actual expected ok?))
