;; Imported from upstream s7test.scm line 25127.
;; Original form:
;; (test (object->string (inlet 'a #<eof>) :readable) "(inlet :a (begin #<eof>))")

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (object->string (inlet 'a #<eof>) :readable))))
       (expected (upstream-safe (lambda () "(inlet :a (begin #<eof>))")))
       (ok? (equal? actual expected)))
  (list 'upstream-test 25127 actual expected ok?))
