;; Imported from upstream s7test.scm line 21464.
;; Original form:
;; (test (char->integer (string-ref "\x0e;" 0)) 14)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (char->integer (string-ref "\x0e;" 0)))))
       (expected (upstream-safe (lambda () 14)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 21464 actual expected ok?))
