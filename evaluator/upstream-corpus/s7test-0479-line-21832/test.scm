;; Imported from upstream s7test.scm line 21832.
;; Original form:
;; (test (format #f "~s~a" #\a #\b) "#\\ab")

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (format #f "~s~a" #\a #\b))))
       (expected (upstream-safe (lambda () "#\\ab")))
       (ok? (equal? actual expected)))
  (list 'upstream-test 21832 actual expected ok?))
