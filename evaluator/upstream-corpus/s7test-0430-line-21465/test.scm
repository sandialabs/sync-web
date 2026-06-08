;; Imported from upstream s7test.scm line 21465.
;; Original form:
;; (test (char->integer (string-ref "\x1e;" 0)) 30)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (char->integer (string-ref "\x1e;" 0)))))
       (expected (upstream-safe (lambda () 30)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 21465 actual expected ok?))
