;; Imported from upstream s7test.scm line 21837.
;; Original form:
;; (test (format #f "~nc" 3 #\a) "aaa")

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (format #f "~nc" 3 #\a))))
       (expected (upstream-safe (lambda () "aaa")))
       (ok? (equal? actual expected)))
  (list 'upstream-test 21837 actual expected ok?))
