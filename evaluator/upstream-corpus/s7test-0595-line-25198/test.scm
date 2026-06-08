;; Imported from upstream s7test.scm line 25198.
;; Original form:
;; (test (object->string (inlet 'a (make-iterator "123")) :readable) "(inlet :a (make-iterator \"123\"))")

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (object->string (inlet 'a (make-iterator "123")) :readable))))
       (expected (upstream-safe (lambda () "(inlet :a (make-iterator \"123\"))")))
       (ok? (equal? actual expected)))
  (list 'upstream-test 25198 actual expected ok?))
