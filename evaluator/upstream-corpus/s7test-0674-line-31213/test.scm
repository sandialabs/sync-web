;; Imported from upstream s7test.scm line 31213.
;; Original form:
;; (test (and . ()) (and))

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (and . ()))))
       (expected (upstream-safe (lambda () (and))))
       (ok? (equal? actual expected)))
  (list 'upstream-test 31213 actual expected ok?))
