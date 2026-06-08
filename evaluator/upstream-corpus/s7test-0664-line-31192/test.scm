;; Imported from upstream s7test.scm line 31192.
;; Original form:
;; (test (or . ()) (or))

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (or . ()))))
       (expected (upstream-safe (lambda () (or))))
       (ok? (equal? actual expected)))
  (list 'upstream-test 31192 actual expected ok?))
