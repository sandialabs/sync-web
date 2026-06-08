;; Imported from upstream s7test.scm line 25136.
;; Original form:
;; (test (object->string (inlet 'a '(1 2 . 3)) :readable) "(inlet :a (cons 1 (cons 2 3)))")

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (object->string (inlet 'a '(1 2 . 3)) :readable))))
       (expected (upstream-safe (lambda () "(inlet :a (cons 1 (cons 2 3)))")))
       (ok? (equal? actual expected)))
  (list 'upstream-test 25136 actual expected ok?))
