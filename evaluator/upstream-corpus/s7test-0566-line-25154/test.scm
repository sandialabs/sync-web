;; Imported from upstream s7test.scm line 25154.
;; Original form:
;; (test (object->string (inlet 'a (lambda (a . b) (list a b))) :readable) "(inlet :a (lambda (a . b) (list a b)))")

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (object->string (inlet 'a (lambda (a . b) (list a b))) :readable))))
       (expected (upstream-safe (lambda () "(inlet :a (lambda (a . b) (list a b)))")))
       (ok? (equal? actual expected)))
  (list 'upstream-test 25154 actual expected ok?))
