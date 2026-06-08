;; Imported from upstream s7test.scm line 25140.
;; Original form:
;; (test (object->string (inlet 'a (hash-table)) :readable) "(inlet :a (hash-table))")

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (object->string (inlet 'a (hash-table)) :readable))))
       (expected (upstream-safe (lambda () "(inlet :a (hash-table))")))
       (ok? (equal? actual expected)))
  (list 'upstream-test 25140 actual expected ok?))
