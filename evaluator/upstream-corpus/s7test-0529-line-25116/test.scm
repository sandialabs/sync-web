;; Imported from upstream s7test.scm line 25116.
;; Original form:
;; (test (object->string (inlet 'a #\newline) :readable) "(inlet :a #\\newline)")

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (object->string (inlet 'a #\newline) :readable))))
       (expected (upstream-safe (lambda () "(inlet :a #\\newline)")))
       (ok? (equal? actual expected)))
  (list 'upstream-test 25116 actual expected ok?))
