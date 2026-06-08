;; Imported from upstream s7test.scm line 21840.
;; Original form:
;; (test (format #f "~01c" #\a) "a")

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (format #f "~01c" #\a))))
       (expected (upstream-safe (lambda () "a")))
       (ok? (equal? actual expected)))
  (list 'upstream-test 21840 actual expected ok?))
