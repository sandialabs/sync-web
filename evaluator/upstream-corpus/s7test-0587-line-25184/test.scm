;; Imported from upstream s7test.scm line 25184.
;; Original form:
;; (test (object->string (inlet 'a (call-with-input-string "1" (lambda (p) p))) :readable) "(inlet :a (call-with-input-string \"\" (lambda (p) p)))")

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (object->string (inlet 'a (call-with-input-string "1" (lambda (p) p))) :readable))))
       (expected (upstream-safe (lambda () "(inlet :a (call-with-input-string \"\" (lambda (p) p)))")))
       (ok? (equal? actual expected)))
  (list 'upstream-test 25184 actual expected ok?))
