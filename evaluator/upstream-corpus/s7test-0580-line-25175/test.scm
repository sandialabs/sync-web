;; Imported from upstream s7test.scm line 25175.
;; Original form:
;; (test (object->string (inlet 'a (dilambda (lambda () 1) (lambda (x) x))) :readable) "(inlet :a (dilambda (lambda () 1) (lambda (x) x)))")

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (object->string (inlet 'a (dilambda (lambda () 1) (lambda (x) x))) :readable))))
       (expected (upstream-safe (lambda () "(inlet :a (dilambda (lambda () 1) (lambda (x) x)))")))
       (ok? (equal? actual expected)))
  (list 'upstream-test 25175 actual expected ok?))
