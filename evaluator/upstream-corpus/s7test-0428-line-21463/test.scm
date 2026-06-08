;; Imported from upstream s7test.scm line 21463.
;; Original form:
;; (test (char->integer (string-ref "\x0;" 0)) 0)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (char->integer (string-ref "\x0;" 0)))))
       (expected (upstream-safe (lambda () 0)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 21463 actual expected ok?))
