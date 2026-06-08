;; Imported from upstream s7test.scm line 25105.
;; Original form:
;; (test (object->string (make-iterator #u(12 41 2)) :readable) "(make-iterator #u(12 41 2))")

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (object->string (make-iterator #u(12 41 2)) :readable))))
       (expected (upstream-safe (lambda () "(make-iterator #u(12 41 2))")))
       (ok? (equal? actual expected)))
  (list 'upstream-test 25105 actual expected ok?))
