;; Imported from upstream s7test.scm line 25161.
;; Original form:
;; (test (object->string (inlet 'a (lambda* (a (b 1) c) (list a b c))) :readable) "(inlet :a (lambda* (a (b 1) c) (list a b c)))")

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (object->string (inlet 'a (lambda* (a (b 1) c) (list a b c))) :readable))))
       (expected (upstream-safe (lambda () "(inlet :a (lambda* (a (b 1) c) (list a b c)))")))
       (ok? (equal? actual expected)))
  (list 'upstream-test 25161 actual expected ok?))
