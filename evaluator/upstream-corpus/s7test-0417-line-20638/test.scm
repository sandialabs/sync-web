;; Imported from upstream s7test.scm line 20638.
;; Original form:
;; (test (call-with-input-string "0123456789" (lambda (p) (set! (port-position p) 3) (list (read-char p) (port-position p)))) '(#\3 4))

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (call-with-input-string "0123456789" (lambda (p) (set! (port-position p) 3) (list (read-char p) (port-position p)))))))
       (expected (upstream-safe (lambda () '(#\3 4))))
       (ok? (equal? actual expected)))
  (list 'upstream-test 20638 actual expected ok?))
