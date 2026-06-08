;; Imported from upstream s7test.scm line 25138.
;; Original form:
;; (test (object->string (inlet 'a #f) :readable) "(inlet :a #f)")

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (object->string (inlet 'a #f) :readable))))
       (expected (upstream-safe (lambda () "(inlet :a #f)")))
       (ok? (equal? actual expected)))
  (list 'upstream-test 25138 actual expected ok?))
