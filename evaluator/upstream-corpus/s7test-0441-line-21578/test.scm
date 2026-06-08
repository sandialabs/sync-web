;; Imported from upstream s7test.scm line 21578.
;; Original form:
;; (test (newline #f) #\newline)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (newline #f))))
       (expected (upstream-safe (lambda () #\newline)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 21578 actual expected ok?))
