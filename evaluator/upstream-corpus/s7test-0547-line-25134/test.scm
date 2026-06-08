;; Imported from upstream s7test.scm line 25134.
;; Original form:
;; (test (object->string (inlet 'a (list "hi")) :readable) "(inlet :a (list \"hi\"))")

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (object->string (inlet 'a (list "hi")) :readable))))
       (expected (upstream-safe (lambda () "(inlet :a (list \"hi\"))")))
       (ok? (equal? actual expected)))
  (list 'upstream-test 25134 actual expected ok?))
