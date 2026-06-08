;; Imported from upstream s7test.scm line 21466.
;; Original form:
;; (test (char->integer (string-ref "\xff;" 0)) 255)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (char->integer (string-ref "\xff;" 0)))))
       (expected (upstream-safe (lambda () 255)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 21466 actual expected ok?))
